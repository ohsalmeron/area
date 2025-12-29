//! Workspace Module
//!
//! Manages virtual desktops/workspaces, workspace switching, and sticky windows.
//! This matches xfwm4's workspace management system.

use anyhow::Result;
use tracing::{debug, info, warn};
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::wm::client::Client;
use crate::wm::display::DisplayInfo;
use crate::wm::screen::ScreenInfo;

/// Workspace manager
pub struct WorkspaceManager {
    /// Current workspace index (0-based)
    pub current_workspace: u32,
    
    /// Number of workspaces
    pub workspace_count: u32,
    
    /// Workspace names
    pub workspace_names: Vec<String>,
    
    /// Desktop layout
    pub desktop_layout: DesktopLayout,
}

/// Desktop layout (EWMH _NET_DESKTOP_LAYOUT)
#[derive(Debug, Clone, Copy)]
pub struct DesktopLayout {
    pub orientation: u32, // 0=horizontal, 1=vertical
    pub columns: u32,
    pub rows: u32,
    pub starting_corner: u32,
}

/// Special workspace value for sticky windows (all workspaces)
pub const ALL_WORKSPACES: u32 = 0xFFFFFFFF;

impl WorkspaceManager {
    /// Create a new workspace manager
    pub fn new(workspace_count: u32) -> Self {
        let workspace_names = (0..workspace_count)
            .map(|i| format!("Workspace {}", i + 1))
            .collect();
        
        Self {
            current_workspace: 0,
            workspace_count,
            workspace_names,
            desktop_layout: DesktopLayout {
                orientation: 0, // horizontal
                columns: 2,
                rows: 2,
                starting_corner: 0,
            },
        }
    }
    
    /// Switch to a workspace
    pub fn switch_workspace(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        workspace: u32,
        clients: &mut std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        if workspace >= self.workspace_count {
            warn!("Invalid workspace index: {} (max: {})", workspace, self.workspace_count - 1);
            return Ok(());
        }
        
        if workspace == self.current_workspace {
            debug!("Already on workspace {}", workspace);
            return Ok(());
        }
        
        info!("Switching from workspace {} to {}", self.current_workspace, workspace);
        
        let old_workspace = self.current_workspace;
        self.current_workspace = workspace;
        
        // Show/hide windows based on workspace
        self.update_window_visibility(conn, clients, old_workspace, workspace)?;
        
        // Update EWMH properties
        self.update_ewmh_properties(conn, display_info, screen_info)?;
        
        Ok(())
    }
    
    /// Move window to a workspace (by window ID)
    pub fn move_window_to_workspace_by_id(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        window_id: u32,
        workspace: u32,
        transient_manager: &crate::wm::transients::TransientManager,
        clients: &mut std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        // Inline the logic to avoid borrow checker issues
        if workspace != ALL_WORKSPACES && workspace >= self.workspace_count {
            warn!("Invalid workspace index: {} (max: {})", workspace, self.workspace_count - 1);
            return Ok(());
        }
        
        let client = clients.get_mut(&window_id)
            .ok_or_else(|| anyhow::anyhow!("Window not found"))?;
        
        debug!("Moving window {} to workspace {}", client.window, workspace);
        
        client.win_workspace = workspace;
        
        // Update visibility if not on current workspace
        if workspace != ALL_WORKSPACES && workspace != self.current_workspace {
            if let Some(frame) = &client.frame {
                conn.unmap_window(frame.frame)?;
            } else {
                conn.unmap_window(client.window)?;
            }
        } else {
            if let Some(frame) = &client.frame {
                conn.map_window(frame.frame)?;
            } else {
                conn.map_window(client.window)?;
            }
        }
        
        // Update _NET_WM_DESKTOP
        conn.change_property32(
            PropMode::REPLACE,
            client.window,
            display_info.atoms.net_wm_desktop,
            AtomEnum::CARDINAL,
            &[workspace],
        )?;
        
        // Move transients with parent to the same workspace
        // Get transient IDs first to avoid borrow conflicts
        let transients: Vec<u32> = transient_manager.get_transients(client.window);
        drop(client); // Drop client borrow before accessing clients again
        
        for transient_id in transients {
            if let Some(transient_client) = clients.get_mut(&transient_id) {
                transient_client.win_workspace = workspace;
                
                if workspace != ALL_WORKSPACES && workspace != self.current_workspace {
                    if let Some(frame) = &transient_client.frame {
                        let _ = conn.unmap_window(frame.frame);
                    } else {
                        let _ = conn.unmap_window(transient_id);
                    }
                } else {
                    if let Some(frame) = &transient_client.frame {
                        let _ = conn.map_window(frame.frame);
                    } else {
                        let _ = conn.map_window(transient_id);
                    }
                }
                
                let _ = conn.change_property32(
                    PropMode::REPLACE,
                    transient_id,
                    display_info.atoms.net_wm_desktop,
                    AtomEnum::CARDINAL,
                    &[workspace],
                );
            }
        }
        
        Ok(())
    }
    
    /// Move window to a workspace
    pub fn move_window_to_workspace(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        client: &mut Client,
        workspace: u32,
        transient_manager: &crate::wm::transients::TransientManager,
        clients: &mut std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        if workspace != ALL_WORKSPACES && workspace >= self.workspace_count {
            warn!("Invalid workspace index: {} (max: {})", workspace, self.workspace_count - 1);
            return Ok(());
        }
        
        debug!("Moving window {} to workspace {}", client.window, workspace);
        
        client.win_workspace = workspace;
        
        // Update visibility if not on current workspace
        if workspace != ALL_WORKSPACES && workspace != self.current_workspace {
            // Hide window
            if let Some(frame) = &client.frame {
                conn.unmap_window(frame.frame)?;
            } else {
                conn.unmap_window(client.window)?;
            }
        } else {
            // Show window
            if let Some(frame) = &client.frame {
                conn.map_window(frame.frame)?;
            } else {
                conn.map_window(client.window)?;
            }
        }
        
        // Update _NET_WM_DESKTOP
        conn.change_property32(
            PropMode::REPLACE,
            client.window,
            display_info.atoms.net_wm_desktop,
            AtomEnum::CARDINAL,
            &[workspace],
        )?;
        
        // Move transients with parent to the same workspace
        let transients = transient_manager.get_transients(client.window);
        for transient_id in transients {
            if let Some(transient_client) = clients.get_mut(&transient_id) {
                // Move transient to same workspace as parent
                transient_client.win_workspace = workspace;
                
                // Update visibility
                if workspace != ALL_WORKSPACES && workspace != self.current_workspace {
                    // Hide transient
                    if let Some(frame) = &transient_client.frame {
                        let _ = conn.unmap_window(frame.frame);
                    } else {
                        let _ = conn.unmap_window(transient_id);
                    }
                } else {
                    // Show transient
                    if let Some(frame) = &transient_client.frame {
                        let _ = conn.map_window(frame.frame);
                    } else {
                        let _ = conn.map_window(transient_id);
                    }
                }
                
                // Update _NET_WM_DESKTOP for transient
                let _ = conn.change_property32(
                    PropMode::REPLACE,
                    transient_id,
                    display_info.atoms.net_wm_desktop,
                    AtomEnum::CARDINAL,
                    &[workspace],
                );
            }
        }
        
        Ok(())
    }
    
    /// Set workspace count
    pub fn set_workspace_count(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        count: u32,
    ) -> Result<()> {
        if count == 0 {
            warn!("Cannot set workspace count to 0");
            return Ok(());
        }
        
        info!("Setting workspace count to {}", count);
        
        // Adjust current workspace if needed
        if self.current_workspace >= count {
            self.current_workspace = count - 1;
        }
        
        // Update workspace names
        while self.workspace_names.len() < count as usize {
            let idx = self.workspace_names.len();
            self.workspace_names.push(format!("Workspace {}", idx + 1));
        }
        self.workspace_names.truncate(count as usize);
        
        self.workspace_count = count;
        
        // Update EWMH properties
        self.update_ewmh_properties(conn, display_info, screen_info)?;
        
        Ok(())
    }
    
    /// Update window visibility based on workspace
    fn update_window_visibility(
        &self,
        conn: &RustConnection,
        clients: &mut std::collections::HashMap<u32, Client>,
        old_workspace: u32,
        new_workspace: u32,
    ) -> Result<()> {
        for client in clients.values_mut() {
            let ws = client.win_workspace;
            
            // Sticky windows (ALL_WORKSPACES) are always visible
            if ws == ALL_WORKSPACES {
                continue;
            }
            
            // Hide windows from old workspace
            if ws == old_workspace {
                if let Some(frame) = &client.frame {
                    conn.unmap_window(frame.frame)?;
                } else {
                    conn.unmap_window(client.window)?;
                }
            }
            
            // Show windows for new workspace
            if ws == new_workspace {
                if let Some(frame) = &client.frame {
                    conn.map_window(frame.frame)?;
                } else {
                    conn.map_window(client.window)?;
                }
            }
        }
        
        Ok(())
    }
    
    /// Update EWMH workspace properties
    fn update_ewmh_properties(
        &self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
    ) -> Result<()> {
        // Update _NET_NUMBER_OF_DESKTOPS
        conn.change_property32(
            PropMode::REPLACE,
            screen_info.root,
            display_info.atoms.net_number_of_desktops,
            AtomEnum::CARDINAL,
            &[self.workspace_count],
        )?;
        
        // Update _NET_CURRENT_DESKTOP
        conn.change_property32(
            PropMode::REPLACE,
            screen_info.root,
            display_info.atoms.net_current_desktop,
            AtomEnum::CARDINAL,
            &[self.current_workspace],
        )?;
        
        // Update _NET_DESKTOP_NAMES
        let names: Vec<u8> = self.workspace_names
            .iter()
            .flat_map(|name| name.as_bytes().iter().copied().chain(std::iter::once(0)))
            .collect();
        
        conn.change_property8(
            PropMode::REPLACE,
            screen_info.root,
            display_info.atoms._net_desktop_names,
            display_info.atoms._utf8_string,
            &names,
        )?;
        
        Ok(())
    }
    
    /// Get current workspace
    pub fn get_current_workspace(&self) -> u32 {
        self.current_workspace
    }
    
    /// Get workspace count
    pub fn get_workspace_count(&self) -> u32 {
        self.workspace_count
    }
    
    /// Check if window is sticky (on all workspaces)
    pub fn is_sticky(&self, client: &Client) -> bool {
        client.win_workspace == ALL_WORKSPACES
    }
}



