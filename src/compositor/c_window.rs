use crate::shared::Geometry;

/// Compositor window state
/// Represents a window being rendered by the Compositor
#[derive(Debug)]
pub struct CWindow {
    /// ID of window to paint (client or frame)
    pub id: u32,
    
    /// Reference to X11 client window ID
    /// If this is a frame, this points to the client content window
    /// If this is a client (unframed), it's the same as `id`
    /// 
    /// WHY: Provides link from CWindow back to Client for geometry lookups  
    /// PLAN: Will be used in frame geometry calculations and Client state sync
    #[allow(dead_code)]
    pub client_id: u32,
    
    /// Window geometry (position and size)
    pub geometry: Geometry,
    
    /// Window border width (from X11 GetGeometry reply)
    pub border_width: u16,
    
    /// Is the window viewable (mapped and visible)?
    pub viewable: bool,
    
    /// Pixmap ID for off-screen rendering
    pub pixmap: Option<u32>,
    
    /// Damage object ID for change tracking
    pub damage: Option<u32>,
    
    /// Window opacity (0.0 - 1.0)
    pub opacity: f32,
    
    /// Is the window damaged and needs redraw?
    pub damaged: bool,
    
    /// Has pixmap binding failed?
    pub bind_failed: bool,
    
    /// Is the window redirected?
    pub redirected: bool,
    
    /// Is the window unredirected (bypassing compositor)?
    pub unredirected: bool,
}

impl CWindow {
    pub fn new(id: u32, client_id: u32, geometry: Geometry, border_width: u16, viewable: bool) -> Self {
        Self {
            id,
            client_id,
            geometry,
            border_width,
            viewable,
            pixmap: None,
            damage: None,
            opacity: 1.0,
            damaged: false,
            bind_failed: false,
            redirected: false,
            unredirected: false,
        }
    }

    /// Calculate the window's total bounding box (includes X11 border width)
    pub fn outer_geometry(&self) -> Geometry {
        Geometry {
            x: self.geometry.x - self.border_width as i32,
            y: self.geometry.y - self.border_width as i32,
            width: self.geometry.width + (self.border_width as u32) * 2,
            height: self.geometry.height + (self.border_width as u32) * 2,
        }
    }

    /// Check if the window is currently covering the entire screen
    pub fn is_fullscreen(&self, screen_width: u16, screen_height: u16) -> bool {
        let outer = self.outer_geometry();
        outer.x <= 0
            && outer.y <= 0
            && outer.width >= screen_width as u32
            && outer.height >= screen_height as u32
    }
}
