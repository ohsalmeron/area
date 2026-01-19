//! Renderer implementation
//!
//! Handles actual rendering using GL or Pixman backend.

use tracing::debug;

/// Renderer handles actual rendering
///
/// PURPOSE: Actual rendering implementation. Will be used when rendering pipeline is implemented.
/// PLAN: Initialize GL or Pixman backend, render surfaces to outputs, handle frame callbacks.
#[allow(dead_code)]
pub struct Renderer {
    // TODO: Store renderer backend (GL or Pixman)
    // TODO: Store renderer state
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        debug!("Creating renderer");
        Self {}
    }

    /// Render a frame
    #[allow(dead_code)]
    pub fn render(&mut self) {
        debug!("Rendering frame");
        // TODO: Set up renderer
        // TODO: Render all surfaces
        // TODO: Handle frame callbacks
        // TODO: Present to output
    }

    /// Initialize renderer with GL backend
    #[allow(dead_code)]
    pub fn init_gl(&mut self) {
        debug!("Initializing GL renderer");
        // TODO: Initialize OpenGL renderer
    }

    /// Initialize renderer with Pixman backend
    #[allow(dead_code)]
    pub fn init_pixman(&mut self) {
        debug!("Initializing Pixman renderer");
        // TODO: Initialize Pixman renderer
    }
}
