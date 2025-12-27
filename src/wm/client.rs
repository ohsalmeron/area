use crate::shared::window_state::{Geometry, WindowFlags, WindowFrame};

/// Window Manager client state
/// Represents a window being managed by the WM
#[derive(Debug)]
pub struct Client {
    /// X11 window ID
    pub id: u32,
    
    /// Window frame (decorations)
    pub frame: Option<WindowFrame>,
    
    /// Last known/validated geometry
    pub geometry: Geometry,
    
    /// Is the window currently mapped?
    pub mapped: bool,
    
    /// Is the window currently focused?
    pub focused: bool,
    
    /// Window state flags (maximized, minimized, etc.)
    pub state: WindowFlags,
    
    /// Window title
    pub title: String,

    /// Restore geometry (for unmaximizing)
    pub restore_geometry: Option<Geometry>,
}

impl Client {
    pub fn new(id: u32, geometry: Geometry) -> Self {
        Self {
            id,
            frame: None,
            geometry,
            mapped: false,
            focused: false,
            state: WindowFlags::default(),
            title: String::new(),
            restore_geometry: None,
        }
    }

    /// Calculate the full geometry of the window including its frame/decorations.
    /// Returns client geometry directly if fullscreen (frame dimensions are 0 per xfwm4 approach).
    pub fn frame_geometry(&self) -> Geometry {
        // If fullscreen, frame dimensions are effectively 0 (like xfwm4)
        if self.state.fullscreen {
            return self.geometry;
        }
        
        if self.frame.is_some() {
            // Frame extends beyond client by frame borders
            // TODO: Get actual frame dimensions from decorations module
            const FRAME_LEFT: i32 = 2;
            const FRAME_RIGHT: i32 = 2;
            const FRAME_TOP: i32 = 32; // titlebar
            const FRAME_BOTTOM: i32 = 2;

            Geometry {
                x: self.geometry.x - FRAME_LEFT,
                y: self.geometry.y - FRAME_TOP,
                width: self.geometry.width + (FRAME_LEFT + FRAME_RIGHT) as u32,
                height: self.geometry.height + (FRAME_TOP + FRAME_BOTTOM) as u32,
            }
        } else {
            self.geometry
        }
    }
}
