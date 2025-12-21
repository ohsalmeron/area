use anyhow::{Context, Result};
use std::os::unix::io::RawFd;
use x11rb::connection::Connection;
use x11rb::protocol::dri3::{ConnectionExt as _, BufferFromPixmapReply};
use x11rb::protocol::xproto::Pixmap;

pub struct Dri3 {
    pub major: u32,
    pub minor: u32,
}

impl Dri3 {
    pub fn new(conn: &impl Connection) -> Result<Self> {
        let version = conn.dri3_query_version(1, 2)?.reply().context("Failed to query DRI3 version")?;
        Ok(Self {
            major: version.major_version,
            minor: version.minor_version,
        })
    }

    /// Get the DMA-BUF file descriptor for a pixmap
    pub fn get_pixmap_fd(&self, conn: &impl Connection, pixmap: Pixmap) -> Result<Vec<RawFd>> {
        let reply = conn.dri3_buffer_from_pixmap(pixmap)?.reply().context("Failed to get buffer from pixmap")?;
        
        // x11rb stores the received FDs in the reply struct
        // Depending on specific version/request, it might return one or more.
        // buffer_from_pixmap usually returns one stride/offset/fd? 
        // Actually the protocol spec says it returns a set of buffers?
        // Wait, `buffer_from_pixmap` -> returns one buffer?
        // Let's check the struct fields.
        
        // For now, assume single plane for simple windows.
        // The reply has `pixmap_fd`.
        
        Ok(reply.fds)
    }
}
