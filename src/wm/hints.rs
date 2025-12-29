//! Hints Module
//!
//! Window hints reading and application (XSizeHints, XWMHints, MWM hints).
//! This matches xfwm4's hints system.

use anyhow::Result;
use tracing::debug;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::shared::Geometry;
use crate::wm::client::Client;
use crate::wm::ewmh::Atoms;

/// Size hints (XSizeHints equivalent)
#[derive(Debug, Clone)]
pub struct SizeHints {
    pub flags: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
    pub width_inc: u32,
    pub height_inc: u32,
    pub min_aspect_num: u32,
    pub min_aspect_den: u32,
    pub max_aspect_num: u32,
    pub max_aspect_den: u32,
    pub base_width: u32,
    pub base_height: u32,
    pub win_gravity: u8,
}

/// WM hints (XWMHints equivalent)
#[derive(Debug, Clone)]
pub struct WmHints {
    pub flags: u32,
    pub input: bool,
    pub initial_state: u32,
    pub icon_pixmap: Option<u32>,
    pub icon_window: Option<u32>,
    pub icon_x: i32,
    pub icon_y: i32,
    pub icon_mask: Option<u32>,
    pub window_group: Option<u32>,
}

impl WmHints {
    /// Check if urgency hint is set
    /// WM_HINTS flags: bit 4 (1 << 4) = InputHint, bit 5 (1 << 5) = StateHint, bit 6 (1 << 6) = IconPixmapHint, etc.
    /// Urgency is indicated by StateHint flag (bit 5) AND initial_state having bit 8 set (0x100)
    /// Actually, X11 spec says: if StateHint is set and initial_state has bit 8 (0x100), window is urgent
    pub fn is_urgent(&self) -> bool {
        // StateHint is bit 5 (1 << 5 = 32)
        // Urgency is indicated by initial_state having bit 8 set (0x100 = 256)
        (self.flags & (1 << 5)) != 0 && (self.initial_state & 0x100) != 0
    }
}

/// Hints manager
pub struct HintsManager;

impl HintsManager {
    /// Read size hints for a window
    pub fn read_size_hints(
        conn: &RustConnection,
        atoms: &Atoms,
        window: u32,
    ) -> Result<Option<SizeHints>> {
        if let Ok(reply) = conn.get_property(
            false,
            window,
            atoms._wm_size_hints,
            atoms._wm_size_hints,
            0,
            18, // XSizeHints has 18 32-bit values
        )?.reply() {
            if let Some(value32) = reply.value32() {
                let values: Vec<u32> = value32.take(18).collect();
                if values.len() >= 18 {
                    return Ok(Some(SizeHints {
                        flags: values[0],
                        x: values[1] as i32,
                        y: values[2] as i32,
                        width: values[3],
                        height: values[4],
                        min_width: values[5],
                        min_height: values[6],
                        max_width: values[7],
                        max_height: values[8],
                        width_inc: values[9],
                        height_inc: values[10],
                        min_aspect_num: values[11],
                        min_aspect_den: values[12],
                        max_aspect_num: values[13],
                        max_aspect_den: values[14],
                        base_width: values[15],
                        base_height: values[16],
                        win_gravity: values[17] as u8,
                    }));
                }
            }
        }
        Ok(None)
    }
    
    /// Read WM hints for a window
    pub fn read_wm_hints(
        conn: &RustConnection,
        atoms: &Atoms,
        window: u32,
    ) -> Result<Option<WmHints>> {
        if let Ok(reply) = conn.get_property(
            false,
            window,
            atoms._wm_hints,
            atoms._wm_hints,
            0,
            9, // XWMHints has 9 32-bit values
        )?.reply() {
            if let Some(value32) = reply.value32() {
                let values: Vec<u32> = value32.take(9).collect();
                if values.len() >= 9 {
                    return Ok(Some(WmHints {
                        flags: values[0],
                        input: (values[1] & 1) != 0,
                        initial_state: values[2],
                        icon_pixmap: if values[3] != 0 { Some(values[3]) } else { None },
                        icon_window: if values[4] != 0 { Some(values[4]) } else { None },
                        icon_x: values[5] as i32,
                        icon_y: values[6] as i32,
                        icon_mask: if values[7] != 0 { Some(values[7]) } else { None },
                        window_group: if values[8] != 0 { Some(values[8]) } else { None },
                    }));
                }
            }
        }
        Ok(None)
    }
    
    /// Apply size hints to geometry
    pub fn apply_size_hints(
        &self,
        hints: &SizeHints,
        geometry: &Geometry,
    ) -> Geometry {
        let mut new_geom = *geometry;
        
        // Apply min/max size constraints
        if (hints.flags & (1 << 4)) != 0 { // PMinSize
            new_geom.width = new_geom.width.max(hints.min_width);
            new_geom.height = new_geom.height.max(hints.min_height);
        }
        
        if (hints.flags & (1 << 5)) != 0 { // PMaxSize
            new_geom.width = new_geom.width.min(hints.max_width);
            new_geom.height = new_geom.height.min(hints.max_height);
        }
        
        // Apply size increments
        if (hints.flags & (1 << 8)) != 0 && hints.width_inc > 0 { // PResizeInc
            let base = if (hints.flags & (1 << 9)) != 0 { hints.base_width } else { 0 };
            let diff = new_geom.width.saturating_sub(base);
            new_geom.width = base + (diff / hints.width_inc) * hints.width_inc;
        }
        
        if (hints.flags & (1 << 8)) != 0 && hints.height_inc > 0 { // PResizeInc
            let base = if (hints.flags & (1 << 9)) != 0 { hints.base_height } else { 0 };
            let diff = new_geom.height.saturating_sub(base);
            new_geom.height = base + (diff / hints.height_inc) * hints.height_inc;
        }
        
        new_geom
    }
}

impl Default for HintsManager {
    fn default() -> Self {
        Self
    }
}



