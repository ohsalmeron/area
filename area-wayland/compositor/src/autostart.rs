//! Autostart functionality
//!
//! Launches applications when the compositor starts.

use anyhow::{Context, Result};
use std::process::Command;
use tracing::{info, warn};

/// Launch an application
pub fn launch_app(command: &str, args: &[&str]) -> Result<()> {
    info!("Launching: {} {:?}", command, args);
    
    // Get WAYLAND_DISPLAY from environment
    let wayland_display = std::env::var("WAYLAND_DISPLAY")
        .unwrap_or_else(|_| "wayland-0".to_string());
    
    let mut cmd = Command::new(command);
    cmd.args(args);
    cmd.env("WAYLAND_DISPLAY", &wayland_display);
    cmd.env("XDG_SESSION_TYPE", "wayland");
    cmd.env("XDG_CURRENT_DESKTOP", "Area");
    
    // Spawn in background
    match cmd.spawn() {
        Ok(child) => {
            info!("Launched {} (PID: {})", command, child.id());
            Ok(())
        }
        Err(e) => {
            warn!("Failed to launch {}: {}", command, e);
            Err(e).context(format!("Failed to launch {}", command))
        }
    }
}

/// Launch applications from autostart list
pub fn launch_autostart_apps(apps: &[String]) -> Result<()> {
    info!("Launching {} autostart applications", apps.len());
    
    for app in apps {
        // Parse app string (could be "alacritty" or "alacritty --option")
        let parts: Vec<&str> = app.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        
        let command = parts[0];
        let args = &parts[1..];
        
        if let Err(e) = launch_app(command, args) {
            warn!("Failed to autostart {}: {}", app, e);
            // Continue with other apps even if one fails
        }
        
        // Small delay between launches
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    
    Ok(())
}
