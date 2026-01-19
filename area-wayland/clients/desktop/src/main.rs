//! Area Desktop
//!
//! Desktop manager for Area Wayland compositor (wallpaper, icons).

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use area_desktop::Desktop;

fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Area Desktop");

    let mut desktop = Desktop::new();
    desktop.run()?;

    Ok(())
}
