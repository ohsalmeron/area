//! Keyboard Module
//!
//! Keyboard shortcut management, key grabs, and Xfce shortcuts integration.
//! This matches xfwm4's keyboard system.

use anyhow::Result;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::wm::display::DisplayInfo;
use crate::wm::screen::ScreenInfo;

/// Keyboard shortcut action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyboardAction {
    /// Close window
    CloseWindow,
    /// Maximize window
    MaximizeWindow,
    /// Restore window
    RestoreWindow,
    /// Minimize window
    MinimizeWindow,
    /// Move window
    MoveWindow,
    /// Resize window
    ResizeWindow,
    /// Raise window
    RaiseWindow,
    /// Lower window
    LowerWindow,
    /// Switch to workspace
    SwitchWorkspace(u32),
    /// Move window to workspace
    MoveToWorkspace(u32),
    /// Show window menu
    ShowWindowMenu,
    /// Cycle windows
    CycleWindows,
    /// Tile window left
    TileLeft,
    /// Tile window right
    TileRight,
}

/// Key binding
#[derive(Debug, Clone)]
pub struct KeyBinding {
    /// Modifier mask
    pub modifiers: u16,
    /// Keycode
    pub keycode: u8,
    /// Action
    pub action: KeyboardAction,
}

/// Keyboard manager
pub struct KeyboardManager {
    /// Key bindings
    pub bindings: HashMap<(u16, u8), KeyboardAction>,
    
    /// Modifier mapping
    pub mod_map: ModifierMap,
}

/// Modifier key mapping
#[derive(Debug, Clone)]
pub struct ModifierMap {
    pub mod1: u16,  // Alt
    pub mod4: u16,  // Super/Windows
    pub control: u16,
    pub shift: u16,
}

impl KeyboardManager {
    /// Create a new keyboard manager
    pub fn new(conn: &RustConnection) -> Result<Self> {
        // Get modifier mapping
        let mod_map = Self::get_modifier_map_internal(conn)?;
        
        let mut manager = Self {
            bindings: HashMap::new(),
            mod_map,
        };
        
        // Set up default bindings
        manager.setup_default_bindings()?;
        
        Ok(manager)
    }
    
    /// Get modifier key mapping (internal helper)
    fn get_modifier_map_internal(conn: &RustConnection) -> Result<ModifierMap> {
        let setup = conn.setup();
        
        // Find modifier keys (simplified - xfwm4 uses XkbGetModifierMap)
        let mod1 = 1 << 3; // Mod1 (Alt) - typically bit 3
        let mod4 = 1 << 6; // Mod4 (Super) - typically bit 6
        let control = 1 << 2; // Control - typically bit 2
        let shift = 1 << 0; // Shift - typically bit 0
        
        Ok(ModifierMap {
            mod1,
            mod4,
            control,
            shift,
        })
    }
    
    /// Set up default key bindings
    fn setup_default_bindings(&mut self) -> Result<()> {
        // Default xfwm4 bindings would go here
        // For now, just set up a few common ones
        debug!("Setting up default keyboard bindings");
        Ok(())
    }
    
    /// Add a key binding
    pub fn add_binding(
        &mut self,
        conn: &RustConnection,
        screen_info: &ScreenInfo,
        modifiers: u16,
        keycode: u8,
        action: KeyboardAction,
    ) -> Result<()> {
        debug!("Adding key binding: modifiers={:x}, keycode={}, action={:?}", 
            modifiers, keycode, action);
        
        // Grab key
        conn.grab_key(
            true,
            screen_info.root,
            ModMask::from(modifiers),
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )?;
        
        self.bindings.insert((modifiers, keycode), action);
        
        Ok(())
    }
    
    /// Remove a key binding
    pub fn remove_binding(
        &mut self,
        conn: &RustConnection,
        screen_info: &ScreenInfo,
        modifiers: u16,
        keycode: u8,
    ) -> Result<()> {
        debug!("Removing key binding: modifiers={:x}, keycode={}", modifiers, keycode);
        
        // Ungrab key
        conn.ungrab_key(keycode, screen_info.root, ModMask::from(modifiers))?;
        
        self.bindings.remove(&(modifiers, keycode));
        
        Ok(())
    }
    
    /// Handle key press
    pub fn handle_key_press(
        &self,
        modifiers: u16,
        keycode: u8,
    ) -> Option<KeyboardAction> {
        self.bindings.get(&(modifiers, keycode)).copied()
    }
    
    /// Get modifier map
    pub fn get_modifier_map(&self) -> &ModifierMap {
        &self.mod_map
    }
}

