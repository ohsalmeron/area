//! Startup Notification Module
//!
//! Startup notification tracking and spinning cursor.
//! This matches xfwm4's startup notification system.

use anyhow::Result;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::wm::client::Client;
use crate::wm::display::DisplayInfo;
use crate::wm::ewmh::Atoms;
use crate::wm::screen::ScreenInfo;

/// Startup notification
#[derive(Debug, Clone)]
pub struct StartupNotification {
    /// Startup ID
    pub startup_id: String,
    
    /// Window ID (if mapped)
    pub window: Option<u32>,
    
    /// Timestamp
    pub timestamp: u32,
    
    /// Is complete?
    pub complete: bool,
}

/// Startup notification manager
pub struct StartupNotificationManager {
    /// Active startup notifications
    pub notifications: HashMap<String, StartupNotification>,
    
    /// Default cursor (for spinning)
    pub busy_cursor: Option<u32>,
}

impl StartupNotificationManager {
    /// Create a new startup notification manager
    pub fn new() -> Self {
        Self {
            notifications: HashMap::new(),
            busy_cursor: None,
        }
    }
    
    /// Set busy cursor from DisplayInfo
    pub fn set_busy_cursor(&mut self, cursor_id: u32) {
        self.busy_cursor = Some(cursor_id);
    }
    
    /// Check if any startup notification is active
    pub fn has_active_startup(&self) -> bool {
        self.notifications.values().any(|n| !n.complete)
    }
    
    /// Get busy cursor ID (if available)
    pub fn get_busy_cursor(&self) -> Option<u32> {
        self.busy_cursor
    }
    
    /// Register a startup notification
    pub fn register_startup(
        &mut self,
        startup_id: String,
        timestamp: u32,
    ) {
        debug!("Registering startup notification: {}", startup_id);
        
        self.notifications.insert(startup_id.clone(), StartupNotification {
            startup_id: startup_id.clone(),
            window: None,
            timestamp,
            complete: false,
        });
    }
    
    /// Associate window with startup notification
    pub fn associate_window(
        &mut self,
        conn: &RustConnection,
        atoms: &Atoms,
        window: u32,
    ) -> Result<()> {
        // Get _NET_STARTUP_ID from window
        if let Ok(reply) = conn.get_property(
            false,
            window,
            atoms._net_startup_id,
            atoms._utf8_string,
            0,
            1024,
        )?.reply() {
            if let Ok(startup_id) = String::from_utf8(reply.value) {
                let startup_id = startup_id.trim_end_matches('\0').to_string();
                if !startup_id.is_empty() {
                    debug!("Associating window {} with startup notification: {}", window, startup_id);
                    // Associate window with existing notification or create new one
                    if let Some(notification) = self.notifications.get_mut(&startup_id) {
                        notification.window = Some(window);
                    } else {
                        // Create new notification for this window
                        self.notifications.insert(startup_id.clone(), StartupNotification {
                            startup_id: startup_id.clone(),
                            window: Some(window),
                            timestamp: x11rb::CURRENT_TIME,
                            complete: false,
                        });
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Mark startup as complete
    pub fn mark_complete(&mut self, startup_id: &str) {
        if let Some(notification) = self.notifications.get_mut(startup_id) {
            notification.complete = true;
            debug!("Startup notification {} marked as complete", startup_id);
        }
    }
    
    /// Remove startup notification
    pub fn remove_startup(&mut self, startup_id: &str) {
        self.notifications.remove(startup_id);
    }
    
    /// Check if window has active startup notification
    pub fn has_startup_for_window(&self, window: u32) -> bool {
        self.notifications.values().any(|n| n.window == Some(window) && !n.complete)
    }
    
    /// Mark startup complete for window
    pub fn mark_window_complete(&mut self, window: u32) {
        for notification in self.notifications.values_mut() {
            if notification.window == Some(window) && !notification.complete {
                notification.complete = true;
                debug!("Startup notification for window {} marked as complete", window);
            }
        }
    }
}

impl Default for StartupNotificationManager {
    fn default() -> Self {
        Self::new()
    }
}




