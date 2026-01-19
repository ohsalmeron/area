//! Desktop client
//!
//! A Wayland layer-shell client that provides desktop background and icons.

use tracing::info;

/// Desktop application
pub struct Desktop {
    // TODO: Store Wayland client state
    // TODO: Store layer shell surface
    // TODO: Store wallpaper
}

impl Default for Desktop {
    fn default() -> Self {
        Self::new()
    }
}

impl Desktop {
    pub fn new() -> Self {
        info!("Initializing desktop");
        Self {}
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        info!("Running desktop");
        // TODO: Connect to Wayland display
        // TODO: Create layer shell surface (background layer)
        // TODO: Load wallpaper
        // TODO: Set up desktop icons (optional)
        // TODO: Run event loop
        Ok(())
    }
}
