//! X11 Pixel Grabbing Utility for Area Shell

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use x11rb::protocol::xproto::{ConnectionExt, ImageFormat};
use x11rb::rust_connection::RustConnection;

#[derive(Resource)]
pub struct X11Grabber {
    conn: RustConnection,
}

impl Default for X11Grabber {
    fn default() -> Self {
        // Use the DISPLAY environment variable, or default to :0
        let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
        let (conn, _) = RustConnection::connect(Some(&display)).expect("Failed to connect to X11");
        Self { conn }
    }
}

pub struct GrabPlugin;

impl Plugin for GrabPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<X11Grabber>();
    }
}

impl X11Grabber {
    /// Fetch window pixels and create/update a Bevy Image
    pub fn capture_window(&self, window_id: u32) -> Option<Image> {
        let geom = self.conn.get_geometry(window_id).ok()?.reply().ok()?;
        
        let width = geom.width;
        let height = geom.height;
        
        if width == 0 || height == 0 {
            return None;
        }

        // Fetch image from X server (this is slow, but compatible)
        // AllPlanes = !0
        let image_reply = self.conn.get_image(
            ImageFormat::Z_PIXMAP,
            window_id,
            0,
            0,
            width,
            height,
            !0,
        ).ok()?.reply().ok()?;

        let data = image_reply.data;

        // X11 usually returns BGRA or BGRX, we need RGBA for Bevy
        // Assuming 32-bit depth for now
        let mut rgba_data = Vec::with_capacity(data.len());
        
        // Simple distinct pixel copy (can be optimized later)
        if geom.depth == 24 || geom.depth == 32 {
            for chunk in data.chunks(4) {
                 if chunk.len() == 4 {
                    // B G R A -> R G B A
                    rgba_data.push(chunk[2]); // R
                    rgba_data.push(chunk[1]); // G
                    rgba_data.push(chunk[0]); // B
                    rgba_data.push(255);      // A (Force opaque for now)
                 }
            }
        } else {
            // Fallback for other depths if needed
            return None;
        }

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
