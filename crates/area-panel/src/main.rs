// Simple panel showing open windows at the bottom of the screen
// Similar to xfce4-panel's window list

use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{COPY_FROM_PARENT, CURRENT_TIME};

// WindowInfo struct removed - not used yet

#[allow(dead_code)] // Reserved for future use
fn get_client_list(conn: &RustConnection, root: u32) -> Result<Vec<u32>> {
    let net_client_list = conn.intern_atom(false, b"_NET_CLIENT_LIST")?.reply()?;
    
    let reply = conn.get_property(
        false,
        root,
        net_client_list.atom,
        AtomEnum::WINDOW,
        0,
        1024,
    )?.reply()?;
    
    let mut windows = Vec::new();
    for chunk in reply.value.chunks(4) {
        if chunk.len() == 4 {
            let window = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            windows.push(window);
        }
    }
    
    Ok(windows)
}

#[allow(dead_code)] // Reserved for future use
fn get_active_window(conn: &RustConnection, root: u32) -> Result<Option<u32>> {
    let net_active_window = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?;
    
    let reply = conn.get_property(
        false,
        root,
        net_active_window.atom,
        AtomEnum::WINDOW,
        0,
        1,
    )?.reply()?;
    
    if reply.value.len() >= 4 {
        let window = u32::from_ne_bytes([reply.value[0], reply.value[1], reply.value[2], reply.value[3]]);
        if window != 0 {
            return Ok(Some(window));
        }
    }
    
    Ok(None)
}

#[allow(dead_code)] // Reserved for future use
fn get_window_title(conn: &RustConnection, window: u32) -> String {
    // Try _NET_WM_NAME first
    let net_wm_name = match conn.intern_atom(false, b"_NET_WM_NAME") {
        Ok(cookie) => match cookie.reply() {
            Ok(atom) => atom.atom,
            Err(_) => return format!("Window {}", window),
        },
        Err(_) => return format!("Window {}", window),
    };
    
    let utf8_string = match conn.intern_atom(false, b"UTF8_STRING") {
        Ok(cookie) => match cookie.reply() {
            Ok(atom) => atom.atom,
            Err(_) => return format!("Window {}", window),
        },
        Err(_) => return format!("Window {}", window),
    };
    
    match conn.get_property(false, window, net_wm_name, utf8_string, 0, 256) {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => {
                if let Ok(title) = String::from_utf8(reply.value) {
                    if !title.is_empty() {
                        return title;
                    }
                }
            }
            Err(_) => {}
        },
        Err(_) => {}
    }
    
    // Fallback to WM_NAME
    match conn.get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256) {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => {
                if let Ok(title) = String::from_utf8(reply.value) {
                    if !title.is_empty() {
                        return title;
                    }
                }
            }
            Err(_) => {}
        },
        Err(_) => {}
    }
    
    format!("Window {}", window)
}

#[allow(dead_code)] // Reserved for future use
fn is_window_mapped(conn: &RustConnection, window: u32) -> bool {
    match conn.get_window_attributes(window) {
        Ok(cookie) => match cookie.reply() {
            Ok(attrs) => attrs.map_state == MapState::VIEWABLE,
            Err(_) => false,
        },
        Err(_) => false,
    }
}

#[allow(dead_code)] // Reserved for future use
fn focus_window(conn: &RustConnection, root: u32, window: u32) -> Result<()> {
    // Set input focus
    conn.set_input_focus(InputFocus::POINTER_ROOT, window, CURRENT_TIME)?;
    
    // Raise to top
    conn.configure_window(window, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
    
    // Update _NET_ACTIVE_WINDOW
    let net_active_window = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?;
    let event = ClientMessageEvent::new(
        32,
        root,
        net_active_window.atom,
        [2, CURRENT_TIME, 0, 0, 0], // source = 2 (application)
    );
    conn.send_event(false, window, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, &event)?;
    conn.flush()?;
    
    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let (conn, screen_num) = RustConnection::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    let screen_width = screen.width_in_pixels;
    let screen_height = screen.height_in_pixels;
    
    tracing::info!("Panel started on display {}, screen {}x{}", screen_num, screen_width, screen_height);
    
    // Create panel window at the bottom
    let panel_height = 40u16;
    let panel_window = conn.generate_id()?;
    
    // Get visual and colormap
    let visual = screen.root_visual;
    let _colormap = screen.default_colormap; // Reserved for future use
    
    // Create window
    conn.create_window(
        COPY_FROM_PARENT as u8,
        panel_window,
        root,
        0,
        (screen_height - panel_height as u16) as i16,
        screen_width as u16,
        panel_height,
        0,
        WindowClass::INPUT_OUTPUT,
        visual,
        &CreateWindowAux::new()
            .background_pixel(screen.black_pixel)
            .event_mask(EventMask::BUTTON_PRESS | EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY),
    )?;
    
    // Set window type to DOCK
    let net_wm_window_type = conn.intern_atom(false, b"_NET_WM_WINDOW_TYPE")?.reply()?;
    let net_wm_window_type_dock = conn.intern_atom(false, b"_NET_WM_WINDOW_TYPE_DOCK")?.reply()?;
    conn.change_property32(
        PropMode::REPLACE,
        panel_window,
        net_wm_window_type.atom,
        AtomEnum::ATOM,
        &[net_wm_window_type_dock.atom],
    )?;
    
    // Set strut to reserve space at bottom
    let net_wm_strut = conn.intern_atom(false, b"_NET_WM_STRUT")?.reply()?;
    conn.change_property32(
        PropMode::REPLACE,
        panel_window,
        net_wm_strut.atom,
        AtomEnum::CARDINAL,
        &[0, 0, panel_height as u32, 0], // left, right, bottom, top
    )?;
    
    // Map window
    conn.map_window(panel_window)?;
    conn.flush()?;
    
    tracing::info!("Panel window created: {}", panel_window);
    
    // Select for property changes on root to detect window list changes
    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?;
    conn.flush()?;
    
    // Event loop
    loop {
        let event = conn.wait_for_event()?;
        
        match event {
            x11rb::protocol::Event::PropertyNotify(e) => {
                if e.window == root {
                    let net_client_list = conn.intern_atom(false, b"_NET_CLIENT_LIST")?.reply()?;
                    if e.atom == net_client_list.atom {
                        // Window list changed, update panel
                        tracing::debug!("Window list changed, updating panel");
                        // For now, just log - in a full implementation, we'd redraw the panel
                    }
                }
            }
            x11rb::protocol::Event::ButtonPress(e) => {
                if e.event == panel_window {
                    // Click on panel - for now just log
                    tracing::debug!("Panel clicked at {}, {}", e.event_x, e.event_y);
                }
            }
            x11rb::protocol::Event::Expose(e) => {
                if e.window == panel_window && e.count == 0 {
                    // Redraw panel
                    tracing::debug!("Panel exposed, should redraw");
                    // In a full implementation, we'd draw window buttons here
                }
            }
            _ => {}
        }
        
        conn.flush()?;
    }
}

