//! OpenGL context creation using GLX directly (like Compiz)

use anyhow::{Context, Result};
use x11rb::rust_connection::RustConnection;
use std::ffi::CString;
use std::ptr;
use tracing::info;
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
const GLX_FRONT_LEFT_EXT: i32 = 0x20DE;
const GLX_MIPMAP_TEXTURE_EXT: i32 = 0x20D7;

/// OpenGL context wrapper using GLX (like Compiz)
#[allow(non_snake_case)]
pub struct GlContext {
    pub glx: Glx,
    pub xlib: Xlib,
    pub display: *mut xlib::Display,
    pub context: glx::GLXContext,
    pub drawable: u32, // Root window (drawable)
    #[allow(dead_code)]
    pub screen_num: i32,
    
    pub config: glx::GLXFBConfig,
    
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

        let screen_num_i32 = screen_num as i32;

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

        // Find TFP-capable FBConfig
        let attribs = [
            glx::GLX_DRAWABLE_TYPE as i32, glx::GLX_WINDOW_BIT as i32 | glx::GLX_PIXMAP_BIT as i32,
            glx::GLX_RENDER_TYPE as i32, glx::GLX_RGBA_BIT as i32,
            glx::GLX_DOUBLEBUFFER as i32, 1,
            glx::GLX_RED_SIZE as i32, 8,
            glx::GLX_GREEN_SIZE as i32, 8,
            glx::GLX_BLUE_SIZE as i32, 8,
            // glx::GLX_ALPHA_SIZE as i32, 8, // Remove strict alpha requirement to find Depth 24 configs
            glx::GLX_DEPTH_SIZE as i32, 0, 
            GLX_BIND_TO_TEXTURE_RGBA_EXT, 1, 
            GLX_BIND_TO_TEXTURE_TARGETS_EXT, GLX_TEXTURE_2D_BIT_EXT,
            0
        ];

        let mut num_configs = 0;
        let configs = unsafe {
            (glx.glXChooseFBConfig)(display, screen_num_i32, attribs.as_ptr(), &mut num_configs)
        };

        if configs.is_null() || num_configs == 0 {
             unsafe { (xlib.XCloseDisplay)(display) };
             return Err(anyhow::anyhow!("No suitable GLX FBConfig found (check TFP support)"));
        }

        // Use the first config
        let config = unsafe { *configs };
        
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
                (xlib.XFree)(configs as *mut _);
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
        
        // Clean up configs array, but keep the `config` (it's a pointer/handle)
        unsafe { (xlib.XFree)(configs as *mut _); }

        if context.is_null() {
            unsafe {
                (xlib.XFree)(vinfo as *mut _);
                (xlib.XCloseDisplay)(display);
            }
            return Err(anyhow::anyhow!("glXCreateNewContext failed"));
        }

        // Make context current with root window as drawable
        let result = unsafe {
            (glx.glXMakeCurrent)(display, root as u64, context)
        };

        if result == 0 {
            unsafe {
                (glx.glXDestroyContext)(display, context);
                (xlib.XFree)(vinfo as *mut _);
                (xlib.XCloseDisplay)(display);
            }
            return Err(anyhow::anyhow!("glXMakeCurrent failed"));
        }

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

        Ok(Self {
            glx,
            xlib,
            display,
            context,
            drawable: root,
            screen_num: screen_num_i32,
            config,
            glXBindTexImageEXT: bind_fn,
            glXReleaseTexImageEXT: release_fn,
        })
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


    /// Find a GLX FBConfig for a specific depth
    pub fn find_config_for_depth(&self, depth: u8) -> Result<glx::GLXFBConfig> {
        // Basic attributes
        let mut attribs = vec![
            glx::GLX_DRAWABLE_TYPE as i32, glx::GLX_WINDOW_BIT as i32 | glx::GLX_PIXMAP_BIT as i32,
            glx::GLX_RENDER_TYPE as i32, glx::GLX_RGBA_BIT as i32,
            glx::GLX_DOUBLEBUFFER as i32, 1,
            glx::GLX_RED_SIZE as i32, 1, // Prefer at least 1 bit
            glx::GLX_GREEN_SIZE as i32, 1,
            glx::GLX_BLUE_SIZE as i32, 1,
            GLX_BIND_TO_TEXTURE_RGBA_EXT, 1,
            GLX_BIND_TO_TEXTURE_TARGETS_EXT, GLX_TEXTURE_2D_BIT_EXT,
        ];
        
        let target_depth = depth as i32;
        
        // Handling depth/alpha differences
        if depth == 32 {
             attribs.push(glx::GLX_ALPHA_SIZE as i32);
             attribs.push(8);
             attribs.push(glx::GLX_BUFFER_SIZE as i32);
             attribs.push(32); // Total 32
        } else if depth == 24 {
             attribs.push(glx::GLX_ALPHA_SIZE as i32);
             attribs.push(0); // Standard X11 24-bit visual has 0 alpha
             attribs.push(glx::GLX_BUFFER_SIZE as i32);
             attribs.push(24);
        } else {
             // For strange depths (e.g. 16), try to match buffer size
             attribs.push(glx::GLX_BUFFER_SIZE as i32);
             attribs.push(target_depth);
        }
        
        attribs.push(0); // Terminate
        
        // Choose config
        let mut num_configs = 0;
        let configs = unsafe {
            (self.glx.glXChooseFBConfig)(self.display, self.screen_num, attribs.as_ptr(), &mut num_configs)
        };

        if configs.is_null() || num_configs == 0 {
             return Err(anyhow::anyhow!("No matching GLX FBConfig found for depth {}", depth));
        }
        
        let config = unsafe { *configs };
        unsafe { (self.xlib.XFree)(configs as *mut _); }
        
        Ok(config)
    }

    /// Create a GLX Pixmap from an X11 Pixmap with specific depth
    pub fn create_glx_pixmap(&self, pixmap: u32, depth: u8) -> Result<u32> {
        let config = self.find_config_for_depth(depth)
            .or_else(|_| {
                // Fallback to storing config if specific match fails (might fail later but worth a shot)
                info!("Fallback to default config for depth {}", depth);
                Ok::<glx::GLXFBConfig, anyhow::Error>(self.config)
            })?;
        
        let attribs = [
            GLX_TEXTURE_FORMAT_EXT, GLX_TEXTURE_FORMAT_RGBA_EXT,
            GLX_TEXTURE_TARGET_EXT, GLX_TEXTURE_2D_EXT,
            GLX_MIPMAP_TEXTURE_EXT, 0,
            0
        ];
        
        let glx_pixmap = unsafe {
            (self.glx.glXCreatePixmap)(self.display, config, pixmap as u64, attribs.as_ptr())
        };

        if glx_pixmap == 0 {
            return Err(anyhow::anyhow!("glXCreatePixmap failed"));
        }
        
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

