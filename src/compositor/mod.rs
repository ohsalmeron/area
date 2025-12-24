//! Compositor Module
//!
//! Handles OpenGL rendering, window textures, and visual effects.

use x11rb::connection::RequestConnection;
pub mod renderer;
pub mod gl_context;
pub mod dri3;
pub mod fps;

use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::composite::{self, ConnectionExt as CompositeExt};
use x11rb::protocol::damage::{self, ConnectionExt as DamageExt};
use x11rb::protocol::xproto::*;

use crate::shared::Window;
use gl_context::GlContext;
use renderer::Renderer;

pub struct Compositor {
    pub overlay_window: u32,
    gl_context: Option<GlContext>,
    renderer: Option<Renderer>,
    fps_counter: fps::FpsCounter,
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
        window: &mut Window,
    ) -> Result<()> {
        debug!("Compositor: Adding window {}", window.id);
        
        let is_mapped = match conn.get_window_attributes(window.id)?.reply() {
            Ok(attrs) => attrs.map_state != x11rb::protocol::xproto::MapState::UNMAPPED,
            Err(_) => false,
        };
        
        if !is_mapped {
            debug!("Window {} is not mapped, skipping compositor setup for now", window.id);
            window.comp.damaged = true;
            return Ok(());
        }
        
        let damage = match conn.generate_id() {
            Ok(id) => {
                match conn.damage_create(id, window.id, damage::ReportLevel::NON_EMPTY) {
                    Ok(cookie) => {
                        conn.flush().ok();
                        if let Err(e) = cookie.check() {
                            warn!("damage_create failed for window {}: {}", window.id, e);
                            None
                        } else {
                            debug!("Created damage object {} for window {}", id, window.id);
                            Some(id)
                        }
                    }
                    Err(e) => {
                        warn!("Failed to create damage for window {}: {}", window.id, e);
                        None
                    }
                }
            }
            Err(e) => {
                warn!("Failed to generate damage ID for window {}: {}", window.id, e);
                None
            }
        };
        window.comp.damage = damage;
        window.comp.damaged = true;
        
        info!("Compositor: Added window {}", window.id);
        Ok(())
    }
    
    /// Remove a window from the compositor
    pub fn remove_window(
        &mut self,
        conn: &x11rb::rust_connection::RustConnection,
        window: &mut Window,
    ) -> Result<()> {
        debug!("Compositor: Removing window {}", window.id);
        
        if let Some(damage) = window.comp.damage {
            conn.damage_destroy(damage)?;
            window.comp.damage = None;
        }
        
        info!("Compositor: Removed window {}", window.id);
        Ok(())
    }
    
    /// Render all windows and shell
    pub fn render(&mut self, conn: &x11rb::rust_connection::RustConnection, windows: &mut HashMap<u32, Window>, shell: &crate::shell::Shell, screen_width: f32, screen_height: f32) -> Result<()> {
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
            let windows_to_bind: Vec<u32> = windows.values()
                .filter(|w| w.mapped && !renderer.has_texture(w.id) && !w.comp.bind_failed)
                .map(|w| w.id)
                .collect();
            
            for window_id in windows_to_bind {
                if let Some(window) = windows.get_mut(&window_id) {
                    if window.comp.bind_failed || !window.mapped {
                        continue;
                    }
                    
                    if let Ok(window_attrs) = conn.get_window_attributes(window.id)?.reply() {
                        use x11rb::protocol::xproto::MapState;
                        if window_attrs.map_state == MapState::UNMAPPED || window_attrs.map_state == MapState::UNVIEWABLE {
                            continue;
                        }
                    }
                    
                    if let Ok(pixmap) = conn.generate_id() {
                        match conn.composite_name_window_pixmap(window.id, pixmap) {
                            Ok(cookie) => {
                                if let Err(e) = cookie.check() {
                                    warn!("composite_name_window_pixmap failed for window {}: {}", window_id, e);
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
                                        
                                        match conn.get_geometry(window.id)?.reply() {
                                            Ok(window_geom) => {
                                                let expected_width = window_geom.width + (window_geom.border_width as u16) * 2;
                                                let expected_height = window_geom.height + (window_geom.border_width as u16) * 2;
                                                
                                                if pixmap_geom.width != expected_width || pixmap_geom.height != expected_height {
                                                    let _ = conn.free_pixmap(pixmap);
                                                    continue;
                                                }
                                        
                                                let depth = window_geom.depth;
                                                window.comp.pixmap = Some(pixmap);
                                                
                                                if let Err(_) = renderer.update_window_pixmap(gl_context, window.id, pixmap, depth) {
                                                    window.comp.pixmap = None;
                                                    window.comp.bind_failed = true;
                                                    let _ = conn.free_pixmap(pixmap);
                                                }
                                            }
                                            Err(_) => {
                                                let _ = conn.free_pixmap(pixmap);
                                            }
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
            }
            
            // Second pass: render windows
            for window in windows.values() {
                if window.mapped && (window.comp.damaged || !renderer.has_texture(window.id)) {
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
                            window.comp.opacity,
                            window.comp.damaged,
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
            for window in windows.values_mut() {
                if window.comp.damaged && window.comp.damage.is_some() {
                    if let Some(damage_id) = window.comp.damage {
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
}
