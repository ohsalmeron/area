//! Transients Module
//!
//! Manages transient windows, modal dialogs, and window groups.
//! This matches xfwm4's transient window management.

use std::collections::HashMap;
use tracing::debug;

use crate::wm::client::Client;
use crate::wm::client_flags::ClientFlags;

/// Transient manager
pub struct TransientManager {
    /// Transient relationships (child -> parent)
    pub transients: std::collections::HashMap<u32, u32>,
}

impl TransientManager {
    /// Create a new transient manager
    pub fn new() -> Self {
        Self {
            transients: std::collections::HashMap::new(),
        }
    }
    
    /// Set transient relationship
    pub fn set_transient_for(
        &mut self,
        window: u32,
        transient_for: Option<u32>,
    ) {
        if let Some(parent) = transient_for {
            self.transients.insert(window, parent);
            debug!("Window {} is transient for {}", window, parent);
        } else {
            self.transients.remove(&window);
        }
    }
    
    /// Get transient parent
    pub fn get_transient_for(&self, window: u32) -> Option<u32> {
        self.transients.get(&window).copied()
    }
    
    /// Check if window is transient
    pub fn is_transient(&self, window: u32) -> bool {
        self.transients.contains_key(&window)
    }
    
    /// Check if window is modal
    pub fn is_modal(&self, client: &Client) -> bool {
        client.flags.contains(ClientFlags::STATE_MODAL) || 
        self.is_transient(client.window)
    }
    
    /// Get all transients for a window
    pub fn get_transients(&self, parent: u32) -> Vec<u32> {
        self.transients
            .iter()
            .filter_map(|(child, &p)| if p == parent { Some(*child) } else { None })
            .collect()
    }
    
    /// Remove transient relationship
    pub fn remove_transient(&mut self, window: u32) {
        self.transients.remove(&window);
    }
}

impl Default for TransientManager {
    fn default() -> Self {
        Self::new()
    }
}



