//! Window Manager Module
//!
//! Handles X11 window management, decorations, and user interactions.

pub mod decorations;
pub mod ewmh;
pub mod client;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::shared::Geometry;
use crate::wm::client::Client;
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
    /// WM owner window (for ICCCM selection)
    /// 
    /// This window owns the WM_S{screen} selection atom and must remain alive
    /// for the lifetime of the window manager. It's used by clients to detect
    /// the active WM and is referenced by _NET_SUPPORTING_WM_CHECK on the root.
    /// We don't actively read it, but keeping it in the struct ensures it
    /// stays alive (window is destroyed when struct is dropped).
    #[allow(dead_code)]
    wm_owner_window: u32,
}

impl WindowManager {
    /// Create a new window manager
    /// 
    /// # Arguments
    /// * `conn` - X11 connection
    /// * `screen_num` - Screen number
    /// * `root` - Root window ID
    /// * `replace` - If true, attempt to replace existing WM (wait for it to exit)
    pub fn new(
        conn: &x11rb::rust_connection::RustConnection,
        screen_num: usize,
        root: u32,
        replace: bool,
    ) -> Result<Self> {
        info!("Initializing window manager (replace={})", replace);
        
        let screen = &conn.setup().roots[screen_num];
        
        // Step 1: Intern WM selection atom (ICCCM: WM_S{screen_num})
        let wm_selection_name = format!("WM_S{}", screen_num);
        debug!("WM: Interning selection atom '{}'", wm_selection_name);
        let wm_selection_atom = conn.intern_atom(false, wm_selection_name.as_bytes())?
            .reply()
            .context("Failed to intern WM selection atom")?
            .atom;
        debug!("WM: Selection atom interned: {}", wm_selection_atom);
        
        // Step 2: Check for existing WM
        debug!("WM: Checking for existing window manager...");
        let current_wm_owner = conn.get_selection_owner(wm_selection_atom)?
            .reply()
            .context("Failed to get current WM selection owner")?
            .owner;
        debug!("WM: Current selection owner: 0x{:x}", current_wm_owner);
        
        if current_wm_owner != 0 {
            if !replace {
                anyhow::bail!(
                    "Another window manager is already running (window 0x{:x}). \
                    Use --replace to attempt to replace it.",
                    current_wm_owner
                );
            }
            
            info!("Existing WM detected (window 0x{:x}), attempting replace...", current_wm_owner);
            
            // Try to select StructureNotifyMask on the previous WM window
            // This allows us to detect when it exits (DestroyNotify)
            let _ = conn.change_window_attributes(
                current_wm_owner,
                &ChangeWindowAttributesAux::new()
                    .event_mask(EventMask::STRUCTURE_NOTIFY),
            );
            conn.flush()?;
        }
        
        // Step 3: Create WM owner window (like xfwm4's xfwm4_win)
        // This window owns the WM selection atom
        debug!("WM: Creating WM owner window...");
        let wm_owner_window = conn.generate_id()?;
        conn.create_window(
            screen.root_depth,
            wm_owner_window,
            root,
            -1000, // Off-screen
            -1000,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .event_mask(EventMask::STRUCTURE_NOTIFY),
        )?;
        conn.map_window(wm_owner_window)?;
        conn.flush()?;
        debug!("WM: Created owner window: 0x{:x}", wm_owner_window);
        
        // Step 4: Acquire WM selection ownership
        debug!("WM: Acquiring WM selection ownership...");
        conn.set_selection_owner(
            wm_owner_window,
            wm_selection_atom,
            x11rb::CURRENT_TIME,
        )?
        .check()
        .context("Failed to set WM selection owner")?;
        conn.flush()?;
        
        // Verify we own the selection
        debug!("WM: Verifying selection ownership...");
        let owner_after = conn.get_selection_owner(wm_selection_atom)?
            .reply()
            .context("Failed to verify WM selection ownership")?
            .owner;
        debug!("WM: Selection owner after acquisition: 0x{:x}", owner_after);
        
        if owner_after != wm_owner_window {
            anyhow::bail!("Failed to acquire WM selection ownership (expected 0x{:x}, got 0x{:x})", wm_owner_window, owner_after);
        }
        debug!("WM: Successfully acquired WM selection ownership");
        
        // Step 5: If replacing, wait for previous WM to exit
        if current_wm_owner != 0 {
            info!("Waiting for previous WM to exit...");
            let timeout = Duration::from_secs(15);
            let start = Instant::now();
            
            while start.elapsed() < timeout {
                // Check if previous WM window still exists
                let current_owner = conn.get_selection_owner(wm_selection_atom)?
                    .reply()
                    .context("Failed to check WM selection owner")?
                    .owner;
                
                // If owner changed or window was destroyed, we're good
                if current_owner == wm_owner_window {
                    // Try to get window attributes - if it fails, window is gone
                    if conn.get_window_attributes(current_wm_owner)?.reply().is_err() {
                        info!("Previous WM window destroyed");
                        break;
                    }
                }
                
                // Process any pending events (including DestroyNotify)
                conn.flush()?;
                std::thread::sleep(Duration::from_millis(100));
            }
            
            if start.elapsed() >= timeout {
                warn!("Timeout waiting for previous WM to exit, proceeding anyway");
            } else {
                info!("Previous WM exited successfully");
            }
        }
        
        // Step 6: Get current root window event mask and preserve it
        debug!("WM: Getting root window attributes to preserve event mask...");
        let root_attrs = conn.get_window_attributes(root)?
            .reply()
            .context("Failed to get root window attributes")?;
        debug!("WM: Current root event mask: {:?}", root_attrs.your_event_mask);
        
        // Required event mask for WM
        let required_mask = EventMask::SUBSTRUCTURE_REDIRECT
            | EventMask::SUBSTRUCTURE_NOTIFY
            | EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::POINTER_MOTION
            | EventMask::ENTER_WINDOW
            | EventMask::LEAVE_WINDOW
            | EventMask::PROPERTY_CHANGE
            | EventMask::FOCUS_CHANGE
            | EventMask::KEY_PRESS;
        
        // Preserve existing mask and add our required mask
        let combined_mask = root_attrs.your_event_mask | required_mask;
        debug!("WM: Combined event mask: {:?}", combined_mask);
        
        // Step 7: Select events on root window
        debug!("WM: Selecting events on root window (SubstructureRedirect)...");
        conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(combined_mask),
        )?
        .check()
        .context("Failed to select events on root window - is another WM running?")?;
        conn.flush()?;
        debug!("WM: Successfully selected events on root window");
        
        // Step 8: Initialize EWMH atoms
        debug!("WM: Initializing EWMH atoms...");
        let atoms = Atoms::new(conn)?;
        atoms.setup_supported(conn, root)?;
        debug!("WM: EWMH atoms initialized");
        
        // Step 9: Set up _NET_SUPPORTING_WM_CHECK (for better interoperability)
        debug!("WM: Setting up _NET_SUPPORTING_WM_CHECK...");
        let net_supporting_wm_check = conn.intern_atom(false, b"_NET_SUPPORTING_WM_CHECK")?
            .reply()
            .context("Failed to intern _NET_SUPPORTING_WM_CHECK")?
            .atom;
        
        // Set _NET_SUPPORTING_WM_CHECK on root to point to our owner window
        conn.change_property32(
            PropMode::REPLACE,
            root,
            net_supporting_wm_check,
            AtomEnum::WINDOW,
            &[wm_owner_window],
        )?;
        
        // Set _NET_WM_NAME on owner window
        let net_wm_name = atoms.net_wm_name;
        let wm_name = b"area\0";
        conn.change_property(
            PropMode::REPLACE,
            wm_owner_window,
            net_wm_name,
            AtomEnum::STRING,
            8,
            wm_name.len() as u32,
            wm_name,
        )?;
        
        conn.flush()?;
        debug!("WM: _NET_SUPPORTING_WM_CHECK set to window 0x{:x}", wm_owner_window);
        
        // Step 10: Grab SUPER key (Mod4) for launcher
        // Note: Key grabbing may fail if keycodes don't match - we'll handle gracefully
        use x11rb::protocol::xproto::ModMask;
        let no_modifier = ModMask::from(0u16);
        
        // Try to grab SUPER key (keycode 133 = left SUPER, 134 = right SUPER)
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
        
        info!("Successfully became window manager (keyboard shortcuts enabled)");
        
        Ok(Self {
            screen_num,
            root,
            atoms,
            drag_state: None,
            wm_owner_window,
        })
    }
    
    /// Manage a new window (called when MapRequest is received)
    pub fn manage_window(
        &mut self,
        conn: &x11rb::rust_connection::RustConnection,
        client: &mut Client,
    ) -> Result<()> {
        debug!("WM: Managing window {}", client.id);
        
        // Get window attributes
        let attrs = match conn.get_window_attributes(client.id)?.reply() {
            Ok(attrs) => attrs,
            Err(e) => {
                debug!("WM: Failed to get attributes for window {}, it probably disappeared: {}", client.id, e);
                return Ok(());
            }
        };
        
        if attrs.override_redirect {
            debug!("Window {} is override-redirect, skipping", client.id);
            return Ok(());
        }
        
        // Get window geometry
        let geom = match conn.get_geometry(client.id)?.reply() {
            Ok(geom) => geom,
            Err(e) => {
                debug!("WM: Failed to get geometry for window {}, it probably disappeared: {}", client.id, e);
                return Ok(());
            }
        };
        
        // Get window's preferred size from WM_NORMAL_HINTS if available
        let mut width = geom.width as u32;
        let mut height = geom.height as u32;
        
        // If window is 1x1 (uninitialized), try to get size from WM_NORMAL_HINTS
        if width == 1 && height == 1 {
            if let Ok(reply) = conn.get_property(
                false,
                client.id,
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
                client.id,
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
        
        client.geometry = Geometry {
            x: geom.x as i32,
            y: adjusted_y,
            width,
            height,
        };
        
        // Get window title
        if let Ok(reply) = conn.get_property(
            false,
            client.id,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            0,
            1024,
        )?.reply() {
            if let Ok(title) = String::from_utf8(reply.value) {
                client.title = title;
            }
        }
        
        // Create window frame with decorations
        // Account for panel height (40px) - frames should start below the panel
        let frame_y = if client.geometry.y < PANEL_HEIGHT {
            PANEL_HEIGHT as i16
        } else {
            client.geometry.y as i16
        };
        
        let screen = &conn.setup().roots[self.screen_num];
        let dec_frame = decorations::WindowFrame::new(
            conn,
            screen,
            client.id,
            client.geometry.x as i16,
            frame_y,
            client.geometry.width as u16,
            client.geometry.height as u16,
        )?;
        
        // Convert to simple WindowFrame for storage
        client.frame = Some(crate::shared::window_state::WindowFrame {
            frame: dec_frame.frame,
            titlebar: dec_frame.titlebar,
            close_button: dec_frame.close_button,
            maximize_button: dec_frame.maximize_button,
            minimize_button: dec_frame.minimize_button,
        });
        client.mapped = true;
        
        conn.flush()?;
        
        debug!("WM: Managed window {} ({})", client.id, client.title);
        
        // Update _NET_FRAME_EXTENTS so client knows about our decorations
        // Currently hardcoded based on our hardcoded decoration sizes
        // Top: 32 (Titlebar), Left/Right/Bottom: 2 (Border)
        let _ = self.atoms.update_frame_extents(conn, client.id, 2, 2, 32, 2);
        
        Ok(())
    }
    
    /// Unmanage a window (called when window is destroyed)
    pub fn unmanage_window(
        &mut self,
        conn: &x11rb::rust_connection::RustConnection,
        client: &mut Client,
    ) -> Result<()> {
        debug!("WM: Unmanaging window {}", client.id);
        
        // Clear drag/resize state if this window was being dragged/resized
        if let Some(ref drag) = self.drag_state {
            if drag.window_id == client.id {
                self.drag_state = None;
            }
        }
        
        // Destroy window frame if it exists
        if let Some(frame_state) = &client.frame {
            let frame = decorations::WindowFrame::from_state(client.id, frame_state);
            let screen = &conn.setup().roots[self.screen_num];
            if let Err(err) = frame.destroy(conn, screen.root) {
                warn!("Failed to destroy frame for window {}: {}", client.id, err);
            }
            client.frame = None;
        }
        
        debug!("WM: Unmanaged window {}", client.id);
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
        windows: &mut HashMap<u32, Client>,
        window_id: u32,
    ) -> Result<()> {
        let client = windows.get_mut(&window_id)
            .context("Window not found")?;
        
        if client.state.maximized {
            self.restore_window(conn, client)?;
        } else {
            self.maximize_window(conn, client)?;
        }
        
        Ok(())
    }
    
    /// Maximize window
    pub fn maximize_window(
        &mut self,
        conn: &RustConnection,
        client: &mut Client,
    ) -> Result<()> {
        info!("Maximizing window {}", client.id);
        
        // Save restore geometry
        client.restore_geometry = Some(client.geometry);
        
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
        client.geometry.x = BORDER_WIDTH as i32;
        client.geometry.y = TITLEBAR_HEIGHT as i32;
        client.geometry.width = client_width;
        client.geometry.height = client_height;
        
        // Resize frame and client window
        if let Some(frame) = &client.frame {
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
                client.id,
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
                client.id,
                &ConfigureWindowAux::new()
                    .x(0)
                    .y(0)
                    .width(max_width)
                    .height(max_height),
            )?;
            client.geometry.x = 0;
            client.geometry.y = 0;
            client.geometry.width = max_width;
            client.geometry.height = max_height;
        }
        
        // Update flags
        client.state.maximized = true;
        
        // Update EWMH state
        self.atoms.set_window_state(
            conn,
            client.id,
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
        client: &mut Client,
    ) -> Result<()> {
        info!("Restoring window {}", client.id);
        
        // Restore from saved geometry
        if let Some(restore) = client.restore_geometry {
            client.geometry = restore;
            
            // Restore frame and client window
            if let Some(frame_state) = &client.frame {
                let frame = decorations::WindowFrame::from_state(client.id, frame_state);
                frame.move_to(conn, client.geometry.x as i16, client.geometry.y as i16)?;
                frame.resize(conn, client.geometry.width as u16, client.geometry.height as u16)?;
            } else {

                // No frame, restore client directly
                conn.configure_window(
                    client.id,
                    &ConfigureWindowAux::new()
                        .x(client.geometry.x)
                        .y(client.geometry.y)
                        .width(client.geometry.width)
                        .height(client.geometry.height),
                )?;
            }
        }
        
        client.state.maximized = false;
        client.restore_geometry = None;
        
        // Remove EWMH maximize state
        self.atoms.set_window_state(
            conn,
            client.id,
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
        windows: &mut HashMap<u32, Client>,
        window_id: u32,
    ) -> Result<()> {
        let client = windows.get_mut(&window_id)
            .context("Window not found")?;
        
        info!("Minimizing window {}", window_id);
        
        // Unmap window (hide it)
        if let Some(frame) = &client.frame {
            conn.unmap_window(frame.frame)?;
        } else {
            conn.unmap_window(window_id)?;
        }
        
        client.mapped = false;
        client.state.minimized = true;
        
        conn.flush()?;
        Ok(())
    }
    
    /// Set focus to a window
    pub fn set_focus(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, Client>,
        window_id: u32,
    ) -> Result<()> {
        // Unfocus previous window
        for client in windows.values_mut() {
            if client.focused && client.id != window_id {
                client.focused = false;
            }
        }
        
        // Focus new window
        if let Some(client) = windows.get_mut(&window_id) {
            client.focused = true;
            
            // Set X11 input focus
            conn.set_input_focus(
                InputFocus::POINTER_ROOT,
                window_id,
                x11rb::CURRENT_TIME,
            )?;
            
            // Raise window to top
            if let Some(frame) = &client.frame {
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
        windows: &HashMap<u32, Client>,
        window_id: u32,
        start_x: i16,
        start_y: i16,
    ) -> Result<()> {
        let client = windows.get(&window_id)
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
            window_start_x: client.geometry.x,
            window_start_y: client.geometry.y,
        });
        
        conn.flush()?;
        Ok(())
    }
    
    /// Update drag position
    pub fn update_drag(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, Client>,
        current_x: i16,
        current_y: i16,
    ) -> Result<()> {
        if let Some(ref drag) = self.drag_state {
            let client = windows.get_mut(&drag.window_id)
                .context("Window not found")?;
            
            // Calculate new position
            let delta_x = current_x - drag.start_x;
            let delta_y = current_y - drag.start_y;
            
            let new_x = drag.window_start_x + delta_x as i32;
            let new_y = drag.window_start_y + delta_y as i32;
            
            // Update window geometry
            client.geometry.x = new_x;
            client.geometry.y = new_y;
            
            // Move frame (if exists)
            if let Some(frame) = &client.frame {
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
                    client.id,
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
        windows: &HashMap<u32, Client>,
        button_window: u32,
    ) -> Option<(u32, Option<ButtonType>)> {
        for (window_id, client) in windows {
            if let Some(frame_state) = &client.frame {
                let frame = decorations::WindowFrame::from_state(*window_id, frame_state);
                if frame.contains(button_window) {
                    return Some((*window_id, frame.get_button_type(button_window)));
                }
            }
        }
        None
    }
}
