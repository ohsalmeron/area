//! IPC client for communicating with area-wm

use anyhow::{Context, Result};
use area_ipc::{socket_path, FramedMessage, ShellCommand, WmEvent};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// IPC client for connecting to area-wm
pub struct IpcClient {
    #[allow(dead_code)]
    sender: mpsc::Sender<ShellCommand>,
    receiver: mpsc::Receiver<WmEvent>,
}

impl IpcClient {
    /// Connect to the window manager via Unix socket
    pub async fn connect() -> Result<Self> {
        let socket_path = socket_path();
        
        info!("Connecting to WM at {:?}", socket_path);
        
        // Wait for socket to be available
        let mut retries = 100;
        while !socket_path.exists() && retries > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            retries -= 1;
        }
        
        if !socket_path.exists() {
            return Err(anyhow::anyhow!("IPC socket not found: {:?}", socket_path));
        }

        let stream = UnixStream::connect(&socket_path)
            .await
            .context("Failed to connect to IPC socket")?;

        info!("Connected to WM");

        let (mut reader, mut writer) = stream.into_split();

        // Channels for bidirectional communication
        let (event_tx, event_rx) = mpsc::channel(256);
        let (command_tx, mut command_rx) = mpsc::channel(256);

        // Spawn reader task (WM → Compositor)
        tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            loop {
                // Read length prefix
                if reader.read_exact(&mut len_buf).await.is_err() {
                    debug!("IPC reader task ended");
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

                // Decode event
                match FramedMessage::decode_wm_event(&msg_buf) {
                    Ok(event) => {
                        debug!("Received WM event: {:?}", event);
                        if event_tx.send(event).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to decode WM event: {}", e);
                    }
                }
            }
        });

        // Spawn writer task (Compositor → WM)
        let writer_task = tokio::spawn(async move {
            loop {
                if let Some(cmd) = command_rx.recv().await {
                    debug!("Sending command: {:?}", cmd);
                    match FramedMessage::new(&cmd) {
                        Ok(msg) => {
                            let encoded = msg.encode();
                            if writer.write_all(&encoded).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Failed to encode command: {}", e);
                        }
                    }
                } else {
                    break;
                }
            }
            debug!("IPC writer task ended");
        });

        // Keep writer task alive
        tokio::spawn(async move {
            let _ = writer_task.await;
        });

        Ok(Self {
            sender: command_tx,
            receiver: event_rx,
        })
    }

    /// Try to receive a WM event (non-blocking)
    pub fn try_recv_event(&mut self) -> Option<WmEvent> {
        self.receiver.try_recv().ok()
    }

    /// Send a command to the window manager
    #[allow(dead_code)]
    pub async fn send_command(&self, command: ShellCommand) -> Result<()> {
        self.sender
            .send(command)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send command: {}", e))
    }
}

