//! Terminate Module
//!
//! Force quit dialogs and unresponsive window handling.
//! This matches xfwm4's termination system.

use anyhow::Result;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::wm::client::Client;
use crate::wm::display::DisplayInfo;
use crate::wm::ewmh::Atoms;
use crate::wm::screen::ScreenInfo;

/// Termination manager
pub struct TerminateManager {
    /// Unresponsive windows (window -> timeout)
    pub unresponsive: std::collections::HashMap<u32, u32>,
}

impl TerminateManager {
    /// Create a new termination manager
    pub fn new() -> Self {
        Self {
            unresponsive: std::collections::HashMap::new(),
        }
    }
    
    /// Show force quit dialog
    pub fn show_force_quit_dialog(
        &self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        window: u32,
    ) -> Result<()> {
        debug!("Showing force quit dialog for window {}", window);
        
        // TODO: Show force quit dialog
        // This would typically use GTK or another UI toolkit
        
        Ok(())
    }
    
    /// Force kill a window
    pub fn force_kill(
        &self,
        conn: &RustConnection,
        window: u32,
    ) -> Result<()> {
        debug!("Force killing window {}", window);
        
        // Use XKillClient (via x11rb)
        // Note: x11rb doesn't have XKillClient directly, so we use KillClient request
        conn.kill_client(window)?;
        conn.flush()?;
        
        Ok(())
    }
    
    /// Check if window is unresponsive
    pub fn is_unresponsive(&self, window: u32) -> bool {
        self.unresponsive.contains_key(&window)
    }
    
    /// Mark window as unresponsive
    pub fn mark_unresponsive(&mut self, window: u32, timeout: u32) {
        self.unresponsive.insert(window, timeout);
    }
    
    /// Mark window as responsive
    pub fn mark_responsive(&mut self, window: u32) {
        self.unresponsive.remove(&window);
    }
}

impl Default for TerminateManager {
    fn default() -> Self {
        Self::new()
    }
}



