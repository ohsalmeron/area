# Adding D-Bus Support to Area

This guide shows how to add D-Bus integration to your unified binary for desktop services and XFCE4 plugin compatibility.

## Why Add D-Bus?

You don't need D-Bus for internal communication (WM ↔ Compositor ↔ Shell), but you DO need it for:

1. **Desktop notifications** - Show system notifications
2. **Power management** - Battery info, suspend/hibernate
3. **Hardware events** - Monitor hotplug, lid switch
4. **XFCE4 plugins** (optional) - Load external panel plugins

## Adding zbus to Cargo.toml

```toml
[dependencies]
# Existing dependencies...
x11rb = { workspace = true }
tokio = { workspace = true }
anyhow = { workspace = true }

# Add zbus for D-Bus
zbus = { version = "5.0", default-features = false, features = ["tokio"] }
```

## Step 1: Basic D-Bus Connection

Create `src/dbus/mod.rs`:

```rust
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
```

## Step 2: Desktop Notifications

Create `src/dbus/notifications.rs`:

```rust
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
```

## Step 3: Power Management

Create `src/dbus/power.rs`:

```rust
//! Power management via org.freedesktop.login1

use anyhow::Result;
use zbus::{Connection, proxy};

/// Proxy for systemd-logind
#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login1Manager {
    /// Suspend the system
    fn suspend(&self, interactive: bool) -> zbus::Result<()>;
    
    /// Hibernate the system  
    fn hibernate(&self, interactive: bool) -> zbus::Result<()>;
    
    /// Power off the system
    fn power_off(&self, interactive: bool) -> zbus::Result<()>;
    
    /// Reboot the system
    fn reboot(&self, interactive: bool) -> zbus::Result<()>;
    
    /// Check if can suspend
    fn can_suspend(&self) -> zbus::Result<String>;
}

/// Proxy for UPower (battery info)
#[proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
trait UPower {
    /// Get whether on battery
    #[zbus(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}

pub struct PowerService {
    logind: Login1ManagerProxy<'static>,
    upower: UPowerProxy<'static>,
}

impl PowerService {
    pub async fn new(conn: &Connection) -> Result<Self> {
        let logind = Login1ManagerProxy::new(conn).await?;
        let upower = UPowerProxy::new(conn).await?;
        
        Ok(Self { logind, upower })
    }
    
    /// Suspend the system
    pub async fn suspend(&self) -> Result<()> {
        self.logind.suspend(true).await?;
        Ok(())
    }
    
    /// Shutdown the system
    pub async fn shutdown(&self) -> Result<()> {
        self.logind.power_off(true).await?;
        Ok(())
    }
    
    /// Reboot the system
    pub async fn reboot(&self) -> Result<()> {
        self.logind.reboot(true).await?;
        Ok(())
    }
    
    /// Check if on battery power
    pub async fn on_battery(&self) -> Result<bool> {
        Ok(self.upower.on_battery().await?)
    }
}
```

## Step 4: Integrate into Main Binary

Update `src/main.rs`:

```rust
mod wm;
mod compositor;
mod shared;
mod shell;
mod dbus;  // Add this

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;

struct AreaApp {
    // Existing fields...
    conn: Arc<x11rb::rust_connection::RustConnection>,
    wm: wm::WindowManager,
    compositor: compositor::Compositor,
    shell: shell::Shell,
    windows: HashMap<u32, Window>,
    
    // Add D-Bus support
    dbus: Option<dbus::DbusManager>,
    notifications: Option<dbus::notifications::NotificationService>,
    power: Option<dbus::power::PowerService>,
}

impl AreaApp {
    async fn new() -> Result<Self> {
        // ... existing X11 initialization ...
        
        // Initialize D-Bus (optional, won't fail if D-Bus unavailable)
        let dbus = match dbus::DbusManager::new().await {
            Ok(d) => {
                tracing::info!("D-Bus initialized");
                Some(d)
            }
            Err(e) => {
                tracing::warn!("D-Bus unavailable: {}. Desktop services disabled.", e);
                None
            }
        };
        
        // Initialize desktop services
        let notifications = if let Some(ref dbus) = dbus {
            match dbus::notifications::NotificationService::new(dbus.connection()).await {
                Ok(n) => Some(n),
                Err(e) => {
                    tracing::warn!("Notifications unavailable: {}", e);
                    None
                }
            }
        } else {
            None
        };
        
        let power = if let Some(ref dbus) = dbus {
            match dbus::power::PowerService::new(dbus.connection()).await {
                Ok(p) => Some(p),
                Err(e) => {
                    tracing::warn!("Power management unavailable: {}", e);
                    None
                }
            }
        } else {
            None
        };
        
        // Show startup notification
        if let Some(ref notif) = notifications {
            let _ = notif.show_simple(
                "Area Started",
                "Window manager and compositor ready"
            ).await;
        }
        
        Ok(Self {
            conn,
            wm,
            compositor,
            shell,
            windows: HashMap::new(),
            dbus,
            notifications,
            power,
        })
    }
}
```

## Step 5: Using D-Bus in Shell

Update `src/shell/logout.rs` to use real power management:

```rust
impl LogoutDialog {
    pub async fn handle_logout_action(&mut self, power: &Option<dbus::power::PowerService>) -> Result<()> {
        if !self.visible {
            return Ok(());
        }
        
        match self.selected_action {
            LogoutAction::Logout => {
                // Just exit the WM
                std::process::exit(0);
            }
            LogoutAction::Shutdown => {
                if let Some(power) = power {
                    power.shutdown().await?;
                } else {
                    // Fallback to system command
                    std::process::Command::new("systemctl")
                        .arg("poweroff")
                        .spawn()?;
                }
            }
            LogoutAction::Reboot => {
                if let Some(power) = power {
                    power.reboot().await?;
                } else {
                    std::process::Command::new("systemctl")
                        .arg("reboot")
                        .spawn()?;
                }
            }
            LogoutAction::Suspend => {
                if let Some(power) = power {
                    power.suspend().await?;
                } else {
                    std::process::Command::new("systemctl")
                        .arg("suspend")
                        .spawn()?;
                }
            }
        }
        
        Ok(())
    }
}
```

## Step 6: XFCE4 Plugin Support (Optional)

If you want to support external XFCE4 plugins, create `src/dbus/xfce_panel.rs`:

```rust
//! XFCE4 Panel D-Bus interface for plugin compatibility

use anyhow::Result;
use zbus::{Connection, interface, SignalContext};
use std::collections::HashMap;

/// Panel server that XFCE plugins can connect to
pub struct XfcePanelServer {
    plugins: HashMap<u32, PluginInfo>,
}

struct PluginInfo {
    name: String,
    pid: u32,
}

#[interface(name = "org.xfce.Panel")]
impl XfcePanelServer {
    /// Register a plugin
    async fn register_plugin(
        &mut self,
        #[zbus(signal_context)] ctx: SignalContext<'_>,
        plugin_id: u32,
        name: String,
    ) -> zbus::fdo::Result<()> {
        tracing::info!("Plugin {} registered: {}", plugin_id, name);
        
        self.plugins.insert(plugin_id, PluginInfo {
            name,
            pid: 0, // TODO: Get actual PID
        });
        
        // Emit signal that plugin was registered
        Self::plugin_registered(&ctx, plugin_id).await?;
        
        Ok(())
    }
    
    /// Unregister a plugin
    async fn unregister_plugin(&mut self, plugin_id: u32) -> zbus::fdo::Result<()> {
        self.plugins.remove(&plugin_id);
        tracing::info!("Plugin {} unregistered", plugin_id);
        Ok(())
    }
    
    /// Signal: Plugin registered
    #[zbus(signal)]
    async fn plugin_registered(ctx: &SignalContext<'_>, plugin_id: u32) -> zbus::Result<()>;
}

impl XfcePanelServer {
    pub async fn new(conn: &Connection) -> Result<Self> {
        let server = Self {
            plugins: HashMap::new(),
        };
        
        // Register D-Bus object at /org/xfce/Panel
        conn.object_server()
            .at("/org/xfce/Panel", server)
            .await?;
        
        // Request well-known name
        conn.request_name("org.xfce.Panel").await?;
        
        tracing::info!("XFCE Panel D-Bus interface registered");
        
        Ok(Self { plugins: HashMap::new() })
    }
}
```

## Testing D-Bus Integration

### Test Notifications

```bash
# Start your area WM
./target/debug/area

# In another terminal, send a test notification
gdbus call --session \
  --dest org.freedesktop.Notifications \
  --object-path /org/freedesktop/Notifications \
  --method org.freedesktop.Notifications.Notify \
  "Area" 0 "dialog-information" "Test" "Hello from D-Bus!" \
  [] {} 5000
```

### Test Power Management

```bash
# Check if can suspend
gdbus call --system \
  --dest org.freedesktop.login1 \
  --object-path /org/freedesktop/login1 \
  --method org.freedesktop.login1.Manager.CanSuspend
```

### Monitor D-Bus Messages

```bash
# Watch session bus
dbus-monitor --session

# Watch system bus (for power events)
dbus-monitor --system
```

## Performance Considerations

**D-Bus is NOT used for:**
- ❌ WM ↔ Compositor communication (in-process, direct memory)
- ❌ Compositor ↔ Shell communication (in-process, direct memory)
- ❌ Window state updates (in-process HashMap)

**D-Bus IS used for:**
- ✅ Desktop notifications (rare events)
- ✅ Power management (button clicks only)
- ✅ External plugins (separate processes)

**Impact:** Minimal! D-Bus calls happen at human speeds (button clicks), not at render speed (60 FPS).

## Debugging D-Bus Issues

```bash
# Check if D-Bus daemon is running
systemctl --user status dbus

# List available services
busctl --user list

# Introspect an object
busctl --user introspect org.freedesktop.Notifications \
  /org/freedesktop/Notifications
```

## Summary

| Component | Communication Method |
|-----------|---------------------|
| WM ↔ Compositor | Direct memory (in-process) |
| Compositor ↔ Shell | Direct memory (in-process) |
| WM ↔ X Server | X11 protocol (x11rb) |
| Area ↔ Desktop Services | D-Bus (zbus) |
| Area ↔ XFCE Plugins | D-Bus (optional) |

Your architecture is optimal: **fast in-process communication** where it matters (WM/Compositor/Shell), **D-Bus only** for external services that are already designed for it.

