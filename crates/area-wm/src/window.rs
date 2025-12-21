//! Window state management

use std::collections::HashMap;

use crate::decorations::WindowFrame;

/// Represents a managed window
#[derive(Debug, Clone)]
pub struct Window {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub class: String,
    pub workspace: u8,
    pub mapped: bool,
    pub sticky: bool,
    pub frame: Option<WindowFrame>,
    pub maximized: bool,
    // Restore geometry for unmaximize
    pub restore_x: i32,
    pub restore_y: i32,
    pub restore_width: u32,
    pub restore_height: u32,
    #[allow(dead_code)] // Reserved for future DRI3/damage tracking
    pub damage: Option<u32>,
    #[allow(dead_code)] // Reserved for future DRI3/damage tracking
    pub pixmap: Option<u32>,
}

impl Window {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            x: 0,
            y: 0,
            width: 640,
            height: 480,
            title: String::new(),
            class: String::new(),
            workspace: 0,
            mapped: false,
            sticky: false,
            frame: None,
            maximized: false,
            restore_x: 0,
            restore_y: 0,
            restore_width: 640,
            restore_height: 480,
            damage: None,
            pixmap: None,
        }
    }
}

/// Manages all windows tracked by the WM
#[derive(Debug, Default)]
pub struct WindowManager {
    windows: HashMap<u32, Window>,
    focused: Option<u32>,
    current_workspace: u8,
    num_workspaces: u8,
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            focused: None,
            current_workspace: 0,
            num_workspaces: 4,
        }
    }

    /// Add a new window
    pub fn add_window(&mut self, window: Window) {
        self.windows.insert(window.id, window);
    }

    /// Remove a window
    pub fn remove_window(&mut self, id: u32) -> Option<Window> {
        if self.focused == Some(id) {
            self.focused = None;
        }
        self.windows.remove(&id)
    }

    /// Get a window by ID
    pub fn get_window(&self, id: u32) -> Option<&Window> {
        self.windows.get(&id)
    }

    /// Get a mutable window by ID
    pub fn get_window_mut(&mut self, id: u32) -> Option<&mut Window> {
        self.windows.get_mut(&id)
    }

    /// Set focus to a window
    pub fn set_focus(&mut self, id: Option<u32>) {
        self.focused = id;
    }

    /// Get the currently focused window
    pub fn _get_focused(&self) -> Option<u32> {
        self.focused
    }

    /// Get all windows
    pub fn all_windows(&self) -> impl Iterator<Item = &Window> {
        self.windows.values()
    }

    /// Get windows on current workspace
    pub fn _visible_windows(&self) -> impl Iterator<Item = &Window> {
        let ws = self.current_workspace;
        self.windows.values().filter(move |w| w.workspace == ws && w.mapped)
    }

    /// Switch workspace
    pub fn switch_workspace(&mut self, workspace: u8) {
        if workspace < self.num_workspaces {
            self.current_workspace = workspace;
        }
    }

    /// Get current workspace
    pub fn current_workspace(&self) -> u8 {
        self.current_workspace
    }

    /// Get number of workspaces
    pub fn num_workspaces(&self) -> u8 {
        self.num_workspaces
    }

    /// Move window to workspace
    pub fn _move_to_workspace(&mut self, id: u32, workspace: u8) {
        if let Some(window) = self.windows.get_mut(&id) {
            window.workspace = workspace;
        }
    }
}
