//! Desktop notifications via org.freedesktop.Notifications

use anyhow::Result;
use zbus::{Connection, proxy};

/// Proxy for org.freedesktop.Notifications
#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    /// Show a notification
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;
    
    /// Close a notification
    fn close_notification(&self, id: u32) -> zbus::Result<()>;
}

pub struct NotificationService {
    proxy: NotificationsProxy<'static>,
}

impl NotificationService {
    pub async fn new(conn: &Connection) -> Result<Self> {
        let proxy = NotificationsProxy::new(conn).await?;
        Ok(Self { proxy })
    }
    
    /// Show a simple notification
    pub async fn show_simple(
        &self,
        title: &str,
        message: &str,
    ) -> Result<u32> {
        let id = self.proxy.notify(
            "Area",           // app_name
            0,                // replaces_id (0 = new notification)
            "dialog-information", // app_icon
            title,            // summary
            message,          // body
            &[],              // actions
            std::collections::HashMap::new(), // hints
            5000,             // expire_timeout (5 seconds)
        ).await?;
        
        Ok(id)
    }
}
