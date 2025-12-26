//! X11 Async Event Stream
//!
//! Provides non-blocking async X11 event polling using mio, following LeftWM's proven architecture.

use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::time::Duration;
use anyhow::{Context, Result};
use tokio::sync::{Notify, oneshot};
use x11rb::rust_connection::RustConnection;
use x11rb::protocol::Event;

/// X11 event stream with async polling support
///
/// Uses mio in a background thread to poll the X11 file descriptor and notify
/// the main async loop when events are available. This avoids blocking threads
/// and enables true async I/O for X11 events.
pub struct X11EventStream {
    conn: Arc<RustConnection>,
    notify: Arc<Notify>,
    _task_guard: oneshot::Receiver<()>,
}

impl X11EventStream {
    /// Create a new X11 event stream with async polling
    ///
    /// Spawns a background thread that uses mio to poll the X11 file descriptor
    /// and notifies the main loop when events are available.
    pub fn new(conn: Arc<RustConnection>) -> Result<Self> {
        let fd = conn.stream().as_raw_fd();
        let notify = Arc::new(Notify::new());
        let task_notify = notify.clone();
        
        // Spawn mio polling thread (like LeftWM)
        let (guard, task_guard) = oneshot::channel::<()>();
        let mut poll = mio::Poll::new()
            .context("Failed to create mio Poll")?;
        let mut events = mio::Events::with_capacity(1);
        
        poll.registry()
            .register(
                &mut mio::unix::SourceFd(&fd),
                mio::Token(0),
                mio::Interest::READABLE,
            )
            .context("Failed to register X11 FD with mio")?;
        
        let timeout = Duration::from_millis(100);
        tokio::task::spawn_blocking(move || {
            loop {
                if guard.is_closed() {
                    tracing::info!("X11 socket polling thread shutting down");
                    return;
                }
                
                if let Err(err) = poll.poll(&mut events, Some(timeout)) {
                    tracing::warn!("X11 socket poll failed: {:?}", err);
                    continue;
                }
                
                events
                    .iter()
                    .filter(|event| event.token() == mio::Token(0))
                    .for_each(|_| task_notify.notify_one());
            }
        });
        
        Ok(Self {
            conn,
            notify,
            _task_guard: task_guard,
        })
    }
    
    /// Non-blocking: poll for events (drains internal buffer)
    ///
    /// Returns `Some(event)` if an event is available, `None` if the buffer is empty.
    /// This is non-blocking and should be called in a loop to drain all pending events.
    pub fn poll_next_event(&self) -> Result<Option<Event>> {
        use x11rb::connection::Connection;
        Ok(self.conn.as_ref().poll_for_event()?)
    }
    
    /// Async wait for X11 FD to become readable
    ///
    /// Returns when the background mio thread detects that the X11 file descriptor
    /// has become readable. This allows the async runtime to efficiently wait
    /// without blocking threads.
    pub async fn wait_readable(&self) {
        self.notify.notified().await;
    }
    
    /// Flush X11 requests (batch optimization)
    ///
    /// Flushes all pending X11 requests to the server. Should be called at the
    /// start of each event loop iteration to batch requests (LeftWM pattern).
    pub fn flush(&self) -> Result<()> {
        use x11rb::connection::Connection;
        self.conn.as_ref().flush()?;
        Ok(())
    }
}

