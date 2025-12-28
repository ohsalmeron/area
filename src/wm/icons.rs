//! Icons Module
//!
//! Window icon loading and caching.
//! This matches xfwm4's icon system.

use anyhow::Result;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::wm::ewmh::Atoms;

/// Icon data
#[derive(Debug, Clone)]
pub struct IconData {
    /// Icon width
    pub width: u32,
    /// Icon height
    pub height: u32,
    /// Icon pixels (ARGB32 format)
    pub pixels: Vec<u32>,
}

/// Icon manager
pub struct IconManager {
    /// Icon cache (window -> icon data)
    pub icon_cache: HashMap<u32, IconData>,
    
    /// Default icon
    pub default_icon: Option<IconData>,
}

impl IconManager {
    /// Create a new icon manager
    pub fn new() -> Self {
        Self {
            icon_cache: HashMap::new(),
            default_icon: None,
        }
    }
    
    /// Load icon for a window
    pub fn load_icon(
        &mut self,
        conn: &RustConnection,
        atoms: &Atoms,
        window: u32,
    ) -> Result<Option<IconData>> {
        // Try _NET_WM_ICON first
        if let Ok(reply) = conn.get_property(
            false,
            window,
            atoms._net_wm_pid, // Use _NET_WM_ICON if available
            AtomEnum::CARDINAL,
            0,
            1024,
        )?.reply() {
            // TODO: Parse _NET_WM_ICON format
            // Format: width, height, pixels...
            debug!("Loading icon for window {} (not yet fully implemented)", window);
        }
        
        // Try KWM_WIN_ICON (legacy)
        // TODO: Implement KWM_WIN_ICON loading
        
        Ok(None)
    }
    
    /// Get icon for a window (from cache or load)
    pub fn get_icon(
        &mut self,
        conn: &RustConnection,
        atoms: &Atoms,
        window: u32,
    ) -> Result<Option<&IconData>> {
        if !self.icon_cache.contains_key(&window) {
            if let Some(icon) = self.load_icon(conn, atoms, window)? {
                self.icon_cache.insert(window, icon);
            }
        }
        
        Ok(self.icon_cache.get(&window).or(self.default_icon.as_ref()))
    }
    
    /// Clear icon cache
    pub fn clear_cache(&mut self) {
        self.icon_cache.clear();
    }
    
    /// Remove icon from cache
    pub fn remove_icon(&mut self, window: u32) {
        self.icon_cache.remove(&window);
    }
}

impl Default for IconManager {
    fn default() -> Self {
        Self::new()
    }
}



