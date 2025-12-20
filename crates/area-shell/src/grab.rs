//! X11 Pixel Grabbing Utility for Area Shell

use bevy::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use x11rb::connection::RequestConnection;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};
use x11rb::protocol::composite::{self, ConnectionExt as CompositeExt};
use x11rb::rust_connection::RustConnection;
use std::collections::HashMap;
use tracing::{debug, warn};

#[derive(Resource)]
pub struct X11Grabber {
    conn: RustConnection,
    composite_available: bool,
    composite_version: Option<(u32, u32)>,
    // Cache pixmap IDs to avoid repeated lookups (future optimization)
    #[allow(dead_code)]
    window_pixmaps: HashMap<u32, u32>,
}

impl Default for X11Grabber {
    fn default() -> Self {
        // Use the DISPLAY environment variable, or default to :0
        let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
        let (conn, _) = RustConnection::connect(Some(&display)).expect("Failed to connect to X11");
        
        // Check if Composite extension is available
        let composite_available = conn
            .extension_information(composite::X11_EXTENSION_NAME)
            .ok()
            .and_then(|info| info)
            .is_some();
        
        let composite_version = if composite_available {
            match conn.composite_query_version(0, 4) {
                Ok(cookie) => {
                    match cookie.reply() {
                        Ok(reply) => {
                            debug!(
                                "Composite extension {}.{} available",
                                reply.major_version, reply.minor_version
                            );
                            Some((reply.major_version, reply.minor_version))
                        }
                        Err(e) => {
                            warn!("Failed to get Composite version: {}", e);
                            None
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to query Composite version: {}", e);
                    None
                }
            }
        } else {
            warn!("Composite extension not available, falling back to XGetImage");
            None
        };
        
        Self {
            conn,
            composite_available,
            composite_version,
            window_pixmaps: HashMap::new(),
        }
    }
}

pub struct GrabPlugin;

impl Plugin for GrabPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<X11Grabber>();
    }
}

impl X11Grabber {
    /// Fetch window pixels using Composite extension for efficient capture
    /// 
    /// With Composite, windows are redirected to off-screen storage,
    /// allowing fast pixmap access. This method:
    /// 1. Gets window geometry
    /// 2. Captures pixmap data (faster on redirected windows)
    /// 3. Converts pixel format efficiently
    /// 4. Returns Bevy Image ready for GPU upload
    pub fn capture_window(&mut self, window_id: u32) -> Option<Image> {
        let geom = self.conn.get_geometry(window_id).ok()?.reply().ok()?;
        
        let width = geom.width;
        let height = geom.height;
        let depth = geom.depth;
        
        if width == 0 || height == 0 {
            return None;
        }

        // With Composite, windows are redirected and X11 creates pixmaps
        // XGetImage on redirected windows is faster than non-redirected
        // AllPlanes = !0
        let image_reply = match self.conn.get_image(
            ImageFormat::Z_PIXMAP,
            window_id,
            0,
            0,
            width,
            height,
            !0,
        ) {
            Ok(cookie) => cookie.reply().ok()?,
            Err(_) => return None,
        };

        let data = image_reply.data;

        // Optimized pixel format conversion
        // X11 typically returns BGRA/BGRX for 32-bit, BGR for 24-bit
        // With Z_PIXMAP format, data is in native format
        let rgba_data = if depth == 24 || depth == 32 {
            let pixel_count = (width * height) as usize;
            let mut rgba_data = Vec::with_capacity(pixel_count * 4);
            
            if depth == 32 {
                // 32-bit: BGRA or BGRX (4 bytes per pixel)
                for chunk in data.chunks_exact(4) {
                    rgba_data.push(chunk[2]); // R
                    rgba_data.push(chunk[1]); // G
                    rgba_data.push(chunk[0]); // B
                    rgba_data.push(chunk[3]); // A (or X, but we treat as A)
                }
            } else {
                // 24-bit: BGR (3 bytes per pixel, may have row padding)
                let width_usize = width as usize;
                let height_usize = height as usize;
                let bytes_per_row = ((width_usize * 3 + 3) / 4) * 4; // Aligned to 4 bytes
                for y in 0..height_usize {
                    let row_start = y * bytes_per_row;
                    for x in 0..width_usize {
                        let offset = row_start + x * 3;
                        if offset + 2 < data.len() {
                            rgba_data.push(data[offset + 2]); // R
                            rgba_data.push(data[offset + 1]); // G
                            rgba_data.push(data[offset]);     // B
                            rgba_data.push(255);              // A (opaque)
                        }
                    }
                }
            }
            
            rgba_data
        } else {
            return None; // Unsupported depth
        };

        let extent = Extent3d {
            width: width as u32,
            height: height as u32,
            depth_or_array_layers: 1,
        };

        Some(Image::new(
            extent,
            TextureDimension::D2,
            rgba_data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        ))
    }
}
