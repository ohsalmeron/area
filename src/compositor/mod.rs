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
use tracing::{debug, error, info};
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
    /// Update cursor position and visibility
    UpdateCursor(i16, i16, bool),
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

    pub fn update_cursor(&self, x: i16, y: i16, visible: bool) {
        let _ = self.tx.send(CompositorCommand::UpdateCursor(x, y, visible));
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
        let cursor_manager = CursorManager::new(&conn, conn.as_ref().setup().roots[screen_num].root).ok();
        let shell = crate::shell::Shell::new(
            conn.as_ref().setup().roots[screen_num].width_in_pixels,
            conn.as_ref().setup().roots[screen_num].height_in_pixels,
        );

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

            // Perform rendering
            if self.any_damaged() {
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
            }
            CompositorCommand::RemoveWindow(id) => {
                if let Some(w) = self.windows.remove(&id) {
                    if let Some(d) = w.damage {
                        let _ = self.conn.as_ref().damage_destroy(d);
                    }
                }
            }
            CompositorCommand::UpdateWindowGeometry(id, geom) => {
                if let Some(w) = self.windows.get_mut(&id) {
                    w.geometry = geom;
                    w.damaged = true;
                }
            }
            CompositorCommand::UpdateWindowDamage(id) => {
                if let Some(w) = self.windows.get_mut(&id) {
                    w.damaged = true;
                }
            }
            CompositorCommand::UpdateCursor(x, y, visible) => {
                if let Some(ref mut c) = self.cursor_manager {
                    c.update_position(x, y);
                    c.visible = visible;
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

    /// Render all managed windows and shell components.
    /// This is called internal to the Compositor thread.
    fn render(&mut self, screen_width: f32, screen_height: f32) -> Result<()> {
        use x11rb::connection::Connection;
        // Local aliases for brevity and compatibility with existing code
        // Note: self.conn is Arc<RustConnection>, so we use as_ref() to get &RustConnection
        let conn = self.conn.as_ref();
        let shell = &self.shell;

        if let (Some(gl_context), Some(renderer)) = (&mut self.gl_context, &mut self.renderer) {
            self.fps_counter.tick();
            gl_context.make_current()?;
            
            unsafe {
                gl::ClearColor(0.15, 0.15, 0.15, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);
                gl::Enable(gl::BLEND);
                gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            }
            
            let panel_height = shell.panel.height();
            
            // First pass: lazy pixmap binding
            let windows_to_bind: Vec<u32> = self.windows.values()
                .filter(|w| !renderer.has_texture(w.id) && !w.bind_failed)
                .map(|w| w.id)
                .collect();
            
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
                
                if needs_redirect {
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
                        }
                        continue;
                    }
                } else {
                    if let Some(w) = self.windows.get_mut(&window_id) {
                        w.bind_failed = true;
                    }
                    continue;
                }

                let window = match self.windows.get_mut(&window_id) {
                    Some(w) => w,
                    None => continue,
                };

                let composite_id = window.id;

                if let Ok(pixmap) = conn.generate_id() {
                    match conn.composite_name_window_pixmap(composite_id, pixmap) {
                        Ok(cookie) => {
                            if cookie.check().is_err() {
                                if let Some(w) = self.windows.get_mut(&window_id) {
                                    w.bind_failed = true;
                                }
                                let _ = conn.free_pixmap(pixmap);
                                continue;
                            }
                            conn.flush().ok();
                            std::thread::sleep(std::time::Duration::from_millis(10));
                            
                            match conn.get_geometry(pixmap) {
                                Ok(cookie) => match cookie.reply() {
                                    Ok(pixmap_geom) => {
                                    if pixmap_geom.width == 0 || pixmap_geom.height == 0 {
                                        let _ = conn.free_pixmap(pixmap);
                                        continue;
                                    }
                                    
                                    let extent = window.extents();
                                    if pixmap_geom.width != extent.width as u16 || pixmap_geom.height != extent.height as u16 {
                                        let _ = conn.free_pixmap(pixmap);
                                        continue;
                                    }
                                    
                                    let depth = match conn.get_geometry(composite_id) {
                                        Ok(cookie) => match cookie.reply() {
                                            Ok(geom) => geom.depth,
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

                                    window.pixmap = Some(pixmap);
                                    if let Err(_) = renderer.update_window_pixmap(gl_context, window.id, pixmap, depth) {
                                        window.pixmap = None;
                                        window.bind_failed = true;
                                        let _ = conn.free_pixmap(pixmap);
                                    }
                                }
                                    Err(_) => {
                                        let _ = conn.free_pixmap(pixmap);
                                    }
                                }
                                Err(_) => {
                                    let _ = conn.free_pixmap(pixmap);
                                }
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
            
            // Second pass: render windows
            let mut windows_to_render: Vec<_> = self.windows.values().collect();
            windows_to_render.sort_by_key(|w| w.id);
            
            for window in windows_to_render {
                let window_y = window.geometry.y as f32;
                let adjusted_y = if window_y < panel_height {
                    panel_height
                } else {
                    window_y
                };
                
                if renderer.has_texture(window.id) {
                    renderer.render_window(
                        gl_context,
                        window.id,
                        window.geometry.x as f32,
                        adjusted_y,
                        window.geometry.width as f32,
                        window.geometry.height as f32,
                        screen_width,
                        screen_height,
                        window.opacity,
                        window.damaged,
                    );
                } else {
                    renderer.render_window_fallback(
                        gl_context,
                        window.id,
                        window.geometry.x as f32,
                        adjusted_y,
                        window.geometry.width as f32,
                        window.geometry.height as f32,
                        screen_width,
                        screen_height,
                    );
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
            
            shell.panel.render(renderer, screen_width, screen_height);
            shell.logout_dialog.render(renderer, screen_width, screen_height);
            
            if let Some(ref mut cursor) = self.cursor_manager {
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
        window_damaged || cursor_moved
    }

    /// Clear all damage flags
    pub fn clear_damage(&mut self) {
        self.force_render = false;
        for window in self.windows.values_mut() {
            window.damaged = false;
        }
        if let Some(ref mut cursor) = self.cursor_manager {
            cursor.prev_x = cursor.x;
            cursor.prev_y = cursor.y;
        }
    }
}
