//! Keyboard handling
//!
//! Processes keyboard events and keybind matching.

use tracing::debug;

/// Keyboard input handler
///
/// PURPOSE: Keyboard event processing and keybind matching. Will be used when input system is wired up.
/// PLAN: Connect to Seat's keyboard devices and process key events to match keybinds.
#[allow(dead_code)]
pub struct Keyboard {
    // TODO: Store keyboard state
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Keyboard {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }

    /// Handle a key event
    #[allow(dead_code)]
    pub fn handle_key(&mut self, key: u32, pressed: bool) {
        debug!("Key event: {} {}", key, if pressed { "pressed" } else { "released" });
        // TODO: Process key event
        // TODO: Match against keybinds
        // TODO: Execute actions
    }

    /// Check if a keybind matches
    #[allow(dead_code)]
    pub fn match_keybind(&self, _key: u32, _modifiers: u32) -> bool {
        // TODO: Implement keybind matching
        false
    }
}
