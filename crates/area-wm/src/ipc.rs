//! IPC server for communicating with area-comp (compositor)

use anyhow::Result;
use area_ipc::{socket_path, FramedMessage, ShellCommand, WmEvent};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, error, info, warn};

/// IPC server that broadcasts events to connected compositors
pub struct IpcServer {
    /// Sender for broadcasting events to all connected clients
    event_tx: broadcast::Sender<WmEvent>,
    /// Receiver for commands from compositors
    command_rx: mpsc::Receiver<ShellCommand>,
    /// Sender for commands (cloned into client handlers)
    command_tx: mpsc::Sender<ShellCommand>,
}

impl IpcServer {
    /// Create a new IPC server
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let (command_tx, command_rx) = mpsc::channel(256);

        Self {
            event_tx,
            command_rx,
            command_tx,
        }
    }

    /// Get a sender for broadcasting events
    pub fn _event_sender(&self) -> broadcast::Sender<WmEvent> {
        self.event_tx.clone()
    }

    /// Take the command receiver (can only be called once)
    pub fn _take_command_receiver(&mut self) -> mpsc::Receiver<ShellCommand> {
        std::mem::replace(&mut self.command_rx, mpsc::channel(1).1)
    }

    /// Start the IPC server (spawns a background task)
    pub async fn start(self) -> Result<IpcHandle> {
        let socket_path = socket_path();

        // Remove existing socket if present
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        // Create parent directory if needed
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        info!("IPC server listening on {:?}", socket_path);

        let event_tx = self.event_tx.clone();
        let command_tx = self.command_tx.clone();

        // Spawn acceptor task
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        info!("Compositor connected");
                        let event_rx = event_tx.subscribe();
                        let cmd_tx = command_tx.clone();
                        tokio::spawn(handle_client(stream, event_rx, cmd_tx));
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
        });

        Ok(IpcHandle {
            event_tx: self.event_tx,
            command_rx: self.command_rx,
        })
    }
}

/// Handle for interacting with the IPC server
pub struct IpcHandle {
    pub event_tx: broadcast::Sender<WmEvent>,
    pub command_rx: mpsc::Receiver<ShellCommand>,
}

impl IpcHandle {
    /// Broadcast an event to all connected compositors
    pub fn broadcast(&self, event: WmEvent) {
        // Ignore error if no receivers
        let _ = self.event_tx.send(event);
    }

    /// Try to receive a command (non-blocking)
    pub fn try_recv_command(&mut self) -> Option<ShellCommand> {
        self.command_rx.try_recv().ok()
    }
}

/// Handle a connected client
async fn handle_client(
    stream: UnixStream,
    mut event_rx: broadcast::Receiver<WmEvent>,
    command_tx: mpsc::Sender<ShellCommand>,
) {
    let (mut reader, mut writer) = stream.into_split();

    // Spawn reader task (compositor → WM)
    let cmd_tx = command_tx.clone();
    let reader_task = tokio::spawn(async move {
        let mut len_buf = [0u8; 4];
        loop {
            // Read length prefix
            if reader.read_exact(&mut len_buf).await.is_err() {
                break;
            }
            let len = u32::from_be_bytes(len_buf) as usize;

            if len > 1024 * 1024 {
                warn!("Message too large: {} bytes", len);
                break;
            }

            // Read message
            let mut msg_buf = vec![0u8; len];
            if reader.read_exact(&mut msg_buf).await.is_err() {
                break;
            }

            // Decode command
            match FramedMessage::decode_shell_command(&msg_buf) {
                Ok(cmd) => {
                    debug!("Received command: {:?}", cmd);
                    if cmd_tx.send(cmd).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    warn!("Failed to decode command: {}", e);
                }
            }
        }
        debug!("Reader task ended");
    });

    // Writer task (WM → compositor)
    let writer_task = tokio::spawn(async move {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    match FramedMessage::new(&event) {
                        Ok(msg) => {
                            let encoded = msg.encode();
                            if writer.write_all(&encoded).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to encode event: {}", e);
                        }
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Lagged {} events", n);
                }
            }
        }
        debug!("Writer task ended");
    });

    // Wait for either task to finish
    tokio::select! {
        _ = reader_task => {}
        _ = writer_task => {}
    }

    info!("Compositor disconnected");
}
