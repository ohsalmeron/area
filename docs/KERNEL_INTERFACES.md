# Kernel-Level Interfaces for Desktop Environments

This guide explains the low-level kernel interfaces your desktop environment can use, comparing X11 vs Wayland approaches.

## The Stack: From Kernel to Desktop

```
┌────────────────────────────────────────────────┐
│         Your App (area)                        │ User space
├────────────────────────────────────────────────┤
│         X11 Server (Xorg)                      │ User space
├────────────────────────────────────────────────┤
│  Libraries: x11rb, libinput, libseat          │ User space
├────────────────────────────────────────────────┤
│  System Calls: open(), ioctl(), read(), ...   │ Kernel interface
├────────────────────────────────────────────────┤
│  Kernel Subsystems:                            │ Kernel space
│  • DRM/KMS (graphics)                          │
│  • evdev (input)                               │
│  • logind (session)                            │
└────────────────────────────────────────────────┘
```

## Current Architecture: X11 (Your Choice)

### What X11 Gives You

```
┌─────────────────────────────────────────┐
│        area (your binary)               │
│  • Window management                    │
│  • Compositing (GLX)                    │
│  • Shell UI                             │
└────────────┬────────────────────────────┘
             │ X11 Protocol (x11rb)
┌────────────▼────────────────────────────┐
│        Xorg Server                      │
│  • Graphics (DRM/KMS)                   │
│  • Input (evdev/libinput)               │
│  • Session (logind)                     │
│  • Hardware management                  │
└────────────┬────────────────────────────┘
             │ System calls
┌────────────▼────────────────────────────┐
│        Linux Kernel                     │
│  • /dev/dri/* (GPU)                     │
│  • /dev/input/* (keyboard, mouse)       │
│  • /sys/class/drm/* (monitor info)      │
└─────────────────────────────────────────┘
```

**Advantages:**
- ✅ Xorg handles all kernel complexity
- ✅ Compatible with all X11 apps (Wine, Steam)
- ✅ XFCE4 plugin compatibility
- ✅ Mature, stable APIs
- ✅ You focus on WM logic

**What Xorg Does For You:**
1. **Graphics**: Opens `/dev/dri/card0`, handles DRM/KMS
2. **Input**: Reads from `/dev/input/event*`, processes raw events
3. **Session**: Talks to systemd-logind for VT switching
4. **Monitor hotplug**: Monitors `/sys/class/drm` for changes

## Alternative: Direct Kernel Access (Wayland Style)

If you wanted to bypass X11 and talk directly to kernel:

```
┌─────────────────────────────────────────┐
│        area (your binary)               │
│  • Window management                    │
│  • Wayland compositor                   │
│  • Shell UI                             │
│  • Session management                   │
│  • Input handling                       │
└───┬─────┬─────┬─────┬───────────────────┘
    │     │     │     │ Direct syscalls
    │     │     │     │
    │     │     │     └──> /dev/dri/card0 (DRM/KMS)
    │     │     └────────> /dev/input/event* (evdev)
    │     └──────────────> systemd-logind (session)
    └────────────────────> /sys/class/drm (hotplug)
```

**This is what cosmic-comp does!**

## Kernel Interfaces Explained

### 1. DRM/KMS - Graphics Hardware

**What it is:** Direct Rendering Manager / Kernel Mode Setting

**Device nodes:**
```bash
/dev/dri/card0       # GPU device
/dev/dri/card1       # Second GPU (if you have one)
/dev/dri/renderD128  # Render-only node
```

**What you can do:**
```rust
use drm::control::Device;

// Open GPU
let fd = std::fs::OpenOptions::new()
    .read(true)
    .write(true)
    .open("/dev/dri/card0")?;

// Get connected monitors
let resources = Device::resource_handles(&fd)?;
for connector in resources.connectors() {
    let info = Device::get_connector(&fd, *connector)?;
    println!("Monitor: {:?}", info);
}

// Set display mode
let crtc = resources.crtcs()[0];
Device::set_crtc(&fd, crtc, fb_id, &connectors, (0, 0), Some(mode))?;
```

**System calls used:**
- `open()` - Open `/dev/dri/card0`
- `ioctl()` - Control GPU (DRM_IOCTL_MODE_GETRESOURCES, etc.)
- `mmap()` - Map framebuffer memory

### 2. evdev - Raw Input Events

**What it is:** Event device interface for keyboards, mice, touchpads

**Device nodes:**
```bash
/dev/input/event0    # Keyboard
/dev/input/event1    # Mouse
/dev/input/event2    # Touchpad
```

**What you can do:**
```rust
use input::Libinput;
use input::event::Event;

// Open input devices
let mut input = Libinput::new_from_udev(interface);
input.udev_assign_seat("seat0")?;

// Read events
for event in input.events() {
    match event {
        Event::Keyboard(KeyboardEvent::Key(e)) => {
            println!("Key pressed: {}", e.key());
        }
        Event::Pointer(PointerEvent::Motion(e)) => {
            println!("Mouse moved: ({}, {})", e.dx(), e.dy());
        }
        _ => {}
    }
}
```

**System calls used:**
- `open()` - Open `/dev/input/event*`
- `read()` - Read raw input events
- `ioctl()` - Query device capabilities

**Raw format:**
```rust
struct InputEvent {
    time: TimeVal,
    type_: u16,  // EV_KEY, EV_REL, EV_ABS
    code: u16,   // KEY_A, REL_X, ABS_X
    value: i32,  // 0=release, 1=press, 2=repeat
}
```

### 3. libseat - Session Management

**What it is:** Library for managing login sessions and device access

**What it does:**
- Gives you permission to access `/dev/dri/*` and `/dev/input/*`
- Handles VT switching (Ctrl+Alt+F1, etc.)
- Releases devices when you switch away

```rust
use libseat::Seat;

// Open session
let mut seat = Seat::open()?;

// Get notified when VT switches
seat.set_session_callback(|active| {
    if active {
        println!("Session activated - resume rendering");
    } else {
        println!("Session deactivated - pause rendering");
    }
});

// Open device (libseat handles permissions)
let fd = seat.open_device("/dev/dri/card0")?;
```

**System calls used:**
- Socket connection to systemd-logind via D-Bus
- `open()` - Opens devices with elevated permissions

### 4. udev - Device Monitoring

**What it is:** Monitor for device hotplug events

```rust
use udev::MonitorBuilder;

// Watch for DRM device changes
let monitor = MonitorBuilder::new()?
    .match_subsystem("drm")?
    .listen()?;

for event in monitor.iter() {
    match event.event_type() {
        udev::EventType::Add => {
            println!("Monitor plugged in: {:?}", event.device());
        }
        udev::EventType::Remove => {
            println!("Monitor unplugged: {:?}", event.device());
        }
        _ => {}
    }
}
```

**System calls used:**
- Socket to udev daemon
- Reads from `/sys/class/drm/*` (sysfs)

## Rust Libraries for Kernel Access

### Using nix (POSIX syscalls)

```rust
use nix::sys::stat::Mode;
use nix::fcntl::{open, OFlag};
use nix::sys::ioctl;

// Open device
let fd = open("/dev/dri/card0", OFlag::O_RDWR, Mode::empty())?;

// Custom ioctl
ioctl_read!(get_version, b'D', 0x00, drm_version);
unsafe {
    let mut ver = drm_version::default();
    get_version(fd, &mut ver)?;
    println!("DRM version: {}.{}", ver.major, ver.minor);
}
```

### Using rustix (Modern, safer)

```rust
use rustix::fs::{open, OFlags, Mode};
use rustix::io::fcntl_getfd;

// Open device
let fd = open("/dev/dri/card0", OFlags::RDWR, Mode::empty())?;

// Get file descriptor flags
let flags = fcntl_getfd(&fd)?;
println!("FD flags: {:?}", flags);
```

### Using smithay (High-level Wayland compositor framework)

```rust
use smithay::backend::drm::{DrmDevice, DrmNode};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::session::libseat::LibSeatSession;

// This is what cosmic-comp uses!

// Open session
let (session, notifier) = LibSeatSession::new()?;

// Open GPU
let drm_node = DrmNode::from_path("/dev/dri/card0")?;
let drm_device = DrmDevice::new(drm_node, true)?;

// Setup input
let libinput = Libinput::new_from_udev(session.clone());
libinput.udev_assign_seat("seat0")?;
```

## What You Should Use for Area

Since you're using **X11**, here's the recommended approach:

```rust
// Cargo.toml
[dependencies]
x11rb = { version = "0.13", features = ["all-extensions"] }  # X11 protocol
gl = "0.14"              # OpenGL for compositing
tokio = { version = "1" } # Async runtime
zbus = { version = "5" }  # D-Bus for desktop services (optional)

# For direct input monitoring (optional, if you want hotkeys outside X11)
input = "0.8"            # libinput bindings
```

### Recommended Architecture

```rust
// src/main.rs
struct AreaApp {
    // X11 handles graphics, input, session for you
    x11_conn: x11rb::RustConnection,
    
    // OpenGL for compositing (via GLX)
    gl_context: GlContext,
    
    // Optional: Direct input for global hotkeys
    input: Option<Libinput>,
    
    // Optional: D-Bus for desktop services
    dbus: Option<DbusManager>,
}
```

## When You Need Direct Kernel Access

You might want direct kernel access for:

1. **Global hotkeys** (even when not focused)
   ```rust
   // Read keyboard directly (needs root or seat permissions)
   let mut input = Libinput::new_from_udev(interface);
   ```

2. **Monitor hotplug detection** (faster than X11 RandR)
   ```rust
   // Monitor udev for DRM events
   let monitor = MonitorBuilder::new()?.match_subsystem("drm")?.listen()?;
   ```

3. **Custom VT switching** (for gaming/kiosk mode)
   ```rust
   // Use libseat to manage session
   let seat = Seat::open()?;
   seat.switch_session(2)?;  // Switch to VT 2
   ```

## Security Implications

### X11 Approach (Your Current Choice)

```
You → X11 Server → Kernel
     (runs as root or with seat permissions)
```

Your binary doesn't need root or special permissions! Xorg handles it.

### Direct Kernel Approach (Wayland Style)

```
You → Kernel
(your binary needs seat permissions)
```

Your binary needs to be:
- Run from a login session (via systemd-logind)
- Given permission by libseat
- Or run as root (not recommended)

## System Calls Comparison

| Operation | X11 (x11rb) | Direct (smithay) |
|-----------|-------------|------------------|
| Open GPU | X11 handles it | `open("/dev/dri/card0")` |
| Set display mode | `RandR` extension | `ioctl(DRM_IOCTL_MODE_SETCRTC)` |
| Read keyboard | `XQueryKeymap` | `read(/dev/input/event0)` |
| Render frame | `glXSwapBuffers` | `ioctl(DRM_IOCTL_MODE_PAGE_FLIP)` |

## Conclusion: Your Best Approach

**For Area (X11-based unified binary):**

```
✅ Use X11 (x11rb) for graphics and window management
✅ Use GLX for OpenGL compositing
✅ Use zbus for D-Bus desktop services
✅ Optionally use libinput for global hotkeys

❌ Don't implement Wayland compositor
❌ Don't directly manage DRM/KMS
❌ Don't handle VT switching yourself
```

**This gives you:**
- Wine/Steam compatibility (X11)
- XFCE4 plugin compatibility
- Simple architecture (Xorg handles kernel complexity)
- Unified binary (WM + Compositor + Shell in one process)

**You get the benefits of:**
- Direct memory access between WM/Compositor/Shell (no IPC overhead)
- Mature X11 ecosystem
- All kernel features (via Xorg)
- Optional D-Bus for desktop integration

Best of both worlds! 🚀

