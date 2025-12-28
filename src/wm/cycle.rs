//! Cycle Module
//!
//! Window cycling (Alt+Tab) functionality.
//! This matches xfwm4's cycle system.

use anyhow::Result;
use tracing::{debug, info};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::wm::client::Client;
use crate::wm::display::DisplayInfo;
use crate::wm::focus::FocusManager;
use crate::wm::screen::ScreenInfo;

/// Cycle mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleMode {
    /// Cycle all windows
    All,
    /// Cycle only on current workspace
    CurrentWorkspace,
    /// Cycle by application group
    Group,
}

/// Cycle manager
pub struct CycleManager {
    /// Current cycle list
    pub cycle_list: Vec<u32>,
    
    /// Current cycle index
    pub cycle_index: usize,
    
    /// Cycle mode
    pub mode: CycleMode,
    
    /// Is cycling active?
    pub active: bool,
}

impl CycleManager {
    /// Create a new cycle manager
    pub fn new() -> Self {
        Self {
            cycle_list: Vec::new(),
            cycle_index: 0,
            mode: CycleMode::All,
            active: false,
        }
    }
    
    /// Start cycling
    pub fn start_cycle(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        focus_manager: &FocusManager,
        clients: &std::collections::HashMap<u32, Client>,
        mode: CycleMode,
    ) -> Result<()> {
        debug!("Starting window cycle (mode={:?})", mode);
        
        self.mode = mode;
        self.active = true;
        
        // Build cycle list based on mode
        self.build_cycle_list(focus_manager, clients, mode);
        
        if self.cycle_list.is_empty() {
            self.active = false;
            return Ok(());
        }
        
        // Start at first window (or next after current)
        if let Some(current) = focus_manager.get_focused_window() {
            if let Some(pos) = self.cycle_list.iter().position(|&w| w == current) {
                self.cycle_index = (pos + 1) % self.cycle_list.len();
            } else {
                self.cycle_index = 0;
            }
        } else {
            self.cycle_index = 0;
        }
        
        // Show cycle preview
        // TODO: Implement cycle preview window
        
        Ok(())
    }
    
    /// Cycle to next window
    pub fn cycle_next(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        focus_manager: &mut FocusManager,
        clients: &mut std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        if !self.active || self.cycle_list.is_empty() {
            return Ok(());
        }
        
        self.cycle_index = (self.cycle_index + 1) % self.cycle_list.len();
        
        if let Some(&window) = self.cycle_list.get(self.cycle_index) {
            if let Some(client) = clients.get_mut(&window) {
                focus_manager.set_focus(
                    conn,
                    display_info,
                    screen_info,
                    client,
                    crate::wm::focus::FocusSource::Other,
                )?;
            }
        }
        
        Ok(())
    }
    
    /// Cycle to previous window
    pub fn cycle_prev(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        focus_manager: &mut FocusManager,
        clients: &mut std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        if !self.active || self.cycle_list.is_empty() {
            return Ok(());
        }
        
        self.cycle_index = if self.cycle_index == 0 {
            self.cycle_list.len() - 1
        } else {
            self.cycle_index - 1
        };
        
        if let Some(&window) = self.cycle_list.get(self.cycle_index) {
            if let Some(client) = clients.get_mut(&window) {
                focus_manager.set_focus(
                    conn,
                    display_info,
                    screen_info,
                    client,
                    crate::wm::focus::FocusSource::Other,
                )?;
            }
        }
        
        Ok(())
    }
    
    /// Finish cycling
    pub fn finish_cycle(&mut self) {
        self.active = false;
        self.cycle_list.clear();
        self.cycle_index = 0;
    }
    
    /// Build cycle list
    fn build_cycle_list(
        &mut self,
        focus_manager: &FocusManager,
        clients: &std::collections::HashMap<u32, Client>,
        mode: CycleMode,
    ) {
        self.cycle_list.clear();
        
        match mode {
            CycleMode::All => {
                // All mapped windows
                for (window, client) in clients.iter() {
                    if client.mapped() {
                        self.cycle_list.push(*window);
                    }
                }
            }
            CycleMode::CurrentWorkspace => {
                // Only windows on current workspace
                // TODO: Filter by workspace
                for (window, client) in clients.iter() {
                    if client.mapped() {
                        self.cycle_list.push(*window);
                    }
                }
            }
            CycleMode::Group => {
                // Group by application
                // TODO: Group windows by application
                for (window, client) in clients.iter() {
                    if client.mapped() {
                        self.cycle_list.push(*window);
                    }
                }
            }
        }
        
        // Sort by focus history (most recent first)
        let history = focus_manager.get_focus_history();
        self.cycle_list.sort_by_key(|&w| {
            history.iter().position(|&h| h == w).unwrap_or(usize::MAX)
        });
    }
}

impl Default for CycleManager {
    fn default() -> Self {
        Self::new()
    }
}



