//! Panel client
//!
//! A Wayland layer-shell client that provides a panel/taskbar.

use tracing::info;

/// Panel application
pub struct Panel {
    // TODO: Store Wayland client state
    // TODO: Store layer shell surface
    // TODO: Store window list
    // TODO: Store system tray
}

impl Default for Panel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel {
    pub fn new() -> Self {
        info!("Initializing panel");
        Self {}
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        info!("Running panel");
        // TODO: Connect to Wayland display
        // TODO: Create layer shell surface
        // TODO: Set up window list
        // TODO: Set up system tray
        // TODO: Run event loop
        Ok(())
    }
}
