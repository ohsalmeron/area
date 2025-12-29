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
use std::sync::Arc;
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

pub struct WindowManager {
    screen_num: usize,
    root: u32,
    pub atoms: Atoms,
    /// WM owner window (for ICCCM selection)
    /// 
    /// This window owns the WM_S{screen} selection atom and must remain alive
    /// for the lifetime of the window manager. It's used by clients to detect
    /// the active WM and is referenced by _NET_SUPPORTING_WM_CHECK on the root.
    /// We don't actively read it, but keeping it in the struct ensures it
    /// stays alive (window is destroyed when struct is dropped).
    #[allow(dead_code)]
    wm_owner_window: u32,
    
    /// Display info (X11 connection, extensions, atoms, cursors)
    pub display_info: std::sync::Arc<display::DisplayInfo>,
    
    /// Screen info (per-screen state, workspaces, monitors)
    pub screen_info: std::sync::Arc<screen::ScreenInfo>,
    
    // Manager modules
    pub focus_manager: focus::FocusManager,
    pub stacking_manager: stacking::StackingManager,
    pub workspace_manager: workspace::WorkspaceManager,
    pub move_resize_manager: moveresize::MoveResizeManager,
    pub placement_manager: placement::PlacementManager,
    pub keyboard_manager: keyboard::KeyboardManager,
    pub settings_manager: settings::SettingsManager,
    pub transient_manager: transients::TransientManager,
    pub hints_manager: hints::HintsManager,
    pub menu_manager: menu::MenuManager,
    pub icon_manager: icons::IconManager,
    pub cycle_manager: cycle::CycleManager,
    pub session_manager: session::SessionManager,
    pub startup_manager: startup::StartupNotificationManager,
    pub terminate_manager: terminate::TerminateManager,
    pub device_manager: device::DeviceManager,
    pub event_filter_manager: event_filter::EventFilterManager,
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
        conn: Arc<x11rb::rust_connection::RustConnection>,
        screen_num: usize,
        root: u32,
        replace: bool,
    ) -> Result<Self> {
        info!("Initializing window manager (replace={})", replace);
        
        use std::sync::Arc;
        let screen = &conn.as_ref().setup().roots[screen_num];
        
        // Step 1: Intern WM selection atom (ICCCM: WM_S{screen_num})
        let wm_selection_name = format!("WM_S{}", screen_num);
        debug!("WM: Interning selection atom '{}'", wm_selection_name);
        let wm_selection_atom = conn.as_ref().intern_atom(false, wm_selection_name.as_bytes())?
            .reply()
            .context("Failed to intern WM selection atom")?
            .atom;
        debug!("WM: Selection atom interned: {}", wm_selection_atom);
        
        // Step 2: Check for existing WM
        debug!("WM: Checking for existing window manager...");
        let current_wm_owner = conn.as_ref().get_selection_owner(wm_selection_atom)?
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
            let _ = conn.as_ref().change_window_attributes(
                current_wm_owner,
                &ChangeWindowAttributesAux::new()
                    .event_mask(EventMask::STRUCTURE_NOTIFY),
            );
            conn.as_ref().flush()?;
        }
        
        // Step 3: Create WM owner window (like xfwm4's xfwm4_win)
        // This window owns the WM selection atom
        debug!("WM: Creating WM owner window...");
        let wm_owner_window = conn.as_ref().generate_id()?;
        conn.as_ref().create_window(
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
        conn.as_ref().map_window(wm_owner_window)?;
        conn.as_ref().flush()?;
        debug!("WM: Created owner window: 0x{:x}", wm_owner_window);
        
        // Step 4: Acquire WM selection ownership
        debug!("WM: Acquiring WM selection ownership...");
        conn.as_ref().set_selection_owner(
            wm_owner_window,
            wm_selection_atom,
            x11rb::CURRENT_TIME,
        )?
        .check()
        .context("Failed to set WM selection owner")?;
        conn.as_ref().flush()?;
        
        // Verify we own the selection
        debug!("WM: Verifying selection ownership...");
        let owner_after = conn.as_ref().get_selection_owner(wm_selection_atom)?
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
                let current_owner = conn.as_ref().get_selection_owner(wm_selection_atom)?
                    .reply()
                    .context("Failed to check WM selection owner")?
                    .owner;
                
                // If owner changed or window was destroyed, we're good
                if current_owner == wm_owner_window {
                    // Try to get window attributes - if it fails, window is gone
                    if conn.as_ref().get_window_attributes(current_wm_owner)?.reply().is_err() {
                        info!("Previous WM window destroyed");
                        break;
                    }
                }
                
                // Process any pending events (including DestroyNotify)
                conn.as_ref().flush()?;
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
        let root_attrs = conn.as_ref().get_window_attributes(root)?
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
        conn.as_ref().change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(combined_mask),
        )?
        .check()
        .context("Failed to select events on root window - is another WM running?")?;
        conn.as_ref().flush()?;
        debug!("WM: Successfully selected events on root window");
        
        // Step 8: Initialize EWMH atoms
        debug!("WM: Initializing EWMH atoms...");
        let atoms = Atoms::new(conn.as_ref())?;
        atoms.setup_supported(conn.as_ref(), root)?;
        debug!("WM: EWMH atoms initialized");
        
        // Step 9: Set up _NET_SUPPORTING_WM_CHECK (for better interoperability)
        debug!("WM: Setting up _NET_SUPPORTING_WM_CHECK...");
        // Use the atom from Atoms struct (now available there)
        let net_supporting_wm_check = atoms._net_supporting_wm_check;
        
        // Set _NET_SUPPORTING_WM_CHECK on root to point to our owner window
        conn.as_ref().change_property32(
            PropMode::REPLACE,
            root,
            net_supporting_wm_check,
            AtomEnum::WINDOW,
            &[wm_owner_window],
        )?;
        
        // Set _NET_WM_NAME on owner window
        let net_wm_name = atoms.net_wm_name;
        let wm_name = b"area\0";
        conn.as_ref().change_property(
            PropMode::REPLACE,
            wm_owner_window,
            net_wm_name,
            AtomEnum::STRING,
            8,
            wm_name.len() as u32,
            wm_name,
        )?;
        
        conn.as_ref().flush()?;
        debug!("WM: _NET_SUPPORTING_WM_CHECK set to window 0x{:x}", wm_owner_window);
        
        // Step 8.5: Initialize DisplayInfo and ScreenInfo
        let display_info = Arc::new(display::DisplayInfo::new(conn.clone())?);
        let screen_info = Arc::new(screen::ScreenInfo::new(
            display_info.clone(),
            screen_num,
            root,
            screen.clone(),
        )?);
        
        // Step 10: Grab SUPER key (Mod4) for launcher
        // Note: Key grabbing may fail if keycodes don't match - we'll handle gracefully
        use x11rb::protocol::xproto::ModMask;
        let no_modifier = ModMask::from(0u16);
        
        // Try to grab SUPER key (keycode 133 = left SUPER, 134 = right SUPER)
        for keycode in [133u8, 134u8] {
            let _ = conn.as_ref().grab_key(
                false, // owner_events
                root,
                no_modifier,
                keycode,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            );
        }
        
        info!("Successfully became window manager (keyboard shortcuts enabled)");
        
        // Initialize manager modules
        let focus_manager = focus::FocusManager::new();
        let stacking_manager = stacking::StackingManager::new();
        let workspace_manager = workspace::WorkspaceManager::new(4); // Default 4 workspaces
        let move_resize_manager = moveresize::MoveResizeManager::new();
        let placement_manager = placement::PlacementManager::new(placement::PlacementPolicy::Smart);
        let keyboard_manager = keyboard::KeyboardManager::new(conn.as_ref(), &screen_info)?;
        let settings_manager = settings::SettingsManager::new();
        let transient_manager = transients::TransientManager::new();
        let hints_manager = hints::HintsManager;
        let menu_manager = menu::MenuManager::new(conn.as_ref(), &atoms)?;
        let icon_manager = icons::IconManager::new();
        let cycle_manager = cycle::CycleManager::new();
        let session_manager = session::SessionManager::new();
        let mut startup_manager = startup::StartupNotificationManager::new();
        // Set busy cursor from DisplayInfo
        startup_manager.set_busy_cursor(display_info.cursors.busy);
        let terminate_manager = terminate::TerminateManager::new();
        let mut device_manager = device::DeviceManager::new();
        // Initialize XInput2 if available
        if let Err(e) = device_manager.initialize_xinput2(conn.as_ref(), &display_info) {
            debug!("XInput2 initialization failed (non-fatal): {}", e);
        }
        let event_filter_manager = event_filter::EventFilterManager::new();
        
        Ok(Self {
            screen_num,
            root,
            atoms,
            wm_owner_window,
            display_info,
            screen_info,
            focus_manager,
            stacking_manager,
            workspace_manager,
            move_resize_manager,
            placement_manager,
            keyboard_manager,
            settings_manager,
            transient_manager,
            hints_manager,
            menu_manager,
            icon_manager,
            cycle_manager,
            session_manager,
            startup_manager,
            terminate_manager,
            device_manager,
            event_filter_manager,
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
        
        // Read window size hints using HintsManager
        let size_hints = hints::HintsManager::read_size_hints(
            conn,
            &self.display_info.atoms,
            client.window,
        )?;
        
        let mut width = geom.width as u32;
        let mut height = geom.height as u32;
        
        // If window is 1x1 (uninitialized), try to get size from hints
        if width == 1 && height == 1 {
            if let Some(ref hints) = size_hints {
                // Use base_width/base_height if available
                if hints.base_width > 0 && hints.base_height > 0 {
                    width = hints.base_width;
                    height = hints.base_height;
                } else if hints.width > 0 && hints.height > 0 {
                    width = hints.width;
                    height = hints.height;
                } else {
                    // Default size if no hints
                    width = 800;
                    height = 600;
                }
            } else {
                // No hints, use default size
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
        
        // Apply size hints constraints (min/max size, increments)
        let mut final_geometry = Geometry {
            x: geom.x as i32,
            y: geom.y as i32,
            width,
            height,
        };
        
        if let Some(ref hints) = size_hints {
            final_geometry = self.hints_manager.apply_size_hints(hints, &final_geometry);
            width = final_geometry.width;
            height = final_geometry.height;
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
        
        // Read WM hints (for initial state, input model, etc.)
        let wm_hints = hints::HintsManager::read_wm_hints(
            conn,
            &self.display_info.atoms,
            client.window,
        )?;
        
        // Store WM hints in client
        if let Some(ref hints) = wm_hints {
            client.wm_hints = Some(crate::wm::client::WmHints {
                flags: hints.flags,
                input: hints.input,
                initial_state: hints.initial_state,
                icon_pixmap: hints.icon_pixmap,
                icon_window: hints.icon_window,
                icon_x: hints.icon_x,
                icon_y: hints.icon_y,
                icon_mask: hints.icon_mask,
                window_group: hints.window_group,
            });
            
            // Apply initial_state hint: Iconic (3) means start minimized
            // X11 WM_HINTS initial_state values:
            // 0 = Withdrawn, 1 = Normal, 3 = Iconic, 4 = Inactive
            if hints.initial_state == 3 {
                // Iconic state - mark window as minimized
                client.flags.insert(crate::wm::client_flags::ClientFlags::ICONIFIED);
                debug!("Window {} has initial_state Iconic, will start minimized", client.window);
            }
            
            // Handle urgency hint
            if hints.is_urgent() {
                debug!("Window {} has urgency hint, setting DEMANDS_ATTENTION", client.window);
                client.flags.insert(crate::wm::client_flags::ClientFlags::DEMANDS_ATTENTION);
                // Set _NET_WM_STATE_DEMANDS_ATTENTION
                self.atoms.set_window_state(
                    conn,
                    client.window,
                    &[self.atoms._net_wm_state_demands_attention],
                    &[],
                )?;
            }
        }
        
        // Read WM_CLIENT_LEADER property for window grouping
        if let Ok(reply) = conn.get_property(
            false,
            client.window,
            self.display_info.atoms.wm_client_leader,
            AtomEnum::WINDOW,
            0,
            1,
        )?.reply() {
            if let Some(mut value32) = reply.value32() {
                if let Some(leader) = value32.next() {
                    if leader != 0 {
                        client.client_leader = Some(leader);
                        // If the leader is the window itself, it's the group leader
                        if leader == client.window {
                            client.group_leader = Some(leader);
                        } else {
                            // Otherwise, find the group leader (the leader's leader, or the leader itself)
                            client.group_leader = Some(leader);
                        }
                        debug!("Window {} has WM_CLIENT_LEADER: {} (group_leader: {:?})", 
                            client.window, leader, client.group_leader);
                    }
                }
            }
        }
        
        // Read transient relationship
        if let Ok(reply) = conn.get_property(
            false,
            client.window,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            0,
            1,
        )?.reply() {
            if let Some(mut value32) = reply.value32() {
                let transient_for = value32.next();
                if let Some(parent) = transient_for {
                    if parent != 0 {
                        self.transient_manager.set_transient_for(client.window, Some(parent));
                        client.transient_for = Some(parent);
                    }
                }
            }
        }
        
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
        
        // Clear move/resize state if this window was being moved/resized
        if let Some(ref state) = self.move_resize_manager.state {
            if state.window == client.window {
                self.move_resize_manager.state = None;
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
        
        // Check if window is unresponsive
        if self.terminate_manager.is_unresponsive(window_id) {
            warn!("Window {} is unresponsive, showing force quit dialog", window_id);
            // Show force quit dialog (user can choose to force kill)
            if let Err(e) = self.terminate_manager.show_force_quit_dialog(
                conn,
                &self.display_info,
                &self.screen_info,
                window_id,
            ) {
                warn!("Failed to show force quit dialog: {}", e);
            }
            // For now, still try to send WM_DELETE_WINDOW
            // In a full implementation, we'd wait for user response from dialog
        }
        
        // Send WM_DELETE_WINDOW message
        self.atoms.send_delete_window(conn, window_id)?;
        
        // Record that we sent WM_DELETE_WINDOW (for unresponsive detection)
        // Use current time (approximate, as we don't have exact server time)
        // In a real implementation, we'd get the server time from the event
        let timestamp = x11rb::CURRENT_TIME;
        self.terminate_manager.record_delete_sent(window_id, timestamp);
        
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
        let is_maximized = windows.get(&window_id)
            .context("Window not found")?
            .is_maximized();
        
        if is_maximized {
            // Restore window - use window_id to avoid borrow issues
            self.restore_window_by_id(conn, windows, window_id)?;
        } else {
            if let Some(client) = windows.get_mut(&window_id) {
                self.maximize_window(conn, client)?;
            }
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
    
    /// Shade window (roll up - hide everything except titlebar)
    pub fn shade_window(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, Client>,
        window_id: u32,
    ) -> Result<()> {
        let client = windows.get_mut(&window_id)
            .context("Window not found")?;
        
        if client.is_shaded() {
            debug!("Window {} is already shaded", window_id);
            return Ok(());
        }
        
        // Windows without titlebars cannot be shaded
        if client.frame.is_none() {
            debug!("Cannot shade window {} - no titlebar", window_id);
            return Ok(());
        }
        
        info!("Shading window {}", client.window);
        
        // Save current height before shading
        let original_height = client.geometry.height;
        client.saved_geometry = Some(client.geometry);
        
        // Set shaded flag
        client.flags.insert(crate::wm::client_flags::ClientFlags::SHADED);
        
        // Resize window to show only titlebar
        const TITLEBAR_HEIGHT: u32 = 32;
        const BORDER_WIDTH: u32 = 2;
        
        if let Some(frame_state) = &client.frame {
            let frame = decorations::WindowFrame::from_state(client.window, frame_state);
            
            // Frame height = titlebar + borders (top and bottom)
            let shaded_frame_height = TITLEBAR_HEIGHT + (BORDER_WIDTH * 2);
            
            // Resize frame to show only titlebar
            frame.resize(conn, (client.geometry.width + (BORDER_WIDTH * 2)) as u16, shaded_frame_height as u16, &crate::config::WindowDecorationConfig {
                titlebar_height: 32,
                border_width: 2,
                button_size: 20,
                button_padding: 5,
            })?;
            
            // Configure client window to be hidden (height = 0, or very small)
            // Client should be positioned at titlebar height
            conn.configure_window(
                client.window,
                &ConfigureWindowAux::new()
                    .x(BORDER_WIDTH as i32)
                    .y((TITLEBAR_HEIGHT + BORDER_WIDTH) as i32)
                    .width(client.geometry.width)
                    .height(1), // Minimal height to keep window mapped
            )?;
            
            // Update client geometry
            client.geometry.height = 1;
        }
        
        // Update EWMH state
        self.atoms.set_window_state(
            conn,
            client.window,
            &[self.atoms._net_wm_state_shaded],
            &[],
        )?;
        
        conn.flush()?;
        Ok(())
    }
    
    /// Unshade window (roll down - restore to original size)
    pub fn unshade_window(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, Client>,
        window_id: u32,
    ) -> Result<()> {
        let client = windows.get_mut(&window_id)
            .context("Window not found")?;
        
        if !client.is_shaded() {
            debug!("Window {} is not shaded", window_id);
            return Ok(());
        }
        
        info!("Unshading window {}", client.window);
        
        // Remove shaded flag
        client.flags.remove(crate::wm::client_flags::ClientFlags::SHADED);
        
        // Restore original geometry
        if let Some(saved) = client.saved_geometry {
            client.geometry = saved;
            client.saved_geometry = None;
        } else {
            // Fallback: restore to a reasonable default size
            warn!("No saved geometry for window {}, using default", window_id);
            client.geometry.height = 600; // Default height
        }
        
        const TITLEBAR_HEIGHT: u32 = 32;
        const BORDER_WIDTH: u32 = 2;
        
        if let Some(frame_state) = &client.frame {
            let frame = decorations::WindowFrame::from_state(client.window, frame_state);
            
            // Restore frame to full size
            let frame_width = client.geometry.width + (BORDER_WIDTH * 2);
            let frame_height = client.geometry.height + TITLEBAR_HEIGHT + (BORDER_WIDTH * 2);
            
            frame.resize(conn, frame_width as u16, frame_height as u16, &crate::config::WindowDecorationConfig {
                titlebar_height: 32,
                border_width: 2,
                button_size: 20,
                button_padding: 5,
            })?;
            
            // Restore client window size
            conn.configure_window(
                client.window,
                &ConfigureWindowAux::new()
                    .x(BORDER_WIDTH as i32)
                    .y((TITLEBAR_HEIGHT + BORDER_WIDTH) as i32)
                    .width(client.geometry.width)
                    .height(client.geometry.height),
            )?;
        }
        
        // Remove EWMH shaded state
        self.atoms.set_window_state(
            conn,
            client.window,
            &[],
            &[self.atoms._net_wm_state_shaded],
        )?;
        
        conn.flush()?;
        Ok(())
    }
    
    /// Toggle shade/unshade
    pub fn toggle_shade(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, Client>,
        window_id: u32,
    ) -> Result<()> {
        let is_shaded = windows.get(&window_id)
            .context("Window not found")?
            .is_shaded();
        
        if is_shaded {
            self.unshade_window(conn, windows, window_id)
        } else {
            self.shade_window(conn, windows, window_id)
        }
    }
    
    /// Restore window from maximized (by window ID)
    pub fn restore_window_by_id(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, Client>,
        window_id: u32,
    ) -> Result<()> {
        // Inline the logic to avoid borrow checker issues
        let client = windows.get_mut(&window_id)
            .context("Window not found")?;
        
        info!("Restoring window {}", client.window);
        
        // Restore from saved geometry
        if let Some(restore) = client.restore_geometry() {
            client.geometry = restore;
            
            // Restore frame and client window
            if let Some(frame_state) = &client.frame {
                let frame = decorations::WindowFrame::from_state(client.window, frame_state);
                const TITLEBAR_HEIGHT: i32 = 32;
                let frame_y = client.geometry.y - TITLEBAR_HEIGHT;
                frame.move_to(conn, client.geometry.x as i16, frame_y as i16)?;
                frame.resize(conn, client.geometry.width as u16, client.geometry.height as u16, &crate::config::WindowDecorationConfig {
                    titlebar_height: 32,
                    border_width: 2,
                    button_size: 20,
                    button_padding: 5,
                })?;
            } else {
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
        
        // Restore transients with parent
        let transients = self.transient_manager.get_transients(client.window);
        for transient_id in transients {
            if let Some(transient_client) = windows.get_mut(&transient_id) {
                if transient_client.is_minimized() {
                    transient_client.flags.remove(crate::wm::client_flags::ClientFlags::ICONIFIED);
                    if let Some(frame) = &transient_client.frame {
                        let _ = conn.map_window(frame.frame);
                    } else {
                        let _ = conn.map_window(transient_id);
                    }
                    transient_client.set_mapped(true);
                }
            }
        }
        
        conn.flush()?;
        Ok(())
    }
    
    /// Restore window from maximized
    pub fn restore_window(
        &mut self,
        conn: &RustConnection,
        windows: &mut HashMap<u32, Client>,
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
        
        // Restore transients with parent (if parent was minimized, restore transients)
        // Note: This is for maximize restore, not minimize restore
        // For minimize restore, we handle it when restoring from minimized state
        let transients = self.transient_manager.get_transients(client.window);
        for transient_id in transients {
            if let Some(transient_client) = windows.get_mut(&transient_id) {
                // Only restore if minimized (transients should be minimized with parent)
                if transient_client.is_minimized() {
                    transient_client.flags.remove(crate::wm::client_flags::ClientFlags::ICONIFIED);
                    if let Some(frame) = &transient_client.frame {
                        let _ = conn.map_window(frame.frame);
                    } else {
                        let _ = conn.map_window(transient_id);
                    }
                    transient_client.set_mapped(true);
                }
            }
        }
        
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
        
        // Minimize transients with parent
        let transients = self.transient_manager.get_transients(window_id);
        for transient_id in transients {
            if let Some(transient_client) = windows.get_mut(&transient_id) {
                // Only minimize if not already minimized
                if !transient_client.is_minimized() {
                    if let Some(frame) = &transient_client.frame {
                        let _ = conn.unmap_window(frame.frame);
                    } else {
                        let _ = conn.unmap_window(transient_id);
                    }
                    transient_client.set_mapped(false);
                    transient_client.flags.insert(crate::wm::client_flags::ClientFlags::ICONIFIED);
                }
            }
        }
        
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
            
            // Raise window using StackingManager (with transients)
            if let Err(err) = self.stacking_manager.raise_window_with_transients(
                conn,
                &self.display_info,
                &self.screen_info,
                window_id,
                windows,
                &self.transient_manager.transients,
            ) {
                warn!("Failed to raise window {}: {}", window_id, err);
            }
            
            // Update EWMH active window
            self.atoms.update_active_window(conn, self.root, Some(window_id))?;
            
            conn.flush()?;
        }
        
        Ok(())
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
