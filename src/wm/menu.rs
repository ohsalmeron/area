//! Menu Module
//!
//! Window menus and GTK_SHOW_WINDOW_MENU support.
//! This matches xfwm4's menu system.

use anyhow::Result;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::wm::client::Client;
use crate::wm::display::DisplayInfo;
use crate::wm::ewmh::Atoms;
use crate::wm::screen::ScreenInfo;

/// Menu action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Restore,
    Move,
    Resize,
    Minimize,
    Maximize,
    MaximizeHorz,
    MaximizeVert,
    Fullscreen,
    AlwaysOnTop,
    AlwaysOnBottom,
    RollUp,
    Unroll,
    Lower,
    Close,
    Workspace(u32),
    MoveToWorkspace(u32),
}

/// Menu manager
pub struct MenuManager {
    /// GTK_SHOW_WINDOW_MENU atom
    pub gtk_show_window_menu: u32,
}

impl MenuManager {
    /// Create a new menu manager
    pub fn new(conn: &RustConnection, atoms: &Atoms) -> Result<Self> {
        // Intern GTK_SHOW_WINDOW_MENU atom
        let gtk_show_window_menu = conn.intern_atom(false, b"GTK_SHOW_WINDOW_MENU")?
            .reply()?
            .atom;
        
        Ok(Self {
            gtk_show_window_menu,
        })
    }
    
    /// Handle GTK_SHOW_WINDOW_MENU client message
    pub fn handle_gtk_show_window_menu(
        &self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        window: u32,
        data: &[u32; 5],
        clients: &std::collections::HashMap<u32, Client>,
    ) -> Result<()> {
        let timestamp = data[0];
        let x = data[1] as i16;
        let y = data[2] as i16;
        
        debug!("GTK_SHOW_WINDOW_MENU: window={}, timestamp={}, x={}, y={}", 
            window, timestamp, x, y);
        
        if let Some(_client) = clients.get(&window) {
            // TODO: Show window menu at (x, y)
            debug!("Window menu requested for window {} at ({}, {})", window, x, y);
        } else {
            warn!("GTK_SHOW_WINDOW_MENU: window {} not found", window);
        }
        
        Ok(())
    }
    
    /// Show window menu
    pub fn show_menu(
        &self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        client: &Client,
        x: i16,
        y: i16,
    ) -> Result<()> {
        debug!("Showing window menu for window {} at ({}, {})", client.window, x, y);
        
        // TODO: Implement window menu display
        // This would typically use GTK or another UI toolkit
        
        Ok(())
    }
}



