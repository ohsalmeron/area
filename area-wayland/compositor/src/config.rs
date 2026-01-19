//! Configuration system for Area Wayland Compositor

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub compositor: CompositorConfig,
    pub window_manager: WindowManagerConfig,
    pub input: InputConfig,
    pub keybindings: KeybindingsConfig,
    #[serde(default)]
    pub autostart: AutostartConfig,
}

impl Config {
    /// Load configuration from file, or use defaults if file doesn't exist
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        
        if !config_path.exists() {
            info!("Config file not found, using defaults: {:?}", config_path);
            let config = Self::default();
            config.save()?;
            return Ok(config);
        }

        let content = fs::read_to_string(&config_path)
            .context("Failed to read config file")?;
        
        let config: Config = toml::from_str(&content)
            .context("Failed to parse config file")?;
        
        debug!("Loaded configuration from {:?}", config_path);
        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        fs::write(&config_path, content)
            .context("Failed to write config file")?;
        
        info!("Saved configuration to {:?}", config_path);
        Ok(())
    }

    fn config_path() -> Result<PathBuf> {
        Ok(dirs::config_dir()
            .context("Failed to get config directory")?
            .join("area-wayland")
            .join("config.toml"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositorConfig {
    /// Use GPU acceleration (wgpu) or software rendering
    pub use_gpu: bool,
    /// Enable VSync
    pub vsync: bool,
    /// Enable tear-free rendering
    pub tear_free: bool,
}

impl Default for CompositorConfig {
    fn default() -> Self {
        Self {
            use_gpu: true,
            vsync: true,
            tear_free: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowManagerConfig {
    /// Window border width
    pub border_width: u32,
    /// Titlebar height
    pub titlebar_height: u32,
    /// Enable window decorations
    pub decorations: bool,
}

impl Default for WindowManagerConfig {
    fn default() -> Self {
        Self {
            border_width: 1,
            titlebar_height: 30,
            decorations: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    /// Mouse acceleration
    pub mouse_acceleration: f64,
    /// Natural scrolling
    pub natural_scrolling: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            mouse_acceleration: 1.0,
            natural_scrolling: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingsConfig {
    /// Keybinding to switch windows (Alt+Tab)
    pub switch_window: String,
    /// Keybinding to close window
    pub close_window: String,
    /// Keybinding to maximize window
    pub maximize_window: String,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            switch_window: "Alt+Tab".to_string(),
            close_window: "Alt+F4".to_string(),
            maximize_window: "Super+A".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutostartConfig {
    /// List of applications to launch on startup
    pub applications: Vec<String>,
}

impl Default for AutostartConfig {
    fn default() -> Self {
        Self {
            applications: vec!["alacritty".to_string()],
        }
    }
}
