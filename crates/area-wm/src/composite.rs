//! Composite extension support for area-wm

use anyhow::{Context, Result};
use tracing::info;
use x11rb::connection::RequestConnection;
use x11rb::protocol::composite::{self, ConnectionExt as _, Redirect};
use x11rb::protocol::xproto::Window;
use x11rb::rust_connection::RustConnection;

pub struct CompositeManager {
    pub root: Window,
    pub overlay_window: Window,
}

impl CompositeManager {
    pub fn new(conn: &RustConnection, root: Window) -> Result<Self> {
        // Check if Composite extension is available
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

        // Redirect all subwindows of root to offscreen storage
        // Redirect::AUTOMATIC means the X server still paints them to the screen
        conn.composite_redirect_subwindows(root, Redirect::AUTOMATIC)?;
        info!("Redirected subwindows of root (Automatic)");

        // Get the Overlay Window (a special window for drawing on top of everything without redirection)
        // This is where we might draw debug info or where the shell might eventually sit if it was in-process
        let overlay_window = conn.composite_get_overlay_window(root)?.reply()?.overlay_win;
        
        // Map the overlay window so it's visible
        // (For area-shell, this might not be strictly necessary as area-shell has its own window,
        //  but standard composite managers usually claim the overlay).
        
        Ok(Self {
            root,
            overlay_window,
        })
    }

    pub fn cleanup(&self, conn: &RustConnection) -> Result<()> {
        info!("Releasing overlay window");
        conn.composite_release_overlay_window(self.overlay_window)?;
        Ok(())
    }
}
