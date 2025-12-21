//! OpenGL rendering backend

use anyhow::{Context, Result};
use std::ffi::CString;
use std::ptr;
use tracing::{debug, info, warn};

/// Texture resources for a window
struct WindowTexture {
    texture: u32,
    glx_pixmap: u32,
    x11_pixmap: u32,
}

/// OpenGL renderer for compositing windows
pub struct Renderer {
    program: u32,
    vao: u32,
    vbo: u32,
    textures: std::collections::HashMap<u32, WindowTexture>, // window_id -> WindowTexture
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

            Ok(Self {
                program,
                vao,
                vbo,
                textures: std::collections::HashMap::new(),
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
    /// Create or update texture for a window using TFP
    /// 
    /// Returns the old X11 Pixmap ID if one was replaced, so it can be freed by the caller.
    pub fn update_window_pixmap(&mut self, ctx: &crate::gl_context::GlContext, window_id: u32, x11_pixmap: u32, depth: u8) -> Result<Option<u32>> {
        unsafe {
            // Check if this appears to be the same pixmap (optimization)
            if let Some(win_tex) = self.textures.get(&window_id) {
                if win_tex.x11_pixmap == x11_pixmap {
                    return Ok(None);
                }
            }

            // Transactional: Create new GLX pixmap FIRST
            let new_glx_pixmap = ctx.create_glx_pixmap(x11_pixmap, depth)
                .with_context(|| format!("Failed to create GLX pixmap for X11 pixmap {} (window {}, depth {})", x11_pixmap, window_id, depth))?;

            // Now update the state
            if let Some(win_tex) = self.textures.get_mut(&window_id) {
                // Release old resources
                ctx.release_tex_image(win_tex.glx_pixmap);
                ctx.destroy_glx_pixmap(win_tex.glx_pixmap);
                
                let old_x11 = win_tex.x11_pixmap;
                
                // Bind new
                gl::BindTexture(gl::TEXTURE_2D, win_tex.texture);
                ctx.bind_tex_image(new_glx_pixmap);
                gl::BindTexture(gl::TEXTURE_2D, 0);

                win_tex.glx_pixmap = new_glx_pixmap;
                win_tex.x11_pixmap = x11_pixmap;
                
                Ok(Some(old_x11))
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
                
                ctx.bind_tex_image(new_glx_pixmap);
                gl::BindTexture(gl::TEXTURE_2D, 0);

                self.textures.insert(window_id, WindowTexture {
                    texture,
                    glx_pixmap: new_glx_pixmap,
                    x11_pixmap,
                });

                Ok(None)
            }
        }
    }

    /// Remove texture for a window
    /// 
    /// Returns the X11 Pixmap ID so it can be freed by the caller.
    pub fn remove_window_texture(&mut self, ctx: &crate::gl_context::GlContext, window_id: u32) -> Option<u32> {
        if let Some(win_tex) = self.textures.remove(&window_id) {
            ctx.release_tex_image(win_tex.glx_pixmap);
            ctx.destroy_glx_pixmap(win_tex.glx_pixmap);
            unsafe {
                gl::DeleteTextures(1, &win_tex.texture);
            }
            Some(win_tex.x11_pixmap)
        } else {
            None
        }
    }

    /// Render a window
    pub fn render_window(
        &self,
        window_id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        screen_width: u32,
        screen_height: u32,
        opacity: f32,
    ) {
        let texture = match self.textures.get(&window_id) {
            Some(t) => {
                debug!("Rendering window {} with texture ID {}", window_id, t.texture);
                t.texture
            },
            None => {
                debug!("No texture for window {} yet, skipping render", window_id);
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

            // Bind texture
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, texture);

            // Render quad
            gl::BindVertexArray(self.vao);
            
            // Quad vertices: position + texcoord
            // Note: GLX TFP texture coordinates might be inverted compared to Image?
            // Usually valid.
            let vertices: [f32; 16] = [
                // Position      TexCoord
                0.0, 0.0,        0.0, 1.0, // Bottom-left
                1.0, 0.0,        1.0, 1.0, // Bottom-right
                1.0, 1.0,        1.0, 0.0, // Top-right
                0.0, 1.0,        0.0, 0.0, // Top-left
            ];
            
            // Wait, standard GL coordinates have (0,0) at bottom-left.
            // X11 Image (0,0) is top-left.
            // If TFP binds directly, the texture orientation depends on implementation.
            // Usually we need to flip Y.
            // My vertex coords above:
            // BL(0,0) -> Eq to X11 BL?
            // If Textures are top-down (Stream), then 0,0 is top-left.
            // Let's stick to existing coords for now.

            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * std::mem::size_of::<f32>()) as isize,
                vertices.as_ptr() as *const _,
                gl::DYNAMIC_DRAW,
            );

            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
            gl::BindVertexArray(0);
            
            // Check for OpenGL errors
            let err = gl::GetError();
            if err != gl::NO_ERROR {
                warn!("OpenGL error after rendering window {}: 0x{:x}", window_id, err);
            }
            
            debug!("Drew quad for window {} at GL coords ({}, {}) size {}x{}", 
                   window_id, x_gl, y_gl, width_gl, height_gl);
        }
    }

    /// Check if texture exists for window
    pub fn has_texture(&self, window_id: u32) -> bool {
        self.textures.contains_key(&window_id)
    }

    /// Clear the screen
    pub fn clear(&self, r: f32, g: f32, b: f32, a: f32) {
        unsafe {
            gl::ClearColor(r, g, b, a);
            gl::Clear(gl::COLOR_BUFFER_BIT);
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
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.program);
        }
    }
}

