//! Area Panel
//!
//! A lightweight panel/bar for Area Wayland compositor using layer-shell protocol.

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use area_panel::Panel;

fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Area Panel");

    let mut panel = Panel::new();
    panel.run()?;

    Ok(())
}
