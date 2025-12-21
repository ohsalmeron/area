//! Window state tracking for compositor

use std::collections::HashMap;

/// Information about a composited window
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub class: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub focused: bool,
    #[allow(dead_code)]
    pub texture_handle: Option<u32>, // OpenGL texture ID
    pub pixmap_dirty: bool, // Needs new pixmap (e.g. resize)
}

/// Window state manager
pub struct WindowState {
    windows: HashMap<u32, WindowInfo>,
    z_order: Vec<u32>, // Window IDs in rendering order (back to front)
}

impl WindowState {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            z_order: Vec::new(),
        }
    }

    /// Add or update a window
    pub fn update_window(&mut self, id: u32, title: String, class: String, x: i32, y: i32, width: u32, height: u32) {
        if let Some(win) = self.windows.get_mut(&id) {
            // Check if size changed
            if win.width != width || win.height != height {
                win.pixmap_dirty = true;
            }
            win.title = title;
            win.class = class;
            win.x = x;
            win.y = y;
            win.width = width;
            win.height = height;
        } else {
            // New window - add to back of Z-order
            self.z_order.push(id);
            self.windows.insert(id, WindowInfo {
                id,
                title,
                class,
                x,
                y,
                width,
                height,
                focused: false,
                texture_handle: None,
                pixmap_dirty: true,
            });
        }
    }

    /// Clear pixmap dirty flag
    pub fn clear_pixmap_dirty(&mut self, id: u32) {
        if let Some(win) = self.windows.get_mut(&id) {
            win.pixmap_dirty = false;
        }
    }

    /// Remove a window
    pub fn remove_window(&mut self, id: u32) {
        self.windows.remove(&id);
        self.z_order.retain(|&wid| wid != id);
    }

    /// Set window focus
    pub fn set_focus(&mut self, id: Option<u32>) {
        // Clear all focus
        for win in self.windows.values_mut() {
            win.focused = false;
        }
        
        // Set focus on specified window and move to front of Z-order
        if let Some(focused_id) = id {
            if let Some(win) = self.windows.get_mut(&focused_id) {
                win.focused = true;
            }
            // Move to front of Z-order
            self.z_order.retain(|&wid| wid != focused_id);
            self.z_order.push(focused_id);
        }
    }

    /// Get window info
    pub fn get_window(&self, id: u32) -> Option<&WindowInfo> {
        self.windows.get(&id)
    }

    /// Get all windows in Z-order (back to front)
    pub fn windows_in_order(&self) -> Vec<&WindowInfo> {
        self.z_order
            .iter()
            .filter_map(|id| self.windows.get(id))
            .collect()
    }

    /// Get all window IDs
    pub fn window_ids(&self) -> Vec<u32> {
        self.windows.keys().copied().collect()
    }
}

