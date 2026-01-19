//! Launcher client
//!
//! A Wayland XDG shell client that provides an application launcher.

use tracing::info;

/// Launcher application
pub struct Launcher {
    // TODO: Store Wayland client state
    // TODO: Store XDG shell surface
    // TODO: Store application list
}

impl Default for Launcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Launcher {
    pub fn new() -> Self {
        info!("Initializing launcher");
        Self {}
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        info!("Running launcher");
        // TODO: Connect to Wayland display
        // TODO: Create XDG shell window
        // TODO: Load desktop entries
        // TODO: Set up search/filter
        // TODO: Run event loop
        Ok(())
    }
}
