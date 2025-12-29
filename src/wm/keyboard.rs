//! Keyboard Module
//!
//! Keyboard shortcut management, key grabs, and Xfce shortcuts integration.
//! This matches xfwm4's keyboard system.

use anyhow::Result;
use std::collections::HashMap;
use tracing::debug;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

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
    /// Cycle windows (previous)
    CycleWindowsPrev,
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
    pub fn new(conn: &RustConnection, screen_info: &ScreenInfo) -> Result<Self> {
        // Get modifier mapping
        let mod_map = Self::get_modifier_map_internal(conn)?;
        
        let mut manager = Self {
            bindings: HashMap::new(),
            mod_map,
        };
        
        // Set up default bindings
        manager.setup_default_bindings(conn, screen_info)?;
        
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
    pub fn setup_default_bindings(
        &mut self,
        conn: &RustConnection,
        screen_info: &ScreenInfo,
    ) -> Result<()> {
        // Default xfwm4 bindings would go here
        debug!("Setting up default keyboard bindings");
        
        // Tab key is typically keycode 23 on most keyboards
        // Alt+Tab (Mod1 + Tab) - cycle windows forward
        let tab_keycode = 23u8;
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            self.mod_map.mod1, // Alt
            tab_keycode,
            KeyboardAction::CycleWindows,
        ) {
            debug!("Failed to add Alt+Tab binding: {}", e);
        }
        
        // Alt+Shift+Tab (Mod1 + Shift + Tab) - cycle windows backward
        let alt_shift = self.mod_map.mod1 | self.mod_map.shift;
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            alt_shift,
            tab_keycode,
            KeyboardAction::CycleWindowsPrev,
        ) {
            debug!("Failed to add Alt+Shift+Tab binding: {}", e);
        }
        
        // Workspace switching shortcuts (Ctrl+Alt+Arrow keys)
        // Arrow keys: Left=113, Right=114, Up=111, Down=116 (typical X11 keycodes)
        let ctrl_alt = self.mod_map.control | self.mod_map.mod1;
        let ctrl_alt_shift = ctrl_alt | self.mod_map.shift;
        
        // Ctrl+Alt+Left - Switch to previous workspace
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            ctrl_alt,
            113, // Left arrow
            KeyboardAction::SwitchWorkspace(0), // Will be handled as "previous" in handler
        ) {
            debug!("Failed to add Ctrl+Alt+Left binding: {}", e);
        }
        
        // Ctrl+Alt+Right - Switch to next workspace
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            ctrl_alt,
            114, // Right arrow
            KeyboardAction::SwitchWorkspace(1), // Will be handled as "next" in handler
        ) {
            debug!("Failed to add Ctrl+Alt+Right binding: {}", e);
        }
        
        // Ctrl+Alt+Up - Switch to workspace above (for vertical layouts)
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            ctrl_alt,
            111, // Up arrow
            KeyboardAction::SwitchWorkspace(2), // Will be handled as "up" in handler
        ) {
            debug!("Failed to add Ctrl+Alt+Up binding: {}", e);
        }
        
        // Ctrl+Alt+Down - Switch to workspace below (for vertical layouts)
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            ctrl_alt,
            116, // Down arrow
            KeyboardAction::SwitchWorkspace(3), // Will be handled as "down" in handler
        ) {
            debug!("Failed to add Ctrl+Alt+Down binding: {}", e);
        }
        
        // Window management shortcuts
        // Alt+F4 - Close window (F4 is typically keycode 70)
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            self.mod_map.mod1,
            70, // F4
            KeyboardAction::CloseWindow,
        ) {
            debug!("Failed to add Alt+F4 binding: {}", e);
        }
        
        // Alt+F10 - Maximize window (F10 is typically keycode 76)
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            self.mod_map.mod1,
            76, // F10
            KeyboardAction::MaximizeWindow,
        ) {
            debug!("Failed to add Alt+F10 binding: {}", e);
        }
        
        // Alt+F9 - Minimize window (F9 is typically keycode 75)
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            self.mod_map.mod1,
            75, // F9
            KeyboardAction::MinimizeWindow,
        ) {
            debug!("Failed to add Alt+F9 binding: {}", e);
        }
        
        // Alt+F7 - Move window (F7 is typically keycode 73)
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            self.mod_map.mod1,
            73, // F7
            KeyboardAction::MoveWindow,
        ) {
            debug!("Failed to add Alt+F7 binding: {}", e);
        }
        
        // Alt+F8 - Resize window (F8 is typically keycode 74)
        if let Err(e) = self.add_binding(
            conn,
            screen_info,
            self.mod_map.mod1,
            74, // F8
            KeyboardAction::ResizeWindow,
        ) {
            debug!("Failed to add Alt+F8 binding: {}", e);
        }
        
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

