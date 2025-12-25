//! Compositor Module
//!
//! Handles OpenGL rendering, window textures, and visual effects.

use x11rb::connection::RequestConnection;
pub mod renderer;
pub mod gl_context;
pub mod dri3;
pub mod fps;
pub mod c_window;

use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};
use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::composite::{self, ConnectionExt as CompositeExt};
use x11rb::protocol::damage::{self, ConnectionExt as DamageExt};
use x11rb::protocol::xproto::*;
use x11rb::protocol::ErrorKind;

use crate::compositor::c_window::CWindow;
use gl_context::GlContext;
use renderer::Renderer;

pub struct Compositor {
    pub overlay_window: u32,
    gl_context: Option<GlContext>,
    renderer: Option<Renderer>,
    fps_counter: fps::FpsCounter,
    
    /// Managed windows (owned by Compositor)
    windows: HashMap<u32, CWindow>,
}

impl Compositor {
    /// Create a new compositor
    pub fn new(
        conn: &x11rb::rust_connection::RustConnection,
        screen_num: usize,
        root: u32,
    ) -> Result<Self> {
        info!("Initializing compositor");
        
        // Check for Composite extension
        let _composite_info = conn
            .extension_information(composite::X11_EXTENSION_NAME)?
            .context("Composite extension not available")?;
        
        let composite_version = conn
            .composite_query_version(0, 4)?
            .reply()
            .context("Failed to query composite version")?;
        
        info!(
            "Composite extension {}.{}",
            composite_version.major_version,
            composite_version.minor_version
        );
        
        // Check for Damage extension
        let _damage_info = conn
            .extension_information(damage::X11_EXTENSION_NAME)?
            .context("Damage extension not available")?;
        
        // CRITICAL: Negotiate Damage extension version before any damage operations
        let damage_version = conn
            .damage_query_version(1, 1)?
            .reply()
            .context("Failed to query damage version")?;
        
        info!(
            "Damage extension {}.{}",
            damage_version.major_version,
            damage_version.minor_version
        );
        
        // Redirect all windows for compositing
        conn.composite_redirect_subwindows(root, composite::Redirect::MANUAL)?;
        
        // Get Composite Overlay Window
        let overlay_window = conn
            .composite_get_overlay_window(root)?
            .reply()?
            .overlay_win;
        
        info!("Using Composite Overlay Window: {}", overlay_window);
        
        // Make overlay window input-transparent so events pass through to root
        use x11rb::protocol::shape::{ConnectionExt as ShapeExt, SK, SO};
        
        conn.shape_rectangles(
            SO::SET,
            SK::INPUT,
            x11rb::protocol::xproto::ClipOrdering::UNSORTED,
            overlay_window,
            0,
            0,
            &[],
        )?;
        
        conn.flush()?;
        
        // Initialize OpenGL context
        let gl_context = match GlContext::new(conn, screen_num, overlay_window) {
            Ok(ctx) => {
                info!("OpenGL context initialized");
                Some(ctx)
            }
            Err(e) => {
                warn!("Failed to initialize OpenGL context: {}", e);
                None
            }
        };
        
        // Initialize renderer
        let renderer = gl_context.as_ref().and_then(|_| {
            match Renderer::new() {
                Ok(r) => {
                    info!("Renderer initialized");
                    Some(r)
                }
                Err(e) => {
                    warn!("Failed to initialize renderer: {}", e);
                    None
                }
            }
        });
        
        Ok(Self {
            overlay_window,
            gl_context,
            renderer,
            fps_counter: fps::FpsCounter::new(),
            windows: HashMap::new(),
        })
    }
    
    /// Get current FPS
    pub fn fps(&self) -> f64 {
        self.fps_counter.fps()
    }
    
    /// Add a window to the compositor
    pub fn add_window(
        &mut self,
        conn: &x11rb::rust_connection::RustConnection,
        window: CWindow,
    ) -> Result<()> {
        debug!("Compositor: Adding window {}", window.id);
        
        let window_id = window.id;
        let is_mapped = window.viewable; // Use viewable directly as it's passed in
        
        if !is_mapped {
            debug!("Window {} is not mapped, skipping compositor setup for now", window.id);
            // Even if not mapped, we store it for later
            let mut w = window;
            w.damaged = true;
            self.windows.insert(window_id, w);
            return Ok(());
        }
        
        // Use mutable variable since we move it into map later
        let mut window = window;
        
        // Determine composite target (FRAME or CLIENT)
        // In new architecture, CWindow.id IS the composite target.
        let composite_id = window.id;
        
        // NOTE: We use composite_redirect_subwindows() on root at startup,
        // so all subwindows are already redirected. No per-window redirect needed.
        window.redirected = true;
        
        let damage = match conn.generate_id() {
            Ok(id) => {
                match conn.damage_create(id, composite_id, damage::ReportLevel::NON_EMPTY) {
                    Ok(cookie) => {
                            conn.flush().ok();
                            if let Err(e) = cookie.check() {
                                warn!("damage_create failed for window {}: {}", composite_id, e);
                                None
                            } else {
                                debug!("Created damage object {} for window {} (target {})", id, window.id, composite_id);
                                Some(id)
                            }
                        }
                        Err(e) => {
                            warn!("Failed to create damage for window {}: {}", composite_id, e);
                            None
                        }
                    }
                }
            Err(e) => {
                warn!("Failed to generate damage ID for window {}: {}", composite_id, e);
                None
            }
        };
        window.damage = damage;
        window.damaged = true;
        
        debug!("Compositor: Added window {}", window.id);
        self.windows.insert(window.id, window);
        Ok(())
    }
    
    /// Remove a window from the compositor
    pub fn remove_window(
        &mut self,
        conn: &x11rb::rust_connection::RustConnection,
        window_id: u32,
    ) -> Result<()> {
        debug!("Compositor: Removing window {}", window_id);
        
        if let Some(mut window) = self.windows.remove(&window_id) {
             if let Some(damage) = window.damage {
                 conn.damage_destroy(damage)?;
             }
             
             // Unredirect window when removing from compositor
             if window.redirected {
                 let composite_id = window.id;
                 if let Err(e) = conn.composite_unredirect_window(composite_id, composite::Redirect::MANUAL) {
                     debug!("Failed to unredirect window {} (may already be unredirected): {}", composite_id, e);
                 }
                 window.redirected = false;
             }
        }
        
        debug!("Compositor: Removed window {}", window_id);
        Ok(())
    }
    
    /// Render all windows and shell
    pub fn render(&mut self, conn: &x11rb::rust_connection::RustConnection, shell: &crate::shell::Shell, screen_width: f32, screen_height: f32) -> Result<()> {
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
            
            // Check if any window is fullscreen (useful for future optimizations)
            for window in self.windows.values() {
                if window.is_fullscreen(screen_width as u16, screen_height as u16) {
                    tracing::trace!("Window {} is fullscreen", window.id);
                }
            }
            
            // First pass: lazy pixmap binding
            // Clone IDs to avoid borrow checker issues with self.windows
            let windows_to_bind: Vec<u32> = self.windows.values()
                // CWindow in compositor map = should be rendered
                .filter(|w| !renderer.has_texture(w.id) && !w.bind_failed)
                .map(|w| w.id)
                .collect();
            
            for window_id in windows_to_bind {
                // Check if window still exists in HashMap (might have been removed)
                if !self.windows.contains_key(&window_id) {
                    continue;
                }
                
                // Get window reference and perform initial checks
                let (window_id_copy, needs_redirect) = {
                    let window = match self.windows.get(&window_id) {
                        Some(w) => w,
                        None => continue,
                    };
                    
                    if window.bind_failed {
                        continue;
                    }
                    
                    // Check window state
                    if let Ok(window_attrs) = conn.get_window_attributes(window.id)?.reply() {
                        use x11rb::protocol::xproto::MapState;
                        if window_attrs.map_state == MapState::UNMAPPED || window_attrs.map_state == MapState::UNVIEWABLE {
                            continue;
                        }
                    }
                    
                    (window.id, !window.redirected)
                };
                
                // NOTE: composite_redirect_subwindows() on root handles all subwindows.
                // Just mark as redirected if not already.
                if needs_redirect {
                    if let Some(window) = self.windows.get_mut(&window_id) {
                        window.redirected = true;
                    }
                }
                
                // Final check: window might have been destroyed between collection and now
                // Verify it still exists in both our HashMap and in X11 before naming pixmap
                // This prevents "Window destroyed" errors that cause log spam
                if !self.windows.contains_key(&window_id) {
                    continue;
                }
                
                // Verify window still exists in X11 before naming pixmap
                match conn.get_window_attributes(window_id_copy) {
                    Ok(cookie) => {
                        if let Err(_) = cookie.reply() {
                            // Window doesn't exist in X11 anymore - mark as failed and skip
                            if let Some(w) = self.windows.get_mut(&window_id) {
                                w.bind_failed = true;
                            }
                            continue;
                        }
                    }
                    Err(_) => {
                        // Can't even query window - it's gone
                        if let Some(w) = self.windows.get_mut(&window_id) {
                            w.bind_failed = true;
                        }
                        continue;
                    }
                }

                // Get window reference for pixmap operation
                let window = match self.windows.get_mut(&window_id) {
                    Some(w) => w,
                    None => continue, // Window removed during X11 check
                };

                // Use frame ID if available, otherwise client ID
                let composite_id = window.id;

                    if let Ok(pixmap) = conn.generate_id() {
                        match conn.composite_name_window_pixmap(composite_id, pixmap) {
                            Ok(cookie) => {
                                if let Err(e) = cookie.check() {
                                    // Provide detailed error context based on error type
                                    match &e {
                                        ReplyError::X11Error(x11_err) => {
                                            match x11_err.error_kind {
                                                ErrorKind::Match => {
                                                    // This can happen if window is unmapped/destroyed during operation
                                                    // or if window doesn't meet Composite extension requirements
                                                    error!(
                                                        "composite_name_window_pixmap failed for window {}: Match error - window may not be redirected or has incompatible visual/depth. Window redirected: {}",
                                                        window_id, window.redirected
                                                    );
                                                }
                                                ErrorKind::Window => {
                                                    // Window was destroyed during the operation - this should be rare now
                                                    // but can still happen in edge cases. Mark as failed and clean up.
                                                    debug!(
                                                        "composite_name_window_pixmap failed for window {}: Window destroyed during operation",
                                                        window_id
                                                    );
                                                }
                                                _ => {
                                                    error!(
                                                        "composite_name_window_pixmap failed for window {}: {}",
                                                        window_id, e
                                                    );
                                                }
                                            }
                                        }
                                        _ => {
                                            error!(
                                                "composite_name_window_pixmap failed for window {}: {}",
                                                window_id, e
                                            );
                                        }
                                    }
                                    // Mark as failed and clean up pixmap
                                    if let Some(w) = self.windows.get_mut(&window_id) {
                                        w.bind_failed = true;
                                    }
                                    let _ = conn.free_pixmap(pixmap);
                                    continue;
                                }
                                conn.flush().ok();
                                std::thread::sleep(std::time::Duration::from_millis(10));
                                
                                match conn.get_geometry(pixmap)?.reply() {
                                    Ok(pixmap_geom) => {
                                        if pixmap_geom.width == 0 || pixmap_geom.height == 0 {
                                            let _ = conn.free_pixmap(pixmap);
                                            continue;
                                        }
                                        
                                        let extent = window.extents();
                                        let expected_width = extent.width as u16;
                                        let expected_height = extent.height as u16;
                                        
                                        if pixmap_geom.width != expected_width || pixmap_geom.height != expected_height {
                                            debug!("Pixmap size mismatch for window {}: expected {}x{}, got {}x{}", 
                                                window.id, expected_width, expected_height, pixmap_geom.width, pixmap_geom.height);
                                            let _ = conn.free_pixmap(pixmap);
                                            continue;
                                        }
                                        
                                        // Still need depth for renderer
                                        let depth = if let Ok(geom) = conn.get_geometry(composite_id)?.reply() {
                                            geom.depth
                                        } else {
                                            let _ = conn.free_pixmap(pixmap);
                                            continue;
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
                            }
                            Err(_) => {}
                        }
                    }
            }
            
            // Second pass: render windows
            for window in self.windows.values() {
                if window.damaged || !renderer.has_texture(window.id) {
                    let window_y = window.geometry.y as f32;
                    let adjusted_y = if window_y < panel_height {
                        panel_height
                    } else {
                        window_y
                    };
                    
                    if renderer.has_texture(window.id) {
                        let render_height = window.geometry.height as f32;
                        
                        renderer.render_window(
                            gl_context,
                            window.id,
                            window.geometry.x as f32,
                            adjusted_y,
                            window.geometry.width as f32,
                            render_height,
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
            
            gl_context.swap_buffers()?;
        }
        
        Ok(())
    }
    
    /// Get a mutable reference to a window
    pub fn get_window_mut(&mut self, window_id: u32) -> Option<&mut CWindow> {
        self.windows.get_mut(&window_id)
    }

    /// Check if any window is damaged
    pub fn any_damaged(&self) -> bool {
        self.windows.values().any(|w| w.damaged || w.damage.is_some())
    }

    /// Clear all damage flags
     pub fn clear_damage(&mut self) {
        for window in self.windows.values_mut() {
            window.damaged = false;
        }
    }
}
