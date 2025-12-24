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
        })
    }

    pub fn update(&mut self, conn: &RustConnection) -> Result<()> {
        let image = conn.xfixes_get_cursor_image()?.reply()?;
        
        // Check if cursor changed
        if image.cursor_serial != self.serial || self.dirty {
            self.width = image.width;
            self.height = image.height;
            self.xhot = image.xhot;
            self.yhot = image.yhot;
            self.serial = image.cursor_serial;
            self.pixels = image.cursor_image;
            self.dirty = true; // Texture needs update
        }
        
        self.x = image.x;
        self.y = image.y;
        
        Ok(())
    }
}
