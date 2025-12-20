//! IPC client for communicating with area-wm

use crate::state::{ShellState, WindowEvent, WindowState, WorkspaceEvent};
use anyhow::Result;
use area_ipc::{socket_path, FramedMessage, ShellCommand, WmEvent};
use bevy::prelude::*;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tracing::{debug, error, info, warn};

/// Plugin for IPC communication with the window manager
pub struct IpcPlugin;

impl Plugin for IpcPlugin {
    fn build(&self, app: &mut App) {
        // Create channels for cross-thread communication
        let (event_tx, event_rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel();

        // Spawn IPC thread
        thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(ipc_task(event_tx, cmd_rx));
        });

        app.insert_resource(IpcReceiver { rx: Arc::new(Mutex::new(event_rx)) })
            .insert_resource(IpcSender { tx: Arc::new(Mutex::new(cmd_tx)) })
            .add_systems(Update, process_ipc_events);
    }
}

/// Resource for receiving WM events
#[derive(Resource, Clone)]
pub struct IpcReceiver {
    rx: Arc<Mutex<Receiver<WmEvent>>>,
}

impl IpcReceiver {
    pub fn try_recv(&self) -> Option<WmEvent> {
        self.rx.lock().ok()?.try_recv().ok()
    }
}

/// Resource for sending commands to WM
#[derive(Resource, Clone)]
pub struct IpcSender {
    tx: Arc<Mutex<Sender<ShellCommand>>>,
}

impl IpcSender {
    /// Send a command to the window manager
    pub fn send(&self, cmd: ShellCommand) {
        if let Ok(tx) = self.tx.lock() {
            if let Err(e) = tx.send(cmd) {
                error!("Failed to send command: {}", e);
            }
        }
    }
}

/// Process incoming IPC events and update shell state
fn process_ipc_events(
    receiver: Res<IpcReceiver>,
    mut state: ResMut<ShellState>,
    mut window_events: EventWriter<WindowEvent>,
    mut workspace_events: EventWriter<WorkspaceEvent>,
) {
    // Process all available events
    while let Some(event) = receiver.try_recv() {
        debug!("Received WM event: {:?}", event);

        match event {
            WmEvent::WindowOpened {
                id,
                title,
                class,
                x,
                y,
                width,
                height,
            } => {
                let window = WindowState {
                    id,
                    title,
                    class,
                    x,
                    y,
                    width,
                    height,
                    workspace: state.current_workspace,
                };
                state.on_window_opened(window.clone());
                window_events.send(WindowEvent::Opened(window));
            }

            WmEvent::WindowClosed { id } => {
                state.on_window_closed(id);
                window_events.send(WindowEvent::Closed(id));
            }

            WmEvent::WindowFocused { id } => {
                state.on_window_focused(id);
                window_events.send(WindowEvent::Focused(id));
            }

            WmEvent::WindowTitleChanged { id, title } => {
                if let Some(win) = state.windows.get_mut(&id) {
                    win.title = title.clone();
                }
                window_events.send(WindowEvent::TitleChanged { id, title });
            }

            WmEvent::WindowGeometryChanged {
                id,
                x,
                y,
                width,
                height,
            } => {
                if let Some(win) = state.windows.get_mut(&id) {
                    win.x = x;
                    win.y = y;
                    win.width = width;
                    win.height = height;
                }
                window_events.send(WindowEvent::GeometryChanged {
                    id,
                    x,
                    y,
                    width,
                    height,
                });
            }

            WmEvent::WorkspaceChanged { current, total } => {
                state.on_workspace_changed(current, total);
                workspace_events.send(WorkspaceEvent::Changed { current, total });
            }

            WmEvent::SyncState {
                windows,
                current_workspace,
                focused_window,
            } => {
                info!("Syncing state: {} windows", windows.len());
                state.windows.clear();
                for win in windows {
                    state.windows.insert(
                        win.id,
                        WindowState {
                            id: win.id,
                            title: win.title,
                            class: win.class,
                            x: win.x,
                            y: win.y,
                            width: win.width,
                            height: win.height,
                            workspace: win.workspace,
                        },
                    );
                }
                state.current_workspace = current_workspace;
                state.focused = focused_window;
                state.connected = true;
            }
        }
    }
}

/// Background IPC task
async fn ipc_task(event_tx: Sender<WmEvent>, cmd_rx: Receiver<ShellCommand>) {
    loop {
        match connect_and_run(&event_tx, &cmd_rx).await {
            Ok(_) => {
                info!("IPC connection closed, reconnecting...");
            }
            Err(e) => {
                warn!("IPC error: {}, retrying in 1s...", e);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

async fn connect_and_run(
    event_tx: &Sender<WmEvent>,
    cmd_rx: &Receiver<ShellCommand>,
) -> Result<()> {
    let socket_path = socket_path();
    info!("Connecting to WM at {:?}", socket_path);

    let stream = UnixStream::connect(&socket_path).await?;
    info!("Connected to WM");

    let (mut reader, mut writer) = stream.into_split();

    // Spawn reader task
    let tx = event_tx.clone();
    let reader_handle = tokio::spawn(async move {
        let mut len_buf = [0u8; 4];
        loop {
            if reader.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let len = u32::from_be_bytes(len_buf) as usize;

            let mut msg_buf = vec![0u8; len];
            if reader.read_exact(&mut msg_buf).await.is_err() {
                break;
            }

            match FramedMessage::decode_wm_event(&msg_buf) {
                Ok(event) => {
                    if tx.send(event).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("Failed to decode event: {}", e);
                }
            }
        }
    });

    // Writer loop (check for commands to send)
    loop {
        // Check for commands (non-blocking)
        match cmd_rx.try_recv() {
            Ok(cmd) => {
                let msg = FramedMessage::new(&cmd)?;
                writer.write_all(&msg.encode()).await?;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => break,
        }

        // Check if reader is done
        if reader_handle.is_finished() {
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    Ok(())
}
