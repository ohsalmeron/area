//! Screen Module
//!
//! Manages per-screen state: window lists, workspaces, monitors, theme, work area.
//! This is the equivalent of xfwm4's ScreenInfo structure.

use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info};
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::shared::Geometry;
use crate::wm::client::Client;
use crate::wm::display::DisplayInfo;

/// Monitor/Output device information
#[derive(Debug, Clone)]
pub struct Monitor {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub name: String,
    pub primary: bool,
}

/// Desktop layout (EWMH)
#[derive(Debug, Clone, Copy)]
pub struct DesktopLayout {
    pub orientation: u32, // 0=horizontal, 1=vertical
    pub columns: u32,
    pub rows: u32,
    pub starting_corner: u32,
}

/// ScreenInfo - Per-screen window manager state
/// 
/// This is the equivalent of xfwm4's ScreenInfo structure.
/// Each screen (in multi-monitor setups) has its own ScreenInfo.
pub struct ScreenInfo {
    /// Reference to display info
    pub display_info: Arc<DisplayInfo>,
    
    /// Screen number
    pub screen_num: usize,
    
    /// Root window
    pub root: u32,
    
    /// X11 Screen
    pub xscreen: Screen,
    
    /// Screen width (all outputs combined)
    pub width: i32,
    
    /// Screen height (all outputs combined)
    pub height: i32,
    
    /// Screen depth
    pub depth: u8,
    
    /// Visual
    pub visual: Visualid,
    
    /// Window list (all managed windows)
    pub windows: Vec<Arc<Client>>,
    
    /// Window stacking list (reverse z-order - bottom to top)
    pub windows_stack: Vec<Arc<Client>>,
    
    /// Client linked list head
    pub clients: Option<Arc<Client>>,
    
    /// Number of clients
    pub client_count: u32,
    
    /// Client serial (for unique IDs)
    pub client_serial: u64,
    
    /// Current workspace
    pub current_ws: u32,
    
    /// Previous workspace
    pub previous_ws: u32,
    
    /// Number of workspaces
    pub workspace_count: u32,
    
    /// Workspace names
    pub workspace_names: Vec<String>,
    
    /// Desktop layout
    pub desktop_layout: DesktopLayout,
    
    /// Monitors/outputs
    pub monitors: Vec<Monitor>,
    
    /// Number of monitors
    pub num_monitors: usize,
    
    /// Work area (screen minus struts/panels)
    pub work_area: Geometry,
    
    /// Margins
    pub margins: [i32; 4], // left, right, top, bottom
    
    /// Last raised window (for focus tracking)
    pub last_raise: Option<Arc<Client>>,
}

impl ScreenInfo {
    /// Create a new ScreenInfo
    pub fn new(
        display_info: Arc<DisplayInfo>,
        screen_num: usize,
        root: u32,
        xscreen: Screen,
    ) -> Result<Self> {
        info!("Initializing ScreenInfo for screen {}", screen_num);
        
        let width = xscreen.width_in_pixels as i32;
        let height = xscreen.height_in_pixels as i32;
        let depth = xscreen.root_depth;
        let visual = xscreen.root_visual;
        
        // Initialize with default workspace count (4, like xfwm4)
        let workspace_count = 4;
        let workspace_names = (0..workspace_count)
            .map(|i| format!("Workspace {}", i + 1))
            .collect();
        
        // Default desktop layout (horizontal, 2x2)
        let desktop_layout = DesktopLayout {
            orientation: 0, // horizontal
            columns: 2,
            rows: 2,
            starting_corner: 0,
        };
        
        // Detect monitors (simplified - full implementation uses XRandR)
        let monitors = vec![Monitor {
            x: 0,
            y: 0,
            width: width as u32,
            height: height as u32,
            name: "Default".to_string(),
            primary: true,
        }];
        
        // Work area starts as full screen (struts will reduce it)
        let work_area = Geometry {
            x: 0,
            y: 0,
            width: width as u32,
            height: height as u32,
        };
        
        Ok(Self {
            display_info,
            screen_num,
            root,
            xscreen,
            width,
            height,
            depth,
            visual,
            windows: Vec::new(),
            windows_stack: Vec::new(),
            clients: None,
            client_count: 0,
            client_serial: 0,
            current_ws: 0,
            previous_ws: 0,
            workspace_count,
            workspace_names,
            desktop_layout,
            monitors,
            num_monitors: 1,
            work_area,
            margins: [0, 0, 0, 0],
            last_raise: None,
        })
    }
    
    /// Add a client to the screen
    pub fn add_client(&mut self, client: Arc<Client>) {
        self.windows.push(Arc::clone(&client));
        self.windows_stack.push(Arc::clone(&client));
        self.client_count += 1;
        self.client_serial += 1;
        
        // Update linked list
        // TODO: Implement proper linked list management
    }
    
    /// Remove a client from the screen
    pub fn remove_client(&mut self, window_id: u32) {
        self.windows.retain(|c| c.window != window_id);
        self.windows_stack.retain(|c| c.window != window_id);
        if self.client_count > 0 {
            self.client_count -= 1;
        }
        
        // Update linked list
        // TODO: Implement proper linked list management
    }
    
    /// Update work area (called when struts change)
    pub fn update_work_area(&mut self) {
        // Start with full screen
        let mut work_x = 0;
        let mut work_y = 0;
        let mut work_width = self.width as u32;
        let mut work_height = self.height as u32;
        
        // Apply margins
        work_x += self.margins[0]; // left
        work_y += self.margins[2]; // top
        work_width = work_width.saturating_sub((self.margins[0] + self.margins[1]) as u32); // left + right
        work_height = work_height.saturating_sub((self.margins[2] + self.margins[3]) as u32); // top + bottom
        
        // TODO: Apply struts from panels/docks
        
        self.work_area = Geometry {
            x: work_x,
            y: work_y,
            width: work_width,
            height: work_height,
        };
        
        debug!("Updated work area: {}x{} at ({}, {})", 
            work_width, work_height, work_x, work_y);
    }
    
    /// Find monitor at point
    pub fn find_monitor_at_point(&self, x: i32, y: i32) -> Option<&Monitor> {
        self.monitors.iter().find(|m| {
            x >= m.x && x < (m.x + m.width as i32) &&
            y >= m.y && y < (m.y + m.height as i32)
        })
    }
    
    /// Get primary monitor
    pub fn get_primary_monitor(&self) -> Option<&Monitor> {
        self.monitors.iter().find(|m| m.primary)
            .or_else(|| self.monitors.first())
    }
}

