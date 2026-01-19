//! Pointer handling
//!
//! Manages cursor and pointer events.

use tracing::debug;

/// Pointer input handler
///
/// PURPOSE: Pointer/cursor event processing. Will be used when input system is wired up.
/// PLAN: Connect to Seat's pointer devices and handle motion, button, and scroll events.
#[allow(dead_code)]
pub struct Pointer {
    // TODO: Store pointer state
    // TODO: Store cursor position
}

impl Default for Pointer {
    fn default() -> Self {
        Self::new()
    }
}

impl Pointer {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }

    /// Handle pointer motion
    #[allow(dead_code)]
    pub fn handle_motion(&mut self, x: f64, y: f64) {
        debug!("Pointer motion: ({}, {})", x, y);
        // TODO: Update cursor position
        // TODO: Handle hover
    }

    /// Handle button press/release
    #[allow(dead_code)]
    pub fn handle_button(&mut self, button: u32, pressed: bool) {
        debug!("Button event: {} {}", button, if pressed { "pressed" } else { "released" });
        // TODO: Handle button actions
        // TODO: Trigger window operations (move, resize)
    }

    /// Handle scroll
    #[allow(dead_code)]
    pub fn handle_scroll(&mut self, _dx: f64, _dy: f64) {
        debug!("Scroll event");
        // TODO: Handle scroll
    }
}
