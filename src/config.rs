//! Configuration system for Area Desktop Environment
//!
//! Loads configuration from TOML file at `~/.config/area/config.toml`
//! Auto-generates default config file on first run if missing.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub input: InputConfig,
    pub window_manager: WindowManagerConfig,
    pub panel: PanelConfig,
    pub keybindings: KeybindingsConfig,
    pub compositor: CompositorConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input: InputConfig::default(),
            window_manager: WindowManagerConfig::default(),
            panel: PanelConfig::default(),
            keybindings: KeybindingsConfig::default(),
            compositor: CompositorConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from file, or use defaults if file doesn't exist
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        
        if !config_path.exists() {
            info!("Config file not found at {:?}, using defaults", config_path);
            // Auto-generate default config file
            if let Err(e) = Self::save_default(&config_path) {
                warn!("Failed to create default config file: {}", e);
            }
            return Ok(Self::default());
        }
        
        let content = fs::read_to_string(&config_path)
            .context("Failed to read config file")?;
        
        let config: Config = toml::from_str(&content)
            .context("Failed to parse config file")?;
        
        info!("Configuration loaded from {:?}", config_path);
        debug!("Config: {:?}", config);
        
        Ok(config)
    }
    
    /// Get the path to the config file
    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to get config directory")?
            .join("area");
        
        Ok(config_dir.join("config.toml"))
    }
    
    /// Save default configuration to file
    fn save_default(path: &PathBuf) -> Result<()> {
        // Create config directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }
        
        let default_config = Self::default();
        let toml_string = toml::to_string_pretty(&default_config)
            .context("Failed to serialize default config")?;
        
        fs::write(path, toml_string)
            .context("Failed to write default config file")?;
        
        info!("Created default config file at {:?}", path);
        Ok(())
    }
}

/// Input configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    pub mouse: MouseConfig,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            mouse: MouseConfig::default(),
        }
    }
}

/// Mouse configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MouseConfig {
    /// Mouse acceleration: -1.0 (slowest) to 1.0 (fastest), negative = slower
    pub accel_speed: Option<f32>,
    /// Acceleration profile: "adaptive" (Windows-like), "flat", or "custom"
    pub accel_profile: Option<String>,
    /// Left-handed mouse: swap left/right buttons
    pub left_handed: Option<bool>,
    /// Natural scrolling: scroll down to move content up (Mac-like)
    pub natural_scrolling: Option<bool>,
    /// Scroll pixel distance per tick
    pub scroll_speed: Option<u32>,
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self {
            accel_speed: None, // System default
            accel_profile: None, // System default
            left_handed: Some(false),
            natural_scrolling: Some(false),
            scroll_speed: Some(15),
        }
    }
}

/// Window manager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowManagerConfig {
    pub decorations: WindowDecorationConfig,
    pub theme: ThemeConfig,
    pub colors: WindowColors,
    pub behavior: WindowBehaviorConfig,
}

impl Default for WindowManagerConfig {
    fn default() -> Self {
        Self {
            decorations: WindowDecorationConfig::default(),
            theme: ThemeConfig::default(),
            colors: WindowColors::default(),
            behavior: WindowBehaviorConfig::default(),
        }
    }
}

/// Window decoration geometry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowDecorationConfig {
    /// Titlebar height in pixels
    pub titlebar_height: u16,
    /// Border width in pixels
    pub border_width: u16,
    /// Button size in pixels
    pub button_size: u16,
    /// Button padding in pixels
    pub button_padding: u16,
}

impl Default for WindowDecorationConfig {
    fn default() -> Self {
        Self {
            titlebar_height: 32,
            border_width: 2,
            button_size: 16,
            button_padding: 8,
        }
    }
}

/// Theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// Theme source: "terminal" (import from terminal config) or "custom"
    pub source: String,
    /// Path to terminal color theme (if source = "terminal")
    pub terminal_theme_path: Option<String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            source: "custom".to_string(),
            terminal_theme_path: None,
        }
    }
}

/// Window colors configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowColors {
    /// Background color (hex: 0xRRGGBB)
    pub background: u32,
    /// Titlebar color (hex: 0xRRGGBB)
    pub titlebar: u32,
    /// Border color (hex: 0xRRGGBB)
    pub border: u32,
    /// Close button color (hex: 0xRRGGBB)
    pub close_button: u32,
    /// Maximize button color (hex: 0xRRGGBB)
    pub maximize_button: u32,
    /// Minimize button color (hex: 0xRRGGBB)
    pub minimize_button: u32,
}

impl Default for WindowColors {
    fn default() -> Self {
        // Nord Theme Colors (current hardcoded values)
        Self {
            background: 0x2e3440,      // Polar Night Darkest
            titlebar: 0x3b4252,        // Polar Night Lighter
            border: 0x5e81ac,          // Frost Blue
            close_button: 0xbf616a,    // Aurora Red
            maximize_button: 0xa3be8c, // Aurora Green
            minimize_button: 0xebcb8b, // Aurora Yellow
        }
    }
}

/// Window behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowBehaviorConfig {
    /// Focus mode: "click_to_focus", "focus_follows_mouse", "sloppy_focus"
    pub focus_mode: String,
    /// Raise window when focused
    pub raise_on_focus: bool,
    /// Window gaps (for tiling, in pixels)
    pub window_gaps: u32,
}

impl Default for WindowBehaviorConfig {
    fn default() -> Self {
        Self {
            focus_mode: "click_to_focus".to_string(),
            raise_on_focus: true,
            window_gaps: 0,
        }
    }
}

/// Panel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PanelConfig {
    /// Panel height in pixels
    pub height: f32,
    /// Panel position: "top", "bottom", "left", "right"
    pub position: String,
    /// Panel opacity (0.0-1.0)
    pub opacity: f32,
    /// Panel background color: RGB values 0.0-1.0
    pub color: [f32; 3],
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            height: 40.0,
            position: "top".to_string(),
            opacity: 0.9,
            color: [0.2, 0.2, 0.2], // Dark gray
        }
    }
}

/// Keyboard shortcuts configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingsConfig {
    /// Launcher key: key name or keycode
    pub launcher_key: String,
    /// Command to run when launcher key is pressed
    pub launcher_command: String,
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            launcher_key: "Super".to_string(),
            launcher_command: "navigator".to_string(),
        }
    }
}

/// Compositor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositorConfig {
    /// VSync mode: "on", "off", "adaptive"
    pub vsync: String,
    /// Prevent screen tearing
    pub tear_free: bool,
    /// Unredirect fullscreen windows for performance
    pub unredirect_fullscreen: bool,
    pub transparency: TransparencyConfig,
}

impl Default for CompositorConfig {
    fn default() -> Self {
        Self {
            vsync: "on".to_string(),
            tear_free: true,
            unredirect_fullscreen: false,
            transparency: TransparencyConfig::default(),
        }
    }
}

/// Transparency configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransparencyConfig {
    /// Enable transparency effects
    pub enabled: bool,
    /// Default window opacity (0.0-1.0)
    pub default_opacity: f32,
}

impl Default for TransparencyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_opacity: 1.0,
        }
    }
}

