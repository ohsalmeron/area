//! Wallpaper system for Area Desktop

use bevy::prelude::*;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

pub struct WallpaperPlugin;

impl Plugin for WallpaperPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_wallpaper);
    }
}

fn setup_wallpaper() {
    // Set wallpaper in background thread
    std::thread::spawn(|| {
        if let Err(e) = set_wallpaper() {
            error!("Failed to set wallpaper: {}", e);
        }
    });
}

fn set_wallpaper() -> anyhow::Result<()> {
    let display = std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into());
    let (conn, screen_num) = RustConnection::connect(Some(&display))?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;

    // Create a nice gradient-like background (purple/blue)
    let colormap = screen.default_colormap;
    let color = conn.alloc_color(colormap, 0x1000, 0x1000, 0x2000)?.reply()?;

    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().background_pixel(color.pixel),
    )?;
    conn.clear_area(false, root, 0, 0, 0, 0)?;
    conn.flush()?;

    info!("Desktop wallpaper set successfully");
    Ok(())
}
