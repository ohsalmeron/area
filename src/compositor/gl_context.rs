//! OpenGL context creation using GLX directly (like Compiz)

use anyhow::{Context, Result};
use x11rb::rust_connection::RustConnection;
use std::ffi::CString;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use tracing::{debug, error, info, warn};
use x11_dl::glx::{self, Glx};
use x11_dl::xlib::{self, Xlib};

// TFP (Texture From Pixmap) attributes
const GLX_BIND_TO_TEXTURE_RGBA_EXT: i32 = 0x20D1;
const GLX_BIND_TO_TEXTURE_TARGETS_EXT: i32 = 0x20D3;
const GLX_TEXTURE_2D_BIT_EXT: i32 = 0x0002;
const GLX_TEXTURE_FORMAT_EXT: i32 = 0x20D5;
const GLX_TEXTURE_TARGET_EXT: i32 = 0x20D6;
const GLX_TEXTURE_2D_EXT: i32 = 0x20DC; // Fixed: was 0x20DB
const GLX_TEXTURE_FORMAT_RGBA_EXT: i32 = 0x20DA; // Fixed: was 0x20D6
const GLX_TEXTURE_FORMAT_RGB_EXT: i32 = 0x20D9; // Added constant
const GLX_FRONT_LEFT_EXT: i32 = 0x20DE;
const GLX_MIPMAP_TEXTURE_EXT: i32 = 0x20D7;
const GLX_Y_INVERTED_EXT: i32 = 0x20D8;
const GLX_BIND_TO_TEXTURE_RGB_EXT: i32 = 0x20D0;
const GLX_BIND_TO_MIPMAP_TEXTURE_EXT: i32 = 0x20D2;

// X error handling (like compiz)
pub(crate) static X_ERROR_OCCURRED: AtomicBool = AtomicBool::new(false);
pub(crate) static X_ERROR_CODE: AtomicI32 = AtomicI32::new(0);

// X error handler callback (like compiz's errorHandler)
// CRITICAL: Return 0 to indicate we handled the error and prevent default X error handler
// The default handler would print to stderr and potentially exit the process
unsafe extern "C" fn x_error_handler(
    _display: *mut xlib::Display,
    event: *mut xlib::XErrorEvent,
) -> i32 {
    if !event.is_null() {
        let (error_code, request_code, minor_code) = unsafe {
            ((*event).error_code, (*event).request_code, (*event).minor_code)
        };
        
        X_ERROR_CODE.store(error_code as i32, Ordering::Relaxed);
        X_ERROR_OCCURRED.store(true, Ordering::Relaxed);
        
        // Log all errors for debugging, but don't let them crash the process
        // BadMatch (8) and BadPixmap (4) are common during GLX pixmap creation
        if error_code == 4 { // BadPixmap
            warn!("X Error BadPixmap (code=4): request={}, minor={} - GLX pixmap creation failed, will use fallback rendering", 
                request_code, minor_code);
        } else if error_code == 8 { // BadMatch
            warn!("X Error BadMatch (code=8): request={}, minor={} - Invalid parameter attributes, will use fallback rendering", 
                request_code, minor_code);
        } else {
            warn!("X Error: code={}, request={}, minor={}", 
                error_code, request_code, minor_code);
        }
    }
    // Return 0 to indicate we handled the error and prevent default X error handler
    // This prevents the process from exiting on X errors
    0
}

/// FBConfig info for a specific depth (like compiz's CompFBConfig)
#[derive(Clone, Copy)]
pub struct DepthFBConfig {
    pub fb_config: glx::GLXFBConfig,
    pub texture_format: i32, // GLX_TEXTURE_FORMAT_RGBA_EXT or GLX_TEXTURE_FORMAT_RGB_EXT
    pub mipmap: bool,
    pub y_inverted: bool,
}

/// OpenGL context wrapper using GLX (like Compiz)
#[allow(non_snake_case)]
pub struct GlContext {
    pub glx: Glx,
    pub xlib: Xlib,
    pub display: *mut xlib::Display,
    pub context: glx::GLXContext,
    pub drawable: u64, // GLX window or overlay window (for swap_buffers)
    #[allow(dead_code)]
    pub root: u32,
    #[allow(dead_code)]
    pub screen_num: i32,
    
    pub config: glx::GLXFBConfig, // Default config (for overlay window)
    
    // Per-depth FBConfig cache (like compiz's glxPixmapFBConfigs[depth])
    // Indexed by depth: 8, 15, 16, 24, 32
    depth_configs: [Option<DepthFBConfig>; 33], // 0-32
    
    // TFP function pointers
    #[allow(non_snake_case)]
    glXBindTexImageEXT: unsafe extern "C" fn(*mut xlib::Display, u32, i32, *mut i32),
    #[allow(non_snake_case)]
    glXReleaseTexImageEXT: unsafe extern "C" fn(*mut xlib::Display, u32, i32, *mut i32),
}

impl GlContext {
    /// Create a new OpenGL context using GLX directly (like Compiz)
    pub fn new(_conn: &RustConnection, screen_num: usize, root: u32) -> Result<Self> {
        // Load X11 and GLX libraries
        let xlib = Xlib::open().context("Failed to load libX11")?;
        let glx = Glx::open().context("Failed to load libGLX")?;

        // Get display name from connection
        let display_name = std::env::var("DISPLAY")
            .unwrap_or_else(|_| ":0".into());
        let display_cstr = CString::new(display_name)?;
        
        // Open X11 display
        let display = unsafe { (xlib.XOpenDisplay)(display_cstr.as_ptr()) };
        if display.is_null() {
            return Err(anyhow::anyhow!("Failed to open X11 display"));
        }
        
        // Set up X error handler (like compiz does with XSetErrorHandler)
        // This allows us to catch X errors before they become fatal
        unsafe {
            (xlib.XSetErrorHandler)(Some(x_error_handler));
        }

        let screen_num_i32 = screen_num as i32;
        
        // Get screen root window to query its visual
        let _screen = unsafe { (xlib.XDefaultScreen)(display) };
        // let _root_win = unsafe { (xlib.XRootWindow)(display, screen) }; // Removed as unused
        
        // Query overlay window's visual (passed as 'root' parameter, but it's actually overlay)
        let mut overlay_visual_id = 0u64;
        let mut overlay_depth = 0u8;
        unsafe {
            let mut attrs = std::mem::zeroed::<xlib::XWindowAttributes>();
            if (xlib.XGetWindowAttributes)(display, root as u64, &mut attrs) != 0 {
                overlay_depth = attrs.depth as u8;
                if !attrs.visual.is_null() {
                    overlay_visual_id = (*(attrs.visual)).visualid;
                }
            }
        }
        info!("Overlay window depth: {}, visual ID: 0x{:x}", overlay_depth, overlay_visual_id);

        // Verify GLX version
        let mut major = 0;
        let mut minor = 0;
        unsafe {
            (glx.glXQueryVersion)(display, &mut major, &mut minor);
        }
        info!("GLX version {}.{}", major, minor);

        // Check for TFP extension
        let extensions_str = unsafe {
            let s = (glx.glXQueryExtensionsString)(display, screen_num_i32);
            if s.is_null() { "" } else { std::ffi::CStr::from_ptr(s).to_str().unwrap_or("") }
        };
        
        if !extensions_str.contains("GLX_EXT_texture_from_pixmap") {
             unsafe { (xlib.XCloseDisplay)(display) };
             return Err(anyhow::anyhow!("GLX_EXT_texture_from_pixmap not supported"));
        }

        // First, try to find FBConfig matching overlay visual ID (like xfwm4 does)
        // Start with basic attributes (don't require TFP yet - we'll check it later)
        let basic_attribs = [
            glx::GLX_DRAWABLE_TYPE as i32, glx::GLX_WINDOW_BIT as i32 | glx::GLX_PIXMAP_BIT as i32,
            glx::GLX_RENDER_TYPE as i32, glx::GLX_RGBA_BIT as i32,
            glx::GLX_DOUBLEBUFFER as i32, 1,
            glx::GLX_RED_SIZE as i32, 8,
            glx::GLX_GREEN_SIZE as i32, 8,
            glx::GLX_BLUE_SIZE as i32, 8,
            0
        ];

        let mut num_configs = 0;
        let configs_ptr = unsafe {
            (glx.glXChooseFBConfig)(display, screen_num_i32, basic_attribs.as_ptr(), &mut num_configs)
        };

        if configs_ptr.is_null() || num_configs == 0 {
             unsafe { (xlib.XCloseDisplay)(display) };
             return Err(anyhow::anyhow!("No suitable GLX FBConfig found"));
        }

        // Find FBConfig with matching visual ID (prioritize visual match over TFP - like xfwm4 does)
        let mut config: Option<glx::GLXFBConfig> = None;
        let mut matched_config_tfp = false;
        
        if overlay_visual_id != 0 {
            for i in 0..num_configs as usize {
                let test_config = unsafe { *configs_ptr.add(i) };
                let vinfo = unsafe {
                    (glx.glXGetVisualFromFBConfig)(display, test_config)
                };
                
                if !vinfo.is_null() {
                    let config_visual_id = unsafe { (*vinfo).visualid };
                    unsafe { (xlib.XFree)(vinfo as *mut _); }
                    
                    if config_visual_id == overlay_visual_id {
                        // Found matching visual - check TFP support but use it anyway
                        let mut bind_to_texture = 0;
                        unsafe {
                            (glx.glXGetFBConfigAttrib)(display, test_config, GLX_BIND_TO_TEXTURE_RGBA_EXT, &mut bind_to_texture);
                        }
                        config = Some(test_config);
                        matched_config_tfp = bind_to_texture != 0;
                        if matched_config_tfp {
                            info!("Found FBConfig matching overlay visual ID 0x{:x} with TFP support", overlay_visual_id);
                        } else {
                            warn!("Found FBConfig matching overlay visual ID 0x{:x} but without TFP support (TFP may not work)", overlay_visual_id);
                        }
                        break;
                    }
                }
            }
        }
        
        // Fallback: try TFP-capable configs if no visual match
        if config.is_none() {
            let tfp_attribs = [
                glx::GLX_DRAWABLE_TYPE as i32, glx::GLX_WINDOW_BIT as i32 | glx::GLX_PIXMAP_BIT as i32,
                glx::GLX_RENDER_TYPE as i32, glx::GLX_RGBA_BIT as i32,
                glx::GLX_DOUBLEBUFFER as i32, 1,
                glx::GLX_RED_SIZE as i32, 8,
                glx::GLX_GREEN_SIZE as i32, 8,
                glx::GLX_BLUE_SIZE as i32, 8,
                GLX_BIND_TO_TEXTURE_RGBA_EXT, 1, 
                GLX_BIND_TO_TEXTURE_TARGETS_EXT, GLX_TEXTURE_2D_BIT_EXT,
                0
            ];
            
            let mut num_tfp_configs = 0;
            let tfp_configs_ptr = unsafe {
                (glx.glXChooseFBConfig)(display, screen_num_i32, tfp_attribs.as_ptr(), &mut num_tfp_configs)
            };
            
            if !tfp_configs_ptr.is_null() && num_tfp_configs > 0 {
                config = Some(unsafe { *tfp_configs_ptr });
                warn!("No FBConfig matching overlay visual ID 0x{:x}, using TFP-capable config (visual mismatch - glXCreateWindow may fail)", overlay_visual_id);
                unsafe { (xlib.XFree)(tfp_configs_ptr as *mut _); }
            } else {
                config = Some(unsafe { *configs_ptr });
                warn!("No TFP-capable FBConfig found, using first available (TFP may not work)");
            }
        }
        
        // Warn if TFP is required but not available
        if config.is_some() && !matched_config_tfp {
            warn!("Selected FBConfig does not support TFP - texture from pixmap may not work");
        }
        
        unsafe { (xlib.XFree)(configs_ptr as *mut _); }
        let config = config.unwrap();
        
        // Debug buffer depth
        unsafe {
            let mut alpha = 0;
            let mut depth = 0;
            (glx.glXGetFBConfigAttrib)(display, config, glx::GLX_ALPHA_SIZE as i32, &mut alpha);
            (glx.glXGetFBConfigAttrib)(display, config, glx::GLX_BUFFER_SIZE as i32, &mut depth);
            info!("Selected FBConfig: Alpha={}, Depth={}", alpha, depth);
        }
        
        // Retrieve visual from config
        let vinfo = unsafe {
            (glx.glXGetVisualFromFBConfig)(display, config)
        };

        if vinfo.is_null() {
            unsafe {
                (xlib.XCloseDisplay)(display);
            }
             return Err(anyhow::anyhow!("Failed to get visual from FBConfig"));
        }

        // Create GLX context with the config (Standard way for TFP)
        let context = unsafe {
            (glx.glXCreateNewContext)(
                display,
                config,
                glx::GLX_RGBA_TYPE as i32,
                ptr::null_mut(), 
                1, // Direct
            )
        };

        if context.is_null() {
            unsafe {
                (xlib.XFree)(vinfo as *mut _);
                (xlib.XCloseDisplay)(display);
            }
            return Err(anyhow::anyhow!("glXCreateNewContext failed"));
        }

        // For glXMakeCurrent, we need a drawable with a visual matching the FBConfig
        // CRITICAL: We should use the overlay window directly for rendering, not create a separate GLX window
        // The overlay window is what's actually visible on screen
        // Try overlay window first - it should work if the visual matches
        let mut make_current_success = false;
        let mut actual_drawable: u64 = root as u64; // Default to overlay window
        
        info!("Attempting to use overlay window {} directly for rendering", root);
        let result = unsafe {
            (glx.glXMakeCurrent)(display, root as u64, context)
        };
        unsafe {
            (xlib.XSync)(display, 0);
        }
        if result != 0 {
            make_current_success = true;
            actual_drawable = root as u64; // Use overlay window for swap_buffers
            info!("✓ Made GLX context current with overlay window (handle: {})", root);
        } else {
            warn!("Failed to make overlay window current - visual mismatch, trying GLX window creation");
            
            // Fallback: Create GLX window if overlay doesn't work
            let glx_version_ok = major > 1 || (major == 1 && minor >= 3);
            if glx_version_ok {
                info!("Creating GLX window from overlay window {} (glXCreateWindow should handle visual)", root);
                let glx_window = unsafe {
                    let attribs = [0i32]; // No additional attributes
                    (glx.glXCreateWindow)(display, config, root as u64, attribs.as_ptr())
                };
                
                unsafe {
                    (xlib.XSync)(display, 0);
                }
                
                if glx_window != 0 {
                    info!("Created GLX window: {}, attempting to make current", glx_window);
                    let result2 = unsafe {
                        (glx.glXMakeCurrent)(display, glx_window, context)
                    };
                    unsafe {
                        (xlib.XSync)(display, 0);
                    }
                    if result2 != 0 {
                        make_current_success = true;
                        actual_drawable = glx_window;
                        info!("✓ Made GLX context current with GLX window (handle: {})", glx_window);
                    } else {
                        warn!("Failed to make GLX window current");
                        unsafe {
                            (glx.glXDestroyWindow)(display, glx_window);
                        }
                    }
                } else {
                    warn!("glXCreateWindow returned 0 (failed)");
                }
            }
        }
        
        if !make_current_success {
            error!("All glXMakeCurrent attempts failed. Overlay window: {}, GLX version: {}.{}", 
                root, major, minor);
            unsafe {
                (glx.glXDestroyContext)(display, context);
                (xlib.XFree)(vinfo as *mut _);
                (xlib.XCloseDisplay)(display);
            }
            return Err(anyhow::anyhow!("glXMakeCurrent failed - could not use overlay window or create GLX window"));
        }
        
        info!("Successfully made GLX context current!");

        // Load OpenGL function pointers
        gl::load_with(|symbol| {
            let symbol_cstr = CString::new(symbol).unwrap();
            unsafe { 
                let proc = (glx.glXGetProcAddress)(symbol_cstr.as_ptr() as *const _);
                match proc {
                    Some(f) => f as *const _,
                    None => ptr::null(),
                }
            }
        });

        // Load TFP extension functions
        let bind_tex = unsafe {
            let sym = CString::new("glXBindTexImageEXT").unwrap();
             (glx.glXGetProcAddress)(sym.as_ptr() as *const _)
        };
        let release_tex = unsafe {
            let sym = CString::new("glXReleaseTexImageEXT").unwrap();
             (glx.glXGetProcAddress)(sym.as_ptr() as *const _)
        };
        let swap_interval = unsafe {
            let sym = CString::new("glXSwapIntervalEXT").unwrap();
             (glx.glXGetProcAddress)(sym.as_ptr() as *const _)
        };


        if bind_tex.is_none() || release_tex.is_none() {
            unsafe {
                (glx.glXDestroyContext)(display, context);
                (xlib.XFree)(vinfo as *mut _);
                (xlib.XCloseDisplay)(display);
            }
            return Err(anyhow::anyhow!("GLX_EXT_texture_from_pixmap functions missing"));
        }

        unsafe {
            (xlib.XFree)(vinfo as *mut _);
        }

        info!("GLX context created successfully (TFP enabled)");

        let bind_fn = unsafe { std::mem::transmute(bind_tex) };
        let release_fn = unsafe { std::mem::transmute(release_tex) };
        let swap_fn: Option<unsafe extern "C" fn(*mut xlib::Display, u32, i32)> = unsafe { 
            if let Some(ptr) = swap_interval {
                Some(std::mem::transmute(ptr))
            } else {
                None
            }
        };


        // Enable VSync (Swap Interval 1) unless benchmarking
        // CRITICAL: glXSwapIntervalEXT doesn't work with GLX windows created via glXCreateWindow
        // We must use the overlay window for swap interval control, even though we render to the GLX window
        // This is because swap interval is tied to the window's swap chain, not the rendering context
        if std::env::var("AREA_BENCHMARK").is_err() {
            if let Some(swap_func) = swap_fn {
                unsafe {
                    // Use overlay window for swap interval (this is what controls VSync)
                    // Even though we render to the GLX window, swap interval must be set on the overlay
                    info!("Enabling VSync (glXSwapIntervalEXT) on overlay window {}", root);
                    (swap_func)(display, root, 1);
                    (xlib.XSync)(display, 0);
                }
            } else {
                warn!("glXSwapIntervalEXT not supported - VSync may be disabled");
            }
        } else {
             info!("Benchmark Mode: VSync Disabled (uncapped FPS)");
             if let Some(swap_func) = swap_fn {
                unsafe {
                    (swap_func)(display, root, 0); // Explicitly disable vsync on overlay window
                    (xlib.XSync)(display, 0);
                }
            }
        }

        let mut ctx = Self {
            glx,
            xlib,
            display,
            context,
            drawable: actual_drawable, // GLX window if created, otherwise overlay window
            root, // In main.rs we pass COW here effectively
            screen_num: screen_num_i32,
            config,
            depth_configs: [None; 33], // Initialize all to None
            glXBindTexImageEXT: bind_fn,
            glXReleaseTexImageEXT: release_fn,
        };
        
        // Initialize per-depth FBConfigs (like compiz)
        ctx.initialize_depth_configs()?;
        
        Ok(ctx)
    }

    /// Swap buffers
    pub fn swap_buffers(&self) -> Result<()> {
        unsafe {
            (self.glx.glXSwapBuffers)(self.display, self.drawable as u64);
        }
        Ok(())
    }

    /// Make context current (if needed)
    pub fn make_current(&self) -> Result<()> {
        let result = unsafe {
            (self.glx.glXMakeCurrent)(self.display, self.drawable as u64, self.context)
        };
        if result == 0 {
            return Err(anyhow::anyhow!("glXMakeCurrent failed"));
        }
        Ok(())
    }

    /// Wait for Vertical Blank (VSync) using SGI_video_sync


    /// Initialize per-depth FBConfig cache (like compiz's glxPixmapFBConfigs)
    /// Finds the BEST config for each depth, prioritizing mipmap support, double buffer, etc.
    fn initialize_depth_configs(&mut self) -> Result<()> {
        // Get all available FBConfigs
        let basic_attribs = [
            glx::GLX_DRAWABLE_TYPE as i32, glx::GLX_WINDOW_BIT as i32 | glx::GLX_PIXMAP_BIT as i32,
            glx::GLX_RENDER_TYPE as i32, glx::GLX_RGBA_BIT as i32,
            0
        ];
        
        let mut num_configs = 0;
        let configs_ptr = unsafe {
            (self.glx.glXChooseFBConfig)(self.display, self.screen_num, basic_attribs.as_ptr(), &mut num_configs)
        };
        
        if configs_ptr.is_null() || num_configs == 0 {
            warn!("No FBConfigs available for depth initialization");
            return Ok(()); // Non-fatal, will fall back to default config
        }
        
        // Process each depth (8, 15, 16, 24, 32 are common)
        for depth in [8u8, 15, 16, 24, 32] {
            let mut best_config: Option<DepthFBConfig> = None;
            let mut best_mipmap = 0i32;
            let mut best_db = i32::MAX;
            let mut best_stencil = i32::MAX;
            let mut best_depth_size = i32::MAX;
            
            // Find best config for this depth (like compiz's algorithm)
            for i in 0..num_configs as usize {
                let test_config = unsafe { *configs_ptr.add(i) };
                
                // Get visual depth
                let vinfo = unsafe { (self.glx.glXGetVisualFromFBConfig)(self.display, test_config) };
                if vinfo.is_null() {
                    continue;
                }
                
                let visual_depth = unsafe { (*vinfo).depth } as u8;
                unsafe { (self.xlib.XFree)(vinfo as *mut _); }
                
                if visual_depth != depth {
                    continue;
                }
                
                // Check buffer size matches depth
                let mut buffer_size = 0i32;
                let mut alpha_size = 0i32;
                unsafe {
                    (self.glx.glXGetFBConfigAttrib)(self.display, test_config, glx::GLX_BUFFER_SIZE as i32, &mut buffer_size);
                    (self.glx.glXGetFBConfigAttrib)(self.display, test_config, glx::GLX_ALPHA_SIZE as i32, &mut alpha_size);
                }
                
                // Buffer size should match depth (or depth + alpha for 32-bit)
                if buffer_size != depth as i32 && (buffer_size - alpha_size) != depth as i32 {
                    continue;
                }
                
                // Check texture format (prefer RGBA for 32-bit, RGB for others)
                let mut bind_rgba = 0i32;
                let mut bind_rgb = 0i32;
                let mut texture_format = GLX_TEXTURE_FORMAT_RGB_EXT;
                
                if depth == 32 {
                    unsafe {
                        (self.glx.glXGetFBConfigAttrib)(self.display, test_config, GLX_BIND_TO_TEXTURE_RGBA_EXT, &mut bind_rgba);
                    }
                    if bind_rgba != 0 {
                        texture_format = GLX_TEXTURE_FORMAT_RGBA_EXT;
                    } else {
                        continue; // 32-bit needs RGBA
                    }
                } else {
                    unsafe {
                        (self.glx.glXGetFBConfigAttrib)(self.display, test_config, GLX_BIND_TO_TEXTURE_RGB_EXT, &mut bind_rgb);
                    }
                    if bind_rgb == 0 {
                        continue; // Need RGB binding
                    }
                }
                
                // Get quality metrics (like compiz prioritizes)
                let mut db = 0i32;
                let mut stencil = 0i32;
                let mut depth_size = 0i32;
                let mut mipmap = 0i32;
                let mut y_inverted = 0i32;
                let mut _texture_targets = 0i32;
                
                unsafe {
                    (self.glx.glXGetFBConfigAttrib)(self.display, test_config, glx::GLX_DOUBLEBUFFER as i32, &mut db);
                    (self.glx.glXGetFBConfigAttrib)(self.display, test_config, glx::GLX_STENCIL_SIZE as i32, &mut stencil);
                    (self.glx.glXGetFBConfigAttrib)(self.display, test_config, glx::GLX_DEPTH_SIZE as i32, &mut depth_size);
                    (self.glx.glXGetFBConfigAttrib)(self.display, test_config, GLX_BIND_TO_MIPMAP_TEXTURE_EXT, &mut mipmap);
                    (self.glx.glXGetFBConfigAttrib)(self.display, test_config, GLX_Y_INVERTED_EXT, &mut y_inverted);
                    (self.glx.glXGetFBConfigAttrib)(self.display, test_config, GLX_BIND_TO_TEXTURE_TARGETS_EXT, &mut _texture_targets);
                }
                
                // Prefer configs with better quality (like compiz)
                // Priority: mipmap > double buffer > minimize stencil > minimize depth
                if mipmap < best_mipmap {
                    continue;
                }
                if db > best_db {
                    continue;
                }
                if stencil > best_stencil {
                    continue;
                }
                if depth_size > best_depth_size {
                    continue;
                }
                
                best_config = Some(DepthFBConfig {
                    fb_config: test_config,
                    texture_format,
                    mipmap: mipmap != 0,
                    y_inverted: y_inverted != 0,
                });
                best_mipmap = mipmap;
                best_db = db;
                best_stencil = stencil;
                best_depth_size = depth_size;
            }
            
            if let Some(config) = best_config {
                self.depth_configs[depth as usize] = Some(config);
                info!("Found optimal FBConfig for depth {}: mipmap={}, y_inverted={}", 
                    depth, config.mipmap, config.y_inverted);
            } else {
                debug!("No optimal FBConfig found for depth {}", depth);
            }
        }
        
        unsafe { (self.xlib.XFree)(configs_ptr as *mut _); }
        Ok(())
    }
    
    /// Get cached FBConfig for a specific depth (like compiz's glxPixmapFBConfigs[depth])
    pub fn get_config_for_depth(&self, depth: u8) -> Result<DepthFBConfig> {
        if depth as usize >= self.depth_configs.len() {
            return Err(anyhow::anyhow!("Invalid depth {}", depth));
        }
        
        if let Some(config) = self.depth_configs[depth as usize] {
            Ok(config)
        } else {
            // Fallback to default config with basic settings
            warn!("No cached config for depth {}, using default", depth);
            Ok(DepthFBConfig {
                fb_config: self.config,
                texture_format: if depth == 32 { GLX_TEXTURE_FORMAT_RGBA_EXT } else { GLX_TEXTURE_FORMAT_RGB_EXT },
                mipmap: false,
                y_inverted: false,
            })
        }
    }

    /// Verify pixmap exists and is valid (like xfwm4 checks before GLX operations)
    fn verify_pixmap(&self, pixmap: u32) -> Result<()> {
        // Clear error state
        X_ERROR_OCCURRED.store(false, Ordering::Relaxed);
        X_ERROR_CODE.store(0, Ordering::Relaxed);
        
        // Try to get pixmap geometry to verify it exists (XGetGeometry works on drawables)
        unsafe {
            let mut root_return: u64 = 0;
            let mut x_return: i32 = 0;
            let mut y_return: i32 = 0;
            let mut width_return: u32 = 0;
            let mut height_return: u32 = 0;
            let mut border_width_return: u32 = 0;
            let mut depth_return: u32 = 0;
            
            let result = (self.xlib.XGetGeometry)(
                self.display,
                pixmap as u64,
                &mut root_return,
                &mut x_return,
                &mut y_return,
                &mut width_return,
                &mut height_return,
                &mut border_width_return,
                &mut depth_return,
            );
            
            (self.xlib.XSync)(self.display, 0);
            
            let had_error = X_ERROR_OCCURRED.load(Ordering::Relaxed);
            if result == 0 || had_error {
                let error_code = X_ERROR_CODE.load(Ordering::Relaxed);
                X_ERROR_OCCURRED.store(false, Ordering::Relaxed);
                X_ERROR_CODE.store(0, Ordering::Relaxed);
                return Err(anyhow::anyhow!("Pixmap {} verification failed: X error code {} (pixmap may not exist)", pixmap, error_code));
            }
            
            // Also check that pixmap has valid dimensions
            if width_return == 0 || height_return == 0 {
                X_ERROR_OCCURRED.store(false, Ordering::Relaxed);
                X_ERROR_CODE.store(0, Ordering::Relaxed);
                return Err(anyhow::anyhow!("Pixmap {} has invalid dimensions {}x{}", pixmap, width_return, height_return));
            }
        }
        
        X_ERROR_OCCURRED.store(false, Ordering::Relaxed);
        X_ERROR_CODE.store(0, Ordering::Relaxed);
        Ok(())
    }

    /// Create a GLX Pixmap from an X11 Pixmap with specific depth
    /// Returns None if creation fails (BadPixmap, etc.) instead of crashing
    /// Uses error trapping like xfwm4 (myDisplayErrorTrapPush/Pop) and checks return values like compiz
    /// Uses optimal FBConfig for the depth (like compiz's glxPixmapFBConfigs[depth])
    pub fn create_glx_pixmap(&self, pixmap: u32, depth: u8) -> Result<u32> {
        // Verify pixmap exists before attempting GLX creation (like xfwm4)
        // This prevents BadPixmap errors from occurring
        if let Err(e) = self.verify_pixmap(pixmap) {
            return Err(e.context("Pixmap verification failed before GLX creation"));
        }
        
        // Clear error state before attempting
        X_ERROR_OCCURRED.store(false, Ordering::Relaxed);
        X_ERROR_CODE.store(0, Ordering::Relaxed);
        
        // Get optimal config for this depth (like compiz uses glxPixmapFBConfigs[depth])
        let depth_config = self.get_config_for_depth(depth)?;
        let config = depth_config.fb_config;
        let texture_format = depth_config.texture_format;

        let attribs = [
            GLX_TEXTURE_FORMAT_EXT, texture_format,
            GLX_TEXTURE_TARGET_EXT, GLX_TEXTURE_2D_EXT,
            GLX_MIPMAP_TEXTURE_EXT, 0,
            0
        ];
        
        // Clear any pending X errors before attempting creation (like xfwm4's error trap push)
        // CRITICAL: Sync before and after to ensure errors are caught by our handler
        unsafe {
            (self.xlib.XSync)(self.display, 0);
        }
        X_ERROR_OCCURRED.store(false, Ordering::Relaxed);
        X_ERROR_CODE.store(0, Ordering::Relaxed);
        
        // Try to create GLX pixmap directly
        // If the pixmap doesn't exist yet or is invalid, glXCreatePixmap will return 0
        // or generate a BadPixmap/BadMatch error (which we'll catch via error handler)
        let glx_pixmap = unsafe {
            (self.glx.glXCreatePixmap)(self.display, config, pixmap as u64, attribs.as_ptr())
        };
        
        // Immediately sync to flush errors (like xfwm4's error trap pop)
        // This ensures our error handler catches any X errors before they can cause issues
        unsafe {
            (self.xlib.XSync)(self.display, 0);
        }
        
        // Check for errors (like compiz checks return value and xfwm4 checks error trap)
        let had_error = X_ERROR_OCCURRED.load(Ordering::Relaxed);
        let error_code = X_ERROR_CODE.load(Ordering::Relaxed);
        
        // Check if pixmap creation failed (like compiz does)
        // glXCreatePixmap returns 0 on failure, and BadPixmap (error code 4) or BadMatch (error code 8) indicate failure
        if glx_pixmap == 0 || had_error {
            let error_msg = if error_code == 4 {
                format!("BadPixmap - pixmap {} is invalid or incompatible with GLX config", pixmap)
            } else if error_code == 8 {
                format!("BadMatch - pixmap {} has invalid parameter attributes for GLX config", pixmap)
            } else if had_error {
                format!("X Error code {} - pixmap {} GLX creation failed", error_code, pixmap)
            } else {
                format!("glXCreatePixmap returned 0 - pixmap {} may be incompatible with GLX config", pixmap)
            };
            
            // Clear error state for next attempt
            // CRITICAL: Clear errors so they don't accumulate and cause issues
            X_ERROR_OCCURRED.store(false, Ordering::Relaxed);
            X_ERROR_CODE.store(0, Ordering::Relaxed);
            
            // Return error but don't crash - the caller will handle it gracefully
            return Err(anyhow::anyhow!("{}", error_msg));
        }
        
        // Clear error state on success
        X_ERROR_OCCURRED.store(false, Ordering::Relaxed);
        X_ERROR_CODE.store(0, Ordering::Relaxed);
        
        Ok(glx_pixmap as u32)
    }

    /// Destroy a GLX Pixmap
    pub fn destroy_glx_pixmap(&self, glx_pixmap: u32) {
        unsafe {
            (self.glx.glXDestroyPixmap)(self.display, glx_pixmap as u64);
        }
    }

    /// Bind a GLX pixmap to the current texture unit
    pub fn bind_tex_image(&self, glx_pixmap: u32) {
        unsafe {
            // CRITICAL: Wait for X server to finish rendering before GL reads the pixmap
            // This synchronizes the X11 and GLX rendering pipelines
            (self.glx.glXWaitX)();
            (self.glXBindTexImageEXT)(self.display, glx_pixmap, GLX_FRONT_LEFT_EXT, ptr::null_mut());
        }
    }

    /// Release a GLX pixmap from the current texture unit
    pub fn release_tex_image(&self, glx_pixmap: u32) {
        unsafe {
            (self.glXReleaseTexImageEXT)(self.display, glx_pixmap, GLX_FRONT_LEFT_EXT, ptr::null_mut());
        }
    }
}

impl Drop for GlContext {
    fn drop(&mut self) {
        unsafe {
            (self.glx.glXMakeCurrent)(self.display, 0, ptr::null_mut());
            (self.glx.glXDestroyContext)(self.display, self.context);
            (self.xlib.XCloseDisplay)(self.display);
        }
    }
}

