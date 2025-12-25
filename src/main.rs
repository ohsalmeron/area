
//!
//! A high-performance X11 window manager with built-in OpenGL compositor,
//! written in Rust. Inspired by XFWM4's integrated architecture.

mod wm;
mod compositor;
mod shared;
mod shell;
mod dbus;

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt;
use x11rb::protocol::xproto::ConfigureWindowAux;
use x11rb::protocol::Event;
use wm::client::Client;
use compositor::c_window::CWindow;

/// Main application state
struct AreaApp {
    /// X11 connection
    conn: Arc<x11rb::rust_connection::RustConnection>,
    
    /// Root window
    root: u32,
    
    /// WM Clients (Window Manager state)
    wm_windows: HashMap<u32, Client>,
    
    /// Window manager state
    wm: wm::WindowManager,
    
    /// Compositor state
    compositor: compositor::Compositor,
    
    /// Shell state
    shell: shell::Shell,
    
    /// Last frame time (for delta calculations)
    last_frame: Instant,
    
    /// Screen dimensions
    screen_width: u16,
    screen_height: u16,

    /// D-Bus manager
    _dbus: Option<dbus::DbusManager>,
    
    /// Notification service
    _notifications: Option<dbus::notifications::NotificationService>,
    
    /// Power management service
    power: Option<dbus::power::PowerService>,
    
    /// Windows currently being reparented (to ignore UnmapNotify/MapNotify from our own operations)
    reparenting_windows: HashSet<u32>,
    
    /// Frame windows created by the WM (to prevent recursive management)
    frame_windows: HashSet<u32>,
}

impl AreaApp {
    /// Initialize the application
    /// 
    /// # Arguments
    /// * `replace` - If true, attempt to replace existing WM
    async fn new(replace: bool) -> Result<Self> {
        // Connect to X11
        let (conn, screen_num) = x11rb::connect(None)
            .context("Failed to connect to X server")?;
        
        let conn = Arc::new(conn);
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let screen_width = screen.width_in_pixels;
        let screen_height = screen.height_in_pixels;
        
        info!("Connected to X server, screen {}, root window {}", screen_num, root);
        info!("Screen size: {}x{}", screen_width, screen_height);
        
        // Initialize window manager
        let wm = wm::WindowManager::new(&conn, screen_num, root, replace)
            .context("Failed to initialize window manager")?;
        
        // Initialize compositor
        let compositor = compositor::Compositor::new(&conn, screen_num, root)
            .context("Failed to initialize compositor")?;
        
        // Initialize shell
        let shell = shell::Shell::new(screen_width, screen_height);
        
        // Initialize D-Bus (optional, won't fail if D-Bus unavailable)
        let dbus = match dbus::DbusManager::new().await {
            Ok(d) => {
                info!("D-Bus initialized");
                Some(d)
            }
            Err(e) => {
                warn!("D-Bus unavailable: {}. Desktop services disabled.", e);
                None
            }
        };
        
        // Initialize desktop services
        let notifications = if let Some(ref dbus) = dbus {
            match dbus::notifications::NotificationService::new(dbus.connection()).await {
                Ok(n) => Some(n),
                Err(e) => {
                    warn!("Notifications unavailable: {}", e);
                    None
                }
            }
        } else {
            None
        };
        
        let power = if let Some(ref dbus) = dbus {
            match dbus::power::PowerService::new(dbus.connection()).await {
                Ok(p) => Some(p),
                Err(e) => {
                    warn!("Power management unavailable: {}", e);
                    None
                }
            }
        } else {
            None
        };
        
        // Show startup notification
        if let Some(ref notif) = notifications {
            let _ = notif.show_simple(
                "Area Started",
                "Window manager and compositor ready"
            ).await;
        }
        
        let mut app = Self {
            conn,
            root,
            wm_windows: HashMap::new(),
            wm,
            compositor,
            shell,
            last_frame: Instant::now(),
            screen_width,
            screen_height,
            _dbus: dbus,
            _notifications: notifications,
            power,
            reparenting_windows: HashSet::new(),
            frame_windows: HashSet::new(),
        };
        
        // Scan for existing windows
        app.scan_existing_windows()?;
        
        Ok(app)
    }
    
    /// Scan for existing windows and manage them
    /// This restores windows that were open before area restarted
    fn scan_existing_windows(&mut self) -> Result<()> {
        let tree = self.conn.query_tree(self.root)?.reply()?;
        
        info!("Scanning {} existing windows for restoration", tree.children.len());
        
        // Collect windows to manage (to avoid borrow checker issues)
        let mut windows_to_manage = Vec::new();
        
        for &window_id in &tree.children {
            // Skip the overlay window
            if window_id == self.compositor.overlay_window {
                continue;
            }
            
            // Get window attributes to check if it's a valid window to manage
            if let Ok(attrs) = self.conn.get_window_attributes(window_id)?.reply() {
                // Skip override-redirect windows (popups, tooltips, etc.)
                if attrs.override_redirect {
                    debug!("Skipping override-redirect window {}", window_id);
                    continue;
                }
                
                // Check if window is mapped or unmapped
                let map_state = attrs.map_state;
                debug!("Found existing window {} (map_state: {:?})", window_id, map_state);
                
                // Manage both mapped and unmapped windows
                // Unmapped windows will be mapped when we manage them
                windows_to_manage.push((window_id, map_state));
            }
        }
        
        info!("Found {} windows to restore", windows_to_manage.len());
        
        // Now manage and restore the windows
        for (window_id, map_state) in windows_to_manage {
            info!("Restoring existing window {} (was {:?})", window_id, map_state);
            if let Err(err) = self.handle_map_request(window_id) {
                warn!("Failed to restore existing window {}: {}", window_id, err);
            } else {
                info!("Successfully restored window {}", window_id);
            }
        }
        
        info!("Window restoration complete");
        Ok(())
    }
    
    /// Main event loop
    async fn run(mut self) -> Result<()> {
        info!("Starting main event loop");
        
        // Use damage-based rendering: only render when windows are damaged
        // This allows idle at 0% CPU when nothing changes
        let mut needs_render = true; // Render once at startup
        
        // Periodic scan for unmanaged windows (every 2 seconds)
        let mut scan_interval = tokio::time::interval(Duration::from_secs(2));
        scan_interval.tick().await; // Skip first immediate tick
        
        // Fallback timer: render at least once per second even if no damage (for animations, etc.)
        let mut fallback_render_interval = tokio::time::interval(Duration::from_secs(1));
        fallback_render_interval.tick().await;
        
        loop {
            tokio::select! {
                // Handle X11 events (blocking wait - this is the main idle point)
                event_result = tokio::task::spawn_blocking({
                    let conn = self.conn.clone();
                    move || conn.wait_for_event()
                }) => {
                    match event_result {
                        Ok(Ok(event)) => {
                            if let Err(e) = self.handle_event(event).await {
                                error!("Error handling event: {}", e);
                            }
                            // After handling event, check if we need to render
                            needs_render = self.compositor.any_damaged();
                        }
                        Ok(Err(e)) => {
                            error!("Error receiving X11 event: {}", e);
                            break;
                        }
                        Err(e) => {
                            error!("Task join error: {}", e);
                            break;
                        }
                    }
                }
                
                // Render when needed (damage-based)
                _ = async {
                    if needs_render {
                        // Small delay to batch multiple damage events
                        tokio::time::sleep(Duration::from_millis(16)).await;
                    } else {
                        // Wait indefinitely until something needs rendering
                        std::future::pending::<()>().await
                    }
                }, if needs_render => {
                    if let Err(e) = self.render_frame() {
                        error!("Error rendering frame: {}", e);
                    }
                    // Clear damage flags after rendering
                    self.compositor.clear_damage();
                    needs_render = false;
                }
                
                // Fallback: render at least once per second (for animations, cursor updates, etc.)
                _ = fallback_render_interval.tick() => {
                    // Only render if there are animations or if we haven't rendered recently
                    if needs_render {
                        if let Err(e) = self.render_frame() {
                            error!("Error rendering frame: {}", e);
                        }
                        // Clear damage flags after rendering
                        self.compositor.clear_damage();
                        needs_render = false;
                    }
                }
                
                // Periodic scan for unmanaged windows
                _ = scan_interval.tick() => {
                    if let Err(e) = self.scan_for_unmanaged_windows() {
                        debug!("Error scanning for unmanaged windows: {}", e);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Scan for windows that exist but aren't being managed
    fn scan_for_unmanaged_windows(&mut self) -> Result<()> {
        let tree = self.conn.query_tree(self.root)?.reply()?;
        
        // Collect window IDs to manage (to avoid borrow checker issues)
        let mut windows_to_manage = Vec::new();
        
        for &window_id in &tree.children {
            // Skip overlay window
            if window_id == self.compositor.overlay_window {
                continue;
            }
            
            // Skip if already managed
            if self.wm_windows.contains_key(&window_id) {
                continue;
            }
            
            // Check if it's a valid window to manage
            if let Ok(attrs) = self.conn.get_window_attributes(window_id)?.reply() {
                // Skip override-redirect windows
                if attrs.override_redirect {
                    continue;
                }
                
                windows_to_manage.push(window_id);
            }
        }
        
        // Now manage the windows
        let mut managed_count = 0;
        let mut failed_count = 0;
        
        for window_id in windows_to_manage {
            debug!("Found unmanaged window {}, attempting to manage", window_id);
            if let Err(err) = self.handle_map_request(window_id) {
                debug!("Failed to manage window {}: {}", window_id, err);
                failed_count += 1;
            } else {
                debug!("Successfully managed previously unmanaged window {}", window_id);
                managed_count += 1;
            }
        }
        
        if managed_count > 0 || failed_count > 0 {
            info!("Window scan complete: {} managed, {} failed", managed_count, failed_count);
        }
        
        Ok(())
    }
    
    /// Handle an X11 event
    async fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::MapRequest(e) => {
                info!("⭐ MapRequest for window {}", e.window);
                self.handle_map_request(e.window)?;
            }
            
            Event::UnmapNotify(e) => {
                // Ignore UnmapNotify events caused by our own reparenting operations
                if self.reparenting_windows.contains(&e.window) {
                    debug!("Ignoring UnmapNotify for window {} (caused by reparenting)", e.window);
                    return Ok(());
                }
                
                // Don't unmanage framed windows on UnmapNotify - they get unmapped during
                // reparenting and other normal operations. Only unmanage on DestroyNotify.
                if let Some(client) = self.wm_windows.get(&e.window) {
                    if client.frame.is_some() {
                        debug!("Ignoring UnmapNotify for framed window {} (will unmanage on DestroyNotify)", e.window);
                        return Ok(());
                    }
                }
                
                debug!("UnmapNotify for window {}", e.window);
                self.handle_unmap(e.window)?;
            }
            
            Event::ConfigureRequest(e) => {
                info!("ConfigureRequest for window {} ({}x{} at {},{}))", 
                    e.window, e.width, e.height, e.x, e.y);
                
                // Grant the configure request
                self.conn.as_ref().configure_window(
                    e.window,
                    &ConfigureWindowAux::new()
                        .x(e.x as i32)
                        .y(e.y as i32)
                        .width(e.width as u32)
                        .height(e.height as u32)
                        .border_width(e.border_width as u32)
                        .sibling(e.sibling)
                        .stack_mode(e.stack_mode),
                )?;
                self.conn.flush()?;
                
                // Update geometry if window is already managed
                if let Some(client) = self.wm_windows.get_mut(&e.window) {
                    if e.width > 10 && e.height > 10 {
                        client.geometry.width = e.width as u32;
                        client.geometry.height = e.height as u32;
                        client.geometry.x = e.x as i32;
                        client.geometry.y = e.y as i32;
                        info!("Updated geometry for managed window {} to {}x{}", e.window, e.width, e.height);
                    }
                } else if !self.wm_windows.contains_key(&e.window) && e.width > 10 && e.height > 10 {
                    // If this window isn't managed yet and has reasonable size, try to manage it
                    info!("Window {} configured with size {}x{}, attempting to manage", e.window, e.width, e.height);
                    if let Err(err) = self.handle_map_request(e.window) {
                        info!("Failed to manage window {}: {}", e.window, err);
                    }
                }
            }
            
            Event::CreateNotify(e) => {
                debug!("CreateNotify for window {}", e.window);
                
                // Skip frame windows created by the WM
                if self.frame_windows.contains(&e.window) {
                    debug!("Skipping CreateNotify for frame window {}", e.window);
                    return Ok(());
                }
                
                // Auto-manage windows on creation if they're not override-redirect
                // This ensures windows get managed even if they don't send MapRequest
                let window_id = e.window;
                if window_id != self.compositor.overlay_window && !self.wm_windows.contains_key(&window_id) {
                    // Check if window is override-redirect
                    let should_manage = match self.conn.get_window_attributes(window_id)?.reply() {
                        Ok(attrs) => !attrs.override_redirect,
                        Err(_) => false,
                    };
                    
                    if should_manage {
                        // Window is not override-redirect and not already managed
                        // Try to manage it - it will be mapped when ready
                        debug!("Auto-managing window {} on CreateNotify", window_id);
                        if let Err(err) = self.handle_map_request(window_id) {
                            debug!("Failed to auto-manage window {}: {}", window_id, err);
                        }
                    }
                }
            }
            
            Event::DestroyNotify(e) => {
                debug!("DestroyNotify for window {}", e.window);
            }
            
            Event::MapNotify(e) => {
                // Skip overlay window MapNotify - it's expected and handled during compositor init
                if e.window == self.compositor.overlay_window {
                    debug!("MapNotify for overlay window {} (ignored)", e.window);
                } else if self.frame_windows.contains(&e.window) {
                    debug!("Skipping MapNotify for frame window {}", e.window);
                } else {
                    // Ignore MapNotify events caused by our own reparenting operations
                    if self.reparenting_windows.remove(&e.window) {
                        debug!("Ignoring MapNotify for window {} (caused by reparenting)", e.window);
                        // Window is already managed, just mark it as mapped
                        if let Some(client) = self.wm_windows.get_mut(&e.window) {
                            client.mapped = true;
                        }
                        return Ok(());
                    }
                    debug!("MapNotify for window {}", e.window);
                    // If window is mapped but not managed, manage it now
                    if !self.wm_windows.contains_key(&e.window) {
                        debug!("Window {} mapped but not managed, managing now", e.window);
                        if let Err(err) = self.handle_map_request(e.window) {
                            debug!("Failed to manage mapped window {}: {}", e.window, err);
                        }
                    } else {
                        // Window is already managed, just mark it as mapped
                        if let Some(client) = self.wm_windows.get_mut(&e.window) {
                            client.mapped = true;
                        }
                    }
                }
            }
            
            Event::ButtonPress(e) => {
                // Check if click is on panel
                if self.shell.panel.contains_point(e.event_x, e.event_y) {
                    if let Err(err) = self.shell.panel.handle_click(e.event_x, e.event_y, &mut self.shell.logout_dialog) {
                        warn!("Error handling panel click: {}", err);
                    }
                    return Ok(());
                }

                debug!("ButtonPress on window {} at ({}, {})", e.event, e.event_x, e.event_y);
                
                // Check if click is on shell elements first
                if let Err(err) = self.shell.handle_click(e.event_x, e.event_y, &self.power).await {
                    warn!("Error handling shell click: {}", err);
                }
                
                // Check if click is on a window decoration button or titlebar
                if let Some((window_id, button_type)) = self.wm.find_window_from_button(&self.wm_windows, e.event) {
                    if let Some(btn_type) = button_type {
                        // Handle button click
                        match btn_type {
                            wm::ButtonType::Close => {
                                if let Err(err) = self.wm.close_window(&self.conn, window_id) {
                                    error!("Failed to close window {}: {}", window_id, err);
                                }
                            }
                            wm::ButtonType::Maximize => {
                                if let Err(err) = self.wm.toggle_maximize(&self.conn, &mut self.wm_windows, window_id) {
                                    error!("Failed to toggle maximize window {}: {}", window_id, err);
                                }
                            }
                            wm::ButtonType::Minimize => {
                                if let Err(err) = self.wm.minimize_window(&self.conn, &mut self.wm_windows, window_id) {
                                    error!("Failed to minimize window {}: {}", window_id, err);
                                }
                            }
                        }
                    } else {
                        // Titlebar click - start drag and focus window
                        if let Err(err) = self.wm.set_focus(&self.conn, &mut self.wm_windows, window_id) {
                            warn!("Failed to focus window {}: {}", window_id, err);
                        }
                        if let Err(err) = self.wm.start_drag(&self.conn, &self.wm_windows, window_id, e.event_x, e.event_y) {
                            warn!("Failed to start drag for window {}: {}", window_id, err);
                        }
                    }
                } else {
                    // Click on client window - focus it
                    if self.wm_windows.contains_key(&e.event) {
                        if let Err(err) = self.wm.set_focus(&self.conn, &mut self.wm_windows, e.event) {
                            warn!("Failed to focus window {}: {}", e.event, err);
                        }
                    }
                }
            }
            
            Event::ButtonRelease(_e) => {
                // End drag/resize
                if let Err(err) = self.wm.end_drag(&self.conn) {
                    debug!("Error ending drag: {}", err);
                }
            }
            
            Event::MotionNotify(e) => {
                // Handle drag
                if self.wm.is_dragging() {
                    if let Err(err) = self.wm.update_drag(&self.conn, &mut self.wm_windows, e.event_x, e.event_y) {
                        debug!("Error updating drag: {}", err);
                    }
                }
            }
            
            Event::Expose(e) => {
                debug!("Expose for window {}", e.window);
                // Mark window as damaged
                if let Some(window) = self.compositor.get_window_mut(e.window) {
                    window.damaged = true;
                }
            }
            
            Event::DamageNotify(e) => {
                // Handle Damage extension events - window content has changed
                debug!("🔴 DamageNotify for drawable {} (damage {}, level {:?})", e.drawable, e.damage, e.level);
                
                // Try to find and mark window as damaged
                let found = if let Some(window) = self.compositor.get_window_mut(e.drawable) {
                    window.damaged = true;
                    debug!("✅ Marked window {} as damaged (damage ID {}, drawable {})", window.id, e.damage, e.drawable);
                    true
                } else {
                    false
                };
                if !found {
                    // Damage events for destroyed windows are expected - downgrade to trace
                    use tracing::trace;
                    trace!("DamageNotify for unknown window (damage {}, drawable {})", e.damage, e.drawable);
                }
            }
            
            Event::ConfigureNotify(e) => {
                // Sync CWindow geometry when window is resized/moved
                if let Some(c_window) = self.compositor.get_window_mut(e.window) {
                    c_window.geometry = shared::Geometry::new(
                        e.x as i32,
                        e.y as i32,
                        e.width as u32,
                        e.height as u32
                    );
                    c_window.border_width = e.border_width;
                    c_window.damaged = true; // Repaint after resize
                }
            }
            
            Event::KeyPress(e) => {
                debug!("KeyPress: detail={}, state={:?}", e.detail, e.state);
                // Check for SUPER key (Mod4) - bit 12 (0x1000)
                // SUPER key alone or with other modifiers
                // Check if Mod4 bit is set (0x1000 = bit 12) or if it's SUPER key (keycode 133/134)
                let mod4_bit = 0x1000u16;
                if (u16::from(e.state) & mod4_bit) != 0 || e.detail == 133 || e.detail == 134 {
                    // Launch navigator on SUPER key press (keycode 133/134 or Mod4 modifier)
                    info!("SUPER key pressed (keycode {}), launching navigator", e.detail);
                    let _ = std::process::Command::new("navigator")
                        .env("DISPLAY", format!("{}", std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into())))
                        .spawn();
                }
            }
            
            Event::ReparentNotify(e) => {
                // We don't need to do anything for reparent events, but we track them
                // to ignore subsequent Map/Unmap events if needed.
                // Just log at trace to avoid spamming "Unhandled event"
                use tracing::trace;
                trace!("ReparentNotify for window {}", e.window);
            }

            Event::Error(e) => {
                // Handle X11 errors - many are expected (e.g., operations on destroyed windows)
                use x11rb::protocol::ErrorKind;
                match e.error_kind {
                    ErrorKind::Window | ErrorKind::Drawable | ErrorKind::Match => {
                        // Expected errors when windows are destroyed - trace level
                        use tracing::trace;
                        trace!("X11 error (expected for destroyed windows): {:?}", e);
                    }
                    ErrorKind::DamageBadDamage => {
                        // Also common during destruction
                        use tracing::trace;
                        trace!("Damage error (expected for destroyed windows): {:?}", e);
                    }
                    _ => {
                        // Unexpected errors - warn level
                        warn!("X11 error: {:?}", e);
                    }
                }
            }
            
            _ => {
                // Log unknown events at debug level
                debug!("Unhandled event: {:?}", event);
            }
        }
        
        Ok(())
    }
    
    /// Handle MapRequest event
    fn handle_map_request(&mut self, window_id: u32) -> Result<()> {
        // Skip if already managed
        if self.wm_windows.contains_key(&window_id) {
            debug!("Window {} already managed, mapping it", window_id);
            // Map the window if it's not already mapped
            if let Some(client) = self.wm_windows.get_mut(&window_id) {
                // If window was minimized, restore it
                if client.state.minimized {
                    client.state.minimized = false;
                    if let Some(frame) = &client.frame {
                        self.conn.map_window(frame.frame)?;
                    } else {
                        self.conn.map_window(window_id)?;
                    }
                } else {
                    self.conn.map_window(window_id)?;
                }
                client.mapped = true;
            }
            self.conn.flush()?;
            return Ok(());
        }
        
        // Create new client with default geometry (will be updated by manage_window)
        let mut client = Client::new(window_id, shared::Geometry::new(0, 0, 100, 100));
        
        // Check if window was already mapped before we took over
        let was_mapped = match self.conn.get_window_attributes(window_id)?.reply() {
            Ok(attrs) => attrs.map_state != x11rb::protocol::xproto::MapState::UNMAPPED,
            Err(_) => {
                debug!("Window {} disappeared before management started", window_id);
                return Ok(());
            }
        };
        
        // Track this window as being reparented to ignore UnmapNotify/MapNotify events
        // caused by our own reparenting operation
        self.reparenting_windows.insert(window_id);
        
        // Let WM manage the window (creates frame, decorations, etc.)
        // This will restore the window's geometry and decorations
        // Note: This will trigger reparent_window, which causes UnmapNotify -> MapNotify
        // We ignore those events because the window is in reparenting_windows
        self.wm.manage_window(&self.conn, &mut client)?;
        
        // Register frame windows to prevent recursive management
        if let Some(frame) = &client.frame {
            self.frame_windows.insert(frame.frame);
            self.frame_windows.insert(frame.titlebar);
            self.frame_windows.insert(frame.close_button);
            self.frame_windows.insert(frame.maximize_button);
            self.frame_windows.insert(frame.minimize_button);
        }
        
        // Map the window so it becomes visible
        // Map frame first (if exists), then client window
        if let Some(frame) = &client.frame {
            // Frame should already be mapped by decorations code, but ensure it's visible
            self.conn.map_window(frame.frame)?;
        }
        // Map the client window (restore it if it was mapped before)
        if was_mapped {
            self.conn.map_window(window_id)?;
            client.mapped = true;
            debug!("Restored and mapped window {} (was previously mapped)", window_id);
        } else {
            // Window wasn't mapped, but map it anyway so user can see it
            self.conn.map_window(window_id)?;
            client.mapped = true;
            debug!("Mapped new window {}", window_id);
        }
        self.conn.flush()?;
        
        // Raise window to ensure it's visible (bring to front)
        use x11rb::protocol::xproto::StackMode;
        if let Some(frame) = &client.frame {
            self.conn.configure_window(
                frame.frame,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        } else {
            self.conn.configure_window(
                window_id,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        }
        self.conn.flush()?;
        
        // Let compositor register the window (creates texture, damage tracking)
        // Determine composite target (FRAME or CLIENT)
        let composite_id = client.frame.as_ref().map(|f| f.frame).unwrap_or(client.id);
        
        // Get actual geometry, border width and viewable state from X11
        // We use *actual* X11 geometry because pixmap size matches the real window size
        let (geometry, border_width, viewable) = {
            let geom_result = self.conn.get_geometry(composite_id)?.reply();
            let attr_result = self.conn.get_window_attributes(composite_id)?.reply();
            
            match (geom_result, attr_result) {
                (Ok(geom), Ok(attr)) => (
                    shared::Geometry::new(geom.x as i32, geom.y as i32, geom.width as u32, geom.height as u32),
                    geom.border_width,
                    attr.map_state == x11rb::protocol::xproto::MapState::VIEWABLE
                ),
                (Ok(geom), Err(_)) => (
                    shared::Geometry::new(geom.x as i32, geom.y as i32, geom.width as u32, geom.height as u32),
                    geom.border_width,
                    was_mapped
                ),
                (Err(_), Ok(attr)) => (
                    client.frame_geometry(), // Fallback to calculated
                    0,
                    attr.map_state == x11rb::protocol::xproto::MapState::VIEWABLE
                ),
                (Err(_), Err(_)) => (client.frame_geometry(), 0, was_mapped),
            }
        };

        // Use actual X11 geometry for the compositor window
        let c_window = CWindow::new(
            composite_id, 
            client.id, 
            geometry, 
            border_width, 
            viewable
        );

        if let Err(e) = self.compositor.add_window(&self.conn, c_window) {
            warn!("Failed to add window {} to compositor: {}. Window will still be managed.", window_id, e);
        }
        
        // Store window
        self.wm_windows.insert(window_id, client);
        
        debug!("Managed and mapped new window {}", window_id);
        Ok(())
    }
    
    /// Handle UnmapNotify event
    fn handle_unmap(&mut self, window_id: u32) -> Result<()> {
        if let Some(mut client) = self.wm_windows.remove(&window_id) {
            // Track this window as being unmanaged (reparented back to root)
            // to ignore MapNotify events caused by the unparenting operation
            self.reparenting_windows.insert(window_id);
            
            // Unregister frame windows
            if let Some(frame) = &client.frame {
                self.frame_windows.remove(&frame.frame);
                self.frame_windows.remove(&frame.titlebar);
                self.frame_windows.remove(&frame.close_button);
                self.frame_windows.remove(&frame.maximize_button);
                self.frame_windows.remove(&frame.minimize_button);
            }
            
            // Let compositor clean up
            self.compositor.remove_window(&self.conn, window_id)?;
            
            // Let WM clean up (this will reparent window back to root)
            self.wm.unmanage_window(&self.conn, &mut client)?;
            
            debug!("Unmanaged window {}", window_id);
        }
        Ok(())
    }
    
    /// Render a frame
    fn render_frame(&mut self) -> Result<()> {
        let now = Instant::now();
        let _delta_time = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        

        
        // Update shell
        self.shell.update();
        
        // Update shell screen size if needed
        self.shell.logout_dialog.set_screen_size(self.screen_width, self.screen_height);
        
        // NOTE: CWindow geometry is synced via ConfigureNotify events
        // The pixmap validation also updates geometry from X11 when binding

        // Render all windows and shell
        self.compositor.render(&self.conn, &self.shell, self.screen_width as f32, self.screen_height as f32)?;
        
        // Log FPS every 60 frames
        static FRAME_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = FRAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count % 60 == 0 {
            debug!("FPS: {:.2}", self.compositor.fps());
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "area=debug,info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    info!("Starting Area Window Manager + Compositor");
    
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let replace = args.iter().any(|arg| arg == "--replace" || arg == "-r");
    
    if replace {
        info!("--replace flag detected: will attempt to replace existing WM");
    }
    
    // Create and run application
    let app = AreaApp::new(replace).await?;
    app.run().await?;
    
    Ok(())
}
