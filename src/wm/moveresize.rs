//! MoveResize Module
//!
//! Handles interactive window moving and resizing with gravity, constraints, and snapping.
//! This matches xfwm4's move/resize system.

use anyhow::Result;
use tracing::debug;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::shared::Geometry;
use crate::wm::client::Client;
use crate::wm::display::DisplayInfo;
use crate::wm::hints::HintsManager;
use crate::wm::screen::ScreenInfo;

/// Move/resize operation state
#[derive(Debug, Clone)]
pub struct MoveResizeState {
    /// Window being moved/resized
    pub window: u32,
    
    /// Start position (root coordinates)
    pub start_x: i16,
    pub start_y: i16,
    
    /// Window geometry at start
    pub start_geometry: Geometry,
    
    /// Operation type
    pub operation: MoveResizeOperation,
    
    /// Original operation type before switching to Keyboard mode (for keyboard operations)
    pub keyboard_operation: Option<MoveResizeOperation>,
    
    /// Is operation active?
    pub active: bool,
}

/// Move/resize operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveResizeOperation {
    /// Moving window
    Move,
    /// Resizing window (with direction)
    Resize(ResizeDirection),
    /// Keyboard move/resize
    Keyboard,
}

/// Resize direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeDirection {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

/// Move/resize manager
pub struct MoveResizeManager {
    /// Current operation state
    pub state: Option<MoveResizeState>,
    
    /// Snap distance (pixels)
    pub snap_distance: i32,
    
    /// Snap to edges enabled
    pub snap_to_edges: bool,
    
    /// Snap to windows enabled
    pub snap_to_windows: bool,
    
    /// Wrap windows (workspace edge wrapping)
    pub wrap_windows: bool,
}

impl MoveResizeManager {
    /// Create a new move/resize manager
    pub fn new() -> Self {
        Self {
            state: None,
            snap_distance: 10,
            snap_to_edges: true,
            snap_to_windows: true,
            wrap_windows: false,
        }
    }
    
    /// Start a move operation
    pub fn start_move(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        window: u32,
        root_x: i16,
        root_y: i16,
        client: &Client,
    ) -> Result<()> {
        debug!("Starting move operation for window {}", window);
        
        self.state = Some(MoveResizeState {
            window,
            start_x: root_x,
            start_y: root_y,
            start_geometry: client.geometry,
            operation: MoveResizeOperation::Move,
            keyboard_operation: None,
            active: true,
        });
        
        // Grab pointer for move operation
        conn.grab_pointer(
            false,
            screen_info.root,
            EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            0u32,
            0u32,
            x11rb::CURRENT_TIME,
        )?;
        
        Ok(())
    }
    
    /// Start a resize operation
    pub fn start_resize(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        window: u32,
        root_x: i16,
        root_y: i16,
        direction: ResizeDirection,
        client: &Client,
    ) -> Result<()> {
        debug!("Starting resize operation for window {} (direction={:?})", window, direction);
        
        self.state = Some(MoveResizeState {
            window,
            start_x: root_x,
            start_y: root_y,
            start_geometry: client.geometry,
            operation: MoveResizeOperation::Resize(direction),
            keyboard_operation: None,
            active: true,
        });
        
        // Grab pointer for resize operation
        conn.grab_pointer(
            false,
            screen_info.root,
            EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            0u32,
            0u32,
            x11rb::CURRENT_TIME,
        )?;
        
        Ok(())
    }
    
    /// Handle motion during move/resize
    pub fn handle_motion(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        root_x: i16,
        root_y: i16,
        client: &mut Client,
    ) -> Result<()> {
        let state = if let Some(ref mut s) = self.state {
            if !s.active {
                return Ok(());
            }
            s.clone()
        } else {
            return Ok(());
        };
        
        let dx = root_x - state.start_x;
        let dy = root_y - state.start_y;
        
        match state.operation {
                MoveResizeOperation::Move => {
                    let mut new_x = state.start_geometry.x + dx as i32;
                    let mut new_y = state.start_geometry.y + dy as i32;
                    
                    // Apply snapping
                    if self.snap_to_edges {
                        (new_x, new_y) = self.snap_to_screen_edges(
                            screen_info,
                            new_x,
                            new_y,
                            state.start_geometry.width,
                            state.start_geometry.height,
                        );
                    }
                    
                    // Constrain to work area
                    let work_area = &screen_info.work_area;
                    new_x = new_x.max(work_area.x);
                    new_y = new_y.max(work_area.y);
                    new_x = new_x.min(work_area.x + work_area.width as i32 - state.start_geometry.width as i32);
                    new_y = new_y.min(work_area.y + work_area.height as i32 - state.start_geometry.height as i32);
                    
                    client.geometry.x = new_x;
                    client.geometry.y = new_y;
                    
                    // Apply to window
                    // For frames, the frame window position needs to account for titlebar
                    if let Some(frame) = &client.frame {
                        const TITLEBAR_HEIGHT: i32 = 32;
                        // Frame position is client position minus titlebar height
                        conn.configure_window(
                            frame.frame,
                            &ConfigureWindowAux::new()
                                .x(new_x)
                                .y(new_y - TITLEBAR_HEIGHT),
                        )?;
                    } else {
                        // No frame, move client window directly
                        conn.configure_window(
                            state.window,
                            &ConfigureWindowAux::new()
                                .x(new_x)
                                .y(new_y),
                        )?;
                    }
                    
                    // Update state
                    if let Some(ref mut s) = self.state {
                        s.start_geometry.x = new_x;
                        s.start_geometry.y = new_y;
                    }
                }
                MoveResizeOperation::Resize(direction) => {
                    let mut new_geom = state.start_geometry.clone();
                    
                    // Apply resize based on direction
                    match direction {
                        ResizeDirection::TopLeft => {
                            new_geom.x = state.start_geometry.x + dx as i32;
                            new_geom.y = state.start_geometry.y + dy as i32;
                            new_geom.width = ((state.start_geometry.width as i32).saturating_sub(dx as i32)).max(1) as u32;
                            new_geom.height = ((state.start_geometry.height as i32).saturating_sub(dy as i32)).max(1) as u32;
                        }
                        ResizeDirection::Top => {
                            new_geom.y = state.start_geometry.y + dy as i32;
                            new_geom.height = ((state.start_geometry.height as i32).saturating_sub(dy as i32)).max(1) as u32;
                        }
                        ResizeDirection::TopRight => {
                            new_geom.y = state.start_geometry.y + dy as i32;
                            new_geom.width = ((state.start_geometry.width as i32) + dx as i32).max(1) as u32;
                            new_geom.height = ((state.start_geometry.height as i32).saturating_sub(dy as i32)).max(1) as u32;
                        }
                        ResizeDirection::Right => {
                            new_geom.width = ((state.start_geometry.width as i32) + dx as i32).max(1) as u32;
                        }
                        ResizeDirection::BottomRight => {
                            new_geom.width = ((state.start_geometry.width as i32) + dx as i32).max(1) as u32;
                            new_geom.height = ((state.start_geometry.height as i32) + dy as i32).max(1) as u32;
                        }
                        ResizeDirection::Bottom => {
                            new_geom.height = ((state.start_geometry.height as i32) + dy as i32).max(1) as u32;
                        }
                        ResizeDirection::BottomLeft => {
                            new_geom.x = state.start_geometry.x + dx as i32;
                            new_geom.width = ((state.start_geometry.width as i32).saturating_sub(dx as i32)).max(1) as u32;
                            new_geom.height = ((state.start_geometry.height as i32) + dy as i32).max(1) as u32;
                        }
                        ResizeDirection::Left => {
                            new_geom.x = state.start_geometry.x + dx as i32;
                            new_geom.width = ((state.start_geometry.width as i32).saturating_sub(dx as i32)).max(1) as u32;
                        }
                    }
                    
                    // Apply size hints (min/max size, increments)
                    if let Some(ref size_hints) = client.size_hints {
                        use crate::wm::hints::HintsManager;
                        // Convert client::SizeHints to hints::SizeHints
                        let hints_size_hints = crate::wm::hints::SizeHints {
                            flags: size_hints.flags,
                            x: size_hints.x,
                            y: size_hints.y,
                            width: size_hints.width,
                            height: size_hints.height,
                            min_width: size_hints.min_width,
                            min_height: size_hints.min_height,
                            max_width: size_hints.max_width,
                            max_height: size_hints.max_height,
                            width_inc: size_hints.width_inc,
                            height_inc: size_hints.height_inc,
                            min_aspect_num: size_hints.min_aspect_num,
                            min_aspect_den: size_hints.min_aspect_den,
                            max_aspect_num: size_hints.max_aspect_num,
                            max_aspect_den: size_hints.max_aspect_den,
                            base_width: size_hints.base_width,
                            base_height: size_hints.base_height,
                            win_gravity: size_hints.win_gravity,
                        };
                        let hints_manager = HintsManager;
                        new_geom = hints_manager.apply_size_hints(&hints_size_hints, &new_geom);
                    }
                    
                    // Constrain to work area
                    let work_area = &screen_info.work_area;
                    if new_geom.x < work_area.x {
                        let adjust = work_area.x - new_geom.x;
                        new_geom.x = work_area.x;
                        new_geom.width = new_geom.width.saturating_sub(adjust as u32);
                    }
                    if new_geom.y < work_area.y {
                        let adjust = work_area.y - new_geom.y;
                        new_geom.y = work_area.y;
                        new_geom.height = new_geom.height.saturating_sub(adjust as u32);
                    }
                    new_geom.width = new_geom.width.min(work_area.width);
                    new_geom.height = new_geom.height.min(work_area.height);
                    
                    client.geometry = new_geom;
                    
                    // Apply to window
                    // For frames, we need to account for titlebar and borders
                    if let Some(frame) = &client.frame {
                        const TITLEBAR_HEIGHT: i32 = 32;
                        const BORDER_WIDTH: i32 = 2;
                        // Frame size = client size + titlebar + borders
                        let frame_width = new_geom.width + (BORDER_WIDTH * 2) as u32;
                        let frame_height = new_geom.height + (TITLEBAR_HEIGHT + BORDER_WIDTH * 2) as u32;
                        // Frame position = client position - border - titlebar
                        let frame_x = new_geom.x - BORDER_WIDTH;
                        let frame_y = new_geom.y - TITLEBAR_HEIGHT - BORDER_WIDTH;
                        
                        conn.configure_window(
                            frame.frame,
                            &ConfigureWindowAux::new()
                                .x(frame_x)
                                .y(frame_y)
                                .width(frame_width)
                                .height(frame_height),
                        )?;
                    } else {
                        // No frame, resize client directly
                        conn.configure_window(
                            state.window,
                            &ConfigureWindowAux::new()
                                .x(new_geom.x)
                                .y(new_geom.y)
                                .width(new_geom.width)
                                .height(new_geom.height),
                        )?;
                    }
                    
                    // Update state
                    if let Some(ref mut s) = self.state {
                        s.start_geometry = new_geom.clone();
                    }
                }
                MoveResizeOperation::Keyboard => {
                    // Keyboard move/resize is handled via arrow keys in main.rs
                    // This case handles mouse motion during keyboard operations (shouldn't happen normally)
                    // For keyboard operations, we use the original operation type
                    if let Some(keyboard_op) = state.keyboard_operation {
                        match keyboard_op {
                            MoveResizeOperation::Move => {
                                // Handle as move operation
                                let mut new_x = state.start_geometry.x + dx as i32;
                                let mut new_y = state.start_geometry.y + dy as i32;
                                
                                // Apply constraints
                                let work_area = &screen_info.work_area;
                                new_x = new_x.max(work_area.x);
                                new_y = new_y.max(work_area.y);
                                new_x = new_x.min(work_area.x + work_area.width as i32 - state.start_geometry.width as i32);
                                new_y = new_y.min(work_area.y + work_area.height as i32 - state.start_geometry.height as i32);
                                
                                client.geometry.x = new_x;
                                client.geometry.y = new_y;
                                
                                if let Some(frame) = &client.frame {
                                    const TITLEBAR_HEIGHT: i32 = 32;
                                    conn.configure_window(
                                        frame.frame,
                                        &ConfigureWindowAux::new()
                                            .x(new_x)
                                            .y(new_y - TITLEBAR_HEIGHT),
                                    )?;
                                } else {
                                    conn.configure_window(
                                        state.window,
                                        &ConfigureWindowAux::new()
                                            .x(new_x)
                                            .y(new_y),
                                    )?;
                                }
                                
                                if let Some(ref mut s) = self.state {
                                    s.start_geometry.x = new_x;
                                    s.start_geometry.y = new_y;
                                }
                            }
                            MoveResizeOperation::Resize(direction) => {
                                // Handle as resize operation (reuse existing resize logic)
                                let mut new_geom = state.start_geometry.clone();
                                
                                // Apply resize based on direction (same logic as Resize case)
                                match direction {
                                    ResizeDirection::TopLeft => {
                                        new_geom.x = state.start_geometry.x + dx as i32;
                                        new_geom.y = state.start_geometry.y + dy as i32;
                                        new_geom.width = (state.start_geometry.width as i32 - dx as i32).max(1) as u32;
                                        new_geom.height = (state.start_geometry.height as i32 - dy as i32).max(1) as u32;
                                    }
                                    ResizeDirection::Top => {
                                        new_geom.y = state.start_geometry.y + dy as i32;
                                        new_geom.height = (state.start_geometry.height as i32 - dy as i32).max(1) as u32;
                                    }
                                    ResizeDirection::TopRight => {
                                        new_geom.y = state.start_geometry.y + dy as i32;
                                        new_geom.width = (state.start_geometry.width as i32 + dx as i32).max(1) as u32;
                                        new_geom.height = (state.start_geometry.height as i32 - dy as i32).max(1) as u32;
                                    }
                                    ResizeDirection::Right => {
                                        new_geom.width = (state.start_geometry.width as i32 + dx as i32).max(1) as u32;
                                    }
                                    ResizeDirection::BottomRight => {
                                        new_geom.width = (state.start_geometry.width as i32 + dx as i32).max(1) as u32;
                                        new_geom.height = (state.start_geometry.height as i32 + dy as i32).max(1) as u32;
                                    }
                                    ResizeDirection::Bottom => {
                                        new_geom.height = (state.start_geometry.height as i32 + dy as i32).max(1) as u32;
                                    }
                                    ResizeDirection::BottomLeft => {
                                        new_geom.x = state.start_geometry.x + dx as i32;
                                        new_geom.width = (state.start_geometry.width as i32 - dx as i32).max(1) as u32;
                                        new_geom.height = (state.start_geometry.height as i32 + dy as i32).max(1) as u32;
                                    }
                                    ResizeDirection::Left => {
                                        new_geom.x = state.start_geometry.x + dx as i32;
                                        new_geom.width = (state.start_geometry.width as i32 - dx as i32).max(1) as u32;
                                    }
                                }
                                
                                // Apply constraints and update window
                                // (Similar to Resize case, but simplified for keyboard)
                                client.geometry = new_geom.clone();
                                
                                if let Some(frame) = &client.frame {
                                    const TITLEBAR_HEIGHT: i32 = 32;
                                    conn.configure_window(
                                        frame.frame,
                                        &ConfigureWindowAux::new()
                                            .width(new_geom.width)
                                            .height(new_geom.height + TITLEBAR_HEIGHT as u32),
                                    )?;
                                } else {
                                    conn.configure_window(
                                        state.window,
                                        &ConfigureWindowAux::new()
                                            .x(new_geom.x)
                                            .y(new_geom.y)
                                            .width(new_geom.width)
                                            .height(new_geom.height),
                                    )?;
                                }
                                
                                if let Some(ref mut s) = self.state {
                                    s.start_geometry = new_geom;
                                }
                            }
                            _ => {
                                // Shouldn't happen
                                debug!("Unexpected keyboard operation type: {:?}", keyboard_op);
                            }
                        }
                    } else {
                        debug!("Keyboard operation without original operation type");
                    }
                }
            }
            
            conn.flush()?;
        
        Ok(())
    }
    
    /// Finish move/resize operation
    pub fn finish(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
    ) -> Result<()> {
        if let Some(state) = &self.state {
            if state.active {
                // Ungrab pointer
                conn.ungrab_pointer(x11rb::CURRENT_TIME)?;
                
                // Send ConfigureNotify
                if let Some(client) = self.state.as_ref() {
                    let event = ConfigureNotifyEvent {
                        response_type: 22, // ConfigureNotify
                        sequence: 0,
                        event: client.window,
                        window: client.window,
                        above_sibling: 0,
                        x: client.start_geometry.x as i16,
                        y: client.start_geometry.y as i16,
                        width: client.start_geometry.width as u16,
                        height: client.start_geometry.height as u16,
                        border_width: 0,
                        override_redirect: false,
                    };
                    
                    conn.send_event(false, client.window, EventMask::STRUCTURE_NOTIFY, &event)?;
                    conn.flush()?;
                }
                
                debug!("Finished move/resize operation for window {}", state.window);
            }
        }
        
        self.state = None;
        Ok(())
    }
    
    /// Snap to screen edges
    fn snap_to_screen_edges(
        &self,
        screen_info: &ScreenInfo,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> (i32, i32) {
        let work_area = &screen_info.work_area;
        let mut new_x = x;
        let mut new_y = y;
        
        // Snap to left edge
        if (x - work_area.x).abs() < self.snap_distance {
            new_x = work_area.x;
        }
        
        // Snap to right edge
        let right_edge = work_area.x + work_area.width as i32;
        let window_right = x + width as i32;
        if (window_right - right_edge).abs() < self.snap_distance {
            new_x = right_edge - width as i32;
        }
        
        // Snap to top edge
        if (y - work_area.y).abs() < self.snap_distance {
            new_y = work_area.y;
        }
        
        // Snap to bottom edge
        let bottom_edge = work_area.y + work_area.height as i32;
        let window_bottom = y + height as i32;
        if (window_bottom - bottom_edge).abs() < self.snap_distance {
            new_y = bottom_edge - height as i32;
        }
        
        (new_x, new_y)
    }
    
    /// Check if move/resize is active
    pub fn is_active(&self) -> bool {
        self.state.as_ref().map(|s| s.active).unwrap_or(false)
    }
}

impl Default for MoveResizeManager {
    fn default() -> Self {
        Self::new()
    }
}

