//! OpenGL rendering backend

use anyhow::{Context, Result};
use std::ffi::CString;
use std::ptr;
use tracing::{debug, info, trace, warn};

/// Texture resources for a window
struct WindowTexture {
    texture: u32,
    glx_pixmap: Option<u32>, // None if using XGetImage fallback
    x11_pixmap: Option<u32>, // None if using XGetImage fallback
}

/// OpenGL renderer for compositing windows
pub struct Renderer {
    program: u32,
    vao: u32,
    vbo: u32,
    textures: std::collections::HashMap<u32, WindowTexture>, // window_id -> WindowTexture
    white_texture: u32, // Cached 1x1 white texture for solid color rendering
}

impl Renderer {
    /// Initialize the OpenGL renderer
    pub fn new() -> Result<Self> {
        unsafe {
            // Enable blending for transparency
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            // Create shader program
            let program = Self::create_shader_program()?;

            // Create VAO and VBO for window quads
            let mut vao = 0;
            let mut vbo = 0;
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

            // Vertex attributes: position (vec2) and texcoord (vec2)
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 4 * std::mem::size_of::<f32>() as i32, ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(1, 2, gl::FLOAT, gl::FALSE, 4 * std::mem::size_of::<f32>() as i32, (2 * std::mem::size_of::<f32>()) as *const _);
            gl::EnableVertexAttribArray(1);

            gl::BindVertexArray(0);

            info!("OpenGL renderer initialized");
            
            // Create a 1x1 white texture for solid color rendering
            let mut white_texture = 0;
            gl::GenTextures(1, &mut white_texture);
            gl::BindTexture(gl::TEXTURE_2D, white_texture);
            let white_pixel: [u8; 4] = [255, 255, 255, 255];
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as i32,
                1,
                1,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                white_pixel.as_ptr() as *const _,
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);

            Ok(Self {
                program,
                vao,
                vbo,
                textures: std::collections::HashMap::new(),
                white_texture,
            })
        }
    }

    /// Create shader program
    fn create_shader_program() -> Result<u32> {
        let vertex_shader = r#"
            #version 330 core
            layout (location = 0) in vec2 aPos;
            layout (location = 1) in vec2 aTexCoord;
            
            uniform vec2 uPosition;
            uniform vec2 uSize;
            
            out vec2 TexCoord;
            
            void main() {
                vec2 pos = aPos * uSize + uPosition;
                gl_Position = vec4(pos.x, pos.y, 0.0, 1.0);
                TexCoord = aTexCoord;
            }
        "#;

        let fragment_shader = r#"
            #version 330 core
            out vec4 FragColor;
            
            in vec2 TexCoord;
            
            uniform sampler2D uTexture;
            uniform float uOpacity;
            
            void main() {
                vec4 texColor = texture(uTexture, TexCoord);
                FragColor = vec4(texColor.rgb, texColor.a * uOpacity);
            }
        "#;

        unsafe {
            let vs = Self::compile_shader(vertex_shader, gl::VERTEX_SHADER)?;
            let fs = Self::compile_shader(fragment_shader, gl::FRAGMENT_SHADER)?;
            let program = Self::link_program(vs, fs)?;
            
            gl::DeleteShader(vs);
            gl::DeleteShader(fs);
            
            Ok(program)
        }
    }

    fn compile_shader(source: &str, shader_type: u32) -> Result<u32> {
        unsafe {
            let shader = gl::CreateShader(shader_type);
            let c_str = CString::new(source).unwrap();
            gl::ShaderSource(shader, 1, &c_str.as_ptr(), ptr::null());
            gl::CompileShader(shader);

            let mut success = 0;
            gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);
            if success == 0 {
                let mut len = 0;
                gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);
                let mut buffer = Vec::with_capacity(len as usize);
                buffer.set_len(len as usize);
                gl::GetShaderInfoLog(shader, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut _);
                let error = String::from_utf8_lossy(&buffer);
                gl::DeleteShader(shader);
                return Err(anyhow::anyhow!("Shader compilation failed: {}", error));
            }

            Ok(shader)
        }
    }

    fn link_program(vs: u32, fs: u32) -> Result<u32> {
        unsafe {
            let program = gl::CreateProgram();
            gl::AttachShader(program, vs);
            gl::AttachShader(program, fs);
            gl::LinkProgram(program);

            let mut success = 0;
            gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);
            if success == 0 {
                let mut len = 0;
                gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
                let mut buffer = Vec::with_capacity(len as usize);
                buffer.set_len(len as usize);
                gl::GetProgramInfoLog(program, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut _);
                let error = String::from_utf8_lossy(&buffer);
                gl::DeleteProgram(program);
                return Err(anyhow::anyhow!("Program linking failed: {}", error));
            }

            Ok(program)
        }
    }

    /// Create or update texture for a window using TFP
    /// 
    /// Returns the old X11 Pixmap ID if one was replaced, so it can be freed by the caller.
    pub fn update_window_pixmap(&mut self, ctx: &super::gl_context::GlContext, window_id: u32, x11_pixmap: u32, depth: u8) -> Result<Option<u32>> {
        unsafe {
            // Check if this appears to be the same pixmap (optimization)
            if let Some(win_tex) = self.textures.get(&window_id) {
                if let Some(existing_pixmap) = win_tex.x11_pixmap {
                    if existing_pixmap == x11_pixmap {
                        return Ok(None);
                    }
                }
            }

            // Transactional: Create new GLX pixmap FIRST
            trace!("Creating GLX pixmap for window {} (X11 pixmap {}, depth {})", window_id, x11_pixmap, depth);
            let new_glx_pixmap = match ctx.create_glx_pixmap(x11_pixmap, depth) {
                Ok(glx_pixmap) => {
                    trace!("Successfully created GLX pixmap {} for window {}", glx_pixmap, window_id);
                    glx_pixmap
                }
                Err(e) => {
                    warn!("Failed to create GLX pixmap for X11 pixmap {} (window {}, depth {}): {}", x11_pixmap, window_id, depth, e);
                    return Err(e).with_context(|| format!("Failed to create GLX pixmap for X11 pixmap {} (window {}, depth {})", x11_pixmap, window_id, depth));
                }
            };

            // Now update the state
            if let Some(win_tex) = self.textures.get_mut(&window_id) {
                // Release old resources if using TFP
                if let Some(old_glx) = win_tex.glx_pixmap {
                    ctx.release_tex_image(old_glx);
                    ctx.destroy_glx_pixmap(old_glx);
                }
                
                let old_x11 = win_tex.x11_pixmap;
                
                // DON'T bind here - we use strict binding mode (bind every frame)
                // Like compiz: if (!strictBinding) { bindTexImage } - we skip this since strictBinding=true
                trace!("Created GLX pixmap {} for existing texture {} for window {} (strict binding - will bind every frame)", new_glx_pixmap, win_tex.texture, window_id);

                win_tex.glx_pixmap = Some(new_glx_pixmap);
                win_tex.x11_pixmap = Some(x11_pixmap);
                trace!("Updated texture for window {} - glx_pixmap={:?}, texture={}", window_id, win_tex.glx_pixmap, win_tex.texture);
                
                Ok(old_x11)
            } else {
                // New texture
                let mut texture = 0;
                gl::GenTextures(1, &mut texture);
                
                gl::BindTexture(gl::TEXTURE_2D, texture);
                
                // TFP parameters
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
                
                // DON'T bind here - we use strict binding mode (bind every frame)
                // Like compiz: if (!strictBinding) { bindTexImage } - we skip this since strictBinding=true
                trace!("Created GLX pixmap {} for new texture {} for window {} (strict binding - will bind every frame)", new_glx_pixmap, texture, window_id);
                gl::BindTexture(gl::TEXTURE_2D, 0);

                // Get pixmap dimensions for tracking
                // We'll get this from the window geometry later if needed
                self.textures.insert(window_id, WindowTexture {
                    texture,
                    glx_pixmap: Some(new_glx_pixmap),
                    x11_pixmap: Some(x11_pixmap),
                });
                
                trace!("Inserted texture for window {} into HashMap - has_texture now returns: {}", window_id, self.has_texture(window_id));

                Ok(None)
            }
        }
    }

    /// Render a window with per-frame texture binding (like Compiz's strictBinding mode)
    pub fn render_window(
        &self,
        ctx: &super::gl_context::GlContext,
        window_id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        screen_width: f32,
        screen_height: f32,
        opacity: f32,
        damaged: bool, // Only bind texture if window is damaged
    ) {
        let win_tex = match self.textures.get(&window_id) {
            Some(t) => {
                debug!("Rendering window {} with texture {} (glx_pixmap={:?})", window_id, t.texture, t.glx_pixmap);
                t
            }
            None => {
                warn!("render_window called for window {} but no texture exists in HashMap!", window_id);
                return; // No texture yet
            }
        };

        unsafe {
            gl::UseProgram(self.program);

            // Convert X11 coordinates (top-left origin) to OpenGL coordinates (bottom-left origin, normalized)
            let x_gl = (x / screen_width as f32) * 2.0 - 1.0;
            let y_gl = 1.0 - ((y + height) / screen_height as f32) * 2.0;
            let width_gl = (width / screen_width as f32) * 2.0;
            let height_gl = (height / screen_height as f32) * 2.0;

            // Set uniforms
            let pos_loc = gl::GetUniformLocation(self.program, b"uPosition\0".as_ptr() as *const _);
            let size_loc = gl::GetUniformLocation(self.program, b"uSize\0".as_ptr() as *const _);
            let opacity_loc = gl::GetUniformLocation(self.program, b"uOpacity\0".as_ptr() as *const _);
            let tex_loc = gl::GetUniformLocation(self.program, b"uTexture\0".as_ptr() as *const _);

            gl::Uniform2f(pos_loc, x_gl, y_gl);
            gl::Uniform2f(size_loc, width_gl, height_gl);
            gl::Uniform1f(opacity_loc, opacity);
            gl::Uniform1i(tex_loc, 0);

            // Bind texture with per-frame TFP binding (strictBinding mode)
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, win_tex.texture);
            
            // CRITICAL: Bind the pixmap image ONLY when window is damaged (damage-based strict binding)
            // In strict binding mode, we bind when damage occurs to get the latest content
            // The X server updates the pixmap content when window is damaged, and binding makes it available to GL
            // We only bind when damaged=true (set by DamageNotify events), not every frame
            // This is the key to performance: only update textures when content actually changes
            if damaged {
                if let Some(glx_pixmap) = win_tex.glx_pixmap {
                    // Clear any previous X errors before binding
                    use std::sync::atomic::Ordering;
                    use super::gl_context::{X_ERROR_OCCURRED, X_ERROR_CODE};
                    X_ERROR_OCCURRED.store(false, Ordering::Relaxed);
                    X_ERROR_CODE.store(0, Ordering::Relaxed);
                    
                    // Bind the pixmap to the texture (this updates the texture with pixmap content)
                    // glXBindTexImageEXT replaces any existing binding
                    ctx.bind_tex_image(glx_pixmap);
                    
                    // Check for X errors after binding
                    if X_ERROR_OCCURRED.load(Ordering::Relaxed) {
                        let error_code = X_ERROR_CODE.load(Ordering::Relaxed);
                        warn!("X Error during bind_tex_image for window {} (glx_pixmap {}): code={}", window_id, glx_pixmap, error_code);
                        X_ERROR_OCCURRED.store(false, Ordering::Relaxed);
                    }
                }
            }

            // Render quad
            gl::BindVertexArray(self.vao);
            
            let vertices: [f32; 16] = [
                0.0, 0.0, 0.0, 1.0,
                1.0, 0.0, 1.0, 1.0,
                1.0, 1.0, 1.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
            ];

            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * std::mem::size_of::<f32>()) as isize,
                vertices.as_ptr() as *const _,
                gl::DYNAMIC_DRAW,
            );

            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
            
            // CRITICAL: Release the pixmap image AFTER drawing (like Compiz strictBinding)
            if let Some(glx_pixmap) = win_tex.glx_pixmap {
                ctx.release_tex_image(glx_pixmap);
            }
            
            gl::BindVertexArray(0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            
            // Check for OpenGL errors
            let err = gl::GetError();
            if err != gl::NO_ERROR {
                warn!("OpenGL error after rendering window {}: 0x{:x}", window_id, err);
            }
        }
    }


    /// Check if texture exists for window
    pub fn has_texture(&self, window_id: u32) -> bool {
        self.textures.contains_key(&window_id)
    }
    
    /// Render a window as a fallback (colored rectangle) when texture is not available
    /// This ensures windows are visible even if GLX pixmap creation failed
    pub fn render_window_fallback(
        &self,
        _ctx: &super::gl_context::GlContext,
        window_id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        screen_width: f32,
        screen_height: f32,
    ) {
        unsafe {
            gl::UseProgram(self.program);
            
            // Convert X11 coordinates (top-left origin) to OpenGL coordinates (bottom-left origin, normalized)
            // Position is top-left corner, size is width x height
            let x_gl = (x / screen_width) * 2.0 - 1.0;
            let y_gl = 1.0 - ((y + height) / screen_height) * 2.0; // Flip Y axis
            let width_gl = (width / screen_width) * 2.0;
            let height_gl = (height / screen_height) * 2.0;
            
            // Set uniforms
            let pos_loc = gl::GetUniformLocation(self.program, b"uPosition\0".as_ptr() as *const _);
            let size_loc = gl::GetUniformLocation(self.program, b"uSize\0".as_ptr() as *const _);
            let opacity_loc = gl::GetUniformLocation(self.program, b"uOpacity\0".as_ptr() as *const _);
            let tex_loc = gl::GetUniformLocation(self.program, b"uTexture\0".as_ptr() as *const _);
            
            gl::Uniform2f(pos_loc, x_gl, y_gl);
            gl::Uniform2f(size_loc, width_gl, height_gl);
            gl::Uniform1f(opacity_loc, 1.0); // Fully opaque - make windows clearly visible
            gl::Uniform1i(tex_loc, 0);
            
            // Use white texture or solid color
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            
            // Render quad using same method as render_window
            gl::BindVertexArray(self.vao);
            
            // Use same vertex format as render_window (position + texcoord)
            let vertices: [f32; 16] = [
                // Position      TexCoord
                0.0, 0.0,        0.0, 1.0, // Bottom-left
                1.0, 0.0,        1.0, 1.0, // Bottom-right
                1.0, 1.0,        1.0, 0.0, // Top-right
                0.0, 1.0,        0.0, 0.0, // Top-left
            ];
            
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * std::mem::size_of::<f32>()) as isize,
                vertices.as_ptr() as *const _,
                gl::DYNAMIC_DRAW,
            );
            
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
            
            gl::BindVertexArray(0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            
            // Check for OpenGL errors
            let err = gl::GetError();
            if err != gl::NO_ERROR {
                warn!("OpenGL error after rendering fallback window {}: 0x{:x}", window_id, err);
            }
        }
    }
    
    /// Render a colored rectangle (for shell UI)
    pub fn render_rectangle(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        screen_width: f32,
        screen_height: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
    ) {
        unsafe {
            gl::UseProgram(self.program);
            
            // Convert screen coordinates to OpenGL normalized coordinates
            let x_gl = (x / screen_width) * 2.0 - 1.0;
            let y_gl = 1.0 - ((y + height) / screen_height) * 2.0;
            let width_gl = (width / screen_width) * 2.0;
            let height_gl = (height / screen_height) * 2.0;
            
            // Set uniforms
            let pos_loc = gl::GetUniformLocation(self.program, b"uPosition\0".as_ptr() as *const _);
            let size_loc = gl::GetUniformLocation(self.program, b"uSize\0".as_ptr() as *const _);
            let opacity_loc = gl::GetUniformLocation(self.program, b"uOpacity\0".as_ptr() as *const _);
            let tex_loc = gl::GetUniformLocation(self.program, b"uTexture\0".as_ptr() as *const _);
            
            gl::Uniform2f(pos_loc, x_gl, y_gl);
            gl::Uniform2f(size_loc, width_gl, height_gl);
            gl::Uniform1f(opacity_loc, a);
            
            // Create a 1x1 colored texture for this specific color
            // TODO: Cache colored textures to avoid creating/deleting every frame
            let mut color_texture = 0;
            gl::GenTextures(1, &mut color_texture);
            gl::BindTexture(gl::TEXTURE_2D, color_texture);
            let color_pixel: [u8; 4] = [
                (r * 255.0) as u8,
                (g * 255.0) as u8,
                (b * 255.0) as u8,
                (a * 255.0) as u8,
            ];
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as i32,
                1,
                1,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                color_pixel.as_ptr() as *const _,
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            
            gl::ActiveTexture(gl::TEXTURE0);
            gl::Uniform1i(tex_loc, 0);
            
            // Render quad
            gl::BindVertexArray(self.vao);
            
            let vertices: [f32; 16] = [
                0.0, 0.0, 0.0, 1.0,
                1.0, 0.0, 1.0, 1.0,
                1.0, 1.0, 1.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
            ];
            
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * std::mem::size_of::<f32>()) as isize,
                vertices.as_ptr() as *const _,
                gl::DYNAMIC_DRAW,
            );
            
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
            gl::BindVertexArray(0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            
            // Clean up temporary texture
            gl::DeleteTextures(1, &color_texture);
        }
    }
    
    /// Update cursor texture from pixel data
    pub fn update_cursor_texture(
        &self,
        width: u16,
        height: u16,
        pixels: &[u32],
        texture_id: &mut Option<u32>,
    ) {
        unsafe {
            let mut tex_id = texture_id.unwrap_or(0);
            if tex_id == 0 {
                gl::GenTextures(1, &mut tex_id);
                *texture_id = Some(tex_id);
            }
            
            gl::BindTexture(gl::TEXTURE_2D, tex_id);
            
            // Upload pixel data (ARGB32 format from XFixes)
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as i32,
                width as i32,
                height as i32,
                0,
                gl::BGRA, // X11 ARGB32 is BGRA in OpenGL
                gl::UNSIGNED_BYTE,
                pixels.as_ptr() as *const _,
            );
            
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
    }
    
    /// Render cursor texture at specified position
    pub fn render_cursor(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        screen_width: f32,
        screen_height: f32,
        texture_id: Option<u32>,
    ) {
        unsafe {
            // Render the texture
            gl::UseProgram(self.program);
            
            // Convert screen coordinates to OpenGL normalized coordinates
            let x_gl = (x / screen_width) * 2.0 - 1.0;
            let y_gl = 1.0 - ((y + height) / screen_height) * 2.0;
            let width_gl = (width / screen_width) * 2.0;
            let height_gl = (height / screen_height) * 2.0;
            
            // Set uniforms
            let pos_loc = gl::GetUniformLocation(self.program, b"uPosition\0".as_ptr() as *const _);
            let size_loc = gl::GetUniformLocation(self.program, b"uSize\0".as_ptr() as *const _);
            let opacity_loc = gl::GetUniformLocation(self.program, b"uOpacity\0".as_ptr() as *const _);
            let tex_loc = gl::GetUniformLocation(self.program, b"uTexture\0".as_ptr() as *const _);
            
            gl::Uniform2f(pos_loc, x_gl, y_gl);
            gl::Uniform2f(size_loc, width_gl, height_gl);
            gl::Uniform1f(opacity_loc, 1.0);
            
            gl::ActiveTexture(gl::TEXTURE0);
            if let Some(tex_id) = texture_id {
                gl::BindTexture(gl::TEXTURE_2D, tex_id);
            }
            gl::Uniform1i(tex_loc, 0);
            
            // Render quad
            gl::BindVertexArray(self.vao);
            
            let vertices: [f32; 16] = [
                0.0, 0.0, 0.0, 1.0,
                1.0, 0.0, 1.0, 1.0,
                1.0, 1.0, 1.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
            ];
            
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * std::mem::size_of::<f32>()) as isize,
                vertices.as_ptr() as *const _,
                gl::DYNAMIC_DRAW,
            );
            
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
            gl::BindVertexArray(0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            // Clean up textures
            for win_tex in self.textures.values() {
                gl::DeleteTextures(1, &win_tex.texture);
            }
            gl::DeleteTextures(1, &self.white_texture);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.program);
        }
    }
}
