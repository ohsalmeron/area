//! Area Window Manager
//!
//! An X11 window manager written in Rust, designed to work with the
//! Bevy-powered Area shell for a Compiz Fusion-style desktop experience.

mod ewmh;
mod ipc;
mod window;
mod wm;
mod decorations;
mod composite;

use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "area_wm=debug,info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Area Window Manager");

    // Run the window manager
    wm::run().await
}
