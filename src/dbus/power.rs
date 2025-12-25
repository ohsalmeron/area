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
    /// WHY: Part of PowerService API, planned for use when UI adds Suspend button.
    /// SPECIFIC PLAN: Task "Phase 2: Advanced Features", Owner: Bizkit
    #[allow(dead_code)]
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
    /// WHY: Part of PowerService API, planned for use when UI adds Reboot button.
    /// SPECIFIC PLAN: Task "Phase 2: Advanced Features", Owner: Bizkit
    #[allow(dead_code)]
    pub async fn reboot(&self) -> Result<()> {
        self.logind.reboot(true).await?;
        Ok(())
    }
    
    /// Check if on battery power
    /// WHY: Part of PowerService API, planned for use when UI adds Battery indicator.
    /// SPECIFIC PLAN: Task "Phase 2: Advanced Features", Owner: Bizkit
    #[allow(dead_code)]
    pub async fn on_battery(&self) -> Result<bool> {
        Ok(self.upower.on_battery().await?)
    }
}
