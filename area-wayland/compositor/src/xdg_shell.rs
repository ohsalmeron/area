//! XDG shell handling
//!
//! Handles XDG toplevel windows, popups, and shell protocol events.

use smithay::reexports::wayland_server::{
    Dispatch, DisplayHandle, DataInit,
};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::XdgToplevel;
use tracing::debug;
use crate::state::State;
use wayland_server::Client;

/// Handle XDG toplevel requests
impl Dispatch<XdgToplevel, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _toplevel: &XdgToplevel,
        _request: <XdgToplevel as wayland_server::Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        debug!("XDG toplevel request");
        
        // TODO: Handle toplevel requests:
        // - set_parent
        // - set_title
        // - set_app_id
        // - show_window_menu
        // - move
        // - resize
        // - set_max_size
        // - set_min_size
        // - set_maximized
        // - set_fullscreen
        // - set_minimized
        // - set_window_geometry
        // - set_minimized
    }
}
