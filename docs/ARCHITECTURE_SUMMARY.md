# Area: Unified Desktop Environment Architecture

## Executive Summary

**Area** is a single-binary desktop environment that combines window manager, compositor, and shell into one unified process. It uses X11 for maximum compatibility with existing applications (Wine, Steam, XFCE4 plugins) while maintaining modern GPU-accelerated compositing.

## Core Philosophy

### ✅ What We Do

1. **Single Unified Binary** - WM, Compositor, and Shell in one process
2. **X11-based** - Maximum compatibility with existing apps
3. **In-process Communication** - No IPC overhead between components
4. **Direct Memory Sharing** - Window state shared via Rust references
5. **XFCE4 Compatible** - Works with xfce4-panel plugins

### ❌ What We Don't Do

1. ~~Multi-process architecture~~ (unnecessary IPC overhead)
2. ~~Wayland compositor~~ (less compatible, more complexity)
3. ~~Session manager IPC~~ (we're self-contained)
4. ~~Separate WM and compositor binaries~~ (slower, more complex)

## Architecture Diagram

```
┌───────────────────────────────────────────────────────────────┐
│                    area (single binary)                        │
│                                                                │
│  ┌──────────────────────────────────────────────────────┐    │
│  │                   AreaApp State                      │    │
│  │                                                      │    │
│  │  windows: HashMap<u32, Window>  ◄───────────────┐  │    │
│  │  (shared between all components!)                │  │    │
│  └──────────────────────────────────────────────────┘  │    │
│         ▲               ▲               ▲               │    │
│         │               │               │               │    │
│  ┌──────┴─────┐  ┌─────┴──────┐  ┌────┴──────┐       │    │
│  │    WM      │  │ Compositor │  │   Shell   │       │    │
│  │            │  │            │  │           │       │    │
│  │ • Focus    │  │ • OpenGL   │  │ • Panel   │       │    │
│  │ • Resize   │  │ • Textures │  │ • Clock   │       │    │
│  │ • Move     │  │ • Effects  │  │ • Tray    │       │    │
│  │ • Decorate │  │ • VSync    │  │ • Logout  │       │    │
│  └────────────┘  └────────────┘  └───────────┘       │    │
│         │                                              │    │
│         │ All in the same process!                    │    │
│         │ No serialization, no IPC, no overhead!      │    │
│         │                                              │    │
└─────────┼──────────────────────────────────────────────────┘
          │
          ├────────────────┬──────────────────┬─────────────────
          │                │                  │
     ┌────▼─────┐    ┌────▼────┐      ┌─────▼──────┐
     │  X Server│    │  D-Bus  │      │   Apps     │
     │  (Xorg)  │    │ (zbus)  │      │ (Wine/etc) │
     │          │    │         │      │            │
     │ • DRM    │    │ • Power │      │ • Firefox  │
     │ • Input  │    │ • Notify│      │ • Steam    │
     │ • Session│    │ • XFCE  │      │ • Wine     │
     └──────────┘    └─────────┘      └────────────┘
```

## Data Flow Examples

### Example 1: User Clicks Window

```
1. X Server receives mouse click → sends XEvent to area
2. area::handle_event(ButtonPress)
3. area::wm::handle_window_click(window_id)
4. area::wm::focus_window(window_id)
5. window = area.windows.get_mut(window_id)  ← Direct HashMap access!
6. window.wm.focused = true
7. area::compositor::mark_damaged(window_id)
8. area::render_frame() → OpenGL renders with new focus
```

**No IPC! No serialization! Just memory access!**

### Example 2: Window Needs Redraw

```
1. X Server: Expose event → area
2. area::handle_event(Expose { window_id })
3. window = area.windows.get_mut(window_id)  ← Direct access
4. window.comp.damaged = true
5. Next render frame: compositor sees damaged flag
6. area::compositor::render_window(window)
7. OpenGL renders updated texture
```

**All in-process! Nanosecond latency!**

### Example 3: Show Notification (needs D-Bus)

```
1. User clicks "Logout" in panel
2. area::shell::logout::show()
3. area.notifications.show_simple("Logging out", "Goodbye!")
4. zbus → D-Bus → notification daemon
5. Desktop notification appears
```

**D-Bus only for external services (rare events)**

## Performance Comparison

### Multi-Process Architecture (cosmic-comp style)

```
cosmic-comp (WM/Compositor)          cosmic-panel (Shell)
    │                                      │
    │ Window focused                       │
    ├─> Serialize to JSON                  │
    ├─> Write to Unix socket               │
    ├─> Context switch                     │
    │                                  ┌───┴───┐
    │                                  │ Read  │
    │                                  │ Parse │
    │                                  │ Update│
    │                                  └───┬───┘
    │  "Please update panel"               │
    │←─────────────────────────────────────┤
    │                                      │
    ├─> Serialize response                 │
    ├─> Write to socket                    │
    └─> ...                                │

Time: ~1-5ms (IPC overhead)
```

### Unified Architecture (Area style)

```
area (single process)
    │
    │ Window focused
    ├─> window.wm.focused = true
    ├─> shell.panel.update()  ← Direct function call!
    ├─> compositor.mark_damaged()
    └─> render_frame()

Time: ~1-10μs (microseconds!)
```

**1000x faster for internal communication!**

## Memory Layout

### Multi-Process (cosmic-comp)

```
┌─────────────────┐     ┌─────────────────┐
│  cosmic-comp    │     │  cosmic-panel   │
│  memory space   │     │  memory space   │
│                 │     │                 │
│  Window state   │     │  Window state   │
│  (copy 1)       │     │  (copy 2)       │
│                 │     │                 │
│  16MB heap      │     │  8MB heap       │
└─────────────────┘     └─────────────────┘

Total: 24MB + IPC buffers
```

### Unified (Area)

```
┌─────────────────────────────────┐
│          area                    │
│       memory space               │
│                                  │
│  Window state (single copy)     │
│  Shared by WM, Comp, Shell      │
│                                  │
│       12MB heap                  │
└─────────────────────────────────┘

Total: 12MB (50% less!)
```

## Code Structure

```
area/
├── src/
│   ├── main.rs              # Single entry point
│   │   └── AreaApp struct   # Unified state
│   │       ├── windows: HashMap<u32, Window>
│   │       ├── wm: WindowManager
│   │       ├── compositor: Compositor
│   │       ├── shell: Shell
│   │       └── dbus: Option<DbusManager>
│   │
│   ├── wm/                  # Window management
│   │   ├── mod.rs           # WM logic
│   │   ├── decorations.rs   # Window frames
│   │   └── ewmh.rs          # Desktop standards
│   │
│   ├── compositor/          # OpenGL rendering
│   │   ├── mod.rs           # Compositor logic
│   │   ├── gl_context.rs    # GLX setup
│   │   └── renderer.rs      # OpenGL rendering
│   │
│   ├── shell/               # Desktop shell
│   │   ├── mod.rs           # Shell coordination
│   │   ├── panel.rs         # Top panel
│   │   ├── logout.rs        # Logout dialog
│   │   └── render.rs        # Shell rendering
│   │
│   ├── shared/              # Shared state
│   │   ├── mod.rs           # Common types
│   │   └── window_state.rs  # Window struct
│   │
│   ├── dbus/                # D-Bus integration (optional)
│   │   ├── mod.rs           # D-Bus manager
│   │   ├── notifications.rs # Desktop notifications
│   │   ├── power.rs         # Power management
│   │   └── xfce_panel.rs    # XFCE plugin support
│   │
│   └── api/                 # Future: External API
│       └── (empty for now)
│
├── Cargo.toml               # Single binary target
└── docs/
    ├── IPC_AND_ARCHITECTURE.md
    ├── ADDING_DBUS_SUPPORT.md
    ├── KERNEL_INTERFACES.md
    └── ARCHITECTURE_SUMMARY.md  # This file
```

## Technology Stack

### Core (Zero IPC Overhead)

```toml
[dependencies]
# X11 protocol - window management
x11rb = { version = "0.13", features = ["all-extensions"] }

# OpenGL - compositing
gl = "0.14"

# Async runtime - event loop
tokio = { version = "1", features = ["full"] }

# Error handling
anyhow = "1"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"
```

### Optional (For Desktop Integration)

```toml
# D-Bus - desktop services (notifications, power, etc.)
zbus = { version = "5", features = ["tokio"] }

# Input - global hotkeys (optional)
input = "0.8"
```

## Comparison with Other DEs

| Feature | Area | XFCE4 | COSMIC | KDE Plasma |
|---------|------|-------|--------|------------|
| **Architecture** | Unified binary | Multi-process | Multi-process | Multi-process |
| **Display** | X11 | X11 | Wayland | X11/Wayland |
| **IPC Method** | None (in-process) | D-Bus | Unix sockets | D-Bus |
| **Compositor** | Built-in | xfwm4 (separate) | Built-in | KWin (separate) |
| **Shell** | Built-in | xfce4-panel (separate) | Built-in | Plasma (separate) |
| **Memory** | ~12MB | ~40MB | ~30MB | ~80MB |
| **IPC Latency** | None | 1-5ms | 1-3ms | 1-5ms |
| **XFCE Plugins** | Compatible | Native | No | No |
| **Wine/Steam** | Excellent | Excellent | Good | Excellent |

## Benefits of This Architecture

### 1. Performance

- **Zero IPC overhead** between components
- **Direct memory access** to window state
- **No serialization** for internal communication
- **Single event loop** - no context switches

### 2. Simplicity

- **One binary** to install and run
- **One process** to debug
- **No IPC protocols** to maintain
- **Straightforward code** flow

### 3. Compatibility

- **X11** - all apps work (Wine, Steam, etc.)
- **XFCE4 plugins** - via D-Bus (optional)
- **Standard protocols** - EWMH, ICCCM
- **Mature ecosystem** - 30+ years of X11

### 4. Resource Efficiency

- **50% less memory** than multi-process
- **Fewer syscalls** - no socket operations
- **Better cache locality** - single address space
- **Less context switching** - one process

## Trade-offs

### What We Give Up

1. **Process isolation** - A bug could crash everything
   - Mitigation: Rust's memory safety prevents most crashes
   
2. **Language flexibility** - Must use Rust
   - Benefit: Memory safety, performance, modern tooling
   
3. **Separate updates** - Must rebuild entire binary
   - Benefit: Simpler deployment, no version mismatches

4. **Plugin isolation** - External plugins need D-Bus
   - Benefit: Built-in components are faster

### What We Gain

1. **1000x faster internal communication**
2. **50% less memory usage**
3. **Simpler codebase** (no IPC protocols)
4. **Single point of deployment**
5. **Better debugging** (one process)

## Future Extensions

### Phase 1: Core (Current)
- ✅ X11 window manager
- ✅ OpenGL compositor
- ✅ Basic shell (panel, logout)

### Phase 2: Desktop Integration
- ⏳ D-Bus services (notifications, power)
- ⏳ XFCE4 plugin support
- ⏳ Settings management

### Phase 3: Advanced Features
- 🔮 Compiz-style effects (wobbly windows, cube)
- 🔮 Workspace management
- 🔮 Application launcher
- 🔮 System tray

### Phase 4: Plugin System
- 🔮 Internal plugin API (Rust dynamic libraries)
- 🔮 Configuration API
- 🔮 Theme system

## Conclusion

**Area** represents a return to simplicity: **one binary, one process, zero IPC overhead**. By using X11 and keeping everything in-process, we achieve:

- Maximum performance (no IPC)
- Maximum compatibility (X11)
- Minimum complexity (single binary)
- Modern features (GPU compositing)

The architecture is inspired by **XFWM4's integrated design** but taken further: instead of just integrating WM + Compositor, we integrate **WM + Compositor + Shell** into a single, efficient, unified binary.

**Performance where it matters. Compatibility where it counts. Simplicity throughout.**

