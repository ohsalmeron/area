//! Shared window state between WM and Compositor
//!
//! This module defines the unified `Window` structure that contains both
//! window manager state and compositor state, eliminating the need for IPC.

/// Unified window structure containing both WM and compositor state
#[derive(Debug)]
pub struct Window {
    /// X11 window ID
    pub id: u32,
    
    /// Window manager state
    pub wm: WmState,
    
    /// Compositor state
    pub comp: CompositorState,
    
    /// Geometry (shared between WM and compositor)
    pub geometry: Geometry,
    
    /// Is the window currently mapped?
    pub mapped: bool,
    
    /// Is the window currently focused?
    pub focused: bool,
}

/// Window manager specific state
#[derive(Debug)]
pub struct WmState {
    /// Window frame (decorations)
    pub frame: Option<WindowFrame>,
    
    /// Window title
    pub title: String,
    
    /// Window flags
    pub flags: WindowFlags,
    pub restore_geometry: Option<Geometry>,
}

/// Compositor specific state
#[derive(Debug)]
pub struct CompositorState {
    /// X11 pixmap
    pub pixmap: Option<u32>,
    
    /// Damage tracking
    pub damage: Option<u32>,
    
    /// Window opacity (0.0 - 1.0)
    pub opacity: f32,
    
    /// Is the window damaged and needs redraw?
    pub damaged: bool,
    
    /// Has pixmap binding failed? (like compiz's bindFailed flag)
    pub bind_failed: bool,
}

/// Window geometry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Window frame (decorations)
#[derive(Debug)]
pub struct WindowFrame {
    pub frame: u32,
    pub titlebar: u32,
    pub close_button: u32,
    pub maximize_button: u32,
    pub minimize_button: u32,
}

/// Window flags
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowFlags {
    pub maximized: bool,
    pub minimized: bool,
}

impl Window {
    /// Create a new window with default state
    pub fn new(id: u32) -> Self {
        Self {
            id,
            wm: WmState {
                frame: None,
                title: String::new(),
                flags: WindowFlags::default(),
                restore_geometry: None,
            },
            comp: CompositorState {
                pixmap: None,
                damage: None,
                opacity: 1.0,
                damaged: false,
                bind_failed: false,
            },
            geometry: Geometry::new(0, 0, 1, 1),
            mapped: false,
            focused: false,
        }
    }
    
    /// Check if window needs rendering
    pub fn needs_render(&self) -> bool {
        self.mapped && self.comp.damaged
    }
}

impl Geometry {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
}
