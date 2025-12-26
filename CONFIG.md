# Area Desktop Environment Configuration

This document lists all currently hardcoded settings in Area and provides implementation todos for making them configurable.

## Current Hardcoded Settings

### Input/Peripheral Settings

#### Mouse Settings
- **Mouse Acceleration**: Not configured (defaults to system/libinput)
  - **Location**: Not implemented
  - **Current Value**: System default
  - **Priority**: 🔴 **CRITICAL** - Required for user experience

- **Natural Scrolling**: Not configured
  - **Location**: Not implemented
  - **Current Value**: System default
  - **Priority**: 🟡 Medium

- **Scroll Speed**: Not configured
  - **Location**: Not implemented
  - **Current Value**: System default
  - **Priority**: 🟡 Medium

- **Left-Handed Mouse**: Not configured
  - **Location**: Not implemented
  - **Current Value**: System default
  - **Priority**: 🟢 Low (but important for accessibility)

- **Touch Support**: Not implemented
  - **Location**: Not implemented
  - **Current Value**: N/A
  - **Priority**: ⚪ Very Low (end of roadmap)

### Window Manager Settings

#### Window Decorations
- **Titlebar Height**: `32px`
  - **Location**: `src/wm/decorations.rs:9`, `src/wm/mod.rs:560,629,810`, `src/main.rs:598`
  - **Current Value**: `const TITLEBAR_HEIGHT: u16 = 32`
  - **Priority**: 🟡 Medium

- **Border Width**: `2px`
  - **Location**: `src/wm/decorations.rs:12`, `src/wm/mod.rs:561`
  - **Current Value**: `const BORDER_WIDTH: u16 = 2`
  - **Priority**: 🟡 Medium

- **Button Size**: `16px`
  - **Location**: `src/wm/decorations.rs:10`
  - **Current Value**: `const BUTTON_SIZE: u16 = 16`
  - **Priority**: 🟡 Medium

- **Button Padding**: `8px`
  - **Location**: `src/wm/decorations.rs:11`
  - **Current Value**: `const BUTTON_PADDING: u16 = 8`
  - **Priority**: 🟡 Medium

#### Window Colors (Nord Theme - Hardcoded)
- **Background Color**: `0x2e3440` (Polar Night Darkest)
  - **Location**: `src/wm/decorations.rs:15`
  - **Current Value**: `const COLOR_BG: u32 = 0x2e3440`
  - **Priority**: 🟡 Medium (should support terminal color themes)

- **Titlebar Color**: `0x3b4252` (Polar Night Lighter)
  - **Location**: `src/wm/decorations.rs:16`
  - **Current Value**: `const COLOR_TITLEBAR: u32 = 0x3b4252`
  - **Priority**: 🟡 Medium

- **Border Color**: `0x5e81ac` (Frost Blue)
  - **Location**: `src/wm/decorations.rs:17`
  - **Current Value**: `const COLOR_BORDER: u32 = 0x5e81ac`
  - **Priority**: 🟡 Medium

- **Close Button Color**: `0xbf616a` (Aurora Red)
  - **Location**: `src/wm/decorations.rs:18`
  - **Current Value**: `const COLOR_CLOSE: u32 = 0xbf616a`
  - **Priority**: 🟡 Medium

- **Maximize Button Color**: `0xa3be8c` (Aurora Green)
  - **Location**: `src/wm/decorations.rs:19`
  - **Current Value**: `const COLOR_MAX: u32 = 0xa3be8c`
  - **Priority**: 🟡 Medium

- **Minimize Button Color**: `0xebcb8b` (Aurora Yellow)
  - **Location**: `src/wm/decorations.rs:20`
  - **Current Value**: `const COLOR_MIN: u32 = 0xebcb8b`
  - **Priority**: 🟡 Medium

#### Window Behavior
- **Panel Height Offset**: `40px`
  - **Location**: `src/wm/mod.rs:369`, `src/shell/panel.rs:8`
  - **Current Value**: `const PANEL_HEIGHT: i32 = 40` / `const PANEL_HEIGHT: f32 = 40.0`
  - **Priority**: 🟡 Medium

- **Focus Mode**: Click-to-focus (implicit)
  - **Location**: `src/main.rs:608` (set_focus on ButtonPress)
  - **Current Value**: Implicit behavior
  - **Priority**: 🟢 Low

- **Raise on Focus**: Enabled (implicit)
  - **Location**: `src/wm/mod.rs:715-728` (configure_window with StackMode::ABOVE)
  - **Current Value**: Implicit behavior
  - **Priority**: 🟢 Low

### Panel/Shell Settings

#### Panel Appearance
- **Panel Height**: `40px`
  - **Location**: `src/shell/panel.rs:8`
  - **Current Value**: `const PANEL_HEIGHT: f32 = 40.0`
  - **Priority**: 🟡 Medium

- **Panel Position**: Top (hardcoded)
  - **Location**: `src/shell/panel.rs:30`
  - **Current Value**: `let position_top = true;`
  - **Priority**: 🟡 Medium

- **Panel Opacity**: `0.9` (90%)
  - **Location**: `src/shell/panel.rs:83`
  - **Current Value**: Hardcoded in render call
  - **Priority**: 🟡 Medium

- **Panel Background Color**: `RGB(0.2, 0.2, 0.2)` (Dark Gray)
  - **Location**: `src/shell/panel.rs:80-82`
  - **Current Value**: Hardcoded in render call
  - **Priority**: 🟡 Medium

#### Panel Elements
- **Button Width**: `80px`
  - **Location**: `src/shell/panel.rs:9`
  - **Current Value**: `const BUTTON_WIDTH: f32 = 80.0`
  - **Priority**: 🟢 Low

- **Button Height**: `30px`
  - **Location**: `src/shell/panel.rs:10`
  - **Current Value**: `const BUTTON_HEIGHT: f32 = 30.0`
  - **Priority**: 🟢 Low

- **Button Padding**: `5px`
  - **Location**: `src/shell/panel.rs:11`
  - **Current Value**: `const BUTTON_PADDING: f32 = 5.0`
  - **Priority**: 🟢 Low

- **Logout Button Position**: Right side (hardcoded)
  - **Location**: `src/shell/panel.rs:34`
  - **Current Value**: Calculated from right edge
  - **Priority**: 🟢 Low

### Keyboard Shortcuts

- **Launcher Key**: SUPER (Mod4) / Keycode 133, 134
  - **Location**: `src/wm/mod.rs:265`, `src/main.rs:728`
  - **Current Value**: Hardcoded keycodes `[133u8, 134u8]`
  - **Priority**: 🟡 Medium

- **Launcher Command**: `"navigator"`
  - **Location**: `src/main.rs:731`
  - **Current Value**: `std::process::Command::new("navigator")`
  - **Priority**: 🟡 Medium

### Compositor Settings

#### Rendering
- **Background Clear Color**: `RGB(0.15, 0.15, 0.15)` (Dark Gray)
  - **Location**: `src/compositor/mod.rs:415`
  - **Current Value**: `gl::ClearColor(0.15, 0.15, 0.15, 1.0)`
  - **Priority**: 🟢 Low

- **VSync**: Not explicitly configured (driver default)
  - **Location**: Not implemented
  - **Current Value**: Driver default
  - **Priority**: 🟡 Medium

- **Tear-Free**: Not configured
  - **Location**: Not implemented
  - **Current Value**: Driver default
  - **Priority**: 🟡 Medium

- **Unredirect Fullscreen**: Not configured
  - **Location**: Not implemented
  - **Current Value**: Not enabled
  - **Priority**: 🟢 Low

#### Effects
- **Animations**: Not implemented
  - **Location**: Not implemented
  - **Current Value**: N/A
  - **Priority**: 🟢 Low

- **Shadows**: Not implemented
  - **Location**: Not implemented
  - **Current Value**: N/A
  - **Priority**: 🟢 Low

- **Blur**: Not implemented
  - **Location**: Not implemented
  - **Current Value**: N/A
  - **Priority**: 🟢 Low

- **Transparency**: Not configured (opacity hardcoded)
  - **Location**: Various render calls
  - **Current Value**: Hardcoded opacity values
  - **Priority**: 🟡 Medium (efficiency important)

### Logout Dialog Settings

- **Dialog Width**: `300px`
  - **Location**: `src/shell/logout.rs:7`
  - **Current Value**: `const DIALOG_WIDTH: f32 = 300.0`
  - **Priority**: 🟢 Low

- **Dialog Height**: `150px`
  - **Location**: `src/shell/logout.rs:8`
  - **Current Value**: `const DIALOG_HEIGHT: f32 = 150.0`
  - **Priority**: 🟢 Low

- **Dialog Button Width**: `100px`
  - **Location**: `src/shell/logout.rs:9`
  - **Current Value**: `const BUTTON_WIDTH: f32 = 100.0`
  - **Priority**: 🟢 Low

- **Dialog Button Height**: `35px`
  - **Location**: `src/shell/logout.rs:10`
  - **Current Value**: `const BUTTON_HEIGHT: f32 = 35.0`
  - **Priority**: 🟢 Low

- **Dialog Button Spacing**: `20px`
  - **Location**: `src/shell/logout.rs:11`
  - **Current Value**: `const BUTTON_SPACING: f32 = 20.0`
  - **Priority**: 🟢 Low

## Implementation Todos

### Phase 1: Critical Input Settings (HIGH PRIORITY)

#### TODO-1: Mouse Acceleration Configuration
- **Priority**: 🔴 **CRITICAL**
- **Reason**: Essential for user experience - users need precise control over mouse sensitivity
- **Implementation**:
  - Add `xinput` feature to x11rb in `Cargo.toml`
  - Create `src/input.rs` module with `InputManager` struct
  - Implement `set_libinput_accel_speed()` using XInput extension
  - Enumerate all pointer devices and apply settings
  - Add config field: `[input.mouse] accel_speed = -0.8`
  - Apply during WM initialization

#### TODO-2: Mouse Smoothness/Acceleration Profile
- **Priority**: 🔴 **CRITICAL**
- **Reason**: Works with acceleration - users need adaptive vs flat profiles
- **Implementation**:
  - Extend `InputManager` with `set_libinput_accel_profile()`
  - Support adaptive/flat/custom profiles
  - Add config: `[input.mouse] accel_profile = "adaptive"`
  - Apply during WM initialization

#### TODO-3: Left-Handed Mouse Support
- **Priority**: 🟡 Medium (Accessibility)
- **Reason**: Important accessibility feature for left-handed users
- **Implementation**:
  - Add `set_libinput_left_handed()` to `InputManager`
  - Add config: `[input.mouse] left_handed = false`
  - Apply during WM initialization

### Phase 2: Configuration System Infrastructure

#### TODO-4: TOML Configuration System with Build-Time Defaults
- **Priority**: 🔴 **CRITICAL** (Foundation)
- **Reason**: All other configs depend on this infrastructure
- **Implementation**:
  - Create `src/config.rs` module
  - Define `Config` struct with `#[derive(Serialize, Deserialize)]`
  - Implement `Config::default()` with all current hardcoded values
  - Implement `Config::load()` to read from `~/.config/area/config.toml`
  - Use `dirs` crate for config directory
  - Auto-generate default config file on first run if missing
  - Use `serde` + `toml` for parsing
  - Add build script or `include_str!` for default config template

#### TODO-5: Config Value Marking System (Candid-like)
- **Priority**: 🟡 Medium
- **Reason**: User wants to mark config values similar to ICP/Candid
- **Implementation**:
  - Create `#[area_config]` derive macro or attribute
  - Mark config struct fields that are user-configurable
  - Generate documentation from marked fields
  - Possibly generate config schema/validation

### Phase 3: Window Manager Configuration

#### TODO-6: Window Decoration Geometry Configuration
- **Priority**: 🟡 Medium
- **Reason**: Users want to customize window decoration sizes
- **Implementation**:
  - Create `WindowDecorationConfig` struct with:
    - `titlebar_height: u16`
    - `border_width: u16`
    - `button_size: u16`
    - `button_padding: u16`
  - Replace all hardcoded constants with config values
  - Add to config: `[window_manager.decorations]` section
  - Use single "decoration area" object as user requested

#### TODO-7: Terminal Color Theme Support
- **Priority**: 🟡 Medium
- **Reason**: Users want to use terminal color themes for window decorations
- **Implementation**:
  - Research terminal color theme formats (alacritty, kitty, etc.)
  - Create `ColorTheme` enum/struct
  - Implement theme parser/loader
  - Map terminal colors to window decoration colors:
    - Background → Window background
    - Foreground → Titlebar text (future)
    - Color0-15 → Button colors, borders, etc.
  - Add config: `[window_manager.theme] source = "terminal"` or `source = "custom"`
  - Add config: `[window_manager.theme] terminal_theme_path = "~/.config/alacritty/alacritty.toml"`

#### TODO-8: Window Decoration Colors Configuration
- **Priority**: 🟡 Medium
- **Reason**: Users want customizable colors (via themes or manual)
- **Implementation**:
  - Add `WindowColors` struct to config
  - Support both terminal theme import and manual RGB/hex colors
  - Replace hardcoded `COLOR_*` constants
  - Add config: `[window_manager.colors]` section
  - Apply colors during window frame creation

#### TODO-9: Panel Configuration
- **Priority**: 🟡 Medium
- **Reason**: Users want customizable panel appearance
- **Implementation**:
  - Create `PanelConfig` struct
  - Add fields: `height`, `position`, `opacity`, `color`
  - Replace hardcoded values in `src/shell/panel.rs`
  - Add config: `[panel]` section

### Phase 4: Keyboard & Compositor Configuration

#### TODO-10: Keyboard Shortcuts Configuration
- **Priority**: 🟡 Medium
- **Reason**: Users want customizable keybindings
- **Implementation**:
  - Create `KeybindingsConfig` struct
  - Parse key combinations (e.g., "Super+Up", "Alt+F4")
  - Replace hardcoded keycodes in `src/wm/mod.rs` and `src/main.rs`
  - Add config: `[keybindings]` section
  - Implement keybinding parser/validator

#### TODO-11: Compositor Rendering Configuration
- **Priority**: 🟢 Low
- **Reason**: Advanced users want control over rendering
- **Implementation**:
  - Add `CompositorConfig` struct
  - Add VSync, tear-free, unredirect options
  - Add config: `[compositor]` section
  - Apply during compositor initialization

#### TODO-12: Transparency Configuration (Efficient)
- **Priority**: 🟡 Medium
- **Reason**: Users want transparency but it must be efficient
- **Implementation**:
  - Add opacity settings to window/panel configs
  - Implement efficient transparency (avoid overdraw)
  - Use GPU blending efficiently
  - Add config: `[compositor.transparency]` section

### Phase 5: Advanced Features (LOW PRIORITY)

#### TODO-13: Touch Support
- **Priority**: ⚪ Very Low (End of Roadmap)
- **Reason**: Mobile/touchscreen support - not urgent
- **Implementation**:
  - Research XInput touch device support
  - Implement touch event handling
  - Add touch gestures
  - Add config: `[input.touch]` section
  - **Note**: Mark as future work, not immediate priority

## Configuration File Structure

```toml
# Area Desktop Environment Configuration
# Auto-generated defaults on first run
# Edit this file to customize your desktop

[input.mouse]
# Mouse acceleration: -1.0 (slowest) to 1.0 (fastest)
# Negative values slow down the pointer
accel_speed = -0.8
# Acceleration profile: "adaptive" (Windows-like), "flat", or "custom"
accel_profile = "adaptive"
# Left-handed mouse: swap left/right buttons
left_handed = false
# Natural scrolling: scroll down to move content up (Mac-like)
natural_scrolling = false
# Scroll pixel distance per tick
scroll_speed = 15

[window_manager.decorations]
# Window decoration geometry (all in pixels)
titlebar_height = 32
border_width = 2
button_size = 16
button_padding = 8

[window_manager.theme]
# Theme source: "terminal" (import from terminal config) or "custom"
source = "terminal"
# Path to terminal color theme (if source = "terminal")
# Supports: alacritty.toml, kitty.conf, etc.
terminal_theme_path = "~/.config/alacritty/alacritty.toml"

[window_manager.colors]
# Manual colors (used if theme.source = "custom")
# Colors in hex format: 0xRRGGBB
background = 0x2e3440
titlebar = 0x3b4252
border = 0x5e81ac
close_button = 0xbf616a
maximize_button = 0xa3be8c
minimize_button = 0xebcb8b

[window_manager.behavior]
# Focus mode: "click_to_focus", "focus_follows_mouse", "sloppy_focus"
focus_mode = "click_to_focus"
# Raise window when focused
raise_on_focus = true
# Window gaps (for tiling, in pixels)
window_gaps = 0

[panel]
height = 40
# Position: "top", "bottom", "left", "right"
position = "top"
opacity = 0.9
# Background color: RGB values 0.0-1.0
color = [0.2, 0.2, 0.2]

[keybindings]
# Launcher key: key name or keycode
launcher_key = "Super"
# Command to run when launcher key is pressed
launcher_command = "navigator"
# Window management shortcuts (future)
# close_window = "Alt+F4"
# maximize_window = "Super+Up"
# minimize_window = "Super+Down"

[compositor]
# VSync: "on", "off", "adaptive"
vsync = "on"
# Prevent screen tearing
tear_free = true
# Unredirect fullscreen windows for performance
unredirect_fullscreen = false

[compositor.transparency]
# Enable transparency effects
enabled = true
# Default window opacity (0.0-1.0)
default_opacity = 1.0
```

## Notes

- All config values should have sensible defaults matching current hardcoded values
- Config file should be auto-generated on first run with current defaults
- Config changes should ideally be hot-reloadable (future enhancement)
- Terminal theme support should be extensible to support multiple formats
- Performance is critical - transparency and effects must be efficient
- Touch support is explicitly marked as low priority/end of roadmap

