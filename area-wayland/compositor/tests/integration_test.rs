//! Integration tests for the compositor
//!
//! Tests that critical integrations actually work.

use anyhow::Result;
use area_compositor::config::Config;
use area_compositor::server::Server;
use area_compositor::state::State;
use smithay::{
    wayland::{
        compositor::CompositorState,
        shell::xdg::XdgShellState,
    },
    reexports::wayland_server::Display,
};

#[test]
fn test_display_creation() -> Result<()> {
    // Test that we can create a Wayland display
    let _display = Display::<State>::new()?;
    Ok(())
}

#[test]
fn test_compositor_state_creation() -> Result<()> {
    // Test that we can create CompositorState
    let display = Display::<State>::new()?;
    let _compositor_state = CompositorState::new::<State>(&display.handle());
    Ok(())
}

#[test]
fn test_xdg_shell_state_creation() -> Result<()> {
    // Test that we can create XdgShellState
    let display = Display::<State>::new()?;
    let _xdg_shell_state = XdgShellState::new::<State>(&display.handle());
    Ok(())
}

#[test]
fn test_state_creation() -> Result<()> {
    // Test that we can create State with both compositor and xdg shell
    let display = Display::<State>::new()?;
    let compositor_state = CompositorState::new::<State>(&display.handle());
    let xdg_shell_state = XdgShellState::new::<State>(&display.handle());
    let _state = State::new(compositor_state, xdg_shell_state);
    Ok(())
}

#[test]
fn test_server_initialization() -> Result<()> {
    // Test that we can initialize the server
    let config = Config::load()?;
    let _server = Server::new(config)?;
    Ok(())
}
