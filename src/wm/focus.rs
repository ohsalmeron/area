//! Focus Module
//!
//! Manages window focus, focus stealing prevention, and focus policies.
//! This matches xfwm4's focus management system.

use anyhow::Result;
use std::collections::VecDeque;
use tracing::{debug, warn};
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::wm::client::Client;
use crate::wm::client_flags::{ClientFlags, XfwmFlags};
use crate::wm::display::DisplayInfo;
use crate::wm::screen::ScreenInfo;

/// Focus policy (matches xfwm4 focus policies)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPolicy {
    /// Click to focus
    ClickToFocus,
    /// Focus follows mouse
    FocusFollowsMouse,
    /// Sloppy focus (focus on enter, unfocus on leave)
    SloppyFocus,
}

/// Source of focus request (for focus stealing prevention)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusSource {
    /// Application requested focus
    Application,
    /// Pager/panel requested focus
    Pager,
    /// Other source (user action, etc.)
    Other,
}

/// Focus manager
pub struct FocusManager {
    /// Currently focused window
    pub focused_window: Option<u32>,
    
    /// Focus history (for Alt+Tab cycling)
    pub focus_history: VecDeque<u32>,
    
    /// Maximum history size
    pub max_history_size: usize,
    
    /// Focus policy
    pub focus_policy: FocusPolicy,
    
    /// Focus stealing prevention enabled
    pub prevent_focus_stealing: bool,
    
    /// Last user interaction time
    pub last_user_time: u32,
    
    /// Focus stealing delay (milliseconds)
    pub focus_stealing_delay: u32,
}

impl FocusManager {
    /// Create a new focus manager
    pub fn new() -> Self {
        Self {
            focused_window: None,
            focus_history: VecDeque::new(),
            max_history_size: 20,
            focus_policy: FocusPolicy::ClickToFocus,
            prevent_focus_stealing: true,
            last_user_time: 0,
            focus_stealing_delay: 250, // 250ms default
        }
    }
    
    /// Set focus on a window
    pub fn set_focus(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        client: &mut Client,
        source: FocusSource,
    ) -> Result<()> {
        // Check input hint - windows with input: false should not receive focus
        if let Some(ref wm_hints) = client.wm_hints {
            // Check if InputHint flag is set and input is false
            const INPUT_HINT_FLAG: u32 = 1 << 0; // InputHint
            if (wm_hints.flags & INPUT_HINT_FLAG) != 0 && !wm_hints.input {
                debug!("Window {} has input: false, skipping focus", client.window);
                return Ok(());
            }
        }
        
        // Check if focus stealing is allowed
        if !self.focus_stealing_allowed(client, source) {
            debug!("Focus stealing prevented for window {}", client.window);
            return Ok(());
        }
        
        // Remove from history if already present
        self.focus_history.retain(|&w| w != client.window);
        
        // Add to front of history
        self.focus_history.push_front(client.window);
        
        // Limit history size
        while self.focus_history.len() > self.max_history_size {
            self.focus_history.pop_back();
        }
        
        // Update focused window
        if let Some(old_focused) = self.focused_window {
            if old_focused != client.window {
                // Unfocus old window
                self.remove_focus(conn, display_info, screen_info, old_focused)?;
            }
        }
        
        self.focused_window = Some(client.window);
        
        // Set X11 focus
        conn.set_input_focus(
            InputFocus::POINTER_ROOT,
            client.window,
            x11rb::CURRENT_TIME,
        )?;
        
        // Note: Window raising is handled by StackingManager in WindowManager::set_focus
        // This keeps stacking logic centralized
        
        // Update client flags
        client.set_focused(true);
        client.xfwm_flags.insert(XfwmFlags::FOCUS);
        
        // Update _NET_ACTIVE_WINDOW
        display_info.atoms.update_active_window(conn, screen_info.root, Some(client.window))?;
        
        debug!("Set focus on window {}", client.window);
        
        Ok(())
    }
    
    /// Remove focus from a window
    pub fn remove_focus(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        window: u32,
    ) -> Result<()> {
        if self.focused_window == Some(window) {
            self.focused_window = None;
            
            // Update _NET_ACTIVE_WINDOW (set to None)
            display_info.atoms.update_active_window(conn, screen_info.root, None)?;
            
            debug!("Removed focus from window {}", window);
        }
        
        Ok(())
    }
    
    /// Check if focus stealing is allowed
    pub fn focus_stealing_allowed(&self, client: &Client, source: FocusSource) -> bool {
        if !self.prevent_focus_stealing {
            return true;
        }
        
        // Always allow if user recently interacted
        let current_time = x11rb::CURRENT_TIME; // TODO: Get actual current time
        if current_time != 0 && self.last_user_time != 0 {
            let time_diff = current_time.saturating_sub(self.last_user_time);
            if time_diff < self.focus_stealing_delay {
                return true;
            }
        }
        
        // Allow if source is user action (pager, panel)
        match source {
            FocusSource::Pager | FocusSource::Other => return true,
            FocusSource::Application => {
                // Check if window demands attention
                if client.flags.contains(ClientFlags::DEMANDS_ATTENTION) {
                    return true;
                }
                
                // Check if window is modal
                if client.flags.contains(ClientFlags::STATE_MODAL) {
                    return true;
                }
                
                // Otherwise, prevent focus stealing
                return false;
            }
        }
    }
    
    /// Handle _NET_ACTIVE_WINDOW request
    pub fn handle_net_active_window(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        window: u32,
        source: FocusSource,
        current_time: u32,
        clients: &mut std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        debug!("_NET_ACTIVE_WINDOW request for window {} (source={:?})", window, source);
        
        if let Some(client) = clients.get_mut(&window) {
            // Check if window is on current workspace
            // TODO: Implement workspace switching if needed
            
            // Set focus
            self.set_focus(conn, display_info, screen_info, client, source)?;
        } else {
            warn!("_NET_ACTIVE_WINDOW: window {} not found", window);
        }
        
        Ok(())
    }
    
    /// Update last user interaction time
    pub fn update_user_time(&mut self, time: u32) {
        if time != 0 {
            self.last_user_time = time;
        }
    }
    
    /// Get focus history (for window cycling)
    pub fn get_focus_history(&self) -> &VecDeque<u32> {
        &self.focus_history
    }
    
    /// Get currently focused window
    pub fn get_focused_window(&self) -> Option<u32> {
        self.focused_window
    }
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}



