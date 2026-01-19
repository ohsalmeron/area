//! Area Notifications
//!
//! Notification daemon for Area Wayland compositor.

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use area_notifications::Notifications;

fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Area Notifications");

    let mut notifications = Notifications::new();
    notifications.run()?;

    Ok(())
}
