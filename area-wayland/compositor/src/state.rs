//! Application state management
//!
//! Implements the State structure for the compositor.

use smithay::{
    wayland::{
        compositor::CompositorState,
        shell::xdg::XdgShellState,
    },
    reexports::wayland_server::{
        protocol::wl_compositor::WlCompositor,
        protocol::wl_subcompositor::WlSubcompositor,
        GlobalDispatch, Dispatch, DisplayHandle, New, DataInit,
    },
    reexports::wayland_protocols::xdg::shell::server::xdg_wm_base::XdgWmBase,
};
use wayland_server::Client;
use tracing::debug;

/// Main application state
///
/// This State struct holds our application-specific state and implements
/// GlobalDispatch for protocols that require it.
pub struct State {
    /// Compositor state (used by Smithay internally for protocol handling)
    #[allow(dead_code)]
    pub compositor_state: CompositorState,
    /// XDG shell state (used by Smithay internally for protocol handling)
    #[allow(dead_code)]
    pub xdg_shell_state: XdgShellState,
    // Add more application state here as needed
}

impl State {
    pub fn new(
        compositor_state: CompositorState,
        xdg_shell_state: XdgShellState,
    ) -> Self {
        Self {
            compositor_state,
            xdg_shell_state,
        }
    }
}

// Implement Dispatch for WlCompositor
impl Dispatch<WlCompositor, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &WlCompositor,
        _request: <WlCompositor as wayland_server::Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        debug!("WlCompositor request");
    }
}

// Implement Dispatch for WlSubcompositor
impl Dispatch<WlSubcompositor, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &WlSubcompositor,
        _request: <WlSubcompositor as wayland_server::Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        debug!("WlSubcompositor request");
    }
}

// Implement Dispatch for XdgWmBase
impl Dispatch<XdgWmBase, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &XdgWmBase,
        _request: <XdgWmBase as wayland_server::Resource>::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        debug!("XdgWmBase request");
    }
}

// Implement GlobalDispatch for WlCompositor (required by CompositorState)
impl GlobalDispatch<WlCompositor, (), Self> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        _resource: New<WlCompositor>,
        _global_data: &(),
        _data_init: &mut DataInit<'_, Self>,
    ) {
        debug!("WlCompositor bound");
    }
}

// Implement GlobalDispatch for WlSubcompositor (required by CompositorState)
impl GlobalDispatch<WlSubcompositor, (), Self> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        _resource: New<WlSubcompositor>,
        _global_data: &(),
        _data_init: &mut DataInit<'_, Self>,
    ) {
        debug!("WlSubcompositor bound");
    }
}

// Implement GlobalDispatch for XdgWmBase (required by XdgShellState)
impl GlobalDispatch<XdgWmBase, (), Self> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        _resource: New<XdgWmBase>,
        _global_data: &(),
        _data_init: &mut DataInit<'_, Self>,
    ) {
        debug!("XdgWmBase bound");
    }
}
