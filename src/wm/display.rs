//! Display Module
//!
//! Manages X11 display connection, extensions, atoms, cursors, and global state.
//! This is the top-level structure in xfwm4's architecture.

use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::wm::ewmh::Atoms;

/// X11 Extension information
#[derive(Debug, Clone)]
pub struct Extensions {
    pub have_shape: bool,
    pub have_render: bool,
    pub have_xrandr: bool,
    pub have_xsync: bool,
    pub have_xres: bool,
    pub have_composite: bool,
    pub have_damage: bool,
    pub have_fixes: bool,
    pub have_name_window_pixmap: bool,
    pub have_overlays: bool,
    pub have_present: bool,
    pub have_xinput2: bool,
    
    // Extension version info
    pub shape_version: (i32, i32),
    pub shape_event_base: i32,
    pub render_error_base: i32,
    pub render_event_base: i32,
    pub xrandr_error_base: i32,
    pub xrandr_event_base: i32,
    pub xsync_event_base: i32,
    pub xsync_error_base: i32,
    pub xres_event_base: i32,
    pub xres_error_base: i32,
    pub composite_error_base: i32,
    pub composite_event_base: i32,
    pub damage_error_base: i32,
    pub damage_event_base: i32,
    pub fixes_error_base: i32,
    pub fixes_event_base: i32,
    pub present_error_base: i32,
    pub present_event_base: i32,
    pub xinput2_error_base: i32,
    pub xinput2_event_base: i32,
}

impl Default for Extensions {
    fn default() -> Self {
        Self {
            have_shape: false,
            have_render: false,
            have_xrandr: false,
            have_xsync: false,
            have_xres: false,
            have_composite: false,
            have_damage: false,
            have_fixes: false,
            have_name_window_pixmap: false,
            have_overlays: false,
            have_present: false,
            have_xinput2: false,
            shape_version: (0, 0),
            shape_event_base: 0,
            render_error_base: 0,
            render_event_base: 0,
            xrandr_error_base: 0,
            xrandr_event_base: 0,
            xsync_event_base: 0,
            xsync_error_base: 0,
            xres_event_base: 0,
            xres_error_base: 0,
            composite_error_base: 0,
            composite_event_base: 0,
            damage_error_base: 0,
            damage_event_base: 0,
            fixes_error_base: 0,
            fixes_event_base: 0,
            present_error_base: 0,
            present_event_base: 0,
            xinput2_error_base: 0,
            xinput2_event_base: 0,
        }
    }
}

/// Cursor management
#[derive(Debug)]
pub struct Cursors {
    pub busy: u32,
    pub move_cursor: u32,
    pub root: u32,
    pub resize: [u32; 8], // 4 sides + 4 corners
}

impl Cursors {
    pub fn new(conn: &RustConnection, screen: &Screen) -> Result<Self> {
        let font = conn.generate_id()?;
        conn.open_font(font, b"cursor")?;
        
        // Create cursors using font glyphs
        // Cursor font glyph IDs from X11 cursor font:
        // - watch (busy): 75
        // - fleur (move): 26
        // - left_ptr (root): 34
        // - Resize cursors: top_left_corner=67, top_side=69, top_right_corner=68,
        //   right_side=48, bottom_right_corner=7, bottom_side=8,
        //   bottom_left_corner=6, left_side=35
        
        // Helper function to create a glyph cursor
        let create_cursor = |glyph_id: u16| -> Result<u32> {
            let cursor_id = conn.generate_id()?;
            conn.create_glyph_cursor(
                cursor_id,
                font,
                font,
                glyph_id,      // source_char
                glyph_id + 1,  // mask_char (glyph + 1 for mask)
                0, 0, 0,       // foreground: black
                0xffff, 0xffff, 0xffff,  // background: white
            )?;
            Ok(cursor_id)
        };
        
        // Create busy cursor (watch/hourglass)
        let busy = create_cursor(75)?;
        
        // Create move cursor (fleur)
        let move_cursor = create_cursor(26)?;
        
        // Create root cursor (left_ptr/default pointer)
        let root = create_cursor(34)?;
        
        // Create resize cursors in order: top-left, top, top-right, right, bottom-right, bottom, bottom-left, left
        let resize = [
            create_cursor(67)?,  // top_left_corner
            create_cursor(69)?,  // top_side
            create_cursor(68)?,  // top_right_corner
            create_cursor(48)?,  // right_side
            create_cursor(7)?,   // bottom_right_corner
            create_cursor(8)?,   // bottom_side
            create_cursor(6)?,   // bottom_left_corner
            create_cursor(35)?,  // left_side
        ];
        
        conn.close_font(font)?;
        
        Ok(Self {
            busy,
            move_cursor,
            root,
            resize,
        })
    }
}

/// DisplayInfo - Top-level display connection and global state
/// 
/// This is the equivalent of xfwm4's DisplayInfo structure.
/// It manages the X11 connection, extensions, atoms, cursors, and global WM state.
pub struct DisplayInfo {
    /// X11 connection
    pub conn: Arc<RustConnection>,
    
    /// All EWMH/ICCCM atoms
    pub atoms: Atoms,
    
    /// X11 extension information
    pub extensions: Extensions,
    
    /// Cursor management
    pub cursors: Cursors,
    
    /// Timestamp window (for getting current time)
    pub timestamp_win: u32,
    
    /// Current X11 time
    pub current_time: u32,
    
    /// Last user interaction time
    pub last_user_time: u32,
    
    /// Quit flag
    pub quit: bool,
    
    /// Reload flag (for settings reload)
    pub reload: bool,
    
    /// Double-click time (milliseconds)
    pub double_click_time: i32,
    
    /// Double-click distance (pixels)
    pub double_click_distance: i32,
    
    /// Hostname
    pub hostname: String,
    
    /// Number of screens
    pub nb_screens: usize,
}

impl DisplayInfo {
    /// Create a new DisplayInfo
    pub fn new(conn: Arc<RustConnection>) -> Result<Self> {
        info!("Initializing DisplayInfo");
        
        let setup = conn.setup();
        let _screen = &setup.roots[0];
        
        // Initialize atoms
        let atoms = Atoms::new(conn.as_ref())?;
        debug!("Initialized {} atoms", std::mem::size_of::<Atoms>());
        
        // Detect X11 extensions (need to clone Arc to avoid borrow issues)
        let conn_ref = conn.as_ref();
        let extensions = Self::detect_extensions(conn_ref)?;
        info!("X11 Extensions: shape={}, render={}, randr={}, sync={}, composite={}, damage={}, fixes={}, present={}, xinput2={}",
            extensions.have_shape,
            extensions.have_render,
            extensions.have_xrandr,
            extensions.have_xsync,
            extensions.have_composite,
            extensions.have_damage,
            extensions.have_fixes,
            extensions.have_present,
            extensions.have_xinput2,
        );
        
        // Create cursors
        let cursors = Cursors::new(conn_ref, &setup.roots[0])?;
        
        // Create timestamp window
        let timestamp_win = conn_ref.generate_id()?;
        conn_ref.create_window(
            setup.roots[0].root_depth,
            timestamp_win,
            setup.roots[0].root,
            -1000, // Off-screen
            -1000,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new(),
        )?;
        conn_ref.map_window(timestamp_win)?;
        conn_ref.flush()?;
        debug!("Created timestamp window: 0x{:x}", timestamp_win);
        
        // Get hostname
        let hostname = std::env::var("HOSTNAME")
            .unwrap_or_else(|_| "unknown".to_string());
        
        let nb_screens = setup.roots.len();
        
        Ok(Self {
            conn,
            atoms,
            extensions,
            cursors,
            timestamp_win,
            current_time: x11rb::CURRENT_TIME,
            last_user_time: 0,
            quit: false,
            reload: false,
            double_click_time: 400, // Default 400ms
            double_click_distance: 5, // Default 5 pixels
            hostname,
            nb_screens,
        })
    }
    
    /// Detect available X11 extensions
    fn detect_extensions(conn: &RustConnection) -> Result<Extensions> {
        let mut ext = Extensions::default();
        
        // Query extensions
        let extensions = conn.query_extension(b"SHAPE")?;
        if let Ok(reply) = extensions.reply() {
            if reply.present {
                ext.have_shape = true;
                ext.shape_event_base = reply.first_event as i32;
                // Query version
                use x11rb::protocol::shape::ConnectionExt;
                if let Ok(version_reply) = conn.shape_query_version()?.reply() {
                    ext.shape_version = (version_reply.major_version as i32, version_reply.minor_version as i32);
                    debug!("Shape extension version: {}.{}", version_reply.major_version, version_reply.minor_version);
                }
            }
        }
        
        let extensions = conn.query_extension(b"RENDER")?;
        if let Ok(reply) = extensions.reply() {
            if reply.present {
                ext.have_render = true;
                ext.render_event_base = reply.first_event as i32;
                ext.render_error_base = reply.first_error as i32;
            }
        }
        
        let extensions = conn.query_extension(b"RANDR")?;
        if let Ok(reply) = extensions.reply() {
            if reply.present {
                ext.have_xrandr = true;
                ext.xrandr_event_base = reply.first_event as i32;
                ext.xrandr_error_base = reply.first_error as i32;
            }
        }
        
        let extensions = conn.query_extension(b"SYNC")?;
        if let Ok(reply) = extensions.reply() {
            if reply.present {
                ext.have_xsync = true;
                ext.xsync_event_base = reply.first_event as i32;
                ext.xsync_error_base = reply.first_error as i32;
            }
        }
        
        let extensions = conn.query_extension(b"X-Resource")?;
        if let Ok(reply) = extensions.reply() {
            if reply.present {
                ext.have_xres = true;
                ext.xres_event_base = reply.first_event as i32;
                ext.xres_error_base = reply.first_error as i32;
            }
        }
        
        let extensions = conn.query_extension(b"Composite")?;
        if let Ok(reply) = extensions.reply() {
            if reply.present {
                ext.have_composite = true;
                ext.composite_event_base = reply.first_event as i32;
                ext.composite_error_base = reply.first_error as i32;
                // Check for NameWindowPixmap (version >= 0.2)
                use x11rb::protocol::composite::ConnectionExt;
                if let Ok(version_reply) = conn.composite_query_version(0, 4)?.reply() {
                    debug!("Composite extension version: {}.{}", version_reply.major_version, version_reply.minor_version);
                    // Check for NameWindowPixmap (version >= 0.2)
                    if version_reply.major_version > 0 || version_reply.minor_version >= 2 {
                        ext.have_name_window_pixmap = true;
                    }
                }
            }
        }
        
        let extensions = conn.query_extension(b"DAMAGE")?;
        if let Ok(reply) = extensions.reply() {
            if reply.present {
                ext.have_damage = true;
                ext.damage_event_base = reply.first_event as i32;
                ext.damage_error_base = reply.first_error as i32;
            }
        }
        
        let extensions = conn.query_extension(b"XFIXES")?;
        if let Ok(reply) = extensions.reply() {
            if reply.present {
                ext.have_fixes = true;
                ext.fixes_event_base = reply.first_event as i32;
                ext.fixes_error_base = reply.first_error as i32;
            }
        }
        
        let extensions = conn.query_extension(b"Present")?;
        if let Ok(reply) = extensions.reply() {
            if reply.present {
                ext.have_present = true;
                ext.present_event_base = reply.first_event as i32;
                ext.present_error_base = reply.first_error as i32;
            }
        }
        
        let extensions = conn.query_extension(b"XInputExtension")?;
        if let Ok(reply) = extensions.reply() {
            if reply.present {
                ext.xinput2_event_base = reply.first_event as i32;
                ext.xinput2_error_base = reply.first_error as i32;
                // Check for XInput2 (version >= 2.0)
                use x11rb::protocol::xinput::xi_query_version;
                if let Ok(version_reply) = xi_query_version(conn, 2, 0)?.reply() {
                    debug!("XInput extension version: {}.{}", version_reply.major_version, version_reply.minor_version);
                    // XInput2 is version >= 2.0
                    if version_reply.major_version >= 2 {
                        ext.have_xinput2 = true;
                    }
                }
            }
        }
        
        Ok(ext)
    }
    
    /// Update current time from X11 server
    pub fn update_current_time(&mut self) -> Result<()> {
        // Send a request to get current time
        // In practice, we use the timestamp from events
        // This is a placeholder - actual implementation uses event timestamps
        Ok(())
    }
    
    /// Get current time (from last event or server)
    pub fn get_current_time(&self) -> u32 {
        self.current_time
    }
    
    /// Set current time (called from event handlers)
    pub fn set_current_time(&mut self, time: u32) {
        if time != 0 {
            self.current_time = time;
        }
    }
}

