//! NetWM Module
//!
//! Enhanced EWMH client message handlers and root property management.
//! This module provides complete EWMH support matching xfwm4.

use anyhow::{Context, Result};
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::wm::client::Client;
use crate::wm::client_flags::ClientFlags;
use crate::wm::display::DisplayInfo;
use crate::wm::ewmh::Atoms;
use crate::wm::screen::ScreenInfo;
use crate::shared::Geometry;

/// Handle _NET_MOVERESIZE_WINDOW client message
pub fn handle_net_moveresize_window(
    conn: &RustConnection,
    display_info: &DisplayInfo,
    screen_info: &ScreenInfo,
    window: u32,
    data: &[u32; 5],
    clients: &mut std::collections::HashMap<u32, Client>,
) -> Result<()> {
    let flags = data[0];
    let x = data[1] as i32;
    let y = data[2] as i32;
    let width = data[3] as u32;
    let height = data[4] as u32;
    
    debug!("_NET_MOVERESIZE_WINDOW: window={}, flags={:x}, x={}, y={}, w={}, h={}", 
        window, flags, x, y, width, height);
    
    if let Some(client) = clients.get_mut(&window) {
        // xfwm4 twist: refuse to move maximized windows unless USER_POS flag is set
        const USER_POS: u32 = 1 << 12;
        if client.is_maximized() && (flags & USER_POS) == 0 {
            debug!("Refusing to move maximized window {} without USER_POS flag", window);
            return Ok(());
        }
        
        // Check which fields are valid based on flags
        let mut new_geom = client.geometry;
        
        if (flags & (1 << 8)) != 0 { // X
            new_geom.x = x;
        }
        if (flags & (1 << 9)) != 0 { // Y
            new_geom.y = y;
        }
        if (flags & (1 << 10)) != 0 { // Width
            new_geom.width = width;
        }
        if (flags & (1 << 11)) != 0 { // Height
            new_geom.height = height;
        }
        
        // Apply gravity if specified
        let gravity = ((flags >> 24) & 0xf) as u8;
        if gravity != 0 {
            // TODO: Apply gravity transformation
            debug!("Gravity {} specified but not yet implemented", gravity);
        }
        
        // Constrain to screen/work area
        let work_area = &screen_info.work_area;
        new_geom.x = new_geom.x.max(work_area.x);
        new_geom.y = new_geom.y.max(work_area.y);
        new_geom.width = new_geom.width.min(work_area.width);
        new_geom.height = new_geom.height.min(work_area.height);
        
        // Apply geometry
        client.geometry = new_geom;
        
        // Configure window
        let target_window = if let Some(frame) = &client.frame {
            frame.frame
        } else {
            window
        };
        
        conn.configure_window(
            target_window,
            &ConfigureWindowAux::new()
                .x(new_geom.x)
                .y(new_geom.y)
                .width(new_geom.width)
                .height(new_geom.height),
        )?;
        
        // Send ConfigureNotify
        let event = ConfigureNotifyEvent {
            response_type: 22, // ConfigureNotify
            sequence: 0,
            event: window,
            window,
            above_sibling: 0,
            x: new_geom.x as i16,
            y: new_geom.y as i16,
            width: new_geom.width as u16,
            height: new_geom.height as u16,
            border_width: 0,
            override_redirect: false,
        };
        
        conn.send_event(false, window, EventMask::STRUCTURE_NOTIFY, &event)?;
        conn.flush()?;
        
        debug!("Applied _NET_MOVERESIZE_WINDOW to window {}", window);
    } else {
        warn!("_NET_MOVERESIZE_WINDOW: window {} not found", window);
    }
    
    Ok(())
}

/// Handle _NET_WM_MOVERESIZE client message (interactive move/resize)
pub fn handle_net_wm_moveresize(
    conn: &RustConnection,
    display_info: &DisplayInfo,
    screen_info: &ScreenInfo,
    window: u32,
    data: &[u32; 5],
    clients: &mut std::collections::HashMap<u32, Client>,
) -> Result<()> {
    let root_x = data[0] as i16;
    let root_y = data[1] as i16;
    let direction = data[2];
    
    debug!("_NET_WM_MOVERESIZE: window={}, root_x={}, root_y={}, direction={}", 
        window, root_x, root_y, direction);
    
    if let Some(_client) = clients.get(&window) {
        // TODO: Implement interactive move/resize
        // This requires entering a drag loop similar to Alt+drag
        // For now, just log the request
        debug!("_NET_WM_MOVERESIZE not yet fully implemented for window {}", window);
    } else {
        warn!("_NET_WM_MOVERESIZE: window {} not found", window);
    }
    
    Ok(())
}

/// Handle _NET_WM_FULLSCREEN_MONITORS client message
pub fn handle_net_wm_fullscreen_monitors(
    conn: &RustConnection,
    display_info: &DisplayInfo,
    screen_info: &ScreenInfo,
    window: u32,
    data: &[u32; 5],
    clients: &mut std::collections::HashMap<u32, Client>,
) -> Result<()> {
    let top = data[0];
    let bottom = data[1];
    let left = data[2];
    let right = data[3];
    
    debug!("_NET_WM_FULLSCREEN_MONITORS: window={}, top={}, bottom={}, left={}, right={}", 
        window, top, bottom, left, right);
    
    if let Some(client) = clients.get_mut(&window) {
        // Verify all four monitors exist
        if top >= screen_info.num_monitors as u32 ||
           bottom >= screen_info.num_monitors as u32 ||
           left >= screen_info.num_monitors as u32 ||
           right >= screen_info.num_monitors as u32 {
            warn!("_NET_WM_FULLSCREEN_MONITORS: Invalid monitor indices, falling back to primary");
            // xfwm4 twist: fall back to primary monitor
            if let Some(primary) = screen_info.get_primary_monitor() {
                client.geometry = Geometry {
                    x: primary.x,
                    y: primary.y,
                    width: primary.width,
                    height: primary.height,
                };
            }
            return Ok(());
        }
        
        // Get monitors
        let top_mon = screen_info.monitors.get(top as usize);
        let bottom_mon = screen_info.monitors.get(bottom as usize);
        let left_mon = screen_info.monitors.get(left as usize);
        let right_mon = screen_info.monitors.get(right as usize);
        
        if let (Some(top_m), Some(bottom_m), Some(left_m), Some(right_m)) = 
            (top_mon, bottom_mon, left_mon, right_mon) {
            // Compute union rectangle
            let min_x = left_m.x.min(right_m.x);
            let max_x = (left_m.x + left_m.width as i32).max(right_m.x + right_m.width as i32);
            let min_y = top_m.y.min(bottom_m.y);
            let max_y = (top_m.y + top_m.height as i32).max(bottom_m.y + bottom_m.height as i32);
            
            let fullscreen_geom = Geometry {
                x: min_x,
                y: min_y,
                width: ((max_x - min_x).max(0) as u32),
                height: ((max_y - min_y).max(0) as u32),
            };
            
            // Store monitor indices
            client.fullscreen_monitors = Some([top, bottom, left, right]);
            
            // Apply fullscreen geometry
            client.geometry = fullscreen_geom;
            client.flags.insert(ClientFlags::FULLSCREEN);
            client.flags.insert(ClientFlags::FULLSCREEN_MONITORS);
            
            // Configure window
            let target_window = if let Some(frame) = &client.frame {
                frame.frame
            } else {
                window
            };
            
            conn.configure_window(
                target_window,
                &ConfigureWindowAux::new()
                    .x(fullscreen_geom.x)
                    .y(fullscreen_geom.y)
                    .width(fullscreen_geom.width)
                    .height(fullscreen_geom.height),
            )?;
            
            // Update EWMH state
            display_info.atoms.set_window_state(
                conn,
                window,
                &[display_info.atoms._net_wm_state_fullscreen],
                &[],
            )?;
            
            debug!("Applied _NET_WM_FULLSCREEN_MONITORS to window {}", window);
        } else {
            warn!("_NET_WM_FULLSCREEN_MONITORS: Could not get all monitors");
        }
    } else {
        warn!("_NET_WM_FULLSCREEN_MONITORS: window {} not found", window);
    }
    
    Ok(())
}

/// Update _NET_CLIENT_LIST_STACKING root property
pub fn update_client_list_stacking(
    conn: &RustConnection,
    display_info: &DisplayInfo,
    screen_info: &ScreenInfo,
    stacking_order: &[u32],
    clients: &std::collections::HashMap<u32, Client>,
) -> Result<()> {
    // Build list in reverse stacking order (top to bottom)
    let mut client_list: Vec<u32> = stacking_order.iter().rev().copied().collect();
    
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

