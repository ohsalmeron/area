# Area

> The fastest lightweight desktop environment.

A high-performance X11 window manager with built-in OpenGL compositor, written in Rust. Provides a stable ecosystem to run all kinds of apps with a Windows-like experience and feel.

## Features

### ✅ Implemented

**Window Management**
- Full EWMH/ICCCM compliance for maximum app compatibility
- Window decorations (titlebar, close/maximize/minimize buttons)
- Window operations: move, resize, maximize, minimize, fullscreen
- Window dragging (Alt + Left Drag) and resizing (Alt + Right Drag)
- Double-click titlebar to maximize/restore
- Window state management (above, below, sticky, skip taskbar, etc.)
- Fullscreen support with compositor bypass for games
- Window focus and stacking management

**Compositor**
- OpenGL-based compositor with DRI3 support
- Damage tracking for efficient rendering
- Cursor management with shape updates
- Window texture management
- FPS monitoring
- VSync support

**Shell**
- Top panel/bar (configurable position, height, opacity, color)
- Logout dialog with power management (shutdown, reboot, suspend)
- Click handling for shell elements

**Desktop Integration**
- D-Bus integration (notifications, power management)
- Configuration system (TOML-based, auto-generated defaults)
- Mouse input configuration (acceleration, profile, left-handed)
- Launcher keybinding (Super key, configurable)
- Workspace support (EWMH desktops)

**System Services**
- Desktop notifications (org.freedesktop.Notifications)
- Power management (org.freedesktop.login1, UPower)
- Graceful shutdown handling

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Xorg Server                       │
└─────────────────┬───────────────────────────────────┘
                  │ XCB
┌─────────────────▼───────────────────────────────────┐
│              area-wm (Window Manager)                │
│  • Window management (move, resize, focus)          │
│  • Workspaces (EWMH desktops)                       │
│  • EWMH/ICCCM compliance                            │
│  • Window decorations                               │
│  • Keybindings                                      │
└─────────────────┬───────────────────────────────────┘
                  │ Async Event Stream
┌─────────────────▼───────────────────────────────────┐
│            area-compositor (OpenGL)                  │
│  • GPU-accelerated rendering                        │
│  • Damage tracking                                  │
│  • Cursor management                                │
│  • Window textures                                  │
└─────────────────┬───────────────────────────────────┘
                  │ Direct Rendering
┌─────────────────▼───────────────────────────────────┐
│              area-shell (Shell UI)                   │
│  • Top panel/bar                                    │
│  • Logout dialog                                    │
│  • Power management                                 │
└─────────────────────────────────────────────────────┘
```

## Building

### Prerequisites

- Rust 2024 edition (nightly)
- X11 development libraries
- OpenGL/Vulkan-capable GPU

```bash
# Arch Linux
sudo pacman -S base-devel libx11 libxcb vulkan-icd-loader

# Ubuntu/Debian
sudo apt install build-essential libx11-dev libxcb1-dev libvulkan1
```

### Build

```bash
cargo build --release
```

## Installation

### LightDM Session

```bash
# User installation (installs to ~/.local)
./scripts/install-session.sh

# System-wide installation (requires sudo)
sudo ./scripts/install-session.sh
```

**After installation:**
1. Log out from your current session
2. Select "Area" from the LightDM session menu
3. Log in

**Viewing logs:**
```bash
# Window manager logs
journalctl --user -u area-wm -f

# Shell logs
journalctl --user -u area-shell -f
```

## Configuration

Configuration is stored in `~/.config/area/config.toml` and is auto-generated on first run with sensible defaults.

### Key Settings

- **Mouse**: Acceleration, profile, left-handed mode
- **Window Decorations**: Titlebar height, border width, button sizes
- **Window Colors**: Background, titlebar, border, button colors
- **Panel**: Height, position, opacity, color
- **Keybindings**: Launcher key and command
- **Compositor**: VSync, tear-free, fullscreen unredirect

See `CONFIG.md` for the complete configuration reference.

## Keybindings

| Key | Action |
|-----|--------|
| `Alt + Left Drag` | Move window |
| `Alt + Right Drag` | Resize window |
| `Super` | Launch launcher (configurable) |
| `Double-click titlebar` | Toggle maximize |

## Roadmap

### ✅ Completed
- [x] X11 window manager with EWMH/ICCCM compliance
- [x] OpenGL compositor with damage tracking
- [x] Window decorations and management
- [x] Panel/bar with logout dialog
- [x] D-Bus integration (notifications, power)
- [x] Configuration system
- [x] Mouse input configuration
- [x] Workspace support

### 🔄 In Progress
- [ ] System tray (StatusNotifierItem/SNI)
- [ ] Desktop manager (icons, wallpaper)
- [ ] Window snapping (Windows-style tiling)

### 📋 Planned
- [ ] Taskbar/window list with previews
- [ ] Session management (save/restore windows)
- [ ] Multi-monitor support
- [ ] Startup notifications
- [ ] File associations & MIME types
- [ ] Overview mode enhancements
- [ ] Compiz effects (wobbly windows, cube, etc.)

## License

MIT