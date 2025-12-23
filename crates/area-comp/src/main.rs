//! Area Compositor - OpenGL-based X11 compositor

mod capture;
mod dri3;
mod gl_context;
mod ipc;
mod renderer;
mod window_state;
mod cursor;

use anyhow::{Context, Result};
use area_ipc::WmEvent;
use capture::WindowCapture;
use gl_context::GlContext;
use cursor::CursorManager;
use ipc::IpcClient;
use renderer::Renderer;
use window_state::WindowState;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::composite::{self, ConnectionExt as CompositeExt};
use std::time::{Duration, Instant};
use std::path::PathBuf;
use tracing::{debug, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

struct CompositorApp {
    gl_context: Option<GlContext>,
    renderer: Option<Renderer>,
    capture: WindowCapture,
    ipc: IpcClient,
    window_state: WindowState,
    screen_width: u32,
    screen_height: u32,
    last_frame: Instant,
    screen_num: usize,
    root: u32,
    cursor_manager: Option<CursorManager>,
}

impl CompositorApp {
    async fn new() -> Result<Self> {
        // Connect to X11
        let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
        let capture = WindowCapture::new(Some(&display))
            .context("Failed to create window capture")?;
        
        let conn = capture.connection();
        let screen = &conn.setup().roots[0];
        let root = screen.root;
        let screen_width = screen.width_in_pixels as u32;
        let screen_height = screen.height_in_pixels as u32;

        // Initialize Composite extension
        let _composite_info = conn
            .extension_information(composite::X11_EXTENSION_NAME)?
            .context("Composite extension not available")?;

        let composite_version = conn
            .composite_query_version(0, 4)?
            .reply()
            .context("Failed to query composite version")?;

        info!(
            "Initialized Composite Extension {}.{}",
            composite_version.major_version, composite_version.minor_version
        );

        // Note: Composite redirect is set by area-wm (Redirect::MANUAL)
        // We don't set it here to avoid conflicts

        // Connect to WM via IPC
        let ipc = IpcClient::connect().await
            .context("Failed to connect to WM via IPC")?;

        // Get Composite Overlay Window (COW) for rendering
        // We must render to this window to be visible over redirected subwindows
        let overlay_window = conn.composite_get_overlay_window(root)?.reply()?.overlay_win;
        info!("Using Composite Overlay Window: {}", overlay_window);

        // Initialize Cursor Manager
        // Be careful: we want to listen for cursor events on the REAL root window, not the overlay.
        // `root` variable holds the real root window (passed to composite_get_overlay_window).
        // Let's use THAT root for cursor events.
        let cursor_manager = match CursorManager::new(conn, root) {
            Ok(cm) => {
                info!("Hardware cursor emulation initialized");
                Some(cm)
            },
            Err(e) => {
                warn!("Failed to initialize hardware cursor emulation: {}", e);
                None
            }
        };

        Ok(Self {
            capture,
            ipc,
            window_state: WindowState::new(),
            screen_width,
            screen_height,
            last_frame: Instant::now(),
            gl_context: None,
            renderer: None,
            screen_num: 0,
            root: overlay_window, // Use COW as the main 'root' for rendering
            cursor_manager,
        })
    }

    fn init_gl(&mut self) -> Result<()> {
        let gl_ctx = GlContext::new(self.capture.connection(), self.screen_num, self.root)
            .context("Failed to create GL context")?;
        
        let renderer = Renderer::new()
            .context("Failed to create renderer")?;

        self.gl_context = Some(gl_ctx);
        self.renderer = Some(renderer);
        
        info!("OpenGL renderer ready");
        
        // Create ready signal file
        let ready_file = Self::get_ready_file_path();
        if let Err(e) = std::fs::write(&ready_file, "ready\n") {
            warn!("Failed to write ready signal file: {}", e);
        } else {
            info!("Compositor ready signal written to {:?}", ready_file);
        }
        
        Ok(())
    }
    
    fn get_ready_file_path() -> PathBuf {
        std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
            .join("area-comp-ready")
    }

    fn handle_wm_event(&mut self, event: WmEvent) {
        match event {
            WmEvent::WindowOpened { id, title, class, x, y, width, height } => {
                info!("Window opened: {} ({})", title, id);
                self.window_state.update_window(id, title, class, x, y, width, height);
                self.capture.track_window_damage(id);
            }
            WmEvent::WindowClosed { id } => {
                info!("Window closed: {}", id);
                self.capture.untrack_window_damage(id);
                
                let pixmap_to_free = if let Some(ref mut renderer) = self.renderer {
                    if let Some(ref ctx) = self.gl_context {
                        renderer.remove_window_texture(ctx, id)
                    } else { None }
                } else { None };

                if let Some(p) = pixmap_to_free {
                    self.capture.free_pixmap(p);
                }

                self.window_state.remove_window(id);
            }
            WmEvent::WindowFocused { id } => {
                self.window_state.set_focus(Some(id));
            }
            WmEvent::WindowGeometryChanged { id, x, y, width, height } => {
                if let Some(win) = self.window_state.get_window(id) {
                    let title = win.title.clone();
                    let class = win.class.clone();
                    self.window_state.update_window(id, title, class, x, y, width, height);
                }
            }
            WmEvent::SyncState { windows, focused_window, .. } => {
                info!("Syncing state: {} windows", windows.len());
                for win in windows {
                    self.window_state.update_window(
                        win.id, win.title, win.class,
                        win.x, win.y, win.width, win.height
                    );
                    self.capture.track_window_damage(win.id);
                }
                self.window_state.set_focus(focused_window);
            }
            _ => {}
        }
    }

    fn render_frame(&mut self) {
        let Some(ref mut renderer) = self.renderer else { return };
        let Some(ref gl_ctx) = self.gl_context else { return };

        // Ensure GL context is current
        if let Err(e) = gl_ctx.make_current() {
            warn!("Failed to make GL context current: {}", e);
            return;
        }

        // Set viewport (CRITICAL - OpenGL needs this!)
        unsafe {
            gl::Viewport(0, 0, self.screen_width as i32, self.screen_height as i32);
        }

        // Process damage events
        if let Err(e) = self.capture.process_damage_events() {
            warn!("Error processing damage events: {}", e);
        }

        // 1. Update textures for dirty/new windows
        // We collect IDs first to avoid borrowing conflicts with window_state
        let window_ids = self.window_state.window_ids();
        for id in window_ids {
            let needs_update = if let Some(win) = self.window_state.get_window(id) {
                // Update if texture missing OR marked dirty (resize)
                // Also check window is valid (mapped, non-zero size)
                (!renderer.has_texture(id) || win.pixmap_dirty) 
                    && win.width > 0 && win.height > 0
            } else {
                false
            };

            if needs_update {
                // Validate window is mapped before capturing
                if !self.capture.is_window_ready(id) {
                    debug!("Window {} not ready yet, skipping pixmap capture", id);
                    continue;
                }
                
                 if let Some((pixmap, depth)) = self.capture.capture_window_pixmap(id) {
                    match renderer.update_window_pixmap(gl_ctx, id, pixmap, depth) {
                        Ok(maybe_old) => {
                             if let Some(old) = maybe_old {
                                 self.capture.free_pixmap(old);
                             }
                             self.window_state.clear_pixmap_dirty(id);
                        },
                        Err(e) => {
                             warn!("Failed to update window texture for {} (pixmap {}, depth {}): {}", id, pixmap, depth, e);
                             // If update failed, free the new pixmap we just created
                             self.capture.free_pixmap(pixmap);
                        }
                    }
                }
            }
        }

        // Clear screen (dark gray background)
        renderer.clear(0.1, 0.1, 0.1, 1.0);

        // 2. Render windows
        for win in self.window_state.windows_in_order() {
            // Skip if window is too small or not visible
            if win.width == 0 || win.height == 0 {
                continue;
            }

            // Mark damage as processed (we are drawing the current state)
            // Handle damage (re-bind texture if needed)
            if self.capture.is_damaged(win.id) {
                self.capture.clear_damage(win.id);
                // For valid TFP updates, we might need to re-bind or re-create the GLX pixmap
                // especially if the underlying storage changed (resize) or if synchronization is needed.
                // Simple approach: re-capture the pixmap.
                if let Some((pixmap, depth)) = self.capture.capture_window_pixmap(win.id) {
                     match renderer.update_window_pixmap(gl_ctx, win.id, pixmap, depth) {
                         Ok(maybe_old) => {
                             if let Some(old) = maybe_old {
                                 self.capture.free_pixmap(old);
                             }
                             // trace!("Updated texture for damaged window {}", win.id);
                         },
                         Err(e) => {
                             warn!("Failed to update damaged window texture {}: {}", win.id, e);
                             self.capture.free_pixmap(pixmap);
                         }
                     }
                }
            }

            // Render window
            let opacity = if win.focused { 1.0 } else { 0.9 };
            // trace!("Rendering window {} at ({}, {}) size {}x{}", win.id, win.x, win.y, win.width, win.height);
            renderer.render_window(
                gl_ctx,
                win.id,
                win.x as f32,
                win.y as f32,
                win.width as f32,
                win.height as f32,
                self.screen_width as f32,
                self.screen_height as f32,
                opacity,
            );
        }

        // 3. Render Cursor (Software emulation)
        if let Some(cursor) = &mut self.cursor_manager {
            // Update cursor state
            let _ = cursor.update(self.capture.connection());
            
            if cursor.visible {
                // Update texture if dirty
                if cursor.dirty && !cursor.pixels.is_empty() {
                    let tex_id = renderer.update_cursor_texture(
                        cursor.width,
                        cursor.height,
                        &cursor.pixels
                    );
                    cursor.texture_id = Some(tex_id);
                    cursor.dirty = false;
                }
                
                if let Some(tex_id) = cursor.texture_id {
                    // Render cursor ON TOP of everything
                    renderer.render_cursor(
                        tex_id,
                        cursor.x,
                        cursor.y,
                        cursor.width,
                        cursor.height,
                        cursor.xhot,
                        cursor.yhot,
                        self.screen_width as f32,
                        self.screen_height as f32,
                    );
                }
            }
        }
        // Swap buffers
        if let Err(e) = gl_ctx.swap_buffers() {
            warn!("Failed to swap buffers: {}", e);
        }
    }

    fn tick(&mut self) {
        // Process IPC events
        while let Some(event) = self.ipc.try_recv_event() {
            self.handle_wm_event(event);
        }

        // Render frame
        self.render_frame();

        // Throttle to ~60fps
        let elapsed = self.last_frame.elapsed();
        if elapsed < Duration::from_millis(16) {
            std::thread::sleep(Duration::from_millis(16) - elapsed);
        }
        self.last_frame = Instant::now();
    }
}

impl CompositorApp {
    fn run(&mut self) -> Result<()> {
        // Initialize GL
        self.init_gl()?;

        // Main compositor loop
        loop {
            self.tick();
            
            // Sleep to throttle to ~60fps
            std::thread::sleep(Duration::from_millis(16));
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "area_comp=debug,info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Area Compositor");

    let mut app = CompositorApp::new().await?;

    // Run compositor
    app.run()?;

    Ok(())
}
