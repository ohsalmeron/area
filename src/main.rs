
//!
//! A high-performance X11 window manager with built-in OpenGL compositor,
//! written in Rust. Inspired by XFWM4's integrated architecture.

mod wm;
mod compositor;
mod shared;
mod shell;
mod dbus;
mod x11_async;
mod config;
mod input;

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, ConfigureWindowAux};
use x11rb::protocol::Event;
use wm::client::Client;
use compositor::c_window::CWindow;

/// Main application state
struct AreaApp {
    /// X11 connection (Arc for sharing across threads)
    conn: Arc<x11rb::rust_connection::RustConnection>,
    
    /// X11 async event stream (non-blocking polling)
    x11_stream: x11_async::X11EventStream,
    
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

    /// Configuration
    config: config::Config,

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
    
    /// Last titlebar click for double-click detection
    last_titlebar_click: Option<(u32, u32, i16, i16)>, // (window_id, time, x, y)
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
        let screen = &conn.as_ref().setup().roots[screen_num];
        let root = screen.root;
        let screen_width = screen.width_in_pixels;
        let screen_height = screen.height_in_pixels;
        
        info!("Connected to X server, screen {}, root window {}", screen_num, root);
        info!("Screen size: {}x{}", screen_width, screen_height);
        
        // Load configuration
        let config = config::Config::load()
            .context("Failed to load configuration")?;
        
        // Initialize input manager and apply mouse configuration
        if let Ok(input_manager) = input::InputManager::new(conn.clone()) {
            if let Err(e) = input_manager.apply_mouse_config(&config.input.mouse) {
                warn!("Failed to apply mouse configuration: {}", e);
            }
        } else {
            warn!("Failed to initialize input manager - input configuration disabled");
        }
        
        // Initialize X11 async event stream (non-blocking polling)
        let x11_stream = x11_async::X11EventStream::new(conn.clone())
            .context("Failed to initialize X11 event stream")?;
        info!("X11 async event stream initialized");
        
        // Initialize window manager
        let wm = wm::WindowManager::new(&conn, screen_num, root, replace)
            .context("Failed to initialize window manager")?;
        
        // Initialize shell
        let shell = shell::Shell::new(screen_width, screen_height, config.panel.clone());
        
        // Initialize compositor (spawns in separate thread)
        let compositor = compositor::Compositor::spawn(conn.clone(), screen_num, root)
            .context("Failed to initialize compositor")?;
        
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
        
        let mut app = Self {
            conn: conn.clone(),
            x11_stream,
            root,
            wm_windows: HashMap::new(),
            wm,
            compositor,
            shell,
            last_frame: Instant::now(),
            screen_width,
            screen_height,
            config,
            _dbus: dbus,
            _notifications: notifications,
            power,
            reparenting_windows: HashSet::new(),
            frame_windows: HashSet::new(),
            last_titlebar_click: None,
        };
        
        // Show startup notification
        if let Some(ref notif) = app._notifications {
            let _ = notif.show_simple(
                "Area Started",
                "Window manager and compositor ready"
            ).await;
        }
        
        // Scan for existing windows
        app.scan_existing_windows()?;
        
        Ok(app)
    }
    
    /// Scan for existing windows and manage them
    /// This restores windows that were open before area restarted
    fn scan_existing_windows(&mut self) -> Result<()> {
        let tree = self.conn.as_ref().query_tree(self.root)?.reply()?;
        
        info!("Scanning {} existing windows for restoration", tree.children.len());
        
        // Collect windows to manage (to avoid borrow checker issues)
        let mut windows_to_manage = Vec::new();
        
        for &window_id in &tree.children {
            // Skip the overlay window
            if window_id == self.compositor.overlay_window {
                continue;
            }
            
            // Get window attributes to check if it's a valid window to manage
            if let Ok(attrs) = self.conn.as_ref().get_window_attributes(window_id)?.reply() {
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
    
    /// Main event loop (LeftWM pattern with event buffering)
    async fn run(mut self) -> Result<()> {
        info!("Starting main event loop");
        info!("Overlay window ID: {}", self.compositor.overlay_window);
        
        // Event buffer for batching events (LeftWM pattern)
        let mut event_buffer: Vec<Event> = Vec::new();
        let mut needs_render = false; // Will be set to true when events require rendering
        let mut should_exit = false; // Flag to signal clean exit when connection is lost
        
        // Periodic scan for unmanaged windows (every 2 seconds)
        let mut scan_interval = tokio::time::interval(Duration::from_secs(2));
        scan_interval.tick().await; // Skip first immediate tick
        
        // Fallback timer: render at least once per second even if no damage (for animations, etc.)
        let mut fallback_render_interval = tokio::time::interval(Duration::from_secs(1));
        fallback_render_interval.tick().await;
        
        // Performance monitoring: log FPS and frame timing every 5 seconds
        let mut perf_log_interval = tokio::time::interval(Duration::from_secs(5));
        perf_log_interval.tick().await;
        
        // Trigger initial render (compositor handles rendering in its own thread)
        self.compositor.trigger_render();
        
        loop {
            // Check exit flag
            if should_exit {
                info!("Exiting main loop");
                return Ok(());
            }
            
            // Flush X11 requests at start of loop (LeftWM pattern - batch optimization)
            if let Err(e) = self.x11_stream.flush() {
                // Check if connection is broken - if so, exit cleanly
                let error_str = e.to_string();
                if error_str.contains("Broken pipe") || error_str.contains("Connection reset") {
                    info!("X11 connection lost, exiting cleanly");
                    should_exit = true;
                    continue;
                }
                warn!("Failed to flush X11 requests: {}", e);
            }
            
            // Process buffered events first if available (LeftWM pattern)
            if !event_buffer.is_empty() {
                self.execute_events(&mut event_buffer, &mut needs_render).await;
                continue;
            }
            
            tokio::select! {
                // Wait for X11 events (only when buffer is empty)
                () = self.x11_stream.wait_readable() => {
                    // Collect all pending events (non-blocking loop)
                    loop {
                        match self.x11_stream.poll_next_event() {
                            Ok(Some(event)) => event_buffer.push(event),
                            Ok(None) => break,
                            Err(e) => {
                                // Check if connection is broken
                                let error_str = e.to_string();
                                if error_str.contains("Broken pipe") || error_str.contains("Connection reset") {
                                    error!("X11 connection lost, exiting cleanly");
                                    should_exit = true;
                                    break;
                                }
                                error!("Error polling for X11 events: {}", e);
                                break;
                            }
                        }
                    }
                    // Process events in next iteration
                }
                
                // Render when needed (damage-based, but immediate for cursor)
                _ = async {
                    if needs_render {
                        // Small delay to batch multiple damage events
                        tokio::time::sleep(Duration::from_millis(16)).await;
                    } else {
                        // Wait indefinitely until something needs rendering
                        std::future::pending::<()>().await
                    }
                }, if needs_render => {
                    // Trigger render in compositor thread
                    self.compositor.trigger_render();
                    needs_render = false;
                }
                
                // Fallback: render at least once per second (for animations, cursor updates, etc.)
                _ = fallback_render_interval.tick() => {
                    // Only render if there are animations or if we haven't rendered recently
                    if needs_render {
                        self.compositor.trigger_render();
                        needs_render = false;
                    }
                }
                
                // Performance monitoring: log FPS and frame timing
                _ = perf_log_interval.tick() => {
                    let now = Instant::now();
                    let frame_delta = now.duration_since(self.last_frame);
                    self.last_frame = now;
                    
                    // Log performance metrics (FPS from compositor, frame timing)
                    if frame_delta.as_secs_f64() > 0.0 {
                        let avg_fps = 1.0 / frame_delta.as_secs_f64();
                        debug!("Performance: avg_frame_time={:.2}ms, compositor_fps={:.1}", 
                            frame_delta.as_secs_f64() * 1000.0, avg_fps);
                    }
                }
                
                // Periodic scan for unmanaged windows
                _ = scan_interval.tick() => {
                    if let Err(e) = self.scan_for_unmanaged_windows() {
                        // Check if connection is broken - if so, exit cleanly
                        let error_str = e.to_string();
                        if error_str.contains("Broken pipe") || error_str.contains("Connection reset") {
                            info!("X11 connection lost during window scan, exiting cleanly");
                            should_exit = true;
                        } else {
                            debug!("Error scanning for unmanaged windows: {}", e);
                        }
                    }
                }
            };
        }
    }
    
    /// Execute buffered events (LeftWM drain pattern)
    async fn execute_events(&mut self, event_buffer: &mut Vec<Event>, needs_render: &mut bool) {
        // Process all buffered events at once (LeftWM drain pattern)
        // Note: We process events sequentially to maintain order and state consistency
        for event in event_buffer.drain(..) {
            if let Err(e) = self.handle_event(event).await {
                error!("Error handling event: {}", e);
            }
            // Mark that we need to render (compositor will check damage internally)
            // Note: needs_render is set to true here, but we also check compositor damage
            // The compositor thread handles its own rendering, so we just trigger it
            *needs_render = true;
        }
    }
    
    /// Scan for windows that exist but aren't being managed
    fn scan_for_unmanaged_windows(&mut self) -> Result<()> {
        let tree = self.conn.as_ref().query_tree(self.root)?.reply()?;
        
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
            if let Ok(attrs) = self.conn.as_ref().get_window_attributes(window_id)?.reply() {
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
        // Check for screen size changes (detect via root window geometry)
        let current_screen = &self.conn.as_ref().setup().roots[0];
        let current_width = current_screen.width_in_pixels;
        let current_height = current_screen.height_in_pixels;
        if current_width != self.screen_width || current_height != self.screen_height {
            info!("Screen size changed: {}x{} -> {}x{}", 
                self.screen_width, self.screen_height, current_width, current_height);
            self.screen_width = current_width;
            self.screen_height = current_height;
            // Update shell with new screen size
            self.shell.set_screen_size(current_width, current_height);
        }
        
        match event {
            Event::MapRequest(e) => {
                info!("⭐ MapRequest for window {}", e.window);
                self.handle_map_request(e.window)?;
            }
            
            Event::UnmapNotify(e) => {
                // Ignore UnmapNotify events caused by our own reparenting operations
                if self.reparenting_windows.contains(&e.window) {
                    return Ok(());
                }
                
                // Don't unmanage framed windows on UnmapNotify - they get unmapped during
                // reparenting and other normal operations. Only unmanage on DestroyNotify.
                if let Some(client) = self.wm_windows.get(&e.window) {
                    if client.frame.is_some() {
                        return Ok(());
                    }
                }
                
                self.handle_unmap(e.window)?;
            }
            
            Event::ConfigureRequest(e) => {
                info!("ConfigureRequest for window {} ({}x{} at {},{}))", 
                    e.window, e.width, e.height, e.x, e.y);
                
                // Find the client window (could be direct or via frame)
                let client_id = if let Some(_) = self.wm_windows.get(&e.window) {
                    Some(e.window)
                } else {
                    self.wm.find_client_from_window(&self.wm_windows, e.window)
                };
                
                // Check if this is a fullscreen-size request (games often request screen size)
                if let Some(cid) = client_id {
                    if let Some(client) = self.wm_windows.get_mut(&cid) {
                        let screen_width = self.screen_width as u32;
                        let screen_height = self.screen_height as u32;
                        
                        // Check if requested size is close to screen size (within 20px tolerance)
                        // Games might request slightly less than screen size
                        let is_screen_size = (e.width as u32) >= screen_width.saturating_sub(20) 
                                          && (e.width as u32) <= screen_width + 20
                                          && (e.height as u32) >= screen_height.saturating_sub(20)
                                          && (e.height as u32) <= screen_height + 20
                                          && e.x <= 20 && e.y <= 20;
                        
                        // If window requests fullscreen size and has bypass_compositor, force fullscreen
                        // This handles games that resize to fullscreen without setting EWMH state first
                        if is_screen_size && !client.state.fullscreen {
                            if let Ok(bypass) = self.wm.atoms.check_bypass_compositor(&self.conn, cid) {
                                if bypass {
                                    debug!("ConfigureRequest: Window {} requests fullscreen size with bypass_compositor, setting fullscreen", cid);
                                    if let Err(err) = self.wm.set_fullscreen(&self.conn, client, true) {
                                        warn!("Failed to set fullscreen for window {} (ConfigureRequest detection): {}", cid, err);
                                    } else {
                                        // If window has a frame, add client window to compositor (frame is unmapped)
                                        if client.frame.is_some() {
                                            // Add client window to compositor for fullscreen rendering
                                            let client_geom = client.geometry;
                                            let c_window = crate::compositor::c_window::CWindow::new(
                                                cid,  // composite_id = client window
                                                cid,  // client_id = client window
                                                client_geom,
                                                0,  // border_width = 0 for fullscreen
                                                true,  // viewable = true (client is mapped)
                                            );
                                            self.compositor.add_window(c_window);
                                        }
                                        // Coordinate with compositor: unredirect if config allows
                                        // Use client window directly for fullscreen (frame is hidden)
                                        if self.config.compositor.unredirect_fullscreen {
                                            self.compositor.unredirect_window(cid);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
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
                self.conn.as_ref().flush()?;
                
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
                    let should_manage = match self.conn.as_ref().get_window_attributes(window_id)?.reply() {
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
                if let Err(err) = self.handle_destroy(e.window) {
                    warn!("Error handling DestroyNotify for window {}: {}", e.window, err);
                }
            }
            
            Event::ClientMessage(e) => {
                // Handle _NET_CLOSE_WINDOW (EWMH close request)
                if let Ok(net_close_atom) = self.conn.as_ref().intern_atom(false, b"_NET_CLOSE_WINDOW")?.reply() {
                    if e.type_ == net_close_atom.atom && e.format == 32 {
                        debug!("ClientMessage: _NET_CLOSE_WINDOW for window {}", e.window);
                        // Find the client window (could be the window itself or its frame)
                        let client_id = self.wm.find_client_from_window(&self.wm_windows, e.window);
                        if let Some(client_id) = client_id {
                            if let Err(err) = self.wm.close_window(&self.conn, client_id) {
                                warn!("Failed to close window {} via _NET_CLOSE_WINDOW: {}", client_id, err);
                            }
                        } else {
                            debug!("_NET_CLOSE_WINDOW for unmanaged window {}", e.window);
                        }
                        return Ok(());
                    }
                }
                
                // Handle _NET_WM_STATE (EWMH state change requests)
                // EWMH spec: action = 0 (REMOVE), 1 (ADD), 2 (TOGGLE)
                if e.type_ == self.wm.atoms.net_wm_state && e.format == 32 {
                    debug!("ClientMessage: _NET_WM_STATE for window {}", e.window);
                    // Find the client window
                    let client_id = self.wm.find_client_from_window(&self.wm_windows, e.window);
                    if let Some(client_id) = client_id {
                        let data32 = e.data.as_data32();
                        let action = data32[0]; // 0=REMOVE, 1=ADD, 2=TOGGLE (EWMH spec)
                        let first_atom = data32[1];
                        let second_atom = data32[2];
                        
                        // Clone atom values to avoid borrow checker issues
                        let net_wm_state_fullscreen = self.wm.atoms._net_wm_state_fullscreen;
                        let net_wm_state_maximized_vert = self.wm.atoms._net_wm_state_maximized_vert;
                        let net_wm_state_maximized_horz = self.wm.atoms._net_wm_state_maximized_horz;
                        let net_wm_state_hidden = self.wm.atoms._net_wm_state_hidden;
                        let net_wm_state_above = self.wm.atoms._net_wm_state_above;
                        let net_wm_state_below = self.wm.atoms._net_wm_state_below;
                        let net_wm_state_shaded = self.wm.atoms._net_wm_state_shaded;
                        let net_wm_state_sticky = self.wm.atoms._net_wm_state_sticky;
                        let net_wm_state_modal = self.wm.atoms._net_wm_state_modal;
                        let net_wm_state_skip_pager = self.wm.atoms._net_wm_state_skip_pager;
                        let net_wm_state_skip_taskbar = self.wm.atoms._net_wm_state_skip_taskbar;
                        let net_wm_state_demands_attention = self.wm.atoms._net_wm_state_demands_attention;
                        let net_wm_state_atom = self.wm.atoms.net_wm_state;
                        
                        let mut state_changed = false;
                        
                        // Helper to determine if we should apply a state change
                        let should_apply = |current: bool, action: u32| -> bool {
                            match action {
                                0 => current,      // REMOVE: only if currently set
                                1 => !current,    // ADD: only if not currently set
                                2 => true,        // TOGGLE: always apply
                                _ => false,
                            }
                        };
                        
                        // Handle FULLSCREEN (mutually exclusive with MAXIMIZED)
                        if first_atom == net_wm_state_fullscreen || second_atom == net_wm_state_fullscreen {
                            debug!("_NET_WM_STATE FULLSCREEN requested for window {} (action={}, current={})", 
                                   client_id, action, 
                                   self.wm_windows.get(&client_id).map(|c| c.state.fullscreen).unwrap_or(false));
                            if let Some(client) = self.wm_windows.get(&client_id) {
                                let current = client.state.fullscreen;
                                let should_change = should_apply(current, action);
                                
                                if should_change {
                                    debug!("Setting fullscreen={} for window {}", !current, client_id);
                                    if let Some(client) = self.wm_windows.get_mut(&client_id) {
                                        if let Err(err) = self.wm.set_fullscreen(&self.conn, client, !current) {
                                            warn!("Failed to set fullscreen for window {}: {}", client_id, err);
                                        } else {
                                            debug!("Successfully set fullscreen={} for window {}", !current, client_id);
                                            state_changed = true;
                                            // Coordinate with compositor: unredirect/redirect based on fullscreen state
                                            // Use client window directly for fullscreen (frame is hidden)
                                            if !current {
                                                // Entering fullscreen
                                                // If window has a frame, add client window to compositor (frame is unmapped)
                                                if client.frame.is_some() {
                                                    // Add client window to compositor for fullscreen rendering
                                                    let client_geom = client.geometry;
                                                    let c_window = crate::compositor::c_window::CWindow::new(
                                                        client_id,  // composite_id = client window
                                                        client_id,  // client_id = client window
                                                        client_geom,
                                                        0,  // border_width = 0 for fullscreen
                                                        true,  // viewable = true (client is mapped)
                                                    );
                                                    self.compositor.add_window(c_window);
                                                }
                                                // Unredirect if config allows
                                                if self.config.compositor.unredirect_fullscreen {
                                                    self.compositor.unredirect_window(client_id);
                                                }
                                            } else {
                                                // Exiting fullscreen - redirect back and remove client window
                                                if self.config.compositor.unredirect_fullscreen {
                                                    self.compositor.redirect_window(client_id);
                                                }
                                                // Remove client window from compositor (frame is mapped back, compositor will track frame again)
                                                self.compositor.remove_window(client_id);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Handle MAXIMIZE (mutually exclusive with FULLSCREEN)
                        let handle_maximize = (first_atom == net_wm_state_maximized_vert || second_atom == net_wm_state_maximized_vert) ||
                                             (first_atom == net_wm_state_maximized_horz || second_atom == net_wm_state_maximized_horz);
                        if handle_maximize {
                            if let Some(client) = self.wm_windows.get(&client_id) {
                                let current = client.state.maximized;
                                let should_change = should_apply(current, action);
                                
                                if should_change {
                                    if let Some(client) = self.wm_windows.get_mut(&client_id) {
                                        if current {
                                            if let Err(err) = self.wm.restore_window(&self.conn, client) {
                                                warn!("Failed to restore window {}: {}", client_id, err);
                                            } else {
                                                state_changed = true;
                                            }
                                        } else {
                                            if let Err(err) = self.wm.maximize_window(&self.conn, client) {
                                                warn!("Failed to maximize window {}: {}", client_id, err);
                                            } else {
                                                state_changed = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Handle HIDDEN (minimize)
                        if first_atom == net_wm_state_hidden || second_atom == net_wm_state_hidden {
                            if let Some(client) = self.wm_windows.get(&client_id) {
                                let current = !client.mapped;
                                let should_change = should_apply(current, action);
                                
                                if should_change {
                                    if current {
                                        // Unminimize (restore)
                                        if let Some(client) = self.wm_windows.get_mut(&client_id) {
                                            if let Err(err) = self.wm.restore_window(&self.conn, client) {
                                                warn!("Failed to restore window {}: {}", client_id, err);
                                            } else {
                                                // Map the window
                                                if let Some(frame) = &client.frame {
                                                    self.conn.as_ref().map_window(frame.frame)?;
                                                } else {
                                                    self.conn.as_ref().map_window(client_id)?;
                                                }
                                                client.mapped = true;
                                                client.state.minimized = false;
                                                self.conn.as_ref().flush()?;
                                                state_changed = true;
                                            }
                                        }
                                    } else {
                                        // Minimize
                                        if let Err(err) = self.wm.minimize_window(&self.conn, &mut self.wm_windows, client_id) {
                                            warn!("Failed to minimize window {}: {}", client_id, err);
                                        } else {
                                            state_changed = true;
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Handle ABOVE (mutually exclusive with BELOW)
                        if first_atom == net_wm_state_above || second_atom == net_wm_state_above {
                            if let Some(client) = self.wm_windows.get_mut(&client_id) {
                                let current = client.state.above;
                                let should_change = should_apply(current, action);
                                
                                if should_change {
                                    // Remove BELOW if setting ABOVE
                                    if !current && client.state.below {
                                        client.state.below = false;
                                        self.wm.atoms.set_window_state(
                                            &self.conn,
                                            client_id,
                                            &[],
                                            &[net_wm_state_below],
                                        )?;
                                    }
                                    
                                    client.state.above = !current;
                                    let (add_atoms, remove_atoms) = if !current {
                                        (&[net_wm_state_above] as &[u32], &[] as &[u32])
                                    } else {
                                        (&[] as &[u32], &[net_wm_state_above] as &[u32])
                                    };
                                    self.wm.atoms.set_window_state(
                                        &self.conn,
                                        client_id,
                                        add_atoms,
                                        remove_atoms,
                                    )?;
                                    self.conn.as_ref().flush()?;
                                    state_changed = true;
                                }
                            }
                        }
                        
                        // Handle BELOW (mutually exclusive with ABOVE)
                        if first_atom == net_wm_state_below || second_atom == net_wm_state_below {
                            if let Some(client) = self.wm_windows.get_mut(&client_id) {
                                let current = client.state.below;
                                let should_change = should_apply(current, action);
                                
                                if should_change {
                                    // Remove ABOVE if setting BELOW
                                    if !current && client.state.above {
                                        client.state.above = false;
                                        self.wm.atoms.set_window_state(
                                            &self.conn,
                                            client_id,
                                            &[],
                                            &[net_wm_state_above],
                                        )?;
                                    }
                                    
                                    client.state.below = !current;
                                    let (add_atoms, remove_atoms) = if !current {
                                        (&[net_wm_state_below] as &[u32], &[] as &[u32])
                                    } else {
                                        (&[] as &[u32], &[net_wm_state_below] as &[u32])
                                    };
                                    self.wm.atoms.set_window_state(
                                        &self.conn,
                                        client_id,
                                        add_atoms,
                                        remove_atoms,
                                    )?;
                                    self.conn.as_ref().flush()?;
                                    state_changed = true;
                                }
                            }
                        }
                        
                        // Handle other states (SHADED, STICKY, MODAL, SKIP_PAGER, SKIP_TASKBAR, DEMANDS_ATTENTION)
                        // These are property-only states (no visual changes needed yet)
                        let property_only_states = [
                            (net_wm_state_shaded, "shaded"),
                            (net_wm_state_sticky, "sticky"),
                            (net_wm_state_modal, "modal"),
                            (net_wm_state_skip_pager, "skip_pager"),
                            (net_wm_state_skip_taskbar, "skip_taskbar"),
                            (net_wm_state_demands_attention, "demands_attention"),
                        ];
                        
                        for (atom, state_name) in property_only_states.iter() {
                            if first_atom == *atom || second_atom == *atom {
                                if let Some(_client) = self.wm_windows.get_mut(&client_id) {
                                    // Get current state from property
                                    let mut current = false;
                                    if let Ok(reply) = self.conn.as_ref().get_property(
                                        false,
                                        client_id,
                                        net_wm_state_atom,
                                        AtomEnum::ATOM,
                                        0,
                                        1024,
                                    )?.reply() {
                                        if let Some(mut value32) = reply.value32() {
                                            current = value32.any(|a| a == *atom);
                                        }
                                    }
                                    
                                    let should_change = should_apply(current, action);
                                    if should_change {
                                        let (add_atoms, remove_atoms) = if !current {
                                            (&[*atom] as &[u32], &[] as &[u32])
                                        } else {
                                            (&[] as &[u32], &[*atom] as &[u32])
                                        };
                                        self.wm.atoms.set_window_state(
                                            &self.conn,
                                            client_id,
                                            add_atoms,
                                            remove_atoms,
                                        )?;
                                        self.conn.as_ref().flush()?;
                                        debug!("Updated {} state for window {} to {}", state_name, client_id, !current);
                                        state_changed = true;
                                    }
                                }
                            }
                        }
                        
                        if !state_changed {
                            debug!("_NET_WM_STATE action {} for window {} resulted in no change", action, client_id);
                        }
                    } else {
                        debug!("_NET_WM_STATE for unmanaged window {}", e.window);
                    }
                    return Ok(());
                }
                
                // Handle _NET_ACTIVE_WINDOW (EWMH focus request)
                if let Ok(net_active_atom) = self.conn.as_ref().intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply() {
                    if e.type_ == net_active_atom.atom && e.format == 32 {
                        debug!("ClientMessage: _NET_ACTIVE_WINDOW for window {}", e.window);
                        let data32 = e.data.as_data32();
                        let _source_indication = data32[0]; // 0=application, 1=pager, 2=wm
                        let _timestamp = data32[1]; // timestamp or 0
                        
                        // Find the client window
                        let client_id = self.wm.find_client_from_window(&self.wm_windows, e.window);
                        if let Some(client_id) = client_id {
                            // Focus the window
                            if let Err(err) = self.wm.set_focus(&self.conn, &mut self.wm_windows, client_id) {
                                warn!("Failed to focus window {} via _NET_ACTIVE_WINDOW: {}", client_id, err);
                            }
                        } else {
                            debug!("_NET_ACTIVE_WINDOW for unmanaged window {}", e.window);
                        }
                        return Ok(());
                    }
                }
                
                // Handle _NET_REQUEST_FRAME_EXTENTS (EWMH frame extents request)
                if let Ok(net_frame_extents_atom) = self.conn.as_ref().intern_atom(false, b"_NET_REQUEST_FRAME_EXTENTS")?.reply() {
                    if e.type_ == net_frame_extents_atom.atom {
                        debug!("ClientMessage: _NET_REQUEST_FRAME_EXTENTS for window {}", e.window);
                        // Find the client window
                        let client_id = self.wm.find_client_from_window(&self.wm_windows, e.window);
                        if let Some(client_id) = client_id {
                            if let Some(client) = self.wm_windows.get(&client_id) {
                                // If window has a frame, send frame extents
                                if client.frame.is_some() {
                                    // Top: 32 (Titlebar), Left/Right/Bottom: 2 (Border)
                                    if let Err(err) = self.wm.atoms.update_frame_extents(&self.conn, client_id, 2, 2, 32, 2) {
                                        warn!("Failed to update frame extents for window {}: {}", client_id, err);
                                    }
                                }
                            }
                        } else {
                            // Window not yet managed - use default frame extents
                            // Top: 32 (Titlebar), Left/Right/Bottom: 2 (Border)
                            if let Ok(atoms) = crate::wm::ewmh::Atoms::new(self.conn.as_ref()) {
                                if let Err(err) = atoms.update_frame_extents(&self.conn, e.window, 2, 2, 32, 2) {
                                    debug!("Failed to set default frame extents for window {}: {}", e.window, err);
                                }
                            }
                        }
                        return Ok(());
                    }
                }
                
                // Handle WM_DELETE_WINDOW protocol responses
                // When a window receives WM_DELETE_WINDOW and doesn't respond, we might get a ClientMessage
                let wm_protocols_atom = self.conn.as_ref().intern_atom(false, b"WM_PROTOCOLS")?.reply();
                let wm_delete_atom = self.conn.as_ref().intern_atom(false, b"WM_DELETE_WINDOW")?.reply();
                
                if let (Ok(wm_protocols), Ok(wm_delete)) = (wm_protocols_atom, wm_delete_atom) {
                    if e.type_ == wm_protocols.atom {
                        // as_data32() returns [u32; 5] directly, not Option
                        let data32 = e.data.as_data32();
                        if data32[0] == wm_delete.atom {
                            debug!("ClientMessage: WM_DELETE_WINDOW response for window {}", e.window);
                            // Window is closing - handle destroy
                            if let Err(err) = self.handle_destroy(e.window) {
                                warn!("Error handling destroy for window {}: {}", e.window, err);
                            }
                        }
                    }
                }
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
                // Check if click is on panel (using root coordinates)
                if self.shell.panel.contains_point(e.root_x, e.root_y) {
                    if let Err(err) = self.shell.panel.handle_click(e.root_x, e.root_y, &mut self.shell.logout_dialog) {
                        warn!("Error handling panel click: {}", err);
                    }
                    return Ok(());
                }

                debug!("ButtonPress on window {} at ({}, {})", e.event, e.event_x, e.event_y);
                
                // Check if click is on shell elements first
                if let Err(err) = self.shell.handle_click(e.event_x, e.event_y, &self.power).await {
                    warn!("Error handling shell click: {}", err);
                }
                
                // Find the client window from any window ID (client, frame, titlebar, buttons)
                let client_id = self.wm.find_client_from_window(&self.wm_windows, e.event);
                
                if let Some(client_id) = client_id {
                    // Check if click is on a button
                    if let Some((_window_id, button_type)) = self.wm.find_window_from_button(&self.wm_windows, e.event) {
                        if button_type.is_some() {
                            // Button clicks are handled on ButtonRelease
                            return Ok(());
                        }
                    }
                    
                    // Not a button - could be titlebar or client window
                    if let Some(client) = self.wm_windows.get(&client_id) {
                        let is_titlebar_click = if let Some(frame) = &client.frame {
                            // Check if click is on titlebar window OR frame window in titlebar area
                            if e.event == frame.titlebar {
                                true
                            } else if e.event == frame.frame {
                                // Click on frame window - check if coordinates are in titlebar area
                                // event_x/event_y are relative to the event window (frame)
                                // Titlebar is at y=0 to y=titlebar_height
                                let titlebar_height = self.config.window_manager.decorations.titlebar_height as i16;
                                e.event_y < titlebar_height
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        
                        // Focus the window
                        if let Err(err) = self.wm.set_focus(&self.conn, &mut self.wm_windows, client_id) {
                            warn!("Failed to focus window {}: {}", client_id, err);
                        }
                        
                        // Handle titlebar clicks with Button1
                        if is_titlebar_click && e.detail == 1 {
                            // Check for double-click (within 300ms and 6 pixels)
                            const DOUBLE_CLICK_TIME_MS: u32 = 300;
                            const DOUBLE_CLICK_DISTANCE: i16 = 6;
                            
                            let is_double_click = if let Some((last_window, last_time, last_x, last_y)) = self.last_titlebar_click {
                                last_window == client_id
                                    && e.time < last_time + DOUBLE_CLICK_TIME_MS
                                    && (e.event_x - last_x).abs() < DOUBLE_CLICK_DISTANCE
                                    && (e.event_y - last_y).abs() < DOUBLE_CLICK_DISTANCE
                            } else {
                                false
                            };
                            
                            if is_double_click {
                                // Double-click detected - toggle maximize
                                debug!("Double-click on titlebar for window {} - toggling maximize", client_id);
                                if let Err(err) = self.wm.toggle_maximize(&self.conn, &mut self.wm_windows, client_id) {
                                    warn!("Failed to toggle maximize window {}: {}", client_id, err);
                                }
                                // Reset double-click tracking
                                self.last_titlebar_click = None;
                            } else {
                                // Single click - start drag and track for potential double-click
                                // Get root coordinates for the click
                                if let Ok(pointer) = self.conn.as_ref().query_pointer(self.root)?.reply() {
                                    if let Err(err) = self.wm.start_drag(&self.conn, &self.wm_windows, client_id, pointer.root_x, pointer.root_y) {
                                        warn!("Failed to start drag for window {}: {}", client_id, err);
                                    }
                                }
                                // Track this click for double-click detection
                                self.last_titlebar_click = Some((client_id, e.time, e.event_x, e.event_y));
                            }
                        }
                    }
                }
            }
            
            Event::ButtonRelease(e) => {
                // Handle button clicks on release
                // Check if this is a button window first
                if let Some((window_id, button_type)) = self.wm.find_window_from_button(&self.wm_windows, e.event) {
                    if let Some(btn_type) = button_type {
                        // Handle button click on release
                        match btn_type {
                            wm::ButtonType::Close => {
                                debug!("Close button clicked for window {}", window_id);
                                if let Err(err) = self.wm.close_window(&self.conn, window_id) {
                                    warn!("Failed to close window {}: {}", window_id, err);
                                }
                            }
                            wm::ButtonType::Maximize => {
                                debug!("Maximize button clicked for window {}", window_id);
                                if let Err(err) = self.wm.toggle_maximize(&self.conn, &mut self.wm_windows, window_id) {
                                    warn!("Failed to toggle maximize window {}: {}", window_id, err);
                                }
                            }
                            wm::ButtonType::Minimize => {
                                debug!("Minimize button clicked for window {}", window_id);
                                if let Err(err) = self.wm.minimize_window(&self.conn, &mut self.wm_windows, window_id) {
                                    warn!("Failed to minimize window {}: {}", window_id, err);
                                }
                            }
                        }
                        // Don't end drag if we handled a button click
                        return Ok(());
                    }
                }
                
                // End drag/resize
                if let Err(err) = self.wm.end_drag(&self.conn) {
                    debug!("Error ending drag: {}", err);
                }
            }
            
            Event::MotionNotify(e) => {
                // Update cursor position in compositor
                self.compositor.update_cursor(e.root_x, e.root_y, true);
                
                // Handle drag - use root coordinates for proper dragging
                if self.wm.is_dragging() {
                    if let Err(err) = self.wm.update_drag(&self.conn, &mut self.wm_windows, e.root_x, e.root_y) {
                        debug!("Error updating drag: {}", err);
                    }
                }
            }
            
            Event::Expose(e) => {
                debug!("Expose for window {}", e.window);
                // Mark window as damaged
                // Mark window as damaged in the compositor
                self.compositor.update_window_damage(e.window);
            }
            
            Event::DamageNotify(e) => {
                // If this is a managed client window with a frame, inform compositor about frame damage
                let target_id = if let Some(client) = self.wm_windows.get(&e.drawable) {
                    client.frame.as_ref().map(|f| f.frame).unwrap_or(e.drawable)
                } else {
                    e.drawable
                };
                self.compositor.update_window_damage(target_id);
            }
            
            Event::ConfigureNotify(e) => {
                // Find the client window - could be e.window directly or via frame
                let client_id = if let Some(_) = self.wm_windows.get(&e.window) {
                    // This is the client window
                    Some(e.window)
                } else {
                    // Might be a frame window - find the client
                    self.wm.find_client_from_window(&self.wm_windows, e.window)
                };
                
                // If this is a managed client window with a frame, and e.window is the client,
                // ignore its ConfigureNotify because it's in relative coordinates.
                // Frame's ConfigureNotify will update geometry.
                if let Some(cid) = client_id {
                    if cid == e.window {
                        if let Some(client) = self.wm_windows.get(&cid) {
                            if client.frame.is_some() {
                                // Client ConfigureNotify with frame - ignore (coordinates are relative to frame)
                                return Ok(());
                            }
                        }
                    }
                }

                // Sync CWindow geometry when window is resized/moved
                let geom = shared::Geometry::new(
                    e.x as i32,
                    e.y as i32,
                    e.width as u32,
                    e.height as u32
                );
                self.compositor.update_window_geometry(e.window, geom);
                
                // Geometry-based fullscreen detection: if window/frame resizes to screen size, trigger fullscreen
                // This handles games that resize first, then set EWMH property
                if let Some(cid) = client_id {
                    if let Some(client) = self.wm_windows.get_mut(&cid) {
                        // Check if window/frame geometry matches screen size (within 20px tolerance)
                        let screen_width = self.screen_width as u32;
                        let screen_height = self.screen_height as u32;
                        let is_screen_size = e.width >= (screen_width as u16).saturating_sub(20) 
                                          && e.width <= (screen_width as u16) + 20
                                          && e.height >= (screen_height as u16).saturating_sub(20)
                                          && e.height <= (screen_height as u16) + 20
                                          && e.x <= 20 && e.y <= 20;
                        
                        // Only auto-detect if not already fullscreen
                        if is_screen_size && !client.state.fullscreen {
                            // Check if window has bypass_compositor (indicates game wants fullscreen)
                            let should_fullscreen = if let Ok(bypass) = self.wm.atoms.check_bypass_compositor(&self.conn, cid) {
                                bypass // If bypass is set, definitely fullscreen
                            } else {
                                true // Otherwise, still check (might be fullscreen request)
                            };
                            
                            if should_fullscreen {
                                debug!("Geometry-based fullscreen detection: window {} resized to screen size, setting fullscreen", cid);
                                if let Err(err) = self.wm.set_fullscreen(&self.conn, client, true) {
                                    warn!("Failed to set fullscreen for window {} (geometry-based detection): {}", cid, err);
                                } else {
                                    // If window has a frame, add client window to compositor (frame is unmapped)
                                    if client.frame.is_some() {
                                        // Add client window to compositor for fullscreen rendering
                                        let client_geom = client.geometry;
                                        let c_window = crate::compositor::c_window::CWindow::new(
                                            cid,  // composite_id = client window
                                            cid,  // client_id = client window
                                            client_geom,
                                            0,  // border_width = 0 for fullscreen
                                            true,  // viewable = true (client is mapped)
                                        );
                                        self.compositor.add_window(c_window);
                                    }
                                    // Coordinate with compositor: unredirect if config allows
                                    // Use client window directly for fullscreen (frame is hidden)
                                    if self.config.compositor.unredirect_fullscreen {
                                        self.compositor.unredirect_window(cid);
                                    }
                                }
                            }
                        } else if !is_screen_size && client.state.fullscreen {
                            // Window is no longer screen size but is marked fullscreen - exit fullscreen
                            // (This handles cases where games resize out of fullscreen before clearing EWMH state)
                            debug!("Geometry-based fullscreen detection: window {} no longer screen size, exiting fullscreen", cid);
                            if let Err(err) = self.wm.set_fullscreen(&self.conn, client, false) {
                                warn!("Failed to exit fullscreen for window {} (geometry-based detection): {}", cid, err);
                            } else {
                                // Coordinate with compositor: redirect back and remove client window
                                if self.config.compositor.unredirect_fullscreen {
                                    self.compositor.redirect_window(cid);
                                }
                                // Remove client window from compositor (frame is mapped back, compositor will track frame again)
                                self.compositor.remove_window(cid);
                            }
                        }
                    }
                }
            }
            
            Event::KeyPress(e) => {
                debug!("KeyPress: detail={}, state={:?}", e.detail, e.state);
                // Check for launcher key from config
                // For now, support keycode-based matching (133/134 for SUPER keys)
                // TODO: Add full keybinding parser for key names like "Super"
                let launcher_keycodes: Vec<u8> = if self.config.keybindings.launcher_key == "Super" {
                    vec![133, 134] // Left and right SUPER keys
                } else {
                    // Try to parse as keycode number
                    if let Ok(keycode) = self.config.keybindings.launcher_key.parse::<u8>() {
                        vec![keycode]
                    } else {
                        vec![133, 134] // Default fallback
                    }
                };
                
                // Check if Mod4 bit is set (0x1000 = bit 12) or if keycode matches
                let mod4_bit = 0x1000u16;
                if (u16::from(e.state) & mod4_bit) != 0 || launcher_keycodes.contains(&e.detail) {
                    // Launch launcher command from config
                    info!("Launcher key pressed (keycode {}), launching {}", e.detail, self.config.keybindings.launcher_command);
                    let _ = std::process::Command::new(&self.config.keybindings.launcher_command)
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
            
            Event::XfixesCursorNotify(_e) => {
                // Cursor shape changed - update cursor image in compositor thread
                self.compositor.update_cursor_image();
            }
            
            Event::PropertyNotify(e) => {
                // Check if _NET_WM_STATE changed (for fullscreen detection)
                // Intern the atom to compare (we can't access wm.atoms directly, but we can intern it)
                if let Ok(reply) = self.conn.as_ref().intern_atom(false, b"_NET_WM_STATE")?.reply() {
                    if e.atom == reply.atom {
                        // Window state changed - check for fullscreen
                        debug!("PropertyNotify: _NET_WM_STATE changed for window {}", e.window);
                        
                        // Use frame ID if managed and framed
                        let target_id = if let Some(client) = self.wm_windows.get(&e.window) {
                            client.frame.as_ref().map(|f| f.frame).unwrap_or(e.window)
                        } else {
                            e.window
                        };
                        self.compositor.update_window_state(target_id);
                    }
                }
                
                // Check if _NET_WM_BYPASS_COMPOSITOR changed
                if e.atom == self.wm.atoms._net_wm_bypass_compositor {
                    if let Some(client) = self.wm_windows.get(&e.window) {
                        let composite_id = client.frame.as_ref().map(|f| f.frame).unwrap_or(e.window);
                        if let Ok(bypass) = self.wm.atoms.check_bypass_compositor(&self.conn, e.window) {
                            if bypass {
                                debug!("PropertyNotify: _NET_WM_BYPASS_COMPOSITOR set for window {}, unredirecting", e.window);
                                self.compositor.unredirect_window(composite_id);
                            } else {
                                debug!("PropertyNotify: _NET_WM_BYPASS_COMPOSITOR cleared for window {}, redirecting", e.window);
                                self.compositor.redirect_window(composite_id);
                            }
                        }
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
                        self.conn.as_ref().map_window(window_id)?;
                    }
                } else {
                    self.conn.as_ref().map_window(window_id)?;
                }
                client.mapped = true;
            }
            self.conn.as_ref().flush()?;
            return Ok(());
        }
        
        // Check if window is override-redirect BEFORE attempting management
        // Override-redirect windows (popups, tooltips) should not be managed by WM
        let is_override_redirect = match self.conn.as_ref().get_window_attributes(window_id)?.reply() {
            Ok(attrs) => attrs.override_redirect,
            Err(_) => {
                debug!("Window {} disappeared before we could check attributes", window_id);
                return Ok(());
            }
        };
        
        if is_override_redirect {
            debug!("Window {} is override-redirect, skipping WM management", window_id);
            // Still map it so it's visible, but don't manage or composite it
            self.conn.as_ref().map_window(window_id)?;
            self.conn.as_ref().flush()?;
            return Ok(());
        }
        
        // Create new client with default geometry (will be updated by manage_window)
        let mut client = Client::new(window_id, shared::Geometry::new(0, 0, 100, 100));
        
        // Check if window was already mapped before we took over
        let was_mapped = match self.conn.as_ref().get_window_attributes(window_id)?.reply() {
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
        self.conn.as_ref().flush()?;
        
        // Raise window to ensure it's visible (bring to front)
        use x11rb::protocol::xproto::StackMode;
        if let Some(frame) = &client.frame {
            self.conn.as_ref().configure_window(
                frame.frame,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        } else {
            self.conn.as_ref().configure_window(
                window_id,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        }
        self.conn.as_ref().flush()?;
        
        // Let compositor register the window (creates texture, damage tracking)
        // Determine composite target (FRAME or CLIENT)
        let composite_id = client.frame.as_ref().map(|f| f.frame).unwrap_or(client.id);
        
        // Get actual geometry, border width and viewable state from X11
        // We use *actual* X11 geometry because pixmap size matches the real window size
        let (geometry, border_width, viewable) = {
            let geom_result = self.conn.as_ref().get_geometry(composite_id)?.reply();
            let attr_result = self.conn.as_ref().get_window_attributes(composite_id)?.reply();
            
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

        self.compositor.add_window(c_window);
        
        // Check for _NET_WM_BYPASS_COMPOSITOR hint before storing window
        // Also check if window should be fullscreen (games often set bypass + fullscreen)
        let bypass_compositor = self.wm.atoms.check_bypass_compositor(&self.conn, window_id).unwrap_or(false);
        let mut needs_fullscreen = false;
        
        if bypass_compositor {
            debug!("Window {} requests compositor bypass, unredirecting", window_id);
            self.compositor.unredirect_window(composite_id);
            
            // Check EWMH state first
            if !client.state.fullscreen {
                if let Ok(reply) = self.conn.as_ref().get_property(
                    false,
                    window_id,
                    self.wm.atoms.net_wm_state,
                    AtomEnum::ATOM,
                    0,
                    1024,
                )?.reply() {
                    if let Some(mut value32) = reply.value32() {
                        if value32.any(|atom| atom == self.wm.atoms._net_wm_state_fullscreen) {
                            needs_fullscreen = true;
                        }
                    }
                }
            }
            
            // Also check geometry - if window is screen-sized, it's likely fullscreen
            if !needs_fullscreen && !client.state.fullscreen {
                let screen_width = self.screen_width as u32;
                let screen_height = self.screen_height as u32;
                if client.geometry.width >= screen_width.saturating_sub(20)
                    && client.geometry.width <= screen_width + 20
                    && client.geometry.height >= screen_height.saturating_sub(20)
                    && client.geometry.height <= screen_height + 20
                    && client.geometry.x <= 20 && client.geometry.y <= 20 {
                    needs_fullscreen = true;
                }
            }
        }
        
        // Store window
        self.wm_windows.insert(window_id, client);
        
        // Set fullscreen if needed (after insert so we can get_mut)
        if needs_fullscreen {
            if let Some(client) = self.wm_windows.get_mut(&window_id) {
                debug!("Window {} has bypass_compositor and fullscreen indication, setting fullscreen", window_id);
                if let Err(err) = self.wm.set_fullscreen(&self.conn, client, true) {
                    warn!("Failed to set fullscreen for window {}: {}", window_id, err);
                }
            }
        }
        
        // Update _NET_CLIENT_LIST
        self.update_client_list()?;
        
        debug!("Managed and mapped new window {}", window_id);
        Ok(())
    }
    
    /// Update _NET_CLIENT_LIST root property
    fn update_client_list(&mut self) -> Result<()> {
        let client_list: Vec<u32> = self.wm_windows.keys().copied().collect();
        self.wm.atoms.update_client_list(&self.conn, self.root, &client_list)?;
        self.conn.as_ref().flush()?;
        Ok(())
    }
    
    /// Handle DestroyNotify event
    fn handle_destroy(&mut self, window_id: u32) -> Result<()> {
        // Find the client window - could be the destroyed window itself or its frame
        let client_id = if self.wm_windows.contains_key(&window_id) {
            // Direct client window destruction
            Some(window_id)
        } else {
            // Might be a frame window - find the client
            self.wm.find_client_from_window(&self.wm_windows, window_id)
        };
        
        if let Some(client_id) = client_id {
            debug!("DestroyNotify for client window {} - cleaning up", client_id);
            // Use handle_unmap for proper cleanup
            self.handle_unmap(client_id)?;
        } else {
            // Window not found - check if it's a frame window that was already cleaned up
            if self.frame_windows.contains(&window_id) {
                // Frame window that was already cleaned up - this is expected when client closes
                debug!("DestroyNotify for already-cleaned-up frame window {} (expected)", window_id);
                self.frame_windows.remove(&window_id);
            } else {
                // Unknown window - might be unmanaged or already destroyed
                debug!("DestroyNotify for unknown window {} (not managed or already destroyed)", window_id);
                self.frame_windows.remove(&window_id);
            }
        }
        
        Ok(())
    }
    
    /// Handle UnmapNotify event
    fn handle_unmap(&mut self, window_id: u32) -> Result<()> {
        // If this window is in reparenting_windows, it's part of a fullscreen transition
        // or other reparenting operation - don't unmanage it
        if self.reparenting_windows.contains(&window_id) {
            debug!("Ignoring unmap for window {} (part of reparenting operation)", window_id);
            // Remove it from the set as the reparenting is complete
            self.reparenting_windows.remove(&window_id);
            return Ok(());
        }
        
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
            let composite_id = client.frame.as_ref().map(|f| f.frame).unwrap_or(window_id);
            self.compositor.remove_window(composite_id);
            
            // Let WM clean up (this will reparent window back to root)
            self.wm.unmanage_window(&self.conn, &mut client)?;
            
            // Update _NET_CLIENT_LIST
            self.update_client_list()?;
            
            debug!("Unmanaged window {} (cleaned up)", window_id);
        } else {
            debug!("UnmapNotify for window {} (not managed)", window_id);
        }
        Ok(())
    }
    
    // render_frame is removed, rendering is now managed by the compositor thread actor
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
    
    // Setup signal handlers for graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    
    // Handle SIGTERM and SIGINT
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;
        let tx = shutdown_tx.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, shutting down gracefully");
                    let _ = tx.send(()).await;
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT, shutting down gracefully");
                    let _ = tx.send(()).await;
                }
            }
        });
    }
    
    // Create and run application
    let app = AreaApp::new(replace).await?;
    
    // Get compositor handle before moving app into run()
    let compositor_handle = app.compositor.clone();
    
    // Run app with shutdown handling
    tokio::select! {
        result = app.run() => {
            if let Err(e) = result {
                error!("Application error: {}", e);
                return Err(e);
            }
        }
        _ = shutdown_rx.recv() => {
            info!("Shutdown signal received, cleaning up...");
            // Send shutdown command to compositor
            compositor_handle.shutdown();
            // Give compositor time to clean up
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    Ok(())
}
