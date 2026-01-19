//! Foreign toplevel protocol
//!
//! Implements foreign toplevel management for taskbars and window switchers.

use tracing::debug;

/// Foreign toplevel manager
///
/// PURPOSE: Foreign toplevel protocol for taskbars. Will be used when panel client needs window list.
/// PLAN: Register toplevels with protocol, notify clients of window state changes.
#[allow(dead_code)]
pub struct ForeignToplevelManager {
    // TODO: Store foreign toplevel handles
}

impl Default for ForeignToplevelManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ForeignToplevelManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        debug!("Initializing foreign toplevel manager");
        Self {}
    }

    /// Register a toplevel
    #[allow(dead_code)]
    pub fn register_toplevel(&mut self) {
        debug!("Registering toplevel");
        // TODO: Register toplevel with protocol
    }
}
