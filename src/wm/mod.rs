//! Window Manager Module
//!
//! Handles X11 window management, decorations, and user interactions.

pub mod decorations;
pub mod ewmh;
// pub mod window; // Removed dead code module

use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

use crate::shared::{Window as SharedWindow, Geometry};
pub use decorations::ButtonType;
pub use ewmh::Atoms;
// Removed dead code module usage

/// Drag state for window dragging
#[derive(Debug, Clone)]
struct DragState {
    window_id: u32,
    start_x: i16,
    start_y: i16,
    window_start_x: i32,
    window_start_y: i32,
}


pub struct WindowManager {
    screen_num: usize,
    root: u32,
    atoms: Atoms,
    drag_state: Option<DragState>,
}

impl WindowManager {
    /// Create a new window manager
    pub fn new(
        conn: &x11rb::rust_connection::RustConnection,
        screen_num: usize,
        root: u32,
    ) -> Result<Self> {
        info!("Initializing window manager");
        
        // Become the window manager by selecting SubstructureRedirect on root
        let event_mask = EventMask::SUBSTRUCTURE_REDIRECT
            | EventMask::SUBSTRUCTURE_NOTIFY
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::POINTER_MOTION
            | EventMask::ENTER_WINDOW
            | EventMask::LEAVE_WINDOW
            | EventMask::PROPERTY_CHANGE
            | EventMask::FOCUS_CHANGE
            | EventMask::KEY_PRESS; // For keyboard shortcuts
        
        conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(event_mask),
        )?
        .check()
        .context("Failed to become window manager - is another WM running?")?;
        
        // Grab SUPER key (Mod4) for launcher
        // Note: Key grabbing may fail if keycodes don't match - we'll handle gracefully
        // The actual keycode for SUPER varies by keyboard layout
        use x11rb::protocol::xproto::ModMask;
        let no_modifier = ModMask::from(0u16);
        
        // Try to grab SUPER key (keycode 133 = left SUPER, 134 = right SUPER)
        // These may fail if keycodes differ - that's OK, we'll catch events via KeyPress
        for keycode in [133u8, 134u8] {
            let _ = conn.grab_key(
                false, // owner_events
                root,
                no_modifier,
                keycode,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            );
        }
        // Don't flush here - let errors be handled gracefully
        // conn.flush()?;
        
        // Initialize EWMH atoms
        let atoms = Atoms::new(conn)?;
        atoms.setup_supported(conn, root)?;
        
        info!("Successfully became window manager (keyboard shortcuts enabled)");
        
        Ok(Self {
            screen_num,
            root,
            atoms,
            drag_state: None,
        })
    }
    
    /// Manage a new window (called when MapRequest is received)
    pub fn manage_window(
        &mut self,
        conn: &x11rb::rust_connection::RustConnection,
        window: &mut SharedWindow,
    ) -> Result<()> {
        debug!("WM: Managing window {}", window.id);
        
        // Get window attributes
        let attrs = conn.get_window_attributes(window.id)?
            .reply()
            .context("Failed to get window attributes")?;
        
        if attrs.override_redirect {
            debug!("Window {} is override-redirect, skipping", window.id);
            return Ok(());
        }
        
        // Get window geometry
        let geom = conn.get_geometry(window.id)?
            .reply()
            .context("Failed to get window geometry")?;
        
        // Get window's preferred size from WM_NORMAL_HINTS if available
        let mut width = geom.width as u32;
        let mut height = geom.height as u32;
        
        // If window is 1x1 (uninitialized), try to get size from WM_NORMAL_HINTS
        if width == 1 && height == 1 {
            if let Ok(reply) = conn.get_property(
                false,
                window.id,
                AtomEnum::WM_NORMAL_HINTS,
                AtomEnum::WM_SIZE_HINTS,
                0,
                18, // WM_SIZE_HINTS is 18 32-bit values
            )?.reply() {
                if let Some(value32) = reply.value32() {
                    let hints: Vec<u32> = value32.take(18).collect();
                    if hints.len() >= 18 {
                        // WM_SIZE_HINTS structure:
                        // flags (u32), pad (u32), min_width (u32), min_height (u32), 
                        // max_width (u32), max_height (u32), width_inc (u32), height_inc (u32),
                        // min_aspect (u32), max_aspect (u32), base_width (u32), base_height (u32),
                        // win_gravity (u32), pad (u32), pad (u32), pad (u32), pad (u32), pad (u32)
                        // We want base_width/base_height or a reasonable default
                        let base_width = hints.get(10).copied().unwrap_or(0);
                        let base_height = hints.get(11).copied().unwrap_or(0);
                        if base_width > 0 && base_height > 0 {
                            width = base_width;
                            height = base_height;
                        } else {
                            // Default size if no hints
                            width = 800;
                            height = 600;
                        }
                    }
                }
            } else {
                // No WM_NORMAL_HINTS, use default size
                width = 800;
                height = 600;
            }
            
            // Set window to proper size
            conn.configure_window(
                window.id,
                &ConfigureWindowAux::new()
                    .width(width)
                    .height(height),
            )?;
        }
        
        // Account for panel height (40px) - windows should start below the panel
        const PANEL_HEIGHT: i32 = 40;
        let window_y = geom.y as i32;
        // If window is at the top (y < panel height), position it below the panel
        let adjusted_y = if window_y < PANEL_HEIGHT {
            PANEL_HEIGHT
        } else {
            window_y
        };
        
        window.geometry = Geometry {
            x: geom.x as i32,
            y: adjusted_y,
            width,
            height,
        };
        
        // Get window title
        if let Ok(reply) = conn.get_property(
            false,
            window.id,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            0,
            1024,
        )?.reply() {
            if let Ok(title) = String::from_utf8(reply.value) {
                window.wm.title = title;
            }
        }
        
        // Create window frame with decorations
        // Account for panel height (40px) - frames should start below the panel
        let frame_y = if window.geometry.y < PANEL_HEIGHT {
            PANEL_HEIGHT as i16
        } else {
            window.geometry.y as i16
        };
        
        let screen = &conn.setup().roots[self.screen_num];
        let dec_frame = decorations::WindowFrame::new(
            conn,
            screen,
            window.id,
            window.geometry.x as i16,
            frame_y,
            window.geometry.width as u16,
            window.geometry.height as u16,
        )?;
        
        // Convert to simple WindowFrame for storage
        window.wm.frame = Some(crate::shared::window_state::WindowFrame {
            frame: dec_frame.frame,
            titlebar: dec_frame.titlebar,
            close_button: dec_frame.close_button,
            maximize_button: dec_frame.maximize_button,
            minimize_button: dec_frame.minimize_button,
        });
        window.mapped = true;
        
        conn.flush()?;
        
        info!("WM: Managed window {} ({})", window.id, window.wm.title);
        Ok(())
    }
    
    /// Unmanage a window (called when window is destroyed)
    pub fn unmanage_window(
        &mut self,
        conn: &x11rb::rust_connection::RustConnection,
        window: &mut SharedWindow,
    ) -> Result<()> {
        debug!("WM: Unmanaging window {}", window.id);
        
        // Clear drag/resize state if this window was being dragged/resized
        if let Some(ref drag) = self.drag_state {
            if drag.window_id == window.id {
                self.drag_state = None;
            }
        }
        
        // Destroy window frame if it exists
        if let Some(frame_state) = &window.wm.frame {
            let frame = decorations::WindowFrame::from_state(window.id, frame_state);
            let screen = &conn.setup().roots[self.screen_num];
            if let Err(err) = frame.destroy(conn, screen.root) {
                warn!("Failed to destroy frame for window {}: {}", window.id, err);
            }
            window.wm.frame = None;
        }
        
        info!("WM: Unmanaged window {}", window.id);
        Ok(())
    }
    
    /// Close a window gracefully
    pub fn close_window(
        &mut self,
        conn: &RustConnection,
        window_id: u32,
    ) -> Result<()> {
        info!("Closing window {}", window_id);
        
        // Send WM_DELETE_WINDOW message
        self.atoms.send_delete_window(conn, window_id)?;
        conn.flush()?;
        
        Ok(())
    }
    
    /// Toggle maximize/restore
    pub fn toggle_maximize(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, SharedWindow>,
        window_id: u32,
    ) -> Result<()> {
        let window = windows.get_mut(&window_id)
            .context("Window not found")?;
        
        if window.wm.flags.maximized {
            self.restore_window(conn, window)?;
        } else {
            self.maximize_window(conn, window)?;
        }
        
        Ok(())
    }
    
    /// Maximize window
    pub fn maximize_window(
        &mut self,
        conn: &RustConnection,
        window: &mut SharedWindow,
    ) -> Result<()> {
        info!("Maximizing window {}", window.id);
        
        // Save restore geometry
        window.wm.restore_geometry = Some(window.geometry);
        
        // Get screen size
        let screen = &conn.setup().roots[self.screen_num];
        let max_width = screen.width_in_pixels as u32;
        let max_height = screen.height_in_pixels as u32;
        
        // Account for decorations (titlebar height + borders)
        const TITLEBAR_HEIGHT: u32 = 32;
        const BORDER_WIDTH: u32 = 2;
        let client_width = max_width - (BORDER_WIDTH * 2);
        let client_height = max_height - TITLEBAR_HEIGHT - (BORDER_WIDTH * 2);
        
        // Update window geometry
        window.geometry.x = BORDER_WIDTH as i32;
        window.geometry.y = TITLEBAR_HEIGHT as i32;
        window.geometry.width = client_width;
        window.geometry.height = client_height;
        
        // Resize frame and client window
        if let Some(frame) = &window.wm.frame {
            // Resize frame
            conn.configure_window(
                frame.frame,
                &ConfigureWindowAux::new()
                    .x(BORDER_WIDTH as i32)
                    .y(0)
                    .width(max_width)
                    .height(max_height),
            )?;
            
            // Resize titlebar
            conn.configure_window(
                frame.titlebar,
                &ConfigureWindowAux::new().width(max_width),
            )?;
            
            // Resize client window
            conn.configure_window(
                window.id,
                &ConfigureWindowAux::new()
                    .width(client_width)
                    .height(client_height),
            )?;
            
            // Reposition buttons
            let close_x = max_width - 16 - 8; // BUTTON_SIZE - BUTTON_PADDING
            let max_x = close_x - 16 - 8;
            let min_x = max_x - 16 - 8;
            let _btn_y = (TITLEBAR_HEIGHT - 16) / 2;
            
            conn.configure_window(
                frame.close_button,
                &ConfigureWindowAux::new().x(close_x as i32),
            )?;
            conn.configure_window(
                frame.maximize_button,
                &ConfigureWindowAux::new().x(max_x as i32),
            )?;
            conn.configure_window(
                frame.minimize_button,
                &ConfigureWindowAux::new().x(min_x as i32),
            )?;
        } else {
            // No frame, resize client directly
            conn.configure_window(
                window.id,
                &ConfigureWindowAux::new()
                    .x(0)
                    .y(0)
                    .width(max_width)
                    .height(max_height),
            )?;
            window.geometry.x = 0;
            window.geometry.y = 0;
            window.geometry.width = max_width;
            window.geometry.height = max_height;
        }
        
        // Update flags
        window.wm.flags.maximized = true;
        
        // Update EWMH state
        self.atoms.set_window_state(
            conn,
            window.id,
            &[self.atoms._net_wm_state_maximized_vert, 
              self.atoms._net_wm_state_maximized_horz],
            &[],
        )?;
        
        conn.flush()?;
        Ok(())
    }
    
    /// Restore window from maximized
    pub fn restore_window(
        &mut self,
        conn: &RustConnection,
        window: &mut SharedWindow,
    ) -> Result<()> {
        info!("Restoring window {}", window.id);
        
        // Restore from saved geometry
        if let Some(restore) = window.wm.restore_geometry {
            window.geometry = restore;
            
            // Restore frame and client window
            if let Some(frame_state) = &window.wm.frame {
                let frame = decorations::WindowFrame::from_state(window.id, frame_state);
                frame.move_to(conn, window.geometry.x as i16, window.geometry.y as i16)?;
                frame.resize(conn, window.geometry.width as u16, window.geometry.height as u16)?;
            } else {

                // No frame, restore client directly
                conn.configure_window(
                    window.id,
                    &ConfigureWindowAux::new()
                        .x(window.geometry.x)
                        .y(window.geometry.y)
                        .width(window.geometry.width)
                        .height(window.geometry.height),
                )?;
            }
        }
        
        window.wm.flags.maximized = false;
        window.wm.restore_geometry = None;
        
        // Remove EWMH maximize state
        self.atoms.set_window_state(
            conn,
            window.id,
            &[],
            &[self.atoms._net_wm_state_maximized_vert, 
              self.atoms._net_wm_state_maximized_horz],
        )?;
        
        conn.flush()?;
        Ok(())
    }
    
    /// Minimize window
    pub fn minimize_window(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, SharedWindow>,
        window_id: u32,
    ) -> Result<()> {
        let window = windows.get_mut(&window_id)
            .context("Window not found")?;
        
        info!("Minimizing window {}", window_id);
        
        // Unmap window (hide it)
        if let Some(frame) = &window.wm.frame {
            conn.unmap_window(frame.frame)?;
        } else {
            conn.unmap_window(window_id)?;
        }
        
        window.mapped = false;
        window.wm.flags.minimized = true;
        
        conn.flush()?;
        Ok(())
    }
    
    /// Set focus to a window
    pub fn set_focus(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, SharedWindow>,
        window_id: u32,
    ) -> Result<()> {
        // Unfocus previous window
        for window in windows.values_mut() {
            if window.focused && window.id != window_id {
                window.focused = false;
            }
        }
        
        // Focus new window
        if let Some(window) = windows.get_mut(&window_id) {
            window.focused = true;
            
            // Set X11 input focus
            conn.set_input_focus(
                InputFocus::POINTER_ROOT,
                window_id,
                x11rb::CURRENT_TIME,
            )?;
            
            // Raise window to top
            if let Some(frame) = &window.wm.frame {
                conn.configure_window(
                    frame.frame,
                    &ConfigureWindowAux::new()
                        .stack_mode(StackMode::ABOVE),
                )?;
            } else {
                conn.configure_window(
                    window_id,
                    &ConfigureWindowAux::new()
                        .stack_mode(StackMode::ABOVE),
                )?;
            }
            
            // Update EWMH active window
            self.atoms.update_active_window(conn, self.root, Some(window_id))?;
            
            conn.flush()?;
        }
        
        Ok(())
    }
    
    /// Start dragging a window
    pub fn start_drag(
        &mut self,
        conn: &RustConnection,
        windows: &HashMap<u32, SharedWindow>,
        window_id: u32,
        start_x: i16,
        start_y: i16,
    ) -> Result<()> {
        let window = windows.get(&window_id)
            .context("Window not found")?;
        
        info!("Starting drag for window {}", window_id);
        
        // Grab pointer for smooth dragging
        // Note: grab_pointer may fail if pointer is already grabbed, but we continue anyway
        // Cursor parameter: 0 means use current cursor, but we need to pass a Window
        // Using root window as cursor window (will use current cursor)
        let _ = conn.grab_pointer(
            false, // owner_events
            self.root,
            EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            self.root, // confine_to window
            self.root, // cursor (use root's cursor)
            x11rb::CURRENT_TIME,
        );
        
        // Store drag state
        self.drag_state = Some(DragState {
            window_id,
            start_x,
            start_y,
            window_start_x: window.geometry.x,
            window_start_y: window.geometry.y,
        });
        
        conn.flush()?;
        Ok(())
    }
    
    /// Update drag position
    pub fn update_drag(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, SharedWindow>,
        current_x: i16,
        current_y: i16,
    ) -> Result<()> {
        if let Some(ref drag) = self.drag_state {
            let window = windows.get_mut(&drag.window_id)
                .context("Window not found")?;
            
            // Calculate new position
            let delta_x = current_x - drag.start_x;
            let delta_y = current_y - drag.start_y;
            
            let new_x = drag.window_start_x + delta_x as i32;
            let new_y = drag.window_start_y + delta_y as i32;
            
            // Update window geometry
            window.geometry.x = new_x;
            window.geometry.y = new_y;
            
            // Move frame (if exists)
            if let Some(frame) = &window.wm.frame {
                const TITLEBAR_HEIGHT: u32 = 32;
                // Move frame window
                conn.configure_window(
                    frame.frame,
                    &ConfigureWindowAux::new()
                        .x(new_x)
                        .y(new_y - TITLEBAR_HEIGHT as i32),
                )?;
            } else {
                // No frame, move client window directly
                conn.configure_window(
                    window.id,
                    &ConfigureWindowAux::new()
                        .x(new_x)
                        .y(new_y),
                )?;
            }
            
            conn.flush()?;
        }
        
        Ok(())
    }
    
    /// End drag
    pub fn end_drag(&mut self, conn: &RustConnection) -> Result<()> {
        if self.drag_state.is_some() {
            conn.ungrab_pointer(x11rb::CURRENT_TIME)?;
            conn.flush()?;
            self.drag_state = None;
        }
        Ok(())
    }
    
    /// Check if a window is currently being dragged
    pub fn is_dragging(&self) -> bool {
        self.drag_state.is_some()
    }
    
    /// Check if a window ID belongs to a button
    pub fn find_window_from_button(
        &self,
        windows: &HashMap<u32, SharedWindow>,
        button_window: u32,
    ) -> Option<(u32, Option<ButtonType>)> {
        for (window_id, window) in windows {
            if let Some(frame_state) = &window.wm.frame {
                let frame = decorations::WindowFrame::from_state(*window_id, frame_state);
                if frame.contains(button_window) {
                    return Some((*window_id, frame.get_button_type(button_window)));
                }
            }
        }
        None
    }
}
