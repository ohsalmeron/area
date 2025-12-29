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
    /// Unresponsive windows (window -> timeout timestamp)
    pub unresponsive: std::collections::HashMap<u32, u32>,
    
    /// Windows pending WM_DELETE_WINDOW (window -> send timestamp)
    pub pending_delete: std::collections::HashMap<u32, u32>,
    
    /// Timeout for unresponsive detection (milliseconds)
    pub unresponsive_timeout_ms: u32,
}

impl TerminateManager {
    /// Create a new termination manager
    pub fn new() -> Self {
        Self {
            unresponsive: std::collections::HashMap::new(),
            pending_delete: std::collections::HashMap::new(),
            unresponsive_timeout_ms: 5000, // 5 seconds default
        }
    }
    
    /// Record that WM_DELETE_WINDOW was sent to a window
    pub fn record_delete_sent(&mut self, window: u32, timestamp: u32) {
        self.pending_delete.insert(window, timestamp);
        debug!("Recorded WM_DELETE_WINDOW sent to window {} at timestamp {}", window, timestamp);
    }
    
    /// Check if a window responded to WM_DELETE_WINDOW (window was destroyed/unmapped)
    pub fn check_delete_response(&mut self, window: u32, current_time: u32) -> bool {
        if self.pending_delete.contains_key(&window) {
            // Window responded (was destroyed/unmapped), remove from pending
            self.pending_delete.remove(&window);
            debug!("Window {} responded to WM_DELETE_WINDOW (removed from pending)", window);
            return true;
        }
        false
    }
    
    /// Check for unresponsive windows (timeout after WM_DELETE_WINDOW)
    pub fn check_unresponsive(&mut self, current_time: u32) -> Vec<u32> {
        let mut unresponsive_windows = Vec::new();
        let timeout_ms = self.unresponsive_timeout_ms;
        
        // Check pending delete windows for timeout
        let pending_windows: Vec<(u32, u32)> = self.pending_delete.iter()
            .map(|(&w, &t)| (w, t))
            .collect();
        
        for (window, send_time) in pending_windows {
            let elapsed = current_time.saturating_sub(send_time);
            if elapsed >= timeout_ms {
                // Window is unresponsive
                self.pending_delete.remove(&window);
                self.unresponsive.insert(window, current_time);
                unresponsive_windows.push(window);
                warn!("Window {} is unresponsive (no response to WM_DELETE_WINDOW after {}ms)", window, elapsed);
            }
        }
        
        unresponsive_windows
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
        
        // Show force quit dialog using external tool (zenity, kdialog, etc.)
        let dialog_tools = ["zenity", "kdialog", "yad"];
        let mut dialog_shown = false;
        
        for tool in &dialog_tools {
            // Check if tool exists in PATH
            if std::process::Command::new("which").arg(tool).output().is_ok() {
                // Spawn dialog asking user to force quit
                match std::process::Command::new(tool)
                    .arg("--question")
                    .arg("--text=Application is not responding. Force quit?")
                    .arg("--title=Force Quit")
                    .spawn()
                {
                    Ok(mut child) => {
                        // Wait for response
                        match child.wait() {
                            Ok(status) => {
                                if status.success() {
                                    debug!("User confirmed force quit for window {}", window);
                                    // Force kill the window
                                    self.force_kill(conn, window)?;
                                } else {
                                    debug!("User cancelled force quit for window {}", window);
                                }
                                dialog_shown = true;
                                break;
                            }
                            Err(e) => {
                                debug!("Failed to wait for {}: {}", tool, e);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Failed to spawn {}: {}", tool, e);
                    }
                }
            }
        }
        
        if !dialog_shown {
            warn!("Force quit dialog requested but no dialog tool available, force killing window {}", window);
            // Fallback: force kill without confirmation
            self.force_kill(conn, window)?;
        }
        
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




