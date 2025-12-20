//! EWMH (Extended Window Manager Hints) implementation
//!
//! Provides compatibility with desktop apps, panels, and other X11 clients.

use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::wrapper::ConnectionExt as _;

// EWMH (Extended Window Manager Hints) implementation... (rest of the code below)


/// Holds all interned EWMH atoms
#[derive(Debug)]
pub struct Atoms {
    pub net_supported: Atom,
    pub net_client_list: Atom,
    pub net_number_of_desktops: Atom,
    pub net_current_desktop: Atom,
    pub net_active_window: Atom,
    pub net_supporting_wm_check: Atom,
    pub net_wm_name: Atom,
    pub net_wm_desktop: Atom,
    pub net_wm_window_type: Atom,
    pub _net_wm_window_type_dock: Atom,
    pub _net_wm_window_type_normal: Atom,
    pub net_wm_state: Atom,
    pub _net_wm_state_fullscreen: Atom,
    pub _net_wm_state_maximized_vert: Atom,
    pub _net_wm_state_maximized_horz: Atom,
    pub _net_close_window: Atom,
    pub _wm_protocols: Atom,
    pub _wm_delete_window: Atom,
    pub _wm_state: Atom,
    pub wm_class: Atom,
    pub wm_name: Atom,
    pub utf8_string: Atom,
}

impl Atoms {
    /// Intern all required atoms
    pub fn new<C: Connection>(conn: &C) -> Result<Self> {
        // Helper to intern a single atom
        let intern = |name: &str| -> Result<Atom> {
            Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
        };

        Ok(Self {
            net_supported: intern("_NET_SUPPORTED")?,
            net_client_list: intern("_NET_CLIENT_LIST")?,
            net_number_of_desktops: intern("_NET_NUMBER_OF_DESKTOPS")?,
            net_current_desktop: intern("_NET_CURRENT_DESKTOP")?,
            net_active_window: intern("_NET_ACTIVE_WINDOW")?,
            net_supporting_wm_check: intern("_NET_SUPPORTING_WM_CHECK")?,
            net_wm_name: intern("_NET_WM_NAME")?,
            net_wm_desktop: intern("_NET_WM_DESKTOP")?,
            net_wm_window_type: intern("_NET_WM_WINDOW_TYPE")?,
            _net_wm_window_type_dock: intern("_NET_WM_WINDOW_TYPE_DOCK")?,
            _net_wm_window_type_normal: intern("_NET_WM_WINDOW_TYPE_NORMAL")?,
            net_wm_state: intern("_NET_WM_STATE")?,
            _net_wm_state_fullscreen: intern("_NET_WM_STATE_FULLSCREEN")?,
            _net_wm_state_maximized_vert: intern("_NET_WM_STATE_MAXIMIZED_VERT")?,
            _net_wm_state_maximized_horz: intern("_NET_WM_STATE_MAXIMIZED_HORZ")?,
            _net_close_window: intern("_NET_CLOSE_WINDOW")?,
            _wm_protocols: intern("WM_PROTOCOLS")?,
            _wm_delete_window: intern("WM_DELETE_WINDOW")?,
            _wm_state: intern("WM_STATE")?,
            wm_class: intern("WM_CLASS")?,
            wm_name: intern("WM_NAME")?,
            utf8_string: intern("UTF8_STRING")?,
        })
    }

    /// Set up _NET_SUPPORTED on root window
    pub fn setup_supported<C: Connection>(
        &self,
        conn: &C,
        root: Window,
    ) -> Result<()> {
        let supported = [
            self.net_supported,
            self.net_client_list,
            self.net_number_of_desktops,
            self.net_current_desktop,
            self.net_active_window,
            self.net_wm_name,
            self.net_wm_desktop,
            self.net_wm_window_type,
            self.net_wm_state,
        ];

        conn.change_property32(
            PropMode::REPLACE,
            root,
            self.net_supported,
            AtomEnum::ATOM,
            &supported,
        )?;

        Ok(())
    }

    /// Set up _NET_SUPPORTING_WM_CHECK on root and child windows
    pub fn setup_supporting_wm_check<C: Connection>(
        &self,
        conn: &C,
        root: Window,
        child: Window,
        name: &str,
    ) -> Result<()> {
        // Set _NET_SUPPORTING_WM_CHECK on root window pointing to child
        conn.change_property32(
            PropMode::REPLACE,
            root,
            self.net_supporting_wm_check,
            AtomEnum::WINDOW,
            &[child],
        )?;

        // Set _NET_SUPPORTING_WM_CHECK on child window pointing to child
        conn.change_property32(
            PropMode::REPLACE,
            child,
            self.net_supporting_wm_check,
            AtomEnum::WINDOW,
            &[child],
        )?;

        // Set _NET_WM_NAME on child window
        conn.change_property8(
            PropMode::REPLACE,
            child,
            self.net_wm_name,
            self.utf8_string,
            name.as_bytes(),
        )?;

        Ok(())
    }

    /// Update _NET_CLIENT_LIST
    pub fn update_client_list<C: Connection>(
        &self,
        conn: &C,
        root: Window,
        windows: &[u32],
    ) -> Result<()> {
        conn.change_property32(
            PropMode::REPLACE,
            root,
            self.net_client_list,
            AtomEnum::WINDOW,
            windows,
        )?;
        Ok(())
    }

    /// Update _NET_ACTIVE_WINDOW
    pub fn update_active_window<C: Connection>(
        &self,
        conn: &C,
        root: Window,
        window: Option<u32>,
    ) -> Result<()> {
        let win = window.unwrap_or(0);
        conn.change_property32(
            PropMode::REPLACE,
            root,
            self.net_active_window,
            AtomEnum::WINDOW,
            &[win],
        )?;
        Ok(())
    }

    /// Update _NET_CURRENT_DESKTOP
    pub fn update_current_desktop<C: Connection>(
        &self,
        conn: &C,
        root: Window,
        desktop: u32,
    ) -> Result<()> {
        conn.change_property32(
            PropMode::REPLACE,
            root,
            self.net_current_desktop,
            AtomEnum::CARDINAL,
            &[desktop],
        )?;
        Ok(())
    }

    /// Get window title (tries _NET_WM_NAME first, then WM_NAME)
    pub fn get_window_title<C: Connection>(
        &self,
        conn: &C,
        window: Window,
    ) -> Result<String> {
        // Try _NET_WM_NAME first (UTF-8)
        if let Ok(reply) = conn.get_property(
            false,
            window,
            self.net_wm_name,
            self.utf8_string,
            0,
            1024,
        )?.reply() {
            if !reply.value.is_empty() {
                return Ok(String::from_utf8_lossy(&reply.value).to_string());
            }
        }

        // Fall back to WM_NAME
        if let Ok(reply) = conn.get_property(
            false,
            window,
            self.wm_name,
            AtomEnum::STRING,
            0,
            1024,
        )?.reply() {
            if !reply.value.is_empty() {
                return Ok(String::from_utf8_lossy(&reply.value).to_string());
            }
        }

        Ok(String::new())
    }

    /// Get window class (WM_CLASS)
    pub fn get_window_class<C: Connection>(
        &self,
        conn: &C,
        window: Window,
    ) -> Result<String> {
        if let Ok(reply) = conn.get_property(
            false,
            window,
            self.wm_class,
            AtomEnum::STRING,
            0,
            1024,
        )?.reply() {
            if !reply.value.is_empty() {
                // WM_CLASS is two null-terminated strings: instance\0class\0
                // We want the class (second one)
                let parts: Vec<&[u8]> = reply.value.split(|&b| b == 0).collect();
                if parts.len() >= 2 {
                    return Ok(String::from_utf8_lossy(parts[1]).to_string());
                } else if !parts.is_empty() {
                    return Ok(String::from_utf8_lossy(parts[0]).to_string());
                }
            }
        }
        Ok(String::new())
    }

    /// Set window state (add/remove EWMH states)
    pub fn set_window_state<C: Connection>(
        &self,
        conn: &C,
        window: Window,
        add: &[Atom],
        remove: &[Atom],
    ) -> Result<()> {
        // Get current state
        let mut states = Vec::new();
        if let Ok(reply) = conn.get_property(
            false,
            window,
            self.net_wm_state,
            AtomEnum::ATOM,
            0,
            1024,
        )?.reply() {
            if let Some(value32) = reply.value32() {
                states = value32.collect();
            }
        }
        
        // Remove states
        for atom in remove {
            states.retain(|&a| a != *atom);
        }
        
        // Add states
        for atom in add {
            if !states.contains(atom) {
                states.push(*atom);
            }
        }
        
        // Set new state
        conn.change_property32(
            PropMode::REPLACE,
            window,
            self.net_wm_state,
            AtomEnum::ATOM,
            &states,
        )?;
        
        Ok(())
    }

    /// Send WM_DELETE_WINDOW message to close a window gracefully
    pub fn send_delete_window<C: Connection>(
        &self,
        conn: &C,
        window: Window,
    ) -> Result<()> {
        // Build ClientMessage bytes manually
        // Format: [response_type(1), unused(1), sequence(2), window(4), type(4), format(1), unused(2), data(20)]
        let mut event_bytes = [0u8; 32];
        event_bytes[0] = 33; // ClientMessage
        event_bytes[4..8].copy_from_slice(&window.to_ne_bytes());
        event_bytes[8..12].copy_from_slice(&self._wm_protocols.to_ne_bytes());
        event_bytes[12] = 32; // format (32-bit)
        event_bytes[16..20].copy_from_slice(&self._wm_delete_window.to_ne_bytes());
        // Rest of data is zeros (timestamp = 0 = CurrentTime)
        
        conn.send_event(
            false,
            window,
            EventMask::NO_EVENT,
            event_bytes,
        )?;
        
        Ok(())
    }
}
