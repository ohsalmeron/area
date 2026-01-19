//! Keybind system
//!
//! Handles keybind configuration and matching.

use tracing::debug;

/// A keybind represents a key combination and its action
///
/// PURPOSE: Keybind configuration. Will be used when keyboard input is processed.
/// PLAN: Load from config, match against keyboard events, execute window manager actions.
#[allow(dead_code)]
pub struct Keybind {
    /// Key code
    key: u32,
    /// Modifier mask
    modifiers: u32,
    /// Action to execute (as string for now)
    action: String,
}

impl Keybind {
    #[allow(dead_code)]
    pub fn new(key: u32, modifiers: u32, action: String) -> Self {
        Self {
            key,
            modifiers,
            action,
        }
    }

    /// Check if this keybind matches the given key and modifiers
    #[allow(dead_code)]
    pub fn matches(&self, key: u32, modifiers: u32) -> bool {
        self.key == key && self.modifiers == modifiers
    }

    /// Execute the keybind action
    #[allow(dead_code)]
    pub fn execute(&self) {
        debug!("Executing keybind action: {}", self.action);
        // TODO: Execute action
    }
}

/// Keybind manager
///
/// PURPOSE: Manages collection of keybinds. Will be used when keyboard input is processed.
/// PLAN: Load keybinds from config, match keyboard events against keybinds, execute actions.
#[allow(dead_code)]
pub struct KeybindManager {
    keybinds: Vec<Keybind>,
}

impl Default for KeybindManager {
    fn default() -> Self {
        Self::new()
    }
}

impl KeybindManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            keybinds: Vec::new(),
        }
    }

    /// Add a keybind
    #[allow(dead_code)]
    pub fn add_keybind(&mut self, keybind: Keybind) {
        self.keybinds.push(keybind);
    }

    /// Find a matching keybind
    #[allow(dead_code)]
    pub fn find_match(&self, key: u32, modifiers: u32) -> Option<&Keybind> {
        self.keybinds.iter().find(|k| k.matches(key, modifiers))
    }
}
