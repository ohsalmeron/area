//! Session Module
//!
//! Xfce session manager integration and state persistence.
//! This matches xfwm4's session management.

use anyhow::Result;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::rust_connection::RustConnection;

use crate::wm::client::Client;
use crate::wm::display::DisplayInfo;
use crate::wm::screen::ScreenInfo;

/// Session manager
pub struct SessionManager {
    /// Session client ID
    pub client_id: Option<String>,
    
    /// Session save path
    pub save_path: Option<String>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            client_id: None,
            save_path: None,
        }
    }
    
    /// Initialize session manager
    pub fn initialize(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
    ) -> Result<()> {
        debug!("Initializing session manager");
        
        // TODO: Connect to Xfce session manager
        // This would typically use libxfce4util and XfceSMClient
        
        Ok(())
    }
    
    /// Save window state
    pub fn save_state(
        &self,
        clients: &std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        debug!("Saving window state for {} windows", clients.len());
        
        // Get session file path (default to ~/.config/area/session.json)
        let session_dir = std::path::PathBuf::from(
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
        ).join(".config").join("area");
        
        // Create directory if it doesn't exist
        if let Err(e) = std::fs::create_dir_all(&session_dir) {
            warn!("Failed to create session directory {:?}: {}", session_dir, e);
            return Ok(()); // Non-fatal
        }
        
        let session_file = session_dir.join("session.json");
        
        // Build session data
        let session_data: serde_json::Value = serde_json::json!({
            "windows": clients.iter().map(|(window_id, client)| {
                serde_json::json!({
                    "window_id": window_id,
                    "geometry": {
                        "x": client.geometry.x,
                        "y": client.geometry.y,
                        "width": client.geometry.width,
                        "height": client.geometry.height,
                    },
                    "workspace": client.win_workspace,
                    "maximized": client.is_maximized(),
                    "minimized": client.is_minimized(),
                    "fullscreen": client.is_fullscreen(),
                    "name": client.name,
                    // Note: res_name and res_class are read from WM_CLASS in manage_window
                    // They're not stored in Client struct, so we'll match by name only for now
                })
            }).collect::<Vec<_>>(),
        });
        
        // Write to file
        if let Err(e) = std::fs::write(&session_file, serde_json::to_string_pretty(&session_data)?) {
            warn!("Failed to write session file {:?}: {}", session_file, e);
            return Ok(()); // Non-fatal
        }
        
        info!("Saved window state to {:?}", session_file);
        Ok(())
    }
    
    /// Restore window state
    pub fn restore_state(
        &self,
        _clients: &mut std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        debug!("Restoring window state");
        
        // Get session file path
        let session_file = std::path::PathBuf::from(
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
        ).join(".config").join("area").join("session.json");
        
        // Check if session file exists
        if !session_file.exists() {
            debug!("No session file found at {:?}, skipping restore", session_file);
            return Ok(());
        }
        
        // Read session file
        let contents = match std::fs::read_to_string(&session_file) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to read session file {:?}: {}", session_file, e);
                return Ok(());
            }
        };
        
        let session_data: serde_json::Value = match serde_json::from_str(&contents) {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to parse session file {:?}: {}", session_file, e);
                return Ok(());
            }
        };
        
        // Restore window states by matching windows by name/class
        if let Some(windows) = session_data.get("windows").and_then(|w| w.as_array()) {
            debug!("Found {} windows in session file", windows.len());
            
            for window_data in windows {
                if let Some(name) = window_data.get("name").and_then(|n| n.as_str()) {
                    // Try to find matching window in clients by name
                    // Note: res_name and res_class are not stored in Client struct,
                    // so we match by name only (can be enhanced to read WM_CLASS when needed)
                    let matched_client = _clients.values_mut().find(|client| {
                        // Match by name (case-insensitive)
                        client.name.to_lowercase() == name.to_lowercase()
                    });
                    
                    if let Some(client) = matched_client {
                        let class_info = client.class_hint.as_ref()
                            .map(|h| format!("{}/{}", h.res_name, h.res_class))
                            .unwrap_or_else(|| "unknown".to_string());
                        debug!("Restoring state for window {} (name: {}, class: {})", 
                            client.window, name, class_info);
                        
                        // Restore geometry
                        if let Some(geom) = window_data.get("geometry") {
                            if let (Some(x), Some(y), Some(w), Some(h)) = (
                                geom.get("x").and_then(|v| v.as_i64()),
                                geom.get("y").and_then(|v| v.as_i64()),
                                geom.get("width").and_then(|v| v.as_u64()),
                                geom.get("height").and_then(|v| v.as_u64()),
                            ) {
                                client.geometry.x = x as i32;
                                client.geometry.y = y as i32;
                                client.geometry.width = w as u32;
                                client.geometry.height = h as u32;
                            }
                        }
                        
                        // Restore workspace
                        if let Some(workspace) = window_data.get("workspace").and_then(|w| w.as_u64()) {
                            client.win_workspace = workspace as u32;
                        }
                        
                        // Restore state flags
                        if let Some(maximized) = window_data.get("maximized").and_then(|m| m.as_bool()) {
                            if maximized {
                                client.flags.insert(crate::wm::client_flags::ClientFlags::MAXIMIZED_VERT);
                                client.flags.insert(crate::wm::client_flags::ClientFlags::MAXIMIZED_HORIZ);
                            }
                        }
                        
                        if let Some(minimized) = window_data.get("minimized").and_then(|m| m.as_bool()) {
                            if minimized {
                                client.flags.insert(crate::wm::client_flags::ClientFlags::ICONIFIED);
                            }
                        }
                        
                        if let Some(fullscreen) = window_data.get("fullscreen").and_then(|f| f.as_bool()) {
                            if fullscreen {
                                client.flags.insert(crate::wm::client_flags::ClientFlags::FULLSCREEN);
                            }
                        }
                    } else {
                        debug!("No matching window found for name: {}", name);
                    }
                }
            }
        }
        
        debug!("Session restoration completed");
        Ok(())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}




