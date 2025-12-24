# IPC and Architecture Guide for Area

## What is D-Bus?

**D-Bus** (Desktop Bus) is a message-passing system that allows different programs to communicate with each other on Linux desktops.

### How D-Bus Works

```
┌──────────────┐         ┌──────────────┐         ┌──────────────┐
│  Application │         │    Panel     │         │   Plugin     │
│              │         │              │         │              │
│  "Show me    │ ───1───>│ D-Bus Name:  │<───2────│  "Register   │
│   windows"   │         │ org.xfce.    │         │   me!"       │
│              │         │ Panel        │         │              │
└──────────────┘         └──────────────┘         └──────────────┘
                                │
                                │ D-Bus System
                                ▼
                    ┌────────────────────┐
                    │   dbus-daemon      │
                    │   (Message Router) │
                    │                    │
                    │ Routes messages    │
                    │ between processes  │
                    └────────────────────┘
```

### Two Types of D-Bus

1. **Session Bus**: Per-user, for desktop apps
   - Panel ↔ Plugins
   - Notifications
   - Settings

2. **System Bus**: System-wide, for hardware
   - Power management
   - Hardware events
   - System services

## What xfce4-panel Uses D-Bus For

From `xfce4-panel/common/panel-dbus.h`:

```c
#define PANEL_DBUS_NAME "org.xfce.Panel"
#define PANEL_DBUS_PATH "/org/xfce/Panel"
#define PANEL_DBUS_WRAPPER_PATH PANEL_DBUS_PATH "/Wrapper/%d"
#define PANEL_DBUS_WRAPPER_INTERFACE PANEL_DBUS_NAME ".Wrapper"
#define PANEL_DBUS_PLUGIN_NAME PANEL_DBUS_NAME "Plugin%d"
#define PANEL_DBUS_EXTERNAL_INTERFACE PANEL_DBUS_NAME ".External"
```

**xfce4-panel uses D-Bus to:**
1. **Load plugins** in separate processes (security isolation)
2. **Send commands** to plugins (show/hide, configure)
3. **Receive events** from plugins (click, update)
4. **Plugin registration** when they start

### Why External Plugins Use D-Bus

```
┌─────────────────────────────────────────────────┐
│           xfce4-panel (main process)            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐     │
│  │ Panel UI │  │ Window   │  │ Settings │     │
│  │          │  │ List     │  │          │     │
│  └──────────┘  └──────────┘  └──────────┘     │
└─────────────────────────────────────────────────┘
         │                    │
         │ D-Bus              │ D-Bus
         ▼                    ▼
┌─────────────────┐  ┌─────────────────┐
│ Clock Plugin    │  │ System Tray     │
│ (separate       │  │ (separate       │
│  process)       │  │  process)       │
└─────────────────┘  └─────────────────┘
```

**Benefits:**
- Plugin crash doesn't kill panel
- Plugins can be written in any language
- Easy to reload plugins

## cosmic-comp's session.rs Explained

### The Problem cosmic-comp Solves

cosmic-comp is **separate** from cosmic-session:

```
cosmic-session (PID 1234) ─┬─> cosmic-comp (PID 1235)
                           ├─> cosmic-panel (PID 1236)
                           └─> cosmic-launcher (PID 1237)
```

They need to coordinate startup:
1. Session starts compositor
2. Compositor creates Wayland socket
3. Compositor tells session "I'm ready, socket is at /run/user/1000/wayland-1"
4. Session starts apps with WAYLAND_DISPLAY=/run/user/1000/wayland-1

### The Unix Socket IPC Protocol

```rust
// Message format (from session.rs)
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    SetEnv { variables: HashMap<String, String> },
}
```

**Wire format:**
```
┌────────────┬─────────────────────────────────────┐
│ 2 bytes    │ N bytes                             │
│ length     │ JSON message                        │
├────────────┼─────────────────────────────────────┤
│ 0x00 0x45  │ {"message":"set_env",               │
│            │  "variables":{"WAYLAND_DISPLAY":    │
│            │  "wayland-1"}}                      │
└────────────┴─────────────────────────────────────┘
```

### setup_socket Step-by-Step

```rust
pub fn setup_socket(handle: LoopHandle<State>, common: &Common) -> Result<()> {
    // STEP 1: Check if session manager gave us a socket
    if let Ok(fd_num) = std::env::var("COSMIC_SESSION_SOCK") {
        // Session manager sets: COSMIC_SESSION_SOCK=5
        // This means file descriptor 5 is a Unix socket
        
        if let Ok(fd) = fd_num.parse::<RawFd>() {
            
            // STEP 2: Security - set CLOEXEC flag
            // When we launch child processes (apps), they shouldn't
            // inherit this socket. It's only for session ↔ compositor
            let mut session_socket = match unsafe { set_cloexec(fd) } {
                Ok(_) => unsafe { UnixStream::from_raw_fd(fd) },
                Err(err) => {
                    unsafe { rustix::io::close(fd) };
                    return Err(err);
                }
            };

            // STEP 3: Build our message
            let env = get_env(common)?;  
            // Returns: {"WAYLAND_DISPLAY": "wayland-1", "DISPLAY": ":0"}
            
            let message = serde_json::to_string(&Message::SetEnv { 
                variables: env 
            })?;
            
            // STEP 4: Send length-prefixed message
            let bytes = message.into_bytes();
            let len = (bytes.len() as u16).to_ne_bytes();  // 2-byte length
            session_socket.write_all(&len)?;    // Write: [0x00, 0x45]
            session_socket.write_all(&bytes)?;  // Write: {"message"...}
            
            // STEP 5: Register socket with event loop
            // From now on, if session sends us messages, our callback runs
            handle.insert_source(
                Generic::new(StreamWrapper::from(session_socket), 
                            Interest::READ, Mode::Level),
                move |_, stream, _state| {
                    // This callback runs when data arrives
                    // ... (read length, read message, parse JSON)
                },
            )?;
        }
    };
    Ok(())
}
```

### Why CLOEXEC Matters

```rust
unsafe fn set_cloexec(fd: RawFd) -> rustix::io::Result<()> {
    let fd = unsafe { BorrowedFd::borrow_raw(fd) };
    let flags = rustix::io::fcntl_getfd(fd)?;
    rustix::io::fcntl_setfd(fd, flags | rustix::io::FdFlags::CLOEXEC)
}
```

**Without CLOEXEC:**
```
cosmic-comp (FD 5 = session socket)
    └─> launch Firefox
           Firefox also has FD 5! ❌ Security leak!
```

**With CLOEXEC:**
```
cosmic-comp (FD 5 = session socket)
    └─> launch Firefox
           Firefox: FD 5 is closed ✅ Safe
```

## Your Unified Binary Architecture

Since you're building **one binary**, you don't need IPC between WM/Compositor/Shell!

```
┌──────────────────────────────────────────────────────┐
│                  area (single binary)                 │
│                                                       │
│  ┌──────────┐   ┌────────────┐   ┌──────────┐      │
│  │ X11 WM   │   │ Compositor │   │  Shell   │      │
│  │          │   │            │   │          │      │
│  │ • Focus  │   │ • OpenGL   │   │ • Panel  │      │
│  │ • Resize │◄──┤ • Textures │◄──┤ • Clock  │      │
│  │ • Move   │   │ • VSync    │   │ • Tray   │      │
│  └──────────┘   └────────────┘   └──────────┘      │
│       ▲                                              │
│       │ Direct memory access (no serialization!)    │
│       │                                              │
│  ┌────┴──────────────────────────────────┐         │
│  │   Shared State (in-process)           │         │
│  │   • Windows: HashMap<u32, Window>     │         │
│  │   • No IPC overhead!                  │         │
│  └───────────────────────────────────────┘         │
└──────────────────────────────────────────────────────┘
         │                           │
         │ X11 Protocol              │ D-Bus (for XFCE compat)
         ▼                           ▼
   ┌──────────┐            ┌──────────────────┐
   │ X Server │            │ org.xfce.Panel   │
   │ (Xorg)   │            │ (for plugins)    │
   └──────────┘            └──────────────────┘
```

### What You DO Need D-Bus For (XFCE4 Compatibility)

1. **xfce4-panel plugins** - if you want to use external XFCE plugins
2. **Desktop notifications** - `org.freedesktop.Notifications`
3. **Power management** - `org.freedesktop.UPower`, `org.freedesktop.login1`
4. **Hardware events** - Monitor hotplug, lid switch

### What You DON'T Need

❌ IPC between WM and Compositor (they're in the same process!)
❌ Session socket (you're not separate from a session manager)
❌ Wayland protocols (you're using X11)

## Recommended Architecture for Area

```rust
// src/main.rs
struct AreaApp {
    // X11 window manager
    wm: wm::WindowManager,
    
    // OpenGL compositor
    compositor: compositor::Compositor,
    
    // Built-in shell
    shell: shell::Shell,
    
    // OPTIONAL: D-Bus server for XFCE plugin compatibility
    dbus: Option<dbus::PanelServer>,
    
    // Shared state (no IPC needed!)
    windows: HashMap<u32, Window>,
}

impl AreaApp {
    async fn new() -> Result<Self> {
        // Initialize X11 WM
        let wm = wm::WindowManager::new(&conn, screen_num, root)?;
        
        // Initialize compositor
        let compositor = compositor::Compositor::new(&conn, screen_num, root)?;
        
        // Initialize shell
        let shell = shell::Shell::new(screen_width, screen_height);
        
        // OPTIONAL: Initialize D-Bus for XFCE plugin support
        let dbus = if config.xfce_plugin_support {
            Some(dbus::PanelServer::new()?)
        } else {
            None
        };
        
        Ok(Self { wm, compositor, shell, dbus, windows: HashMap::new() })
    }
    
    async fn run(mut self) -> Result<()> {
        loop {
            tokio::select! {
                // Handle X11 events (window management)
                event = self.wait_for_x11_event() => {
                    self.handle_event(event)?;
                }
                
                // Handle D-Bus messages (XFCE plugins)
                msg = self.dbus.receive_message() => {
                    self.handle_dbus(msg)?;
                }
                
                // Render frame (compositor)
                _ = vsync.tick() => {
                    self.render_frame()?;
                }
            }
        }
    }
}
```

## Desktop Services You Should Integrate

### Essential (via D-Bus):

1. **org.freedesktop.Notifications** - Desktop notifications
   ```rust
   // Show notification
   notify.show("Title", "Message", "icon-name");
   ```

2. **org.freedesktop.UPower** - Battery/power info
   ```rust
   // Get battery percentage for panel
   let battery = upower.get_battery_percentage();
   ```

3. **org.freedesktop.login1** (systemd-logind) - Power management
   ```rust
   // Suspend, hibernate, shutdown
   logind.suspend();
   ```

### Optional (for XFCE plugin compatibility):

4. **org.xfce.Panel** - If you want external XFCE plugins
   ```rust
   // Register as panel, accept plugin connections
   panel_server.register_plugin(plugin_id);
   ```

## Summary

| Feature | cosmic-comp | Your Area Binary |
|---------|-------------|------------------|
| **Process model** | Separate binaries | Single unified binary |
| **IPC overhead** | Unix socket + JSON | None (in-process) |
| **Display protocol** | Wayland | X11 |
| **D-Bus usage** | Session IPC, Power | Power, Plugins only |
| **Plugin support** | Cosmic plugins | XFCE4 plugins |
| **Complexity** | High (multi-process) | Low (single process) |

**Your advantages:**
- ✅ Faster (no IPC serialization)
- ✅ Simpler (single process)
- ✅ XFCE4 compatibility (X11 + optional D-Bus)
- ✅ Wine/Steam game compatibility (X11)
- ✅ Direct memory sharing between WM/Compositor/Shell

**Next steps:**
1. Keep your unified binary architecture
2. Add `zbus` for D-Bus (only for desktop services)
3. Optionally implement `org.xfce.Panel` D-Bus interface for plugin support
4. Don't add unnecessary IPC between internal components!

