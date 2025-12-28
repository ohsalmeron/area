//! Window Manager Module
//!
//! Handles X11 window management, decorations, and user interactions.

pub mod decorations;
pub mod ewmh;
pub mod client;
pub mod client_flags;
pub mod display;
pub mod screen;
pub mod events;
pub mod focus;
pub mod stacking;
pub mod workspace;
pub mod netwm;
pub mod moveresize;
pub mod placement;
pub mod keyboard;
pub mod settings;
pub mod transients;
pub mod hints;
pub mod menu;
pub mod icons;
pub mod cycle;
pub mod session;
pub mod startup;
pub mod terminate;
pub mod device;
pub mod event_filter;

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
    pub atoms: Atoms,
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
        // Use the atom from Atoms struct (now available there)
        let net_supporting_wm_check = atoms._net_supporting_wm_check;
        
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
    
    /// Check if window should be decorated based on class/name patterns
    /// Returns false if window matches a pattern that indicates no decorations
    fn should_decorate_from_patterns<C: Connection>(
        conn: &C,
        window: Window,
        title: &str,
    ) -> Result<bool> {
        // Get WM_CLASS property (contains both res_class and res_name)
        let mut res_class = String::new();
        let mut res_name = String::new();
        
        if let Ok(reply) = conn.get_property(
            false,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            0,
            1024,
        )?.reply() {
            if let Ok(class_string) = String::from_utf8(reply.value) {
                // WM_CLASS format: "res_name\0res_class\0"
                let parts: Vec<&str> = class_string.split('\0').collect();
                if parts.len() >= 2 {
                    res_name = parts[0].to_lowercase();
                    res_class = parts[1].to_lowercase();
                } else if parts.len() == 1 {
                    res_class = parts[0].to_lowercase();
                }
            }
        }
        
        let title_lower = title.to_lowercase();
        
        // Check for Chrome/Chromium
        if res_class.contains("google-chrome") || res_class.contains("chromium") ||
           res_name.contains("chrome") || res_name.contains("chromium") {
            return Ok(false);
        }
        
        // Check for Firefox
        if res_class.contains("firefox") || res_class.contains("navigator") ||
           res_name.contains("firefox") || res_name.contains("navigator") {
            return Ok(false);
        }
        
        // Check for Electron apps
        if res_class.contains("electron") || res_name.contains("electron") ||
           title_lower.contains("electron") {
            return Ok(false);
        }
        
        // Check for Wine apps
        if res_class.contains("wine") || res_name.contains("wine") ||
           title_lower.contains(".exe") {
            return Ok(false);
        }
        
        // Check for common game patterns (Steam games often have specific patterns)
        // Many games set their own decorations or use fullscreen exclusively
        if title_lower.contains("steam") && (title_lower.contains("game") || title_lower.contains("launch")) {
            return Ok(false);
        }
        
        // Default: allow decorations
        Ok(true)
    }
    
    /// Manage a new window (called when MapRequest is received)
    pub fn manage_window(
        &mut self,
        conn: &x11rb::rust_connection::RustConnection,
        client: &mut Client,
    ) -> Result<()> {
        debug!("WM: Managing window {}", client.window);
        
        // Get window attributes
        let attrs = match conn.get_window_attributes(client.window)?.reply() {
            Ok(attrs) => attrs,
            Err(e) => {
                debug!("WM: Failed to get attributes for window {}, it probably disappeared: {}", client.window, e);
                return Ok(());
            }
        };
        
        if attrs.override_redirect {
            debug!("Window {} is override-redirect, skipping", client.window);
            return Ok(());
        }
        
        // Get window geometry
        let geom = match conn.get_geometry(client.window)?.reply() {
            Ok(geom) => geom,
            Err(e) => {
                debug!("WM: Failed to get geometry for window {}, it probably disappeared: {}", client.window, e);
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
                client.window,
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
                client.window,
                &ConfigureWindowAux::new()
                    .width(width)
                    .height(height),
            )?;
        }
        
        // Center window on screen by default (unless window has a specific position hint)
        let screen = &conn.setup().roots[self.screen_num];
        let screen_width = screen.width_in_pixels as i32;
        let screen_height = screen.height_in_pixels as i32;
        
        // Check if window has a position hint (USPosition flag in WM_NORMAL_HINTS)
        let has_position_hint = if let Ok(reply) = conn.get_property(
            false,
            client.window,
            AtomEnum::WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            0,
            18,
        )?.reply() {
            if let Some(value32) = reply.value32() {
                let hints: Vec<u32> = value32.take(18).collect();
                if hints.len() >= 1 {
                    // Check USPosition flag (bit 0)
                    (hints[0] & 0x00000001) != 0
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        
        // Center window if it doesn't have a position hint or is at (0,0) or invalid position
        let (x, y) = if has_position_hint && geom.x != 0 && geom.y != 0 {
            // Window has explicit position hint, use it
            (geom.x as i32, geom.y as i32)
        } else {
            // Center window on screen
            let center_x = (screen_width - width as i32) / 2;
            let center_y = (screen_height - height as i32) / 2;
            (center_x, center_y)
        };
        
        client.geometry = Geometry {
            x,
            y,
            width,
            height,
        };
        
        // Get window title
        if let Ok(reply) = conn.get_property(
            false,
            client.window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            0,
            1024,
        )?.reply() {
            if let Ok(title) = String::from_utf8(reply.value) {
                client.name = title;
            }
        }
        
        // Create window frame with decorations
        // Use window's centered position
        let frame_y = client.geometry.y as i16;
        
        let screen = &conn.setup().roots[self.screen_num];
        
        // Check if window should be decorated
        // Priority: 1. MOTIF_WM_HINTS, 2. _NET_WM_WINDOW_TYPE, 3. Window class/name patterns
        let mut should_decorate = true;
        
        // First, check MOTIF_WM_HINTS (most authoritative for decoration requests)
        if let Ok(Some(motif_should_decorate)) = self.atoms.should_decorate_from_motif_hints(conn, client.window) {
            should_decorate = motif_should_decorate;
            debug!("MOTIF hints for window {}: should_decorate={}", client.window, should_decorate);
        } else {
            // MOTIF hints not present or don't specify - check _NET_WM_WINDOW_TYPE
            let window_types = self.atoms.get_window_type(conn, client.window).unwrap_or_default();
            
            for &win_type in &window_types {
                if win_type == self.atoms._net_wm_window_type_dock ||
                   win_type == self.atoms._net_wm_window_type_tooltip ||
                   win_type == self.atoms._net_wm_window_type_notification ||
                   win_type == self.atoms._net_wm_window_type_splash ||
                   win_type == self.atoms._net_wm_window_type_menu ||
                   win_type == self.atoms._net_wm_window_type_dropdown_menu ||
                   win_type == self.atoms._net_wm_window_type_popup_menu {
                    should_decorate = false;
                    break;
                }
            }
            
            // If still should_decorate, check window class/name patterns
            if should_decorate {
                should_decorate = Self::should_decorate_from_patterns(conn, client.window, &client.name.as_str())?;
                if !should_decorate {
                    debug!("Window {} matched no-decoration pattern (class/name)", client.window);
                }
            }
        }
        
        if should_decorate {
            // Use default decoration config and colors for now
            // TODO: Store these in WindowManager or pass them in
            let dec_config = crate::config::WindowDecorationConfig::default();
            let dec_colors = crate::config::WindowColors::default();
            let dec_frame = decorations::WindowFrame::new(
                conn,
                screen,
                client.window,
                client.geometry.x as i16,
                frame_y,
                client.geometry.width as u16,
                client.geometry.height as u16,
                &dec_config,
                &dec_colors,
            )?;
            
            // Convert to simple WindowFrame for storage
            client.frame = Some(crate::shared::window_state::WindowFrame {
                frame: dec_frame.frame,
                titlebar: dec_frame.titlebar,
                close_button: dec_frame.close_button,
                maximize_button: dec_frame.maximize_button,
                minimize_button: dec_frame.minimize_button,
            });
            
            // Update _NET_FRAME_EXTENTS only if decorated
            // Top: 32 (Titlebar), Left/Right/Bottom: 2 (Border)
            let _ = self.atoms.update_frame_extents(conn, client.window, 2, 2, 32, 2);
        } else {
            // If not decorated, set _NET_FRAME_EXTENTS to 0
            let _ = self.atoms.update_frame_extents(conn, client.window, 0, 0, 0, 0);
            
            // No panel offset needed - use window's actual position
        }
        
        client.set_mapped(true);
        
        conn.flush()?;
        
        debug!("WM: Managed window {} ({})", client.window, client.name.as_str());
        
        // Update _NET_FRAME_EXTENTS so client knows about our decorations
        // Currently hardcoded based on our hardcoded decoration sizes
        // Top: 32 (Titlebar), Left/Right/Bottom: 2 (Border)
        let _ = self.atoms.update_frame_extents(conn, client.window, 2, 2, 32, 2);
        
        Ok(())
    }
    
    /// Unmanage a window (called when window is destroyed)
    pub fn unmanage_window(
        &mut self,
        conn: &x11rb::rust_connection::RustConnection,
        client: &mut Client,
    ) -> Result<()> {
        debug!("WM: Unmanaging window {}", client.window);
        
        // Clear drag/resize state if this window was being dragged/resized
        if let Some(ref drag) = self.drag_state {
            if drag.window_id == client.window {
                self.drag_state = None;
            }
        }
        
        // Destroy window frame if it exists
        if let Some(frame_state) = &client.frame {
            let frame = decorations::WindowFrame::from_state(client.window, frame_state);
            let screen = &conn.setup().roots[self.screen_num];
            if let Err(err) = frame.destroy(conn, screen.root) {
                warn!("Failed to destroy frame for window {}: {}", client.window, err);
            }
            client.frame = None;
        }
        
        debug!("WM: Unmanaged window {}", client.window);
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
        
        if client.is_maximized() {
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
        info!("Maximizing window {}", client.window);
        
        // Save restore geometry
        if !client.is_maximized() {
            client.set_restore_geometry(Some(client.geometry));
        }
        
        // Get screen size
        let screen = &conn.setup().roots[self.screen_num];
        let max_width = screen.width_in_pixels as u32;
        let max_height = screen.height_in_pixels as u32;
        
        // Account for decorations (titlebar height + borders)
        const TITLEBAR_HEIGHT: u32 = 32;
        const BORDER_WIDTH: u32 = 2;
        
        // Final frame outer geometry: (0, 0, max_width, max_height)
        // Internal size of the frame:
        let frame_width = max_width - (BORDER_WIDTH * 2);
        let frame_height = max_height - (BORDER_WIDTH * 2);
        
        // Update window geometry (client relative to root)
        client.geometry.x = BORDER_WIDTH as i32;
        client.geometry.y = (BORDER_WIDTH + TITLEBAR_HEIGHT) as i32;
        client.geometry.width = frame_width;
        client.geometry.height = frame_height - TITLEBAR_HEIGHT;
        client.flags.insert(crate::wm::client_flags::ClientFlags::MAXIMIZED_VERT);
        client.flags.insert(crate::wm::client_flags::ClientFlags::MAXIMIZED_HORIZ);
        
        // Resize frame and client window
        if let Some(frame_state) = &client.frame {
            let frame = decorations::WindowFrame::from_state(client.window, frame_state);
            
            // Move frame so its border is flush with screen edge
            // Frame position: (BORDER_WIDTH, BORDER_WIDTH) to account for borders
            frame.move_to(conn, BORDER_WIDTH as i16, BORDER_WIDTH as i16)?;
            // Get decorations config from default for now
            // TODO: Store decorations config in WindowManager
            frame.resize(conn, frame_width as u16, frame_height as u16, &crate::config::WindowDecorationConfig {
                titlebar_height: 32,
                border_width: 2,
                button_size: 20,
                button_padding: 5,
            })?;
        } else {
            // No frame, resize client directly
            conn.configure_window(
                client.window,
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
        
        // Update EWMH state
        if let Err(e) = self.atoms.set_window_state(conn, client.window, &[
            self.atoms._net_wm_state_maximized_vert,
            self.atoms._net_wm_state_maximized_horz,
        ], &[]) {
            warn!("Failed to update maximized state for window {}: {}", client.window, e);
        }
        
        conn.flush()?;
        Ok(())
    }
    
    /// Set fullscreen state for a window (xfwm4-style: keep frame, hide it)
    pub fn set_fullscreen(
        &mut self,
        conn: &RustConnection,
        client: &mut Client,
        fullscreen: bool,
    ) -> Result<()> {
        debug!("Setting fullscreen={} for window {}", fullscreen, client.window);
        
        if fullscreen {
            // Save geometry before entering fullscreen
            if !client.is_fullscreen() {
                client.set_restore_geometry(Some(client.geometry));
            }
            
            // Set fullscreen flag
            client.flags.insert(crate::wm::client_flags::ClientFlags::FULLSCREEN);
            
            // Get screen dimensions
            let screen = &conn.setup().roots[self.screen_num];
            let screen_width = screen.width_in_pixels as u32;
            let screen_height = screen.height_in_pixels as u32;
            
            // Update client geometry to screen size
            client.geometry.x = 0;
            client.geometry.y = 0;
            client.geometry.width = screen_width;
            client.geometry.height = screen_height;
            
            // Configure windows for fullscreen
            // If window has a frame, unmap it and configure client directly
            // This ensures decorations are not visible during fullscreen
            if let Some(frame_state) = &client.frame {
                let frame = decorations::WindowFrame::from_state(client.window, frame_state);
                // Unmap the frame window to hide decorations
                conn.unmap_window(frame.frame)?;
                // Configure client directly at screen 0,0 with screen dimensions
                conn.configure_window(
                    client.window,
                    &ConfigureWindowAux::new()
                        .x(0)
                        .y(0)
                        .width(screen_width)
                        .height(screen_height)
                        .border_width(0),
                )?;
            } else {
                // No frame - configure client directly
                conn.configure_window(
                    client.window,
                    &ConfigureWindowAux::new()
                        .x(0)
                        .y(0)
                        .width(screen_width)
                        .height(screen_height)
                        .border_width(0),
                )?;
            }
            
            // Set NET_FRAME_EXTENTS to 0,0,0,0 (no decorations visible)
            self.atoms.update_frame_extents(conn, client.window, 0, 0, 0, 0)?;
            
            // Update EWMH state - add FULLSCREEN and ABOVE (always on top)
            // FULLSCREEN windows should always be on top, so set ABOVE state
            self.atoms.set_window_state(
                conn,
                client.window,
                &[self.atoms._net_wm_state_fullscreen, self.atoms._net_wm_state_above],
                &[],
            )?;
        } else {
            // Exit fullscreen: restore geometry
            client.flags.remove(crate::wm::client_flags::ClientFlags::FULLSCREEN);
            
            if let Some(restore) = client.restore_geometry() {
                client.geometry = restore;
                
                // Restore client window geometry
                if let Some(frame_state) = &client.frame {
                    // Window has frame - map it back and restore frame position and client position relative to frame
                    let frame = decorations::WindowFrame::from_state(client.window, frame_state);
                    const TITLEBAR_HEIGHT: i32 = 32;
                    const BORDER_WIDTH: i32 = 2;
                    
            // Frame should be at (x - border, y - titlebar - border)
            // No panel offset - use actual restore position
            let frame_x = restore.x - BORDER_WIDTH;
            let frame_y = restore.y - TITLEBAR_HEIGHT - BORDER_WIDTH;
                    let frame_width = restore.width + (BORDER_WIDTH * 2) as u32;
                    let frame_height = restore.height + (TITLEBAR_HEIGHT + BORDER_WIDTH * 2) as u32;
                    
                    frame.move_to(conn, frame_x as i16, frame_y as i16)?;
                    // Get decorations config - for now use default values
                    // TODO: Store decorations config in WindowManager or pass it in
                    frame.resize(conn, frame_width as u16, frame_height as u16, &crate::config::WindowDecorationConfig {
                        titlebar_height: TITLEBAR_HEIGHT as u16,
                        border_width: BORDER_WIDTH as u16,
                        button_size: 20,
                        button_padding: 5,
                    })?;
                    
                    // Map the frame window back
                    conn.map_window(frame.frame)?;
                    
                    // Client is positioned relative to frame
                    conn.configure_window(
                        client.window,
                        &ConfigureWindowAux::new()
                            .x(BORDER_WIDTH)
                            .y((TITLEBAR_HEIGHT + BORDER_WIDTH) as i32)
                            .width(restore.width)
                            .height(restore.height),
                    )?;
                } else {
                    // No frame - restore client directly
                    conn.configure_window(
                        client.window,
                        &ConfigureWindowAux::new()
                            .x(restore.x)
                            .y(restore.y)
                            .width(restore.width)
                            .height(restore.height),
                    )?;
                }
                
                client.set_restore_geometry(None);
            } else {
                warn!("Cannot restore window {} - no saved geometry found", client.window);
            }
            
            // Restore NET_FRAME_EXTENTS (Top: 32 titlebar, Left/Right/Bottom: 2 border)
            if client.frame.is_some() {
                self.atoms.update_frame_extents(conn, client.window, 2, 2, 32, 2)?;
            } else {
                self.atoms.update_frame_extents(conn, client.window, 0, 0, 0, 0)?;
            }
            
            // Remove EWMH fullscreen and ABOVE state
            self.atoms.set_window_state(
                conn,
                client.window,
                &[],
                &[self.atoms._net_wm_state_fullscreen, self.atoms._net_wm_state_above],
            )?;
        }
        
        conn.flush()?;
        Ok(())
    }
    
    /// Restore window from maximized
    pub fn restore_window(
        &mut self,
        conn: &RustConnection,
        client: &mut Client,
    ) -> Result<()> {
        info!("Restoring window {}", client.window);
        
        // Restore from saved geometry
        if let Some(restore) = client.restore_geometry() {
            client.geometry = restore;
            
            // Restore frame and client window
            if let Some(frame_state) = &client.frame {
                let frame = decorations::WindowFrame::from_state(client.window, frame_state);
                // Frame position needs to account for titlebar - client is reparented at (0, TITLEBAR_HEIGHT)
                // So if client.geometry.y is the client content position, frame should be at y - TITLEBAR_HEIGHT
                const TITLEBAR_HEIGHT: i32 = 32;
                let frame_y = client.geometry.y - TITLEBAR_HEIGHT;
                frame.move_to(conn, client.geometry.x as i16, frame_y as i16)?;
                // Get decorations config from default for now
                // TODO: Store decorations config in WindowManager
                frame.resize(conn, client.geometry.width as u16, client.geometry.height as u16, &crate::config::WindowDecorationConfig {
                    titlebar_height: 32,
                    border_width: 2,
                    button_size: 20,
                    button_padding: 5,
                })?;
            } else {
                // No frame, restore client directly
                conn.configure_window(
                    client.window,
                    &ConfigureWindowAux::new()
                        .x(client.geometry.x)
                        .y(client.geometry.y)
                        .width(client.geometry.width)
                        .height(client.geometry.height),
                )?;
            }
        } else {
            warn!("Cannot restore window {} - no saved geometry found", client.window);
        }
        
        client.flags.remove(crate::wm::client_flags::ClientFlags::MAXIMIZED_VERT);
        client.flags.remove(crate::wm::client_flags::ClientFlags::MAXIMIZED_HORIZ);
        client.set_restore_geometry(None);
        
        // Remove EWMH maximize state
        self.atoms.set_window_state(
            conn,
            client.window,
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
        
        client.set_mapped(false);
        client.flags.insert(crate::wm::client_flags::ClientFlags::ICONIFIED);
        
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
        // #region agent log
        {
            use std::fs::OpenOptions;
            use std::io::Write;
            let log_entry = serde_json::json!({
                "sessionId": "debug-session",
                "runId": "run1",
                "hypothesisId": "B",
                "location": "wm/mod.rs:960",
                "message": "set_focus called",
                "data": {"window_id": window_id, "has_frame": windows.get(&window_id).and_then(|c| c.frame.as_ref().map(|_| true)).unwrap_or(false)},
                "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
            });
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("/home/bizkit/GitHub/area/.cursor/debug.log") {
                let _ = writeln!(file, "{}", log_entry);
            }
        }
        // #endregion
        
        // Unfocus previous window
        for client in windows.values_mut() {
            if client.focused() && client.window != window_id {
                client.set_focused(false);
            }
        }
        
        // Focus new window
        if let Some(client) = windows.get_mut(&window_id) {
            client.set_focused(true);
            
            // #region agent log
            {
                use std::fs::OpenOptions;
                use std::io::Write;
                let log_entry = serde_json::json!({
                    "sessionId": "debug-session",
                    "runId": "run1",
                    "hypothesisId": "B",
                    "location": "wm/mod.rs:978",
                    "message": "Calling set_input_focus",
                    "data": {"window_id": window_id, "has_frame": client.frame.is_some()},
                    "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
                });
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("/home/bizkit/GitHub/area/.cursor/debug.log") {
                    let _ = writeln!(file, "{}", log_entry);
                }
            }
            // #endregion
            
            // Set X11 input focus
            let focus_result = conn.set_input_focus(
                InputFocus::POINTER_ROOT,
                window_id,
                x11rb::CURRENT_TIME,
            );
            
            // #region agent log
            {
                use std::fs::OpenOptions;
                use std::io::Write;
                let log_entry = serde_json::json!({
                    "sessionId": "debug-session",
                    "runId": "run1",
                    "hypothesisId": "B",
                    "location": "wm/mod.rs:985",
                    "message": "set_input_focus result",
                    "data": {"window_id": window_id, "success": focus_result.is_ok(), "error": focus_result.as_ref().err().map(|e| format!("{:?}", e))},
                    "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
                });
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("/home/bizkit/GitHub/area/.cursor/debug.log") {
                    let _ = writeln!(file, "{}", log_entry);
                }
            }
            focus_result?;
            // #endregion
            
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
        // #region agent log
        {
            use std::fs::OpenOptions;
            use std::io::Write;
            let log_entry = serde_json::json!({
                "sessionId": "debug-session",
                "runId": "run1",
                "hypothesisId": "C",
                "location": "wm/mod.rs:1009",
                "message": "start_drag called",
                "data": {"window_id": window_id, "start_x": start_x, "start_y": start_y},
                "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
            });
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("/home/bizkit/GitHub/area/.cursor/debug.log") {
                let _ = writeln!(file, "{}", log_entry);
            }
        }
        // #endregion
        
        let client = windows.get(&window_id)
            .context("Window not found")?;
        
        info!("Starting drag for window {} at root coordinates ({}, {})", window_id, start_x, start_y);
        
        // Grab pointer for smooth dragging
        // Note: grab_pointer may fail if pointer is already grabbed, but we continue anyway
        // Cursor parameter: 0 means use current cursor, but we need to pass a Window
        // Using root window as cursor window (will use current cursor)
        let grab_result = conn.grab_pointer(
            false, // owner_events
            self.root,
            EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            self.root, // confine_to window
            0u32, // cursor (NONE = use current cursor)
            x11rb::CURRENT_TIME,
        );
        
        let grab_success = grab_result.is_ok();
        if let Err(e) = grab_result {
            warn!("Failed to grab pointer for drag: {:?}", e);
        }
        
        // Store drag state with root coordinates
        self.drag_state = Some(DragState {
            window_id,
            start_x,
            start_y,
            window_start_x: client.geometry.x,
            window_start_y: client.geometry.y,
        });
        
        // #region agent log
        {
            use std::fs::OpenOptions;
            use std::io::Write;
            let log_entry = serde_json::json!({
                "sessionId": "debug-session",
                "runId": "run1",
                "hypothesisId": "C",
                "location": "wm/mod.rs:1048",
                "message": "drag_state set",
                "data": {
                    "window_id": window_id,
                    "window_start_x": client.geometry.x,
                    "window_start_y": client.geometry.y,
                    "grab_success": grab_success
                },
                "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
            });
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("/home/bizkit/GitHub/area/.cursor/debug.log") {
                let _ = writeln!(file, "{}", log_entry);
            }
        }
        // #endregion
        
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
        // #region agent log
        {
            use std::fs::OpenOptions;
            use std::io::Write;
            let has_drag = self.drag_state.is_some();
            let log_entry = serde_json::json!({
                "sessionId": "debug-session",
                "runId": "run1",
                "hypothesisId": "C",
                "location": "wm/mod.rs:1055",
                "message": "update_drag called",
                "data": {"current_x": current_x, "current_y": current_y, "has_drag_state": has_drag},
                "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
            });
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("/home/bizkit/GitHub/area/.cursor/debug.log") {
                let _ = writeln!(file, "{}", log_entry);
            }
        }
        // #endregion
        
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
            
            // #region agent log
            {
                use std::fs::OpenOptions;
                use std::io::Write;
                let log_entry = serde_json::json!({
                    "sessionId": "debug-session",
                    "runId": "run1",
                    "hypothesisId": "C",
                    "location": "wm/mod.rs:1075",
                    "message": "Updating window position",
                    "data": {"window_id": drag.window_id, "new_x": new_x, "new_y": new_y, "has_frame": client.frame.is_some()},
                    "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
                });
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("/home/bizkit/GitHub/area/.cursor/debug.log") {
                    let _ = writeln!(file, "{}", log_entry);
                }
            }
            // #endregion
            
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
                    client.window,
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
    
    /// Find client window ID from any window ID (client, frame, titlebar, buttons)
    pub fn find_client_from_window(
        &self,
        windows: &HashMap<u32, Client>,
        window_id: u32,
    ) -> Option<u32> {
        // Check if it's a direct client window
        if windows.contains_key(&window_id) {
            return Some(window_id);
        }
        
        // Check if it's part of a frame (frame, titlebar, buttons)
        for (client_id, client) in windows {
            if let Some(frame_state) = &client.frame {
                let frame = decorations::WindowFrame::from_state(*client_id, frame_state);
                if frame.contains(window_id) {
                    return Some(*client_id);
                }
            }
        }
        
        None
    }
}
