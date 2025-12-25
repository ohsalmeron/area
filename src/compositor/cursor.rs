use anyhow::{Context, Result};
use x11rb::protocol::xfixes::{self, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;
use x11rb::connection::RequestConnection;
use tracing::info;

pub struct CursorManager {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
    pub xhot: u16,
    pub yhot: u16,
    pub serial: u32,
    pub pixels: Vec<u32>,
    pub texture_id: Option<u32>,
    pub visible: bool,
    pub dirty: bool,
    /// Previous position to detect movement (for render triggering)
    pub prev_x: i16,
    pub prev_y: i16,
}

impl CursorManager {
    pub fn new(conn: &RustConnection, root: u32) -> Result<Self> {
        // Enable XFixes cursor events
        let _xfixes_info = conn.extension_information(xfixes::X11_EXTENSION_NAME)?
            .context("XFixes extension not available")?;
            
        // We need XFixes 2.0 or later for cursor image
        let version = conn.xfixes_query_version(5, 0)?.reply()?;
        info!("Initialized XFixes {}.{}", version.major_version, version.minor_version);

        // Select cursor input
        let mask = xfixes::CursorNotifyMask::DISPLAY_CURSOR; 
        // We actually just need to poll or handle events.
        // For now, let's just fetch it every frame or on mouse move.
        // Better: select input on root.
        
        conn.xfixes_select_cursor_input(root, mask)?;

        Ok(Self {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            xhot: 0,
            yhot: 0,
            serial: 0,
            pixels: Vec::new(),
            texture_id: None,
            visible: true,
            dirty: true,
            prev_x: 0,
            prev_y: 0,
        })
    }

    /// Update cursor position from motion events (fast, no X11 round-trip)
    pub fn update_position(&mut self, x: i16, y: i16) {
        if self.x != x || self.y != y {
            self.prev_x = self.x;
            self.prev_y = self.y;
            self.x = x;
            self.y = y;
        }
    }
    
    /// Check if cursor moved (for render triggering)
    pub fn has_moved(&self) -> bool {
        self.x != self.prev_x || self.y != self.prev_y
    }
    
    /// Update cursor image when cursor changes (event-driven, only when needed)
    /// 
    /// This is called only on XfixesCursorNotify events, not every frame.
    /// Position is tracked separately via MotionNotify events for better performance.
    /// 
    /// Performance: This avoids the expensive X11 round-trip that was happening
    /// 60 times per second. Now it only happens when the cursor shape actually changes.
    pub fn update_image(&mut self, conn: &RustConnection) -> Result<()> {
        let image = conn.xfixes_get_cursor_image()?.reply()?;
        
        // Check if cursor actually changed (cache check - avoids unnecessary texture updates)
        if image.cursor_serial != self.serial {
            self.width = image.width;
            self.height = image.height;
            self.xhot = image.xhot;
            self.yhot = image.yhot;
            self.serial = image.cursor_serial;
            self.pixels = image.cursor_image;
            self.dirty = true; // Texture needs update
            
            // Update position from image only on initial load (when x/y are still 0)
            // After that, MotionNotify events provide more accurate and frequent position updates
            // This avoids unnecessary position queries that would add latency
            if self.x == 0 && self.y == 0 {
                // Initial position from image (before any MotionNotify events)
                self.x = image.x;
                self.y = image.y;
            }
        }
        
        Ok(())
    }
}
