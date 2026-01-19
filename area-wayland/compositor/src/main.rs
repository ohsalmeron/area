//! Area Wayland Compositor
//!
//! A lightweight Wayland compositor in Rust using Smithay, inspired by labwc's philosophy.

mod server;
mod state;
mod backend;
mod view;
mod xdg_shell;
mod input;
mod keybinds;
mod output;
mod renderer;
mod workspace;
mod decorations;
mod theme;
mod protocols;
mod config;
mod autostart;

use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"))
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Area Wayland Compositor");

    // Load configuration
    let config = config::Config::load()?;

    // Initialize and run server
    let mut server = server::Server::new(config)?;
    server.run()?;

    Ok(())
}
