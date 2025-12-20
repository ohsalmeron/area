# Area

> A Bevy-powered X11 desktop environment with Compiz Fusion-style effects.

![Status](https://img.shields.io/badge/status-MVP%20in%20progress-yellow)
![Platform](https://img.shields.io/badge/platform-Linux%20%2B%20X11-blue)
![Rust](https://img.shields.io/badge/rust-2024%20edition-orange)

## Vision

Area brings back the glory days of Compiz Fusion and Emerald — wobbly windows, desktop cube, smooth animations — but built with modern Rust and a game engine (Bevy) for maximum performance.

### What makes Area different?

- **Bevy-powered shell**: Real-time GPU-accelerated UI with game engine capabilities
- **X11 compatibility**: Works with all your existing Linux apps (Chromium, Firefox, Steam, etc.)
- **Agentic overlays**: Context-aware suggestions for terminal commands, build actions, git operations
- **Mobile-friendly UX**: Touch-friendly interactions, tap-to-run commands

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Xorg Server                       │
└─────────────────┬───────────────────────────────────┘
                  │ XCB
┌─────────────────▼───────────────────────────────────┐
│              area-wm (Window Manager)                │
│  • Window management (move, resize, focus)          │
│  • Workspaces                                       │
│  • EWMH compliance                                  │
│  • Keybindings                                      │
└─────────────────┬───────────────────────────────────┘
                  │ Unix Socket IPC (area-ipc)
┌─────────────────▼───────────────────────────────────┐
│              area-shell (Bevy UI)                    │
│  • Top bar with workspace switcher + clock          │
│  • Overview mode (Expo-style window grid)           │
│  • App launcher with fuzzy search                   │
│  • Agent overlays (WIP)                             │
│  • Animations & effects (WIP)                       │
└─────────────────────────────────────────────────────┘
```

## Building

### Prerequisites

- Rust 2024 edition (nightly)
- X11 development libraries
- Vulkan-capable GPU

```bash
# Arch Linux
sudo pacman -S base-devel libx11 libxcb vulkan-icd-loader

# Ubuntu/Debian
sudo apt install build-essential libx11-dev libxcb1-dev libvulkan1
```

### Build

```bash
# Clone
git clone https://github.com/bizkit/area
cd area

# Build all crates
cargo build --release

# Install (optional)
sudo install -Dm755 target/release/area-wm /usr/local/bin/
sudo install -Dm755 target/release/area-shell /usr/local/bin/
sudo install -Dm755 session/area-session /usr/local/bin/
sudo install -Dm644 session/area.desktop /usr/share/xsessions/
```

## Running

### Testing (without replacing your current DE)

```bash
# In a nested X server (Xephyr)
Xephyr :1 -screen 1920x1080 &
DISPLAY=:1 cargo run --bin area-wm &
DISPLAY=:1 cargo run --bin area-shell
```

### As your session

1. Install the session files (see above)
2. Log out
3. Select "Area" from your display manager (GDM, SDDM, etc.)
4. Log in

## Keybindings

| Key | Action |
|-----|--------|
| `Alt + Left Drag` | Move window |
| `Alt + Right Drag` | Resize window |
| `Super + 1-4` | Switch workspace |
| `Super + Return` | Launch terminal |
| `F9` | Toggle overview mode |
| `F10` | Toggle launcher |

## Roadmap

### Milestone 1: Basic WM ✅
- [x] X11 connection
- [x] Window management
- [x] Move/resize with Alt+drag
- [x] Workspaces

### Milestone 2: IPC ✅
- [x] Unix socket protocol
- [x] WM → Shell events
- [x] Shell → WM commands

### Milestone 3: Bar 🔄
- [x] Workspace indicator
- [x] Active window title
- [x] Clock
- [ ] System tray

### Milestone 4: Overview
- [x] Basic grid layout
- [ ] Smooth zoom animation
- [ ] Window thumbnails
- [ ] Drag to move between workspaces

### Milestone 5: Launcher
- [x] .desktop file scanning
- [x] Fuzzy search
- [x] Keyboard navigation
- [ ] Icons

### Milestone 6: Compiz Effects
- [ ] Wobbly windows
- [ ] Desktop cube
- [ ] Window open/close animations
- [ ] Blur & transparency

### Milestone 7: Agent Overlays
- [ ] Context detection
- [ ] Suggested actions UI
- [ ] Voice command indicator

### Milestone 8: Nested DEs (v2)
- [ ] Xephyr integration (run other DEs in workspace)
- [ ] VNC/RDP window support
- [ ] Per-workspace DE switching

## License

MIT
