//! Stacking Module
//!
//! Manages window z-order, layers, and stacking operations.
//! This matches xfwm4's stacking system.

use anyhow::Result;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::wm::client::Client;
use crate::wm::client_flags::WindowLayer;
use crate::wm::display::DisplayInfo;
use crate::wm::ewmh::Atoms;
use crate::wm::screen::ScreenInfo;

/// Stacking manager
pub struct StackingManager {
    /// Stacking order (bottom to top)
    pub stacking_order: Vec<u32>,
}

impl StackingManager {
    /// Create a new stacking manager
    pub fn new() -> Self {
        Self {
            stacking_order: Vec::new(),
        }
    }
    
    /// Raise a window to the top of its layer
    pub fn raise_window(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        window: u32,
        clients: &std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        debug!("Raising window {}", window);
        
        // Remove from stacking order
        self.stacking_order.retain(|&w| w != window);
        
        // Add to top
        self.stacking_order.push(window);
        
        // Apply X11 stacking
        conn.configure_window(
            window,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        
        // Update _NET_CLIENT_LIST_STACKING
        self.update_client_list_stacking(conn, display_info, screen_info, clients)?;
        
        Ok(())
    }
    
    /// Lower a window to the bottom of its layer
    pub fn lower_window(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        window: u32,
        clients: &std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        debug!("Lowering window {}", window);
        
        // Remove from stacking order
        self.stacking_order.retain(|&w| w != window);
        
        // Add to bottom
        self.stacking_order.insert(0, window);
        
        // Apply X11 stacking
        conn.configure_window(
            window,
            &ConfigureWindowAux::new().stack_mode(StackMode::BELOW),
        )?;
        
        // Update _NET_CLIENT_LIST_STACKING
        self.update_client_list_stacking(conn, display_info, screen_info, clients)?;
        
        Ok(())
    }
    
    /// Set window layer
    pub fn set_layer(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        client: &mut Client,
        layer: WindowLayer,
        clients: &std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        debug!("Setting layer {:?} for window {}", layer, client.window);
        
        let old_layer = client.win_layer;
        client.win_layer = layer;
        
        // Re-stack windows based on layers
        self.restack_by_layers(conn, display_info, screen_info, clients)?;
        
        // Update _NET_CLIENT_LIST_STACKING
        self.update_client_list_stacking(conn, display_info, screen_info, clients)?;
        
        Ok(())
    }
    
    /// Restack windows based on their layers
    pub fn restack_by_layers(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        clients: &std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        // Group windows by layer
        let mut by_layer: std::collections::BTreeMap<WindowLayer, Vec<u32>> = 
            std::collections::BTreeMap::new();
        
        for window_id in &self.stacking_order {
            if let Some(client) = clients.get(window_id) {
                by_layer
                    .entry(client.win_layer)
                    .or_insert_with(Vec::new)
                    .push(*window_id);
            }
        }
        
        // Build new stacking order (bottom to top, by layer)
        let mut new_order = Vec::new();
        for (_, windows) in by_layer.iter() {
            new_order.extend(windows.iter().rev()); // Reverse to maintain order within layer
        }
        
        self.stacking_order = new_order;
        
        // Apply X11 stacking
        if let Some(&bottom) = self.stacking_order.first() {
            // Stack all windows relative to bottom window
            for &window in self.stacking_order.iter().skip(1) {
                conn.configure_window(
                    window,
                    &ConfigureWindowAux::new()
                        .sibling(bottom)
                        .stack_mode(StackMode::ABOVE),
                )?;
            }
        }
        
        Ok(())
    }
    
    /// Update _NET_CLIENT_LIST_STACKING root property
    pub fn update_client_list_stacking(
        &self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        clients: &std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        // Build list in reverse stacking order (top to bottom)
        let mut client_list: Vec<u32> = self.stacking_order.iter().rev().copied().collect();
        
        // Filter to only include mapped windows
        client_list.retain(|&w| {
            clients.get(&w).map(|c| c.mapped()).unwrap_or(false)
        });
        
        // Set root property
        conn.change_property32(
            PropMode::REPLACE,
            screen_info.root,
            display_info.atoms.net_client_list,
            AtomEnum::WINDOW,
            &client_list,
        )?;
        
        debug!("Updated _NET_CLIENT_LIST_STACKING with {} windows", client_list.len());
        
        Ok(())
    }
    
    /// Add window to stacking order
    pub fn add_window(&mut self, window: u32) {
        if !self.stacking_order.contains(&window) {
            self.stacking_order.push(window);
        }
    }
    
    /// Remove window from stacking order
    pub fn remove_window(&mut self, window: u32) {
        self.stacking_order.retain(|&w| w != window);
    }
    
    /// Get stacking order (bottom to top)
    pub fn get_stacking_order(&self) -> &[u32] {
        &self.stacking_order
    }
}

impl Default for StackingManager {
    fn default() -> Self {
        Self::new()
    }
}



