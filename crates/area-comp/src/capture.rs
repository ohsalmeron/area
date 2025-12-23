//! X11 Window Capture using Composite and Damage extensions

use anyhow::{Context, Result};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::composite::ConnectionExt as CompositeExt;
use x11rb::protocol::xproto::{ConnectionExt as XprotoExt, MapState};
use x11rb::protocol::damage::{self, ConnectionExt as DamageExt, ReportLevel};
use x11rb::rust_connection::RustConnection;
use std::collections::{HashMap, HashSet};
use tracing::{debug, warn};

/// Window capture manager using X11 Composite and Damage extensions
pub struct WindowCapture {
    conn: RustConnection,
    damage_available: bool,
    /// Track Damage objects per window (window_id -> damage_id)
    window_damage: HashMap<u32, u32>,
    /// Track which windows have been damaged (need capture)
    damaged_windows: HashSet<u32>,
}

impl WindowCapture {
    /// Create a new window capture manager
    pub fn new(display: Option<&str>) -> Result<Self> {
        let display = display
            .map(|s| s.to_string())
            .or_else(|| std::env::var("DISPLAY").ok())
            .unwrap_or_else(|| ":0".into());
        
        let (conn, _) = RustConnection::connect(Some(&display))
            .context("Failed to connect to X11 server")?;
        
        // Check if Damage extension is available
        let damage_available = conn
            .extension_information(damage::X11_EXTENSION_NAME)
            .ok()
            .and_then(|info| info)
            .is_some();
        
        if damage_available {
            match conn.damage_query_version(1, 1) {
                Ok(cookie) => {
                    match cookie.reply() {
                        Ok(reply) => {
                            debug!(
                                "Damage extension {}.{} available",
                                reply.major_version, reply.minor_version
                            );
                        }
                        Err(e) => {
                            warn!("Failed to get Damage version: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to query Damage version: {}", e);
                }
            }
        } else {
            debug!("Damage extension not available, will capture all windows");
        }
        
        Ok(Self {
            conn,
            damage_available,
            window_damage: HashMap::new(),
            damaged_windows: HashSet::new(),
        })
    }

    /// Get the X11 connection (for use by other modules)
    pub fn connection(&self) -> &RustConnection {
        &self.conn
    }

    /// Create a Damage object for a window to track when it changes
    /// 
    /// This allows us to only capture windows when they're actually damaged (changed).
    /// Returns true if damage tracking was successfully set up.
    pub fn track_window_damage(&mut self, window_id: u32) -> bool {
        if !self.damage_available {
            return false;
        }
        
        // Check if we're already tracking this window
        if self.window_damage.contains_key(&window_id) {
            return true;
        }
        
        // Generate a Damage ID
        let damage_id = match self.conn.generate_id() {
            Ok(id) => id,
            Err(_) => return false,
        };
        
        // Create Damage object for this window
        // Use BOUNDING_BOX level for efficiency (reports union of all damage as single rectangle)
        match self.conn.damage_create(
            damage_id,
            window_id,
            ReportLevel::BOUNDING_BOX,
        ) {
            Ok(_) => {
                self.window_damage.insert(window_id, damage_id);
                debug!("Tracking damage for window {}", window_id);
                true
            }
            Err(e) => {
                warn!("Failed to create Damage object for window {}: {:?}", window_id, e);
                false
            }
        }
    }
    
    /// Check if a window needs capture (is damaged)
    pub fn is_damaged(&self, window_id: u32) -> bool {
        // If damage tracking is not available, always capture
        if !self.damage_available {
            return true;
        }
        
        // If window is not being tracked, capture it (first time)
        if !self.window_damage.contains_key(&window_id) {
            return true;
        }
        
        // Only capture if window is in damaged set
        self.damaged_windows.contains(&window_id)
    }
    
    /// Clear damage flag for a window after capture
    pub fn clear_damage(&mut self, window_id: u32) {
        self.damaged_windows.remove(&window_id);
    }
    
    /// Remove damage tracking for a window (when window is closed)
    pub fn untrack_window_damage(&mut self, window_id: u32) {
        if let Some(damage_id) = self.window_damage.remove(&window_id) {
            let _ = self.conn.damage_destroy(damage_id);
            self.damaged_windows.remove(&window_id);
        }
    }

    /// Process Damage events to mark windows as damaged
    pub fn process_damage_events(&mut self) -> Result<()> {
        // Poll for Damage events
        while let Some(event) = self.conn.poll_for_event()? {
            use x11rb::protocol::Event;
            if let Event::DamageNotify(e) = event {
                self.damaged_windows.insert(e.drawable);
                
                // CRITICAL: We must subtract the damage to continue receiving events!
                // Passing None for parts means we subtract (clear) all damage from the damage object.
                let _ = self.conn.damage_subtract(e.damage, x11rb::NONE, x11rb::NONE);
            }
        }
        Ok(())
    }
    
    /// Check if a window is ready for pixmap capture (mapped and has valid size)
    pub fn is_window_ready(&self, window_id: u32) -> bool {
        // Get window attributes to check map state
        let attrs_ok = match self.conn.get_window_attributes(window_id) {
            Ok(cookie) => {
                match cookie.reply() {
                    Ok(attrs) => attrs.map_state == MapState::VIEWABLE,
                    Err(e) => {
                        debug!("Failed to get window attributes for {}: {:?}", window_id, e);
                        return false;
                    }
                }
            }
            Err(e) => {
                debug!("Failed to query window attributes for {}: {:?}", window_id, e);
                return false;
            }
        };
        
        if !attrs_ok {
            return false;
        }
        
        // Get geometry to check dimensions
        match self.conn.get_geometry(window_id) {
            Ok(cookie) => {
                match cookie.reply() {
                    Ok(geom) => {
                        // Window must have non-zero dimensions
                        geom.width > 0 && geom.height > 0
                    }
                    Err(e) => {
                        debug!("Failed to get window geometry for {}: {:?}", window_id, e);
                        false
                    }
                }
            }
            Err(e) => {
                debug!("Failed to query window geometry for {}: {:?}", window_id, e);
                false
            }
        }
    }

    /// Capture window pixmap using Composite NameWindowPixmap
    /// 
    /// This returns a Pixmap ID that can be bound to an OpenGL texture via GLX_EXT_texture_from_pixmap
    /// for zero-copy compositing.
    /// 
    /// Returns None if the window is not ready or pixmap creation fails.
    pub fn capture_window_pixmap(&mut self, window_id: u32) -> Option<(u32, u8)> {
        // Validate window is ready first
        if !self.is_window_ready(window_id) {
            debug!("Window {} not ready for pixmap capture", window_id);
            return None;
        }

        // Use Composite NameWindowPixmap to get a stable pixmap for the window
        // x11rb composite wrapper
        let pixmap = match self.conn.generate_id() {
             Ok(p) => p,
             Err(_) => {
                 warn!("Failed to generate pixmap ID for window {}", window_id);
                 return None;
             }
        };

        if let Err(e) = self.conn.composite_name_window_pixmap(window_id, pixmap) {
             warn!("Failed to name window pixmap: {:?}", e);
             return None;
        }
        
        // Debug window depth and return it
        let depth = if let Ok(geom) = self.conn.get_geometry(window_id) {
            if let Ok(g) = geom.reply() {
                 debug!("Created pixmap {} for window {} (depth: {}, size: {}x{})", pixmap, window_id, g.depth, g.width, g.height);
                 g.depth
            } else {
                warn!("Failed to get geometry for window {}", window_id);
                // Return default depth 24 if we can't get it, but this shouldn't happen if is_window_ready passed
                24
            }
        } else {
            24
        };

        Some((pixmap, depth))
    }

    /// Free an X11 pixmap
    pub fn free_pixmap(&self, pixmap_id: u32) {
        if let Err(e) = self.conn.free_pixmap(pixmap_id) {
            warn!("Failed to free pixmap {}: {:?}", pixmap_id, e);
        }
    }
}

