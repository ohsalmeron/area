//! Compositor Module
//!
//! Handles OpenGL rendering, window textures, and visual effects.

use x11rb::connection::RequestConnection;
pub mod renderer;
pub mod gl_context;
pub mod dri3;
pub mod fps;
pub mod c_window;
pub mod cursor;

use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};
use x11rb::protocol::composite::{self, ConnectionExt as CompositeExt};
use x11rb::protocol::damage::{self, ConnectionExt as DamageExt};
use x11rb::protocol::xproto::*;

use crate::compositor::c_window::CWindow;
use gl_context::GlContext;
use renderer::Renderer;
use cursor::CursorManager;

use tokio::sync::mpsc;
use crate::shared::Geometry;

/// Commands sent from the WM logic to the Compositor thread
pub enum CompositorCommand {
    /// Add a new window for compositing
    AddWindow(CWindow),
    /// Remove a window from compositing
    RemoveWindow(u32),
    /// Update window position and size
    UpdateWindowGeometry(u32, Geometry),
    /// Mark a window as damaged (needs re-paint)
    UpdateWindowDamage(u32),
    /// Update window state (for fullscreen detection)
    UpdateWindowState(u32),
    /// Unredirect a window (bypass compositor for performance)
    UnredirectWindow(u32),
    /// Redirect a window (re-enable compositing)
    RedirectWindow(u32),
    /// Update cursor position and visibility
    UpdateCursor(i16, i16, bool),
    /// Update cursor image (shape change detected)
    UpdateCursorImage,
    /// Signal that a render frame is needed
    TriggerRender,
    /// Shutdown the compositor thread
    Shutdown,
}

/// A handle to the compositor actor
#[derive(Clone)]
pub struct Compositor {
    pub overlay_window: u32,
    tx: mpsc::UnboundedSender<CompositorCommand>,
}

/// The actual compositor implementation (internal to the compositor thread)
struct CompositorInner {
    conn: std::sync::Arc<x11rb::rust_connection::RustConnection>,
    overlay_window: u32,
    gl_context: Option<GlContext>,
    renderer: Option<Renderer>,
    fps_counter: fps::FpsCounter,
    cursor_manager: Option<CursorManager>,
    windows: HashMap<u32, CWindow>,
    shell: crate::shell::Shell,
    rx: mpsc::UnboundedReceiver<CompositorCommand>,
    /// Force a render even if no damage/motion
    force_render: bool,
    /// EWMH atoms (cached for performance)
    ewmh_atoms: Option<crate::wm::ewmh::Atoms>,
    /// Count of unredirected windows (for overlay window visibility management)
    unredirected_count: u32,
    /// Whether to unredirect fullscreen windows (from config)
    unredirect_fullscreen: bool,
}

impl Compositor {
    /// Spawn the compositor in its own thread
    pub fn spawn(
        conn: std::sync::Arc<x11rb::rust_connection::RustConnection>,
        screen_num: usize,
        root: u32,
    ) -> Result<Self> {
        use x11rb::connection::Connection;
        info!("Spinning up compositor thread");
        
        // 1. Initial X11 setup (needs to be on main thread to negotiate extensions)
        conn.as_ref().extension_information(composite::X11_EXTENSION_NAME)?
            .context("Composite extension not available")?;
        conn.as_ref().composite_query_version(0, 4)?.reply()?;
        
        conn.as_ref().extension_information(damage::X11_EXTENSION_NAME)?
            .context("Damage extension not available")?;
        conn.as_ref().damage_query_version(1, 1)?.reply()?;
        
        // Redirect all windows
        conn.as_ref().composite_redirect_subwindows(root, composite::Redirect::MANUAL)?;
        
        // Get Overlay Window
        let overlay_window = conn.as_ref().composite_get_overlay_window(root)?.reply()?.overlay_win;
        
        // Make input-transparent
        use x11rb::protocol::shape::{ConnectionExt as ShapeExt, SK, SO};
        conn.as_ref().shape_rectangles(SO::SET, SK::INPUT, x11rb::protocol::xproto::ClipOrdering::UNSORTED,
            overlay_window, 0, 0, &[])?;
            
        conn.as_ref().flush()?;

        let (tx, rx) = mpsc::unbounded_channel();
        let conn_clone = conn.clone();
        
        // 2. Spawn the compositor thread
        std::thread::spawn(move || {
            let mut inner = CompositorInner::new(conn_clone, screen_num, overlay_window, rx);
            if let Err(e) = inner.run() {
                error!("Compositor thread crashed: {}", e);
            }
        });

        Ok(Self {
            overlay_window,
            tx,
        })
    }

    pub fn add_window(&self, window: CWindow) {
        let _ = self.tx.send(CompositorCommand::AddWindow(window));
    }

    pub fn remove_window(&self, window_id: u32) {
        let _ = self.tx.send(CompositorCommand::RemoveWindow(window_id));
    }

    pub fn update_window_geometry(&self, window_id: u32, geometry: Geometry) {
        let _ = self.tx.send(CompositorCommand::UpdateWindowGeometry(window_id, geometry));
    }

    pub fn update_window_damage(&self, window_id: u32) {
        let _ = self.tx.send(CompositorCommand::UpdateWindowDamage(window_id));
    }

    pub fn update_window_state(&self, window_id: u32) {
        let _ = self.tx.send(CompositorCommand::UpdateWindowState(window_id));
    }

    pub fn unredirect_window(&self, window_id: u32) {
        let _ = self.tx.send(CompositorCommand::UnredirectWindow(window_id));
    }

    pub fn redirect_window(&self, window_id: u32) {
        let _ = self.tx.send(CompositorCommand::RedirectWindow(window_id));
    }

    pub fn update_cursor(&self, x: i16, y: i16, visible: bool) {
        let _ = self.tx.send(CompositorCommand::UpdateCursor(x, y, visible));
    }
    
    pub fn update_cursor_image(&self) {
        let _ = self.tx.send(CompositorCommand::UpdateCursorImage);
    }

    pub fn trigger_render(&self) {
        let _ = self.tx.send(CompositorCommand::TriggerRender);
    }
    
    /// Shutdown the compositor gracefully
    pub fn shutdown(&self) {
        let _ = self.tx.send(CompositorCommand::Shutdown);
    }
}

impl CompositorInner {
    fn new(
        conn: std::sync::Arc<x11rb::rust_connection::RustConnection>,
        screen_num: usize,
        overlay_window: u32,
        rx: mpsc::UnboundedReceiver<CompositorCommand>,
    ) -> Self {
        let gl_context = match GlContext::new(&conn, screen_num, overlay_window) {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                error!("Failed to initialize GL context: {}", e);
                None
            }
        };

        use x11rb::connection::Connection;
        let renderer = gl_context.as_ref().and_then(|_| Renderer::new().ok());
        let mut cursor_manager = CursorManager::new(&conn, conn.as_ref().setup().roots[screen_num].root).ok();
        
        // Load initial cursor image and position immediately (don't wait for events)
        if let Some(ref mut cursor) = cursor_manager {
            // Load cursor image
            if let Err(e) = cursor.update_image(&conn) {
                debug!("Failed to load initial cursor image: {}", e);
            }
            
            // Get initial cursor position from X server
            let root = conn.as_ref().setup().roots[screen_num].root;
            if let Ok(cookie) = conn.query_pointer(root) {
                if let Ok(pointer) = cookie.reply() {
                    cursor.update_position(pointer.root_x, pointer.root_y);
                    debug!("Initial cursor position: ({}, {})", pointer.root_x, pointer.root_y);
                }
            }
        }
        // Use default panel config for compositor's shell (it's just for rendering)
        let default_panel_config = crate::config::PanelConfig::default();
        let shell = crate::shell::Shell::new(
            conn.as_ref().setup().roots[screen_num].width_in_pixels,
            conn.as_ref().setup().roots[screen_num].height_in_pixels,
            default_panel_config,
        );
        
        // Try to initialize EWMH atoms (may fail if WM hasn't initialized them yet)
        let ewmh_atoms = crate::wm::ewmh::Atoms::new(conn.as_ref()).ok();

        Self {
            conn,
            overlay_window,
            gl_context,
            renderer,
            fps_counter: fps::FpsCounter::new(),
            cursor_manager,
            windows: HashMap::new(),
            shell,
            rx,
            force_render: true, // Initial render
            ewmh_atoms,
            unredirected_count: 0,
            unredirect_fullscreen: false, // TODO: Pass from config
        }
    }

    fn run(&mut self) -> Result<()> {
        info!("Compositor rendering loop started");
        let mut needs_render = false;

        loop {
            // Process commands
            if needs_render {
                // Non-blocking drain
                while let Ok(cmd) = self.rx.try_recv() {
                    self.handle_command(cmd);
                }
            } else {
                // Blocking wait for first command
                if let Some(cmd) = self.rx.blocking_recv() {
                    self.handle_command(cmd);
                    // Drain any others
                    while let Ok(cmd) = self.rx.try_recv() {
                        self.handle_command(cmd);
                    }
                } else {
                    break; 
                }
            }

            // Check damage after processing commands
            needs_render = self.any_damaged();
            
            // Only render cursor if it moved or is dirty (changed shape/image)
            // This prevents unnecessary rendering every frame when cursor is idle
            if let Some(ref cursor) = self.cursor_manager {
                if cursor.visible {
                    // Render if cursor image not loaded yet (initial load)
                    if cursor.width == 0 || cursor.height == 0 {
                        needs_render = true;
                    } 
                    // Render if cursor moved (for smooth movement tracking)
                    else if cursor.has_moved() {
                        needs_render = true;
                    }
                    // Render if cursor shape/image changed (dirty flag set by XfixesCursorNotify)
                    else if cursor.dirty {
                        needs_render = true;
                    }
                }
            }

            // Perform rendering
            if needs_render {
                use x11rb::connection::Connection;
                let (w, h) = {
                    let screen = &self.conn.as_ref().setup().roots[0];
                    (screen.width_in_pixels as f32, screen.height_in_pixels as f32)
                };
                self.render(w, h)?;
                self.clear_damage();
                needs_render = false;
                
                // Log FPS periodically (every 60 frames, ~1 second at 60fps)
                if self.fps_counter.frame_count() % 60 == 0 {
                    let fps = self.fps();
                    if fps > 0.0 {
                        debug!("Compositor FPS: {:.1} (overlay_window={})", fps, self.overlay_window);
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_command(&mut self, cmd: CompositorCommand) {
        match cmd {
            CompositorCommand::AddWindow(w) => {
                // #region agent log
                {
                    use std::fs::OpenOptions;
                    use std::io::Write;
                    let log_entry = serde_json::json!({
                        "sessionId": "debug-session",
                        "runId": "run1",
                        "hypothesisId": "D",
                        "location": "compositor/mod.rs:311",
                        "message": "Compositor AddWindow command",
                        "data": {"window_id": w.id, "client_id": w.client_id, "viewable": w.viewable, "geometry": {"x": w.geometry.x, "y": w.geometry.y, "width": w.geometry.width, "height": w.geometry.height}},
                        "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
                    });
                    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("/home/bizkit/GitHub/area/.cursor/debug.log") {
                        let _ = writeln!(file, "{}", log_entry);
                    }
                }
                // #endregion
                
                use x11rb::connection::Connection;
                let id = w.id;
                self.windows.insert(id, w);
                // Create damage object
                if let Ok(did) = self.conn.as_ref().generate_id() {
                    let _ = self.conn.as_ref().damage_create(did, id, damage::ReportLevel::NON_EMPTY);
                    if let Some(win) = self.windows.get_mut(&id) {
                        win.damage = Some(did);
                        win.damaged = true;
                    }
                }
                // Check if window is already fullscreen when added
                self.handle_window_state_update(id);
                
                // #region agent log
                {
                    use std::fs::OpenOptions;
                    use std::io::Write;
                    let log_entry = serde_json::json!({
                        "sessionId": "debug-session",
                        "runId": "run1",
                        "hypothesisId": "D",
                        "location": "compositor/mod.rs:325",
                        "message": "Window added to compositor windows map",
                        "data": {"window_id": id, "total_windows": self.windows.len()},
                        "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
                    });
                    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("/home/bizkit/GitHub/area/.cursor/debug.log") {
                        let _ = writeln!(file, "{}", log_entry);
                    }
                }
                // #endregion
            }
            CompositorCommand::RemoveWindow(id) => {
                if let Some(w) = self.windows.remove(&id) {
                    // If window was unredirected, decrement count
                    if w.unredirected && self.unredirected_count > 0 {
                        self.unredirected_count -= 1;
                    }
                    
                    // Clean up damage object
                    if let Some(d) = w.damage {
                        let _ = self.conn.as_ref().damage_destroy(d);
                    }
                    
                    // Free X11 pixmap if it exists
                    if let Some(pixmap) = w.pixmap {
                        let _ = self.conn.as_ref().free_pixmap(pixmap);
                    }
                    
                    // Remove texture from renderer (clean up GLX pixmap and OpenGL texture)
                    if let (Some(gl_ctx), Some(renderer)) = (&mut self.gl_context, &mut self.renderer) {
                        renderer.remove_texture(gl_ctx, id);
                    }
                    
                    debug!("Removed window {} from compositor (cleaned up damage, pixmap, and texture)", id);
                }
            }
            CompositorCommand::UpdateWindowGeometry(id, geom) => {
                if let Some(w) = self.windows.get_mut(&id) {
                    // Check if size changed significantly (more than 10% change)
                    let old_outer = w.outer_geometry();
                    let new_outer = Geometry {
                        x: geom.x - w.border_width as i32,
                        y: geom.y - w.border_width as i32,
                        width: geom.width + (w.border_width as u32) * 2,
                        height: geom.height + (w.border_width as u32) * 2,
                    };
                    
                    let size_changed_significantly = 
                        (old_outer.width as f32 - new_outer.width as f32).abs() / old_outer.width.max(1) as f32 > 0.1 ||
                        (old_outer.height as f32 - new_outer.height as f32).abs() / old_outer.height.max(1) as f32 > 0.1;
                    
                    // If size changed significantly, remove texture to force recreation
                    if size_changed_significantly {
                        if let Some(ref gl_ctx) = self.gl_context {
                            if let Some(ref mut renderer) = self.renderer {
                                renderer.remove_texture(gl_ctx, id);
                                // Also clear pixmap so it gets recreated
                                w.pixmap = None;
                                debug!("Geometry changed significantly for window {}, removed texture for recreation", id);
                            }
                        }
                    }
                    
                    w.geometry = geom;
                    w.damaged = true;
                }
            }
            CompositorCommand::UpdateWindowDamage(id) => {
                if let Some(w) = self.windows.get_mut(&id) {
                    w.damaged = true;
                }
            }
            CompositorCommand::UpdateWindowState(id) => {
                self.handle_window_state_update(id);
            }
            CompositorCommand::UnredirectWindow(id) => {
                self.unredirect_window(id);
            }
            CompositorCommand::RedirectWindow(id) => {
                self.redirect_window(id);
            }
            CompositorCommand::UpdateCursor(x, y, visible) => {
                if let Some(ref mut c) = self.cursor_manager {
                    c.update_position(x, y);
                    c.visible = visible;
                }
            }
            CompositorCommand::UpdateCursorImage => {
                if let Some(ref mut c) = self.cursor_manager {
                    if let Err(e) = c.update_image(self.conn.as_ref()) {
                        debug!("Failed to update cursor image: {}", e);
                    }
                }
            }
            CompositorCommand::TriggerRender => {
                self.force_render = true;
            }
            CompositorCommand::Shutdown => {
                // The channel drop handles this usually, but we could add a flag
            }
        }
    }
    
    /// Get current FPS (proxied from Handle if needed, but here for completeness)
    pub fn fps(&self) -> f64 {
        self.fps_counter.fps()
    }

    /// Check if a window has the _NET_WM_STATE_FULLSCREEN property set
    fn check_ewmh_fullscreen(&self, window_id: u32) -> bool {
        let atoms = match &self.ewmh_atoms {
            Some(a) => a,
            None => return false,
        };
        
        let cookie = match self.conn.as_ref().get_property(
            false,
            window_id,
            atoms.net_wm_state,
            AtomEnum::ATOM,
            0,
            1024,
        ) {
            Ok(c) => c,
            Err(_) => return false,
        };
        
        if let Ok(reply) = cookie.reply() {
            if let Some(mut value32) = reply.value32() {
                return value32.any(|atom| atom == atoms._net_wm_state_fullscreen);
            }
        }
        false
    }

    /// Handle window state updates (just mark for re-render - we composite everything, overlay on top)
    fn handle_window_state_update(&mut self, window_id: u32) {
        // Just mark window as damaged so it re-renders with correct fullscreen handling
        // We don't unredirect - we composite everything and overlay panel/cursor on top
        if let Some(window) = self.windows.get_mut(&window_id) {
            window.damaged = true;
            debug!("Window {} state changed, marked for re-render", window_id);
        }
    }
    
    /// Unredirect a window (allow it to render directly, bypassing compositor)
    fn unredirect_window(&mut self, window_id: u32) {
        use x11rb::connection::Connection;
        
        // Check if window exists and get client_id BEFORE mutable borrow
        let client_id = if let Some(window) = self.windows.get(&window_id) {
            if window.unredirected {
                return; // Already unredirected
            }
            window.client_id
        } else {
            return; // Window not found
        };
        
        // Check if window is fullscreen (EWMH or geometry-based) BEFORE mutable borrow
        let is_fullscreen = {
            // Check EWMH fullscreen state
            let ewmh_fullscreen = self.check_ewmh_fullscreen(window_id) || 
                                 self.check_ewmh_fullscreen(client_id);
            
            // Check geometry-based fullscreen
            let screen = &self.conn.as_ref().setup().roots[0];
            let geometry_fullscreen = if let Some(window) = self.windows.get(&window_id) {
                window.is_fullscreen(
                    screen.width_in_pixels,
                    screen.height_in_pixels,
                )
            } else {
                false
            };
            
            ewmh_fullscreen || geometry_fullscreen
        };
        
        // Now get mutable access to window
        if let Some(window) = self.windows.get_mut(&window_id) {
            // Unredirect the window using Composite extension
            if let Err(e) = self.conn.as_ref().composite_unredirect_window(
                window_id,
                composite::Redirect::MANUAL,
            ) {
                warn!("Failed to unredirect window {}: {}", window_id, e);
                return;
            }
            
            window.unredirected = true;
            window.redirected = false;
            self.unredirected_count += 1;
            
            // When we have unredirected fullscreen windows, lower the overlay window below them
            // This ensures unredirected windows are visible (they render directly to screen)
            if self.unredirected_count > 0 {
                // Lower overlay window below unredirected windows so they can render on top
                if let Err(e) = self.conn.as_ref().configure_window(
                    self.overlay_window,
                    &ConfigureWindowAux::new().stack_mode(StackMode::BELOW),
                ) {
                    warn!("Failed to lower overlay window below unredirected windows: {}", e);
                } else {
                    debug!("Lowered overlay window below unredirected windows (count: {})", self.unredirected_count);
                }
            }
            
            debug!("Unredirected window {} (count: {}, fullscreen: {})", window_id, self.unredirected_count, is_fullscreen);
        }
        
        // CRITICAL FIX: Raise unredirected fullscreen windows above everything
        // This ensures they appear on top even when unredirected (rendering directly to screen)
        // Do this AFTER releasing the mutable borrow of self.windows
        if is_fullscreen {
            if let Err(e) = self.conn.as_ref().configure_window(
                window_id,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            ) {
                warn!("Failed to raise unredirected fullscreen window {}: {}", window_id, e);
            } else {
                debug!("Raised unredirected fullscreen window {} above all windows", window_id);
            }
        }
    }
    
    /// Redirect a window (re-enable compositing)
    fn redirect_window(&mut self, window_id: u32) {
        if let Some(window) = self.windows.get_mut(&window_id) {
            if !window.unredirected {
                // Already redirected
                return;
            }
            
            // Redirect the window using Composite extension
            if let Err(e) = self.conn.as_ref().composite_redirect_window(
                window_id,
                composite::Redirect::MANUAL,
            ) {
                warn!("Failed to redirect window {}: {}", window_id, e);
                return;
            }
            
            window.unredirected = false;
            window.redirected = true;
            if self.unredirected_count > 0 {
                self.unredirected_count -= 1;
            }
            
            debug!("Redirected window {} (count: {})", window_id, self.unredirected_count);
        }
    }

    /// Render all managed windows and shell components.
    /// This is called internal to the Compositor thread.
    fn render(&mut self, screen_width: f32, screen_height: f32) -> Result<()> {
        use x11rb::connection::Connection;
        // Update shell state (animations, clock, etc.)
        self.shell.update();
        
        // Local aliases for brevity and compatibility with existing code
        // Note: self.conn is Arc<RustConnection>, so we use as_ref() to get &RustConnection
        let conn = self.conn.as_ref();
        let shell = &self.shell;

        // Check EWMH fullscreen state BEFORE mutable borrow of gl_context/renderer
        // For windows with frames, check the client window ID (EWMH state is on client, not frame)
        let fullscreen_windows: std::collections::HashSet<u32> = self.windows.values()
            .filter(|w| {
                // Check the client window ID for fullscreen state (EWMH state is on client, not frame)
                let check_id = if w.id != w.client_id {
                    // This is a frame window - check the client window for fullscreen state
                    w.client_id
                } else {
                    // This is a client window - check itself
                    w.id
                };
                self.check_ewmh_fullscreen(check_id)
            })
            .map(|w| w.id)  // Map to the tracked window ID (frame or client)
            .collect();

        if let (Some(gl_context), Some(renderer)) = (&mut self.gl_context, &mut self.renderer) {
            self.fps_counter.tick();
            gl_context.make_current()?;
            
            unsafe {
                gl::ClearColor(0.15, 0.15, 0.15, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
                gl::Enable(gl::BLEND);
                gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            }
            
            // Panel removed - no height adjustment needed
            
            // First pass: lazy pixmap binding
            // Skip unmapped/unviewable windows (performance optimization)
            // CRITICAL: Don't check failed windows every frame - this causes performance issues
            let windows_to_bind: Vec<u32> = self.windows.values()
                .filter(|w| {
                    // Only attempt binding if window is viewable, has no texture, and hasn't failed
                    w.viewable && !renderer.has_texture(w.id) && !w.bind_failed
                })
                .map(|w| w.id)
                .collect();
            
            if !windows_to_bind.is_empty() {
                debug!("Attempting to create pixmaps for {} window(s): {:?}", windows_to_bind.len(), windows_to_bind);
            }
            
            for window_id in windows_to_bind {
                // Get window reference and perform initial checks
                let (window_id_copy, needs_redirect) = {
                    let window = match self.windows.get(&window_id) {
                        Some(w) => w,
                        None => continue,
                    };
                    
                    if window.bind_failed {
                        continue;
                    }
                    
                    match conn.get_window_attributes(window.id) {
                        Ok(cookie) => {
                            if let Ok(window_attrs) = cookie.reply() {
                                use x11rb::protocol::xproto::MapState;
                                if window_attrs.map_state == MapState::UNMAPPED || window_attrs.map_state == MapState::UNVIEWABLE {
                                    continue;
                                }
                            }
                        }
                        Err(_) => continue,
                    }
                    
                    (window.id, !window.redirected)
                };
                
                // CRITICAL: Redirect window BEFORE creating pixmap (required by Composite extension)
                if needs_redirect {
                    let composite_id_for_redirect = {
                        let window = match self.windows.get(&window_id) {
                            Some(w) => w.client_id,  // Use client_id for redirect
                            None => continue,
                        };
                        window
                    };
                    
                    // Redirect the window using Composite extension
                    if let Err(e) = conn.composite_redirect_window(
                        composite_id_for_redirect,
                        composite::Redirect::MANUAL,
                    ) {
                        warn!("Failed to redirect window {} before pixmap creation: {}", composite_id_for_redirect, e);
                        if let Some(w) = self.windows.get_mut(&window_id) {
                            w.bind_failed = true;
                            if !w.bind_failure_logged {
                                warn!("Window {} marked as bind_failed - will skip future pixmap creation attempts", window_id);
                                w.bind_failure_logged = true;
                            }
                        }
                        continue;
                    }
                    // Flush to ensure redirect request is sent to X server
                    // X11 requests are generally synchronous, so flush() should be sufficient
                    // The subsequent get_window_attributes check will verify the redirect took effect
                    conn.flush().ok();
                    
                    // Mark as redirected
                    if let Some(window) = self.windows.get_mut(&window_id) {
                        window.redirected = true;
                    }
                }
                
                if !self.windows.contains_key(&window_id) {
                    continue;
                }
                
                if let Ok(cookie) = conn.get_window_attributes(window_id_copy) {
                    if cookie.reply().is_err() {
                        if let Some(w) = self.windows.get_mut(&window_id) {
                            w.bind_failed = true;
                            if !w.bind_failure_logged {
                                warn!("Window {} marked as bind_failed (get_window_attributes failed) - will skip future attempts", window_id);
                                w.bind_failure_logged = true;
                            }
                        }
                        continue;
                    }
                } else {
                    if let Some(w) = self.windows.get_mut(&window_id) {
                        w.bind_failed = true;
                        if !w.bind_failure_logged {
                            warn!("Window {} marked as bind_failed (get_window_attributes cookie failed) - will skip future attempts", window_id);
                            w.bind_failure_logged = true;
                        }
                    }
                    continue;
                }

                let window = match self.windows.get_mut(&window_id) {
                    Some(w) => w,
                    None => continue,
                };

                // Use client_id for pixmap creation - the actual window content is in the client window
                // window.id might be a frame window (decoration), but we need the client window content
                let composite_id = window.client_id;

                if let Ok(pixmap) = conn.generate_id() {
                    debug!("Attempting to create pixmap {} for window {}", pixmap, window_id);
                    match conn.composite_name_window_pixmap(composite_id, pixmap) {
                        Ok(cookie) => {
                            if cookie.check().is_err() {
                                warn!("composite_name_window_pixmap failed for window {} (pixmap {})", window_id, pixmap);
                                if let Some(w) = self.windows.get_mut(&window_id) {
                                    w.bind_failed = true;
                                    if !w.bind_failure_logged {
                                        warn!("Window {} marked as bind_failed - will skip future pixmap creation attempts", window_id);
                                        w.bind_failure_logged = true;
                                    }
                                }
                                let _ = conn.free_pixmap(pixmap);
                                continue;
                            }
                            // Flush to ensure pixmap creation request is sent to X server
                            // X11 requests are generally synchronous, so flush() should be sufficient
                            // The subsequent get_geometry check will verify the pixmap is ready
                            conn.flush().ok();
                            
                            match conn.get_geometry(pixmap) {
                                Ok(cookie) => match cookie.reply() {
                                    Ok(pixmap_geom) => {
                                    if pixmap_geom.width == 0 || pixmap_geom.height == 0 {
                                        debug!("Pixmap {} for window {} has invalid dimensions: {}x{}", pixmap, window_id, pixmap_geom.width, pixmap_geom.height);
                                        let _ = conn.free_pixmap(pixmap);
                                        continue;
                                    }
                                    
                                    // Compare pixmap size with client window size (not frame window size)
                                    // Get client window geometry directly
                                    let client_geom = match conn.get_geometry(composite_id) {
                                        Ok(cookie) => match cookie.reply() {
                                            Ok(geom) => (geom.width, geom.height),
                                            Err(_) => {
                                                let _ = conn.free_pixmap(pixmap);
                                                continue;
                                            }
                                        }
                                        Err(_) => {
                                            let _ = conn.free_pixmap(pixmap);
                                            continue;
                                        }
                                    };
                                    
                                    if pixmap_geom.width != client_geom.0 || pixmap_geom.height != client_geom.1 {
                                        debug!("Pixmap {} size mismatch for window {} (client {}): pixmap={}x{}, client={}x{}", 
                                            pixmap, window_id, composite_id, pixmap_geom.width, pixmap_geom.height, client_geom.0, client_geom.1);
                                        let _ = conn.free_pixmap(pixmap);
                                        continue;
                                    }
                                    
                                    let depth = match conn.get_geometry(composite_id) {
                                        Ok(cookie) => match cookie.reply() {
                                            Ok(geom) => geom.depth,
                                            Err(e) => {
                                                warn!("Failed to get geometry for window {}: {}", composite_id, e);
                                                let _ = conn.free_pixmap(pixmap);
                                                continue;
                                            }
                                        }
                                        Err(e) => {
                                            warn!("Failed to get geometry cookie for window {}: {}", composite_id, e);
                                            let _ = conn.free_pixmap(pixmap);
                                            continue;
                                        }
                                    };

                                    debug!("Created pixmap {} for window {} ({}x{}, depth {})", pixmap, window_id, pixmap_geom.width, pixmap_geom.height, depth);
                                    window.pixmap = Some(pixmap);
                                    match renderer.update_window_pixmap(gl_context, window.id, pixmap, depth) {
                                        Ok(_) => {
                                            debug!("Successfully created texture for window {}", window_id);
                                            // Mark window as damaged so texture gets bound on next render
                                            // This ensures initial content is displayed even if damage events are delayed
                                            window.damaged = true;
                                            window.frames_since_pixmap = 0; // Reset counter
                                        }
                                        Err(e) => {
                                            warn!("Failed to create texture for window {} (pixmap {}, depth {}): {}", window_id, pixmap, depth, e);
                                            window.pixmap = None;
                                            window.bind_failed = true;
                                            if !window.bind_failure_logged {
                                                warn!("Window {} marked as bind_failed - will skip future pixmap creation attempts", window_id);
                                                window.bind_failure_logged = true;
                                            }
                                            let _ = conn.free_pixmap(pixmap);
                                        }
                                    }
                                }
                                    Err(e) => {
                                        warn!("Failed to get pixmap geometry for window {} (pixmap {}): {}", window_id, pixmap, e);
                                        let _ = conn.free_pixmap(pixmap);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to get pixmap geometry cookie for window {} (pixmap {}): {}", window_id, pixmap, e);
                                    let _ = conn.free_pixmap(pixmap);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("composite_name_window_pixmap error for window {}: {}", window_id, e);
                        }
                    }
                } else {
                    warn!("Failed to generate pixmap ID for window {}", window_id);
                }
            }
            
            // Second pass: render windows
            // Separate normal windows from fullscreen windows
            // Fullscreen windows should render LAST (on top of everything)
            // Collect window IDs and render info first to avoid borrow checker issues
            let mut normal_windows = Vec::new();
            let mut fullscreen_windows_to_render = Vec::new();
            
            // First, collect all window info without mutable borrows
            let window_info: Vec<(u32, u32, bool, bool, bool, bool)> = self.windows.values()
                .map(|w| {
                    // Check fullscreen state: if this is a frame window, check the client window's state
                    let check_fullscreen_id = if w.id != w.client_id {
                        w.client_id  // Frame window - check client window for fullscreen state
                    } else {
                        w.id  // Client window - check itself
                    };
                    let is_fullscreen_geometry = w.is_fullscreen(screen_width as u16, screen_height as u16);
                    let is_fullscreen_ewmh = fullscreen_windows.contains(&check_fullscreen_id);
                    let is_fullscreen = is_fullscreen_geometry || is_fullscreen_ewmh;
                    (w.id, w.client_id, w.unredirected, w.viewable, is_fullscreen, w.id != w.client_id)
                })
                .collect();
            
            for (window_id, client_id, unredirected, viewable, is_fullscreen, has_frame) in window_info {
                // Skip unredirected windows (they render directly, bypassing compositor)
                if unredirected {
                    continue;
                }
                
                // CRITICAL FIX: Skip frame windows if their CLIENT window is fullscreen
                // Frame windows are never fullscreen themselves - only their client window can be fullscreen
                if has_frame {
                    // This is a frame window - check if the CLIENT window is fullscreen
                    let client_is_fullscreen = {
                        // Check client window's geometry
                        let client_geom_fullscreen = if let Some(client_w) = self.windows.get(&client_id) {
                            client_w.is_fullscreen(screen_width as u16, screen_height as u16)
                        } else {
                            false
                        };
                        // Check client window's EWMH state
                        let client_ewmh_fullscreen = fullscreen_windows.contains(&client_id);
                        client_geom_fullscreen || client_ewmh_fullscreen
                    };
                    
                    if client_is_fullscreen {
                        // Frame window should be skipped entirely - client window is fullscreen and will be rendered separately
                        continue;
                    }
                    // Client is not fullscreen, so render frame normally (if viewable)
                    if !viewable {
                        continue; // Skip unmapped frames
                    }
                    normal_windows.push((window_id, window_id));
                    continue;
                }
                
                // This is a client window (or window without frame)
                // Only include viewable windows (or fullscreen windows even if frame is unmapped)
                if !viewable && !is_fullscreen {
                    continue;
                }
                
                // For fullscreen windows, render them in the fullscreen layer (on top)
                let (is_fullscreen_window, render_id) = if is_fullscreen {
                    (true, window_id)
                } else {
                    (false, window_id)
                };
                
                if is_fullscreen_window {
                    fullscreen_windows_to_render.push((window_id, render_id));
                } else {
                    normal_windows.push((window_id, render_id));
                }
            }
            
            // Render normal windows first
            normal_windows.sort_by_key(|(wid, _)| *wid);
            for (window_id, render_id) in normal_windows {
                // Get window from HashMap now (after collecting info)
                if let Some(window) = self.windows.get(&window_id) {
                    let has_texture = renderer.has_texture(render_id);
                    
                    if has_texture {
                        // Normal windows: render at their position
                        renderer.render_window(
                            gl_context,
                            render_id,
                            window.geometry.x as f32,
                            window.geometry.y as f32,
                            window.geometry.width as f32,
                            window.geometry.height as f32,
                            screen_width,
                            screen_height,
                            window.opacity,
                            window.damaged,
                            window.frames_since_pixmap,
                        );
                    } else {
                        // Fallback rendering
                        renderer.render_window_fallback(
                            gl_context,
                            render_id,
                            window.geometry.x as f32,
                            window.geometry.y as f32,
                            window.geometry.width as f32,
                            window.geometry.height as f32,
                            screen_width,
                            screen_height,
                        );
                    }
                }
            }
            
            use x11rb::protocol::xfixes::Region;
            const EMPTY_REGION: Region = 0;
            for window in self.windows.values_mut() {
                if window.damaged && window.damage.is_some() {
                    if let Some(damage_id) = window.damage {
                        let _ = conn.damage_subtract(damage_id, EMPTY_REGION, EMPTY_REGION);
                    }
                }
            }
            
            // Render panel (shell UI at bottom/top of screen)
            shell.panel.render(renderer, screen_width, screen_height);
            
            // Render logout dialog (if needed)
            shell.logout_dialog.render(renderer, screen_width, screen_height);
            
            // Render fullscreen windows LAST (on top of everything)
            fullscreen_windows_to_render.sort_by_key(|(wid, _)| *wid);
            for (window_id, render_id) in fullscreen_windows_to_render {
                // Get window from HashMap now (after collecting info)
                if let Some(window) = self.windows.get(&window_id) {
                    let has_texture = renderer.has_texture(render_id);
                    
                    if has_texture {
                        // Fullscreen windows: render covering entire screen (0,0 to screen_width, screen_height)
                        renderer.render_window(
                            gl_context,
                            render_id,  // Use client window if fullscreen with frame
                            0.0,  // x = 0
                            0.0,  // y = 0
                            screen_width,  // width = full screen
                            screen_height, // height = full screen
                            screen_width,
                            screen_height,
                            window.opacity,
                            window.damaged,
                            window.frames_since_pixmap,
                        );
                    } else {
                        // Fallback rendering for fullscreen
                        renderer.render_window_fallback(
                            gl_context,
                            render_id,
                            0.0,
                            0.0,
                            screen_width,
                            screen_height,
                            screen_width,
                            screen_height,
                        );
                    }
                }
            }
            
            if let Some(ref mut cursor) = self.cursor_manager {
                // Load cursor image if not loaded yet (fallback if XfixesCursorNotify didn't fire)
                if cursor.width == 0 || cursor.height == 0 || cursor.pixels.is_empty() {
                    if let Err(e) = cursor.update_image(self.conn.as_ref()) {
                        debug!("Failed to load cursor image during render: {}", e);
                    }
                }
                
                if cursor.visible && cursor.width > 0 && cursor.height > 0 && !cursor.pixels.is_empty() {
                    if cursor.dirty {
                        renderer.update_cursor_texture(
                            cursor.width,
                            cursor.height,
                            &cursor.pixels,
                            &mut cursor.texture_id,
                        );
                        cursor.dirty = false;
                    }
                    
                    let cursor_x = cursor.x as f32 - cursor.xhot as f32;
                    let cursor_y = cursor.y as f32 - cursor.yhot as f32;
                    
                    renderer.render_cursor(
                        cursor_x,
                        cursor_y,
                        cursor.width as f32,
                        cursor.height as f32,
                        screen_width,
                        screen_height,
                        cursor.texture_id,
                    );
                    
                    // Clear movement flag after rendering to prevent continuous rendering
                    // This ensures we only render when cursor actually moves again
                    cursor.clear_movement();
                }
            }
            
            gl_context.swap_buffers()?;
        }
        
        Ok(())
    }

    /// Check if any window is damaged or cursor moved
    pub fn any_damaged(&self) -> bool {
        if self.force_render {
            return true;
        }
        let window_damaged = self.windows.values().any(|w| w.damaged || w.damage.is_some());
        let cursor_moved = self.cursor_manager.as_ref()
            .map(|c| c.has_moved())
            .unwrap_or(false);
        // Always render if there are no windows (to show panel and background)
        // or if there's damage/cursor movement
        window_damaged || cursor_moved || self.windows.is_empty()
    }

    /// Clear all damage flags and increment frame counters
    pub fn clear_damage(&mut self) {
        self.force_render = false;
        for window in self.windows.values_mut() {
            window.damaged = false;
            // Increment frame counter if pixmap exists (for fallback binding)
            if window.pixmap.is_some() {
                window.frames_since_pixmap = window.frames_since_pixmap.saturating_add(1);
            }
        }
        if let Some(ref mut cursor) = self.cursor_manager {
            cursor.prev_x = cursor.x;
            cursor.prev_y = cursor.y;
        }
    }
}
