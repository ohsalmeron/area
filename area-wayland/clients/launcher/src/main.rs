//! Area Launcher
//!
//! Application launcher for Area Wayland compositor.

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use area_launcher::Launcher;

fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Area Launcher");

    let mut launcher = Launcher::new();
    launcher.run()?;

    Ok(())
}
