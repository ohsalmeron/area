//! D-Bus integration for desktop services

use anyhow::{Context, Result};
use zbus::Connection;
use std::sync::Arc;

pub mod notifications;
pub mod power;

pub struct DbusManager {
    conn: Arc<Connection>,
}

impl DbusManager {
    /// Connect to session D-Bus
    pub async fn new() -> Result<Self> {
        let conn = Connection::session()
            .await
            .context("Failed to connect to D-Bus session bus")?;
        
        tracing::info!("Connected to D-Bus session bus");
        
        Ok(Self {
            conn: Arc::new(conn),
        })
    }
    
    /// Get connection (for creating proxies)
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}
