//! Shared window state between WM and Compositor
//!
//! This module defines the unified `Window` structure that contains both
//! window manager state and compositor state, eliminating the need for IPC.

/// Window geometry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Geometry {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
}

/// Window frame (decorations)
#[derive(Debug, Clone)]
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

