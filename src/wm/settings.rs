//! Settings Module
//!
//! Configuration management and Xfconf integration.
//! This matches xfwm4's settings system.

use anyhow::Result;
use tracing::{debug, info, warn};
use serde::{Deserialize, Serialize};
use crate::wm::placement::PlacementPolicy;

/// Window manager settings (matches xfwm4's XfwmParams)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowManagerSettings {
    /// Focus policy
    pub focus_policy: FocusPolicy,
    
    /// Focus new windows
    pub focus_new: bool,
    
    /// Raise on click
    pub raise_on_click: bool,
    
    /// Prevent focus stealing
    pub prevent_focus_stealing: bool,
    
    /// Compositor enabled
    pub compositor_enabled: bool,
    
    /// Unredirect fullscreen
    pub unredirect_fullscreen: bool,
    
    /// Workspace count
    pub workspace_count: u32,
    
    /// Wrap workspaces
    pub wrap_workspaces: bool,
    
    /// Placement policy
    pub placement_policy: PlacementPolicy,
    
    /// Snap to edges
    pub snap_to_edges: bool,
    
    /// Snap to windows
    pub snap_to_windows: bool,
    
    /// Snap distance
    pub snap_distance: i32,
    
    /// Wrap windows
    pub wrap_windows: bool,
}

/// Focus policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FocusPolicy {
    ClickToFocus,
    FocusFollowsMouse,
    SloppyFocus,
}

/// Settings manager
pub struct SettingsManager {
    /// Current settings
    pub settings: WindowManagerSettings,
    
    /// Settings file path
    pub settings_path: Option<String>,
}

impl SettingsManager {
    /// Create a new settings manager with defaults
    pub fn new() -> Self {
        Self {
            settings: WindowManagerSettings::default(),
            settings_path: None,
        }
    }
    
    /// Load settings from file
    pub fn load_from_file(&mut self, path: &str) -> Result<()> {
        debug!("Loading settings from {}", path);
        
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(settings) = toml::from_str::<WindowManagerSettings>(&content) {
                self.settings = settings;
                self.settings_path = Some(path.to_string());
                info!("Loaded settings from {}", path);
            } else {
                warn!("Failed to parse settings file {}, using defaults", path);
            }
        } else {
            warn!("Failed to read settings file {}, using defaults", path);
        }
        
        Ok(())
    }
    
    /// Save settings to file
    pub fn save_to_file(&self, path: &str) -> Result<()> {
        debug!("Saving settings to {}", path);
        
        let content = toml::to_string_pretty(&self.settings)?;
        std::fs::write(path, content)?;
        
        info!("Saved settings to {}", path);
        Ok(())
    }
    
    /// Get settings
    pub fn get_settings(&self) -> &WindowManagerSettings {
        &self.settings
    }
    
    /// Get mutable settings
    pub fn get_settings_mut(&mut self) -> &mut WindowManagerSettings {
        &mut self.settings
    }
    
    /// Update a setting
    pub fn update_setting<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut WindowManagerSettings),
    {
        f(&mut self.settings);
        
        // Auto-save if path is set
        if let Some(ref path) = self.settings_path {
            self.save_to_file(path)?;
        }
        
        Ok(())
    }
}

impl Default for WindowManagerSettings {
    fn default() -> Self {
        Self {
            focus_policy: FocusPolicy::ClickToFocus,
            focus_new: true,
            raise_on_click: true,
            prevent_focus_stealing: true,
            compositor_enabled: true,
            unredirect_fullscreen: true,
            workspace_count: 4,
            wrap_workspaces: false,
            placement_policy: PlacementPolicy::Smart,
            snap_to_edges: true,
            snap_to_windows: true,
            snap_distance: 10,
            wrap_windows: false,
        }
    }
}

impl Default for SettingsManager {
    fn default() -> Self {
        Self::new()
    }
}

