//! Core server implementation
//!
//! Handles Wayland display initialization, backend setup, and event loop.

use anyhow::{Context, Result};
use calloop::EventLoop;
use smithay::{
    wayland::{
        compositor::CompositorState,
        shell::xdg::XdgShellState,
    },
    reexports::wayland_server::Display,
};
use tracing::{info, warn};
use crate::config::Config;
use crate::state::State;
use crate::backend::BackendManager;
use crate::autostart;

/// Main server structure
pub struct Server {
    display: Display<State>,
    event_loop: EventLoop<'static, State>,
    state: State,
    config: Config,
    _backend: BackendManager,
}

impl Server {
    /// Create a new server instance
    pub fn new(config: Config) -> Result<Self> {
        info!("Initializing Area Wayland Compositor");

        // Initialize backends
        let backend = BackendManager::new()?;

        // Create Wayland display
        let display = Display::new()?;

        // Create event loop
        let event_loop = EventLoop::<State>::try_new()
            .context("Failed to create event loop")?;

        // Initialize compositor state
        // CompositorState manages its own GlobalDispatch implementations
        let compositor_state = CompositorState::new::<State>(
            &display.handle(),
        );

        // Initialize XDG shell state
        // XdgShellState also manages its own GlobalDispatch
        let xdg_shell_state = XdgShellState::new::<State>(
            &display.handle(),
        );

        // Create application state
        let state = State::new(compositor_state, xdg_shell_state);

        info!("Server initialized (foundation)");

        Ok(Self {
            display,
            event_loop,
            state,
            config,
            _backend: backend,
        })
    }

    /// Run the server event loop
    pub fn run(&mut self) -> Result<()> {
        info!("Starting event loop");

        // Create socket - wayland-backend handles this
        let socket_name = "wayland-0".to_string();
        info!("Wayland socket will be: {}", socket_name);
        
        // Set WAYLAND_DISPLAY environment variable for clients
        std::env::set_var("WAYLAND_DISPLAY", &socket_name);

        // For now, use a simple polling approach
        // TODO: Properly integrate with calloop once backend is set up
        info!("Event loop ready (basic implementation)");
        warn!("Full event loop integration pending backend setup");

        // Launch autostart applications
        info!("Launching autostart applications");
        if let Err(e) = autostart::launch_autostart_apps(&self.config.autostart.applications) {
            warn!("Some autostart applications failed to launch: {}", e);
        }

        // Basic event loop - will be enhanced when backend is added
        loop {
            // Dispatch calloop events
            self.event_loop.dispatch(None, &mut self.state)
                .context("Event loop dispatch failed")?;

            // Flush Wayland display to send pending events
            let _ = self.display.flush_clients();

            // Small sleep to prevent busy loop
            std::thread::sleep(std::time::Duration::from_millis(10));

            // TODO: Process backend events
            // TODO: Render frames
            // TODO: Handle other events
        }
    }
}
