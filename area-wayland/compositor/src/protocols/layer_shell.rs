//! Layer shell protocol
//!
//! Implements wlr-layer-shell for panels and desktop components.

use tracing::debug;

/// Layer shell manager
///
/// PURPOSE: wlr-layer-shell protocol implementation. Will be used when panel/desktop clients connect.
/// PLAN: Handle layer surface creation, manage layer ordering, integrate with rendering.
#[allow(dead_code)]
pub struct LayerShellManager {
    // TODO: Store layer surfaces
}

impl Default for LayerShellManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LayerShellManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        debug!("Initializing layer shell");
        Self {}
    }

    /// Handle new layer surface
    #[allow(dead_code)]
    pub fn handle_new_layer_surface(&mut self) {
        debug!("New layer surface");
        // TODO: Handle layer surface creation
    }
}
