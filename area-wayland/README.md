# Area Wayland

> A lightweight Wayland compositor in Rust inspired by labwc's "no-bling" philosophy

A high-performance Wayland compositor written in Rust using Smithay, with modular desktop environment components. Focuses on minimalism, performance, and protocol compliance.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│         area-wayland (Compositor - Smithay)              │
│  • Window management                                     │
│  • Input handling                                        │
│  • Rendering (wgpu/software)                             │
│  • Workspace management                                  │
│  • Window decorations                                    │
└─────────────────┬───────────────────────────────────────┘
                   │ Standard Wayland Protocols
┌──────────────────▼───────────────────────────────────────┐
│         Desktop Environment Components                    │
│                                                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ area-panel   │  │ area-launcher│  │ area-desktop │  │
│  │ (Layer Shell)│  │ (XDG Shell)  │  │ (Layer Shell)│  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└───────────────────────────────────────────────────────────┘
```

## Features

- **Lightweight**: Minimal resource usage, no animations
- **Protocol-based**: Only standard Wayland protocols, no custom IPC
- **Modular**: Desktop components as separate Wayland clients
- **Type-safe**: Pure Rust with Smithay framework
- **Performance**: GPU-accelerated rendering with wgpu

## Project Structure

```
area-wayland/
├── compositor/          # Core compositor (Smithay)
│   ├── server.rs        # Wayland display, backend setup
│   ├── window.rs        # Window/view management
│   ├── input.rs         # Input handling (seat, keyboard, pointer)
│   ├── renderer.rs      # Rendering (wgpu or software)
│   ├── workspace.rs     # Workspace management
│   └── decorations.rs   # Window decorations
│
├── clients/             # Desktop environment components
│   ├── panel/           # Panel/bar (layer-shell client)
│   ├── launcher/        # Application launcher
│   ├── desktop/         # Desktop manager (wallpaper, icons)
│   └── notifications/   # Notification daemon
│
├── protocols/           # Custom protocol implementations (if needed)
└── common/              # Shared utilities
```

## Building

### Prerequisites

**System Dependencies:**

Arch Linux:
```bash
sudo pacman -S base-devel wayland libinput libxkbcommon mesa vulkan-icd-loader
```

Ubuntu/Debian:
```bash
sudo apt install build-essential libwayland-dev libinput-dev libxkbcommon-dev libegl1-mesa-dev libgles2-mesa-dev libvulkan-dev
```

Fedora:
```bash
sudo dnf install gcc wayland-devel libinput-devel libxkbcommon-devel mesa-libEGL-devel mesa-libGLES-devel vulkan-loader-devel
```

**Rust:**

Install Rust 1.70+ (stable or nightly):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build

```bash
cargo build --release
```

### Build Individual Components

```bash
# Build compositor only
cargo build --release -p area-compositor

# Build panel client
cargo build --release -p area-panel

# Build launcher client
cargo build --release -p area-launcher
```

### Development

```bash
# Run tests
cargo test

# Check code
cargo clippy
cargo fmt
```

## LightDM Session Setup

### Quick Setup

Run the setup script to build and install everything:

```bash
~/Documents/GitHub/area/session/setup-lightdm.sh
```

This will:
- Build the compositor in release mode
- Install the session files to the correct location
- Set up everything ready to use

Then log out and select "Area Wayland" from LightDM!

### Usage

1. **Log out** from your current session
2. **Select "Area Wayland"** from the LightDM session menu
3. **Log in** - the compositor will start and launch alacritty automatically

### Configuration

Autostart applications are configured in `~/.config/area-wayland/config.toml`:

```toml
[autostart]
applications = ["alacritty"]
```

You can add more applications:

```toml
[autostart]
applications = ["alacritty", "firefox", "thunar"]
```

### Troubleshooting

- **Compositor not found**: Make sure you've built it with `cargo build --release`
- **Alacritty not launching**: Check that alacritty is installed and in PATH
- **Session not appearing**: Make sure the desktop file is in `/usr/share/xsessions/` or `~/.local/share/xsessions/`
- **Check logs**: The compositor logs to stdout/stderr, check LightDM logs or journalctl

## Current Status

The compositor is in early development. Current implementation includes:
- ✅ Wayland display and protocol setup
- ✅ Event loop integration
- ✅ Autostart functionality
- ✅ LightDM session support
- ⚠️ Rendering not yet fully implemented (windows won't display yet)

## License

MIT
