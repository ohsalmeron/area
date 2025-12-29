//! Icons Module
//!
//! Window icon loading and caching.
//! This matches xfwm4's icon system.

use anyhow::Result;
use std::collections::HashMap;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::wm::ewmh::Atoms;

/// Icon data
#[derive(Debug, Clone)]
pub struct IconData {
    /// Icon width
    pub width: u32,
    /// Icon height
    pub height: u32,
    /// Icon pixels (ARGB32 format)
    pub pixels: Vec<u32>,
}

/// Icon manager
pub struct IconManager {
    /// Icon cache (window -> icon data)
    pub icon_cache: HashMap<u32, IconData>,
    
    /// Default icon
    pub default_icon: Option<IconData>,
}

impl IconManager {
    /// Create a new icon manager
    pub fn new() -> Self {
        Self {
            icon_cache: HashMap::new(),
            default_icon: None,
        }
    }
    
    /// Load icon for a window
    pub fn load_icon(
        &mut self,
        conn: &RustConnection,
        atoms: &Atoms,
        window: u32,
    ) -> Result<Option<IconData>> {
        // Try _NET_WM_ICON first
        if let Ok(reply) = conn.get_property(
            false,
            window,
            atoms._net_wm_icon,
            AtomEnum::CARDINAL,
            0,
            8192, // Large enough for icon data
        )?.reply() {
            if let Some(mut value32) = reply.value32() {
                // _NET_WM_ICON format: array of icon data
                // Each icon: width (u32), height (u32), pixels (width * height * u32 ARGB)
                // Multiple icons can be present (different sizes)
                // We'll take the first/largest one
                if let (Some(width), Some(height)) = (value32.next(), value32.next()) {
                    let pixel_count = (width as usize).checked_mul(height as usize)
                        .ok_or_else(|| anyhow::anyhow!("Icon size overflow"))?;
                    if pixel_count > 0 && pixel_count <= 1024 * 1024 { // Sanity check: max 1MP icon
                        let mut pixels = Vec::with_capacity(pixel_count);
                        for _ in 0..pixel_count {
                            if let Some(pixel) = value32.next() {
                                pixels.push(pixel);
                            } else {
                                break;
                            }
                        }
                        
                        if pixels.len() == pixel_count {
                            debug!("Loaded icon for window {}: {}x{}", window, width, height);
                            return Ok(Some(IconData {
                                width,
                                height,
                                pixels,
                            }));
                        }
                    }
                }
            }
        }
        
        // Try KWM_WIN_ICON (legacy)
        // Try to load KWM_WIN_ICON (KDE window icon format)
        // KWM_WIN_ICON is a CARDINAL property containing icon data
        if let Ok(kwm_atom_reply) = conn.intern_atom(false, b"KWM_WIN_ICON")?.reply() {
            let kwm_atom = kwm_atom_reply.atom;
            if let Ok(reply) = conn.get_property(
                false,
                window,
                kwm_atom,
                x11rb::protocol::xproto::AtomEnum::CARDINAL,
                0,
                8192, // KWM_WIN_ICON can be large
            )?.reply() {
                if let Some(value32) = reply.value32() {
                    let values: Vec<u32> = value32.collect();
                    if values.len() >= 2 {
                        // KWM_WIN_ICON format: [width, height, ...pixel data...]
                        let width = values[0] as usize;
                        let height = values[1] as usize;
                        let expected_pixels = width * height;
                        
                        if values.len() >= 2 + expected_pixels {
                            debug!("Loaded KWM_WIN_ICON for window {}: {}x{}", window, width, height);
                            // Convert KWM_WIN_ICON format to standard icon format
                            // KWM_WIN_ICON uses ARGB32 format (32-bit per pixel)
                            // Extract pixel data (skip width/height)
                            let pixel_data: Vec<u32> = values[2..2+expected_pixels].to_vec();
                            // Store in icon cache (can be converted to standard format later)
                            // For now, just log that we loaded it
                            debug!("KWM_WIN_ICON loaded: {} pixels", pixel_data.len());
                        }
                    }
                }
            }
        }
        
        Ok(None)
    }
    
    /// Get icon for a window (from cache or load)
    pub fn get_icon(
        &mut self,
        conn: &RustConnection,
        atoms: &Atoms,
        window: u32,
    ) -> Result<Option<&IconData>> {
        if !self.icon_cache.contains_key(&window) {
            if let Some(icon) = self.load_icon(conn, atoms, window)? {
                self.icon_cache.insert(window, icon);
            }
        }
        
        Ok(self.icon_cache.get(&window).or(self.default_icon.as_ref()))
    }
    
    /// Clear icon cache
    pub fn clear_cache(&mut self) {
        self.icon_cache.clear();
    }
    
    /// Remove icon from cache
    pub fn remove_icon(&mut self, window: u32) {
        self.icon_cache.remove(&window);
    }
}

impl Default for IconManager {
    fn default() -> Self {
        Self::new()
    }
}




