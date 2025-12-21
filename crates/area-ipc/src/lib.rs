//! Area IPC Protocol
//!
//! Shared message types for communication between `area-wm` (X11 window manager)
//! and `area-shell` (Bevy UI shell).


pub mod compositor_proto;
use serde::{Deserialize, Serialize};

/// Socket path for IPC communication
pub fn socket_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    std::path::PathBuf::from(runtime_dir).join("area-wm.sock")
}

// ============================================================================
// WM → Shell Events
// ============================================================================

/// Events sent from the window manager to the shell
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WmEvent {
    /// A new window was opened
    WindowOpened {
        id: u32,
        title: String,
        class: String,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },

    /// A window was closed
    WindowClosed { id: u32 },

    /// A window received focus
    WindowFocused { id: u32 },

    /// A window's title changed
    WindowTitleChanged { id: u32, title: String },

    /// A window was moved or resized
    WindowGeometryChanged {
        id: u32,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },

    /// A window drag started (user is dragging the window)
    WindowDragStarted { id: u32 },

    /// A window drag ended (user released the window)
    WindowDragEnded { id: u32 },

    /// Workspace changed
    WorkspaceChanged { current: u8, total: u8 },

    /// Initial state sync (sent on shell connect)
    SyncState {
        windows: Vec<WindowInfo>,
        current_workspace: u8,
        focused_window: Option<u32>,
    },
}

/// Window information for state sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub class: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub workspace: u8,
}

// ============================================================================
// Shell → WM Commands
// ============================================================================

/// Commands sent from the shell to the window manager
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ShellCommand {
    /// Focus a specific window
    FocusWindow { id: u32 },

    /// Close a window
    CloseWindow { id: u32 },

    /// Switch to a workspace
    SwitchWorkspace { index: u8 },

    /// Move a window to a workspace
    MoveWindowToWorkspace { id: u32, workspace: u8 },

    /// Launch an application
    LaunchApp { command: String },

    /// Move a window
    MoveWindow { id: u32, x: i32, y: i32 },

    /// Resize a window
    ResizeWindow { id: u32, width: u32, height: u32 },

    /// Toggle overview mode (WM may need to know for input grabs)
    ToggleOverview { active: bool },

    /// Request window thumbnail (for overview mode)
    RequestThumbnail { id: u32 },
}

// ============================================================================
// Message Framing
// ============================================================================

/// A framed message with length prefix for reliable socket reads
#[derive(Debug)]
pub struct FramedMessage {
    pub data: Vec<u8>,
}

impl FramedMessage {
    /// Create a new framed message from serializable data
    pub fn new<T: Serialize>(msg: &T) -> anyhow::Result<Self> {
        let data = serde_json::to_vec(msg)?;
        Ok(Self { data })
    }

    /// Encode message with length prefix (4 bytes, big-endian)
    pub fn encode(&self) -> Vec<u8> {
        let len = self.data.len() as u32;
        let mut buf = Vec::with_capacity(4 + self.data.len());
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(&self.data);
        buf
    }

    /// Decode a WM event from bytes
    pub fn decode_wm_event(data: &[u8]) -> anyhow::Result<WmEvent> {
        Ok(serde_json::from_slice(data)?)
    }

    /// Decode a shell command from bytes
    pub fn decode_shell_command(data: &[u8]) -> anyhow::Result<ShellCommand> {
        Ok(serde_json::from_slice(data)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_wm_event() {
        let event = WmEvent::WindowOpened {
            id: 12345,
            title: "Firefox".into(),
            class: "firefox".into(),
            x: 100,
            y: 100,
            width: 800,
            height: 600,
        };

        let msg = FramedMessage::new(&event).unwrap();
        let decoded = FramedMessage::decode_wm_event(&msg.data).unwrap();

        match decoded {
            WmEvent::WindowOpened { id, title, .. } => {
                assert_eq!(id, 12345);
                assert_eq!(title, "Firefox");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_roundtrip_shell_command() {
        let cmd = ShellCommand::LaunchApp {
            command: "chromium".into(),
        };

        let msg = FramedMessage::new(&cmd).unwrap();
        let decoded = FramedMessage::decode_shell_command(&msg.data).unwrap();

        match decoded {
            ShellCommand::LaunchApp { command } => {
                assert_eq!(command, "chromium");
            }
            _ => panic!("Wrong command type"),
        }
    }
}
