//! Composite extension support for area-wm

use anyhow::{Context, Result};
use tracing::info;
use x11rb::connection::RequestConnection;
use x11rb::protocol::composite::{self, ConnectionExt as _, Redirect};
use x11rb::protocol::xproto::Window;
use x11rb::rust_connection::RustConnection;

pub struct CompositeManager;

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
        // Redirect::MANUAL means we are responsible for painting them to the screen
        conn.composite_redirect_subwindows(root, Redirect::MANUAL)?;
        info!("Redirected subwindows of root (Manual)");

        // Get the Overlay Window (a special window for drawing on top of everything without redirection)
        // This is where we might draw debug info or where the shell might eventually sit if it was in-process
        // We claim it but don't need to store it since we're not using it currently
        let _overlay_window = conn.composite_get_overlay_window(root)?.reply()?.overlay_win;
        
        Ok(Self)
    }

}
