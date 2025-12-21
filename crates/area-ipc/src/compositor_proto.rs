//! Low-level binary protocol for the high-performance rendering path.
//! 
//! This module defines the C-compatible structures used for communicating
//! between the Window Manager and the Compositor over a SOCK_SEQPACKET
//! Unix domain socket.
//! 
//! # Protocol Overview
//! 
//! 1. **Handshake**: (Optional, simplistic for now)
//! 2. **Frame Update**:
//!    - Header: `FrameHeader`
//!    - Payload: `[DamageRect; num_damage_rects]`
//!    - Ancillary Data (SCM_RIGHTS): `[RawFd; num_fds]` (e.g. dma-bufs)

use std::mem;

/// Header for a frame update message.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)] // Add Pod/Zeroable from bytemuck if needed later
pub struct FrameHeader {
    /// Magic number to verify protocol sync (e.g., 0xAREA0001)
    pub magic: u32,
    /// Sequence number (monotonic counter)
    pub sequence: u64,
    /// Timestamp (nanoseconds since boot)
    pub timestamp: u64,
    /// Number of damage rectangles following this header
    pub num_damage_rects: u32,
    /// Number of file descriptors attached to this message (via ancillary data)
    pub num_fds: u32,
    /// ID of the window being updated
    pub window_id: u32,
}

/// A damage rectangle specifying a region of the window that needs repainting.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct DamageRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl FrameHeader {
    pub const MAGIC: u32 = 0x41524541; // "AREA" in ASCII

    /// Size of the header in bytes
    pub const fn size() -> usize {
        mem::size_of::<Self>()
    }
}

impl DamageRect {
    /// Size of a damage rect in bytes
    pub const fn size() -> usize {
        mem::size_of::<Self>()
    }
}
