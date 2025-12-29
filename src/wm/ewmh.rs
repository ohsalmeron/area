//! EWMH (Extended Window Manager Hints) implementation
//!
//! Provides compatibility with desktop apps, panels, and other X11 clients.

use anyhow::Result;
use tracing::debug;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ClientMessageEvent, *};
use x11rb::wrapper::ConnectionExt as _;

// EWMH (Extended Window Manager Hints) implementation... (rest of the code below)


/// Holds all interned EWMH atoms
#[derive(Debug)]
pub struct Atoms {
    pub net_supported: Atom,
    pub net_client_list: Atom,
    pub net_client_list_stacking: Atom,
    pub net_number_of_desktops: Atom,
    pub net_current_desktop: Atom,
    pub net_active_window: Atom,
    pub net_wm_name: Atom,
    pub net_wm_desktop: Atom,
    pub net_wm_window_type: Atom,
    pub _net_wm_window_type_desktop: Atom,
    pub _net_wm_window_type_dock: Atom,
    pub _net_wm_window_type_normal: Atom,
    pub _net_wm_window_type_dialog: Atom,
    pub _net_wm_window_type_utility: Atom,
    pub _net_wm_window_type_toolbar: Atom,
    pub _net_wm_window_type_splash: Atom,
    pub _net_wm_window_type_menu: Atom,
    pub _net_wm_window_type_dropdown_menu: Atom,
    pub _net_wm_window_type_popup_menu: Atom,
    pub _net_wm_window_type_tooltip: Atom,
    pub _net_wm_window_type_notification: Atom,
    pub _net_wm_window_type_combo: Atom,
    pub _net_wm_window_type_dnd: Atom,
    pub net_wm_state: Atom,
    pub _net_wm_state_fullscreen: Atom,
    pub _net_wm_state_maximized_vert: Atom,
    pub _net_wm_state_maximized_horz: Atom,
    pub _net_wm_state_hidden: Atom,
    pub _net_wm_state_shaded: Atom,
    pub _net_wm_state_sticky: Atom,
    pub _net_wm_state_modal: Atom,
    pub _net_wm_state_skip_pager: Atom,
    pub _net_wm_state_skip_taskbar: Atom,
    pub _net_wm_state_above: Atom,
    pub _net_wm_state_below: Atom,
    pub _net_wm_state_demands_attention: Atom,
    pub net_frame_extents: Atom,
    pub _net_wm_bypass_compositor: Atom,
    pub _net_close_window: Atom,
    pub _net_moveresize_window: Atom,
    pub _net_wm_moveresize: Atom,
    pub _net_wm_fullscreen_monitors: Atom,
    // Action atoms
    pub _net_wm_allowed_actions: Atom,
    pub _net_wm_action_move: Atom,
    pub _net_wm_action_resize: Atom,
    pub _net_wm_action_minimize: Atom,
    pub _net_wm_action_shade: Atom,
    pub _net_wm_action_stick: Atom,
    pub _net_wm_action_maximize_horz: Atom,
    pub _net_wm_action_maximize_vert: Atom,
    pub _net_wm_action_fullscreen: Atom,
    pub _net_wm_action_change_desktop: Atom,
    pub _net_wm_action_close: Atom,
    // Supporting/Desktop atoms
    pub _net_supporting_wm_check: Atom,
    pub _net_wm_pid: Atom,
    pub _net_wm_icon: Atom,
    pub _net_startup_id: Atom,
    pub _net_desktop_viewport: Atom,
    pub _net_desktop_names: Atom,
    // Strut atoms
    pub _net_wm_strut: Atom,
    pub _net_wm_strut_partial: Atom,
    pub _net_workarea: Atom,
    // Standard X11 atoms
    pub _wm_protocols: Atom,
    pub _wm_delete_window: Atom,
    pub _wm_state: Atom,
    pub _wm_class: Atom,
    pub _wm_normal_hints: Atom,
    pub _wm_size_hints: Atom,
    pub _wm_hints: Atom,
    pub _utf8_string: Atom,
    // MOTIF WM Hints (for decoration control)
    pub _motif_wm_hints: Atom,
    // WM_CLIENT_LEADER for window grouping
    pub wm_client_leader: Atom,
    // _NET_WM_ICON_GEOMETRY for icon placement hints
    pub _net_wm_icon_geometry: Atom,
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
            net_client_list_stacking: intern("_NET_CLIENT_LIST_STACKING")?,
            net_number_of_desktops: intern("_NET_NUMBER_OF_DESKTOPS")?,
            net_current_desktop: intern("_NET_CURRENT_DESKTOP")?,
            net_active_window: intern("_NET_ACTIVE_WINDOW")?,
            net_wm_name: intern("_NET_WM_NAME")?,
            net_wm_desktop: intern("_NET_WM_DESKTOP")?,
            net_wm_window_type: intern("_NET_WM_WINDOW_TYPE")?,
            _net_wm_window_type_desktop: intern("_NET_WM_WINDOW_TYPE_DESKTOP")?,
            _net_wm_window_type_dock: intern("_NET_WM_WINDOW_TYPE_DOCK")?,
            _net_wm_window_type_normal: intern("_NET_WM_WINDOW_TYPE_NORMAL")?,
            _net_wm_window_type_dialog: intern("_NET_WM_WINDOW_TYPE_DIALOG")?,
            _net_wm_window_type_utility: intern("_NET_WM_WINDOW_TYPE_UTILITY")?,
            _net_wm_window_type_toolbar: intern("_NET_WM_WINDOW_TYPE_TOOLBAR")?,
            _net_wm_window_type_splash: intern("_NET_WM_WINDOW_TYPE_SPLASH")?,
            _net_wm_window_type_menu: intern("_NET_WM_WINDOW_TYPE_MENU")?,
            _net_wm_window_type_dropdown_menu: intern("_NET_WM_WINDOW_TYPE_DROPDOWN_MENU")?,
            _net_wm_window_type_popup_menu: intern("_NET_WM_WINDOW_TYPE_POPUP_MENU")?,
            _net_wm_window_type_tooltip: intern("_NET_WM_WINDOW_TYPE_TOOLTIP")?,
            _net_wm_window_type_notification: intern("_NET_WM_WINDOW_TYPE_NOTIFICATION")?,
            _net_wm_window_type_combo: intern("_NET_WM_WINDOW_TYPE_COMBO")?,
            _net_wm_window_type_dnd: intern("_NET_WM_WINDOW_TYPE_DND")?,
            net_wm_state: intern("_NET_WM_STATE")?,
            _net_wm_state_fullscreen: intern("_NET_WM_STATE_FULLSCREEN")?,
            _net_wm_state_maximized_vert: intern("_NET_WM_STATE_MAXIMIZED_VERT")?,
            _net_wm_state_maximized_horz: intern("_NET_WM_STATE_MAXIMIZED_HORZ")?,
            _net_wm_state_hidden: intern("_NET_WM_STATE_HIDDEN")?,
            _net_wm_state_shaded: intern("_NET_WM_STATE_SHADED")?,
            _net_wm_state_sticky: intern("_NET_WM_STATE_STICKY")?,
            _net_wm_state_modal: intern("_NET_WM_STATE_MODAL")?,
            _net_wm_state_skip_pager: intern("_NET_WM_STATE_SKIP_PAGER")?,
            _net_wm_state_skip_taskbar: intern("_NET_WM_STATE_SKIP_TASKBAR")?,
            _net_wm_state_above: intern("_NET_WM_STATE_ABOVE")?,
            _net_wm_state_below: intern("_NET_WM_STATE_BELOW")?,
            _net_wm_state_demands_attention: intern("_NET_WM_STATE_DEMANDS_ATTENTION")?,
            net_frame_extents: intern("_NET_FRAME_EXTENTS")?,
            _net_wm_bypass_compositor: intern("_NET_WM_BYPASS_COMPOSITOR")?,
            _net_close_window: intern("_NET_CLOSE_WINDOW")?,
            _net_moveresize_window: intern("_NET_MOVERESIZE_WINDOW")?,
            _net_wm_moveresize: intern("_NET_WM_MOVERESIZE")?,
            _net_wm_fullscreen_monitors: intern("_NET_WM_FULLSCREEN_MONITORS")?,
            // Action atoms
            _net_wm_allowed_actions: intern("_NET_WM_ALLOWED_ACTIONS")?,
            _net_wm_action_move: intern("_NET_WM_ACTION_MOVE")?,
            _net_wm_action_resize: intern("_NET_WM_ACTION_RESIZE")?,
            _net_wm_action_minimize: intern("_NET_WM_ACTION_MINIMIZE")?,
            _net_wm_action_shade: intern("_NET_WM_ACTION_SHADE")?,
            _net_wm_action_stick: intern("_NET_WM_ACTION_STICK")?,
            _net_wm_action_maximize_horz: intern("_NET_WM_ACTION_MAXIMIZE_HORZ")?,
            _net_wm_action_maximize_vert: intern("_NET_WM_ACTION_MAXIMIZE_VERT")?,
            _net_wm_action_fullscreen: intern("_NET_WM_ACTION_FULLSCREEN")?,
            _net_wm_action_change_desktop: intern("_NET_WM_ACTION_CHANGE_DESKTOP")?,
            _net_wm_action_close: intern("_NET_WM_ACTION_CLOSE")?,
            // Supporting/Desktop atoms
            _net_supporting_wm_check: intern("_NET_SUPPORTING_WM_CHECK")?,
            _net_wm_pid: intern("_NET_WM_PID")?,
            _net_wm_icon: intern("_NET_WM_ICON")?,
            _net_startup_id: intern("_NET_STARTUP_ID")?,
            _net_desktop_viewport: intern("_NET_DESKTOP_VIEWPORT")?,
            _net_desktop_names: intern("_NET_DESKTOP_NAMES")?,
            // Strut atoms
            _net_wm_strut: intern("_NET_WM_STRUT")?,
            _net_wm_strut_partial: intern("_NET_WM_STRUT_PARTIAL")?,
            _net_workarea: intern("_NET_WORKAREA")?,
            // Standard X11 atoms
            _wm_protocols: intern("WM_PROTOCOLS")?,
            _wm_delete_window: intern("WM_DELETE_WINDOW")?,
            _wm_state: intern("WM_STATE")?,
            _wm_class: intern("WM_CLASS")?,
            _wm_normal_hints: intern("WM_NORMAL_HINTS")?,
            _wm_size_hints: intern("WM_SIZE_HINTS")?,
            _wm_hints: intern("WM_HINTS")?,
            _utf8_string: intern("UTF8_STRING")?,
            _motif_wm_hints: intern("_MOTIF_WM_HINTS")?,
            wm_client_leader: intern("WM_CLIENT_LEADER")?,
            _net_wm_icon_geometry: intern("_NET_WM_ICON_GEOMETRY")?,
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
            self._net_wm_window_type_dock,
            self._net_wm_window_type_normal,
            self._net_wm_window_type_dialog,
            self._net_wm_window_type_utility,
            self._net_wm_window_type_toolbar,
            self._net_wm_window_type_splash,
            self._net_wm_window_type_menu,
            self._net_wm_window_type_dropdown_menu,
            self._net_wm_window_type_popup_menu,
            self._net_wm_window_type_tooltip,
            self._net_wm_window_type_notification,
            self._net_wm_window_type_combo,
            self._net_wm_window_type_dnd,
            self.net_wm_state,
            self._net_wm_state_fullscreen,
            self._net_wm_state_maximized_vert,
            self._net_wm_state_maximized_horz,
            self._net_wm_state_hidden,
            self._net_wm_state_shaded,
            self._net_wm_state_sticky,
            self._net_wm_state_modal,
            self._net_wm_state_skip_pager,
            self._net_wm_state_skip_taskbar,
            self._net_wm_state_above,
            self._net_wm_state_below,
            self._net_wm_state_demands_attention,
            self.net_frame_extents,
            self._net_wm_allowed_actions,
            self._net_wm_action_move,
            self._net_wm_action_resize,
            self._net_wm_action_minimize,
            self._net_wm_action_shade,
            self._net_wm_action_stick,
            self._net_wm_action_maximize_horz,
            self._net_wm_action_maximize_vert,
            self._net_wm_action_fullscreen,
            self._net_wm_action_change_desktop,
            self._net_wm_action_close,
            self._net_supporting_wm_check,
            self._net_wm_pid,
            self._net_desktop_viewport,
            self._net_desktop_names,
            self._net_wm_strut,
            self._net_wm_strut_partial,
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

    /// Update _NET_CLIENT_LIST root property with list of managed windows
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

    /// Update _NET_FRAME_EXTENTS for a window
    pub fn update_frame_extents<C: Connection>(
        &self,
        conn: &C,
        window: Window,
        left: u32,
        right: u32,
        top: u32,
        bottom: u32,
    ) -> Result<()> {
        conn.change_property32(
            PropMode::REPLACE,
            window,
            self.net_frame_extents,
            AtomEnum::CARDINAL,
            &[left, right, top, bottom],
        )?;
        Ok(())
    }


    /// Set window state (add/remove EWMH states)
    /// This updates the _NET_WM_STATE property and sends PropertyNotify
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
        
        // Set new state (this will trigger PropertyNotify automatically)
        conn.change_property32(
            PropMode::REPLACE,
            window,
            self.net_wm_state,
            AtomEnum::ATOM,
            &states,
        )?;
        
        Ok(())
    }

    /// Check if window supports WM_DELETE_WINDOW protocol
    pub fn supports_delete_protocol<C: Connection>(
        &self,
        conn: &C,
        window: Window,
    ) -> Result<bool> {
        // Get WM_PROTOCOLS property
        if let Ok(reply) = conn.get_property(
            false,
            window,
            self._wm_protocols,
            AtomEnum::ATOM,
            0,
            1024,
        )?.reply() {
            if let Some(value32) = reply.value32() {
                // Check if WM_DELETE_WINDOW is in the protocols list
                let protocols: Vec<u32> = value32.collect();
                return Ok(protocols.contains(&self._wm_delete_window));
            }
        }
        Ok(false)
    }

    /// Send WM_DELETE_WINDOW message to close a window gracefully
    pub fn send_delete_window<C: Connection>(
        &self,
        conn: &C,
        window: Window,
    ) -> Result<()> {
        // Validate window ID
        if window == 0 {
            return Err(anyhow::anyhow!("Invalid window ID: 0"));
        }
        
        // Use ClientMessageEvent::new() - the proper x11rb/XCB way
        let event = ClientMessageEvent::new(
            32, // format (32-bit)
            window, // destination window
            self._wm_protocols, // message type atom
            [self._wm_delete_window, 0, 0, 0, 0], // data (timestamp = 0 = CurrentTime)
        );
        
        // NO_EVENT is correct for XCB/x11rb ClientMessage events
        // This is the standard way per x11rb examples and XCB documentation
        if let Err(e) = conn.send_event(
            false, // propagate
            window, // destination
            EventMask::NO_EVENT, // event_mask - correct for XCB/x11rb
            event,
        ) {
            // If window is already destroyed, that's fine - it's already closed
            debug!("Failed to send WM_DELETE_WINDOW to window {} (may already be destroyed): {}", window, e);
            // Don't return error - window closing is the desired outcome
        }
        
        Ok(())
    }
    
    /// Get _NET_WM_WINDOW_TYPE property for a window
    /// Returns a vector of window type atoms
    pub fn get_window_type<C: Connection>(
        &self,
        conn: &C,
        window: Window,
    ) -> Result<Vec<Atom>> {
        if let Ok(reply) = conn.get_property(
            false,
            window,
            self.net_wm_window_type,
            AtomEnum::ATOM,
            0,
            1024,
        )?.reply() {
            if let Some(value32) = reply.value32() {
                return Ok(value32.collect());
            }
        }
        Ok(vec![])
    }
    
    /// Check if a window has _NET_WM_BYPASS_COMPOSITOR set to 1
    /// Returns true if the window requests compositor bypass
    pub fn check_bypass_compositor<C: Connection>(
        &self,
        conn: &C,
        window: Window,
    ) -> Result<bool> {
        if let Ok(reply) = conn.get_property(
            false,
            window,
            self._net_wm_bypass_compositor,
            AtomEnum::CARDINAL,
            0,
            1,
        )?.reply() {
            if let Some(mut value32) = reply.value32() {
                if let Some(value) = value32.next() {
                    // Value 1 means bypass compositor
                    return Ok(value == 1);
                }
            }
        }
        Ok(false)
    }
}

/// MOTIF WM Hints structure
/// Based on MWM (Motif Window Manager) hints specification
#[derive(Debug, Clone, Copy)]
pub struct MotifWmHints {
    pub flags: u32,        // MWM_HINTS_* flags
    pub functions: u32,    // MWM_FUNC_* bits
    pub decorations: u32,  // MWM_DECOR_* bits
}

impl Atoms {
    // MOTIF WM Hints constants
    pub const MWM_HINTS_DECORATIONS: u32 = 1 << 1;
    pub const MWM_HINTS_FUNCTIONS: u32 = 1 << 0;
    pub const MWM_DECOR_ALL: u32 = 1 << 0;
    pub const MWM_DECOR_BORDER: u32 = 1 << 1;
    pub const MWM_DECOR_TITLE: u32 = 1 << 3;
    
    /// Get MOTIF_WM_HINTS property for a window
    /// Returns Some(MotifWmHints) if the property exists and is valid, None otherwise
    pub fn get_motif_hints<C: Connection>(
        &self,
        conn: &C,
        window: Window,
    ) -> Result<Option<MotifWmHints>> {
        // MOTIF_WM_HINTS is a property of type _MOTIF_WM_HINTS containing 5 32-bit values
        // We only need the first 3: flags, functions, decorations
        if let Ok(reply) = conn.get_property(
            false,
            window,
            self._motif_wm_hints,
            self._motif_wm_hints, // Type is the same as the atom
            0,
            5, // Read up to 5 values (we only need 3)
        )?.reply() {
            if let Some(value32) = reply.value32() {
                let values: Vec<u32> = value32.take(3).collect();
                if values.len() >= 3 {
                    return Ok(Some(MotifWmHints {
                        flags: values[0],
                        functions: values[1],
                        decorations: values[2],
                    }));
                }
            }
        }
        Ok(None)
    }
    
    /// Check if MOTIF hints indicate the window should have decorations
    /// Returns true if decorations should be shown, false if they should be hidden
    /// Returns None if MOTIF hints are not present or don't specify decoration preference
    pub fn should_decorate_from_motif_hints<C: Connection>(
        &self,
        conn: &C,
        window: Window,
    ) -> Result<Option<bool>> {
        if let Some(hints) = self.get_motif_hints(conn, window)? {
            // Check if decorations flag is set
            if (hints.flags & Self::MWM_HINTS_DECORATIONS) != 0 {
                // If decorations field is 0, no decorations
                if hints.decorations == 0 {
                    return Ok(Some(false));
                }
                // If MWM_DECOR_ALL or MWM_DECOR_TITLE is set, show decorations
                if (hints.decorations & (Self::MWM_DECOR_ALL | Self::MWM_DECOR_TITLE)) != 0 {
                    return Ok(Some(true));
                }
                // Otherwise, no decorations
                return Ok(Some(false));
            }
        }
        // MOTIF hints not present or don't specify decoration preference
        Ok(None)
    }
    
    /// Update _NET_WORKAREA property on root window
    pub fn update_workarea<C: Connection>(
        &self,
        conn: &C,
        root: Window,
        work_area: &crate::shared::Geometry,
    ) -> Result<()> {
        let data = [
            work_area.x as u32,
            work_area.y as u32,
            work_area.width,
            work_area.height,
        ];
        conn.change_property32(
            x11rb::protocol::xproto::PropMode::REPLACE,
            root,
            self._net_workarea,
            x11rb::protocol::xproto::AtomEnum::CARDINAL,
            &data,
        )?;
        Ok(())
    }
}
