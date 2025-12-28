//! Client Module
//!
//! Represents a window being managed by the window manager.
//! This is the equivalent of xfwm4's Client structure.

use std::sync::Arc;
use crate::shared::window_state::{Geometry, WindowFrame};
use crate::wm::client_flags::{ClientFlags, XfwmFlags, WmFlags, WindowType, WindowLayer, TilePosition};
use crate::wm::screen::ScreenInfo;

/// Window Manager client state
/// 
/// This is the equivalent of xfwm4's Client structure.
/// Represents a window being managed by the WM with all its state.
pub struct Client {
    /// Reference to screen info
    pub screen_info: Option<Arc<ScreenInfo>>,
    
    /// X11 window ID (client window)
    pub window: u32,
    
    /// Frame window (decorations)
    pub frame: Option<WindowFrame>,
    
    /// Transient for window
    pub transient_for: Option<u32>,
    
    /// User time window
    pub user_time_win: Option<u32>,
    
    /// Client leader window
    pub client_leader: Option<u32>,
    
    /// Group leader window
    pub group_leader: Option<u32>,
    
    /// Window layer (for stacking)
    pub win_layer: WindowLayer,
    
    /// Initial layer (before fullscreen)
    pub initial_layer: WindowLayer,
    
    /// Client serial (unique ID)
    pub serial: u64,
    
    /// Ignore unmap count
    pub ignore_unmap: u32,
    
    /// Window type atom
    pub type_atom: u32,
    
    /// Window type
    pub type_: WindowType,
    
    /// Visual ID
    pub visual: u32,
    
    /// Window geometry
    pub geometry: Geometry,
    
    /// Applied geometry (what we told the client)
    pub applied_geometry: Geometry,
    
    /// Saved geometry (for restore)
    pub saved_geometry: Option<Geometry>,
    
    /// Pre-fullscreen geometry
    pub pre_fullscreen_geometry: Option<Geometry>,
    
    /// Pre-fullscreen layer
    pub pre_fullscreen_layer: WindowLayer,
    
    /// Pre-relayout position (for XRandR)
    pub pre_relayout_x: i32,
    pub pre_relayout_y: i32,
    
    /// Frame cache dimensions (for optimization)
    pub frame_cache_width: i32,
    pub frame_cache_height: i32,
    
    /// Depth
    pub depth: u8,
    
    /// Border width
    pub border_width: u16,
    
    /// Gravity
    pub gravity: u8,
    
    /// Workspace (0xFFFFFFFF = all workspaces/sticky)
    pub win_workspace: u32,
    
    /// Blink iterations (for urgency)
    pub blink_iterations: i32,
    
    /// Button status
    pub button_status: [i32; 7], // BUTTON_COUNT
    
    /// Struts (for panels/docks)
    pub struts: [i32; 12], // STRUTS_SIZE
    
    /// Hostname
    pub hostname: String,
    
    /// Window name/title
    pub name: String,
    
    /// User time
    pub user_time: u32,
    
    /// Process ID
    pub pid: u32,
    
    /// Ping time
    pub ping_time: u32,
    
    /// Client flags
    pub flags: ClientFlags,
    
    /// WM flags
    pub wm_flags: WmFlags,
    
    /// XFWM flags
    pub xfwm_flags: XfwmFlags,
    
    /// Fullscreen monitors [top, bottom, left, right]
    pub fullscreen_monitors: Option<[u32; 4]>,
    
    /// Frame extents [left, right, top, bottom]
    pub frame_extents: [i32; 4],
    
    /// Tile mode
    pub tile_mode: TilePosition,
    
    /// Opacity (0-0xFFFFFFFF, 0xFFFFFFFF = opaque)
    pub opacity: u32,
    
    /// Applied opacity
    pub opacity_applied: u32,
    
    /// Opacity flags (which bits are applied)
    pub opacity_flags: u32,
    
    /// Startup ID (for startup notification)
    pub startup_id: Option<String>,
    
    /// XSync counter (if XSync enabled)
    pub xsync_counter: Option<u64>,
    
    /// XSync value
    pub xsync_value: Option<u64>,
    
    /// Next XSync value
    pub next_xsync_value: Option<u64>,
    
    /// XSync alarm
    pub xsync_alarm: Option<u32>,
    
    /// XSync timeout ID
    pub xsync_timeout_id: Option<u32>,
    
    /// Colormap windows
    pub cmap_windows: Vec<u32>,
    
    /// Colormap
    pub cmap: Option<u32>,
    
    /// Number of colormap windows
    pub ncmap: usize,
    
    /// Size hints (min/max size, increments, etc.)
    pub size_hints: Option<SizeHints>,
    
    /// WM hints
    pub wm_hints: Option<WmHints>,
    
    /// Class hint
    pub class_hint: Option<ClassHint>,
    
    /// MWM hints (Motif)
    pub mwm_hints: Option<MwmHints>,
}

/// Size hints (XSizeHints equivalent)
#[derive(Debug, Clone)]
pub struct SizeHints {
    pub flags: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
    pub width_inc: u32,
    pub height_inc: u32,
    pub min_aspect_num: u32,
    pub min_aspect_den: u32,
    pub max_aspect_num: u32,
    pub max_aspect_den: u32,
    pub base_width: u32,
    pub base_height: u32,
    pub win_gravity: u8,
}

/// WM hints (XWMHints equivalent)
#[derive(Debug, Clone)]
pub struct WmHints {
    pub flags: u32,
    pub input: bool,
    pub initial_state: u32,
    pub icon_pixmap: Option<u32>,
    pub icon_window: Option<u32>,
    pub icon_x: i32,
    pub icon_y: i32,
    pub icon_mask: Option<u32>,
    pub window_group: Option<u32>,
}

/// Class hint (XClassHint equivalent)
#[derive(Debug, Clone)]
pub struct ClassHint {
    pub res_name: String,
    pub res_class: String,
}

/// MWM hints (Motif Window Manager hints)
#[derive(Debug, Clone)]
pub struct MwmHints {
    pub flags: u32,
    pub functions: u32,
    pub decorations: u32,
}

impl Client {
    /// Create a new client
    pub fn new(window: u32, geometry: Geometry) -> Self {
        Self {
            screen_info: None,
            window,
            frame: None,
            transient_for: None,
            user_time_win: None,
            client_leader: None,
            group_leader: None,
            win_layer: WindowLayer::Normal,
            initial_layer: WindowLayer::Normal,
            serial: 0,
            ignore_unmap: 0,
            type_atom: 0,
            type_: WindowType::Normal,
            visual: 0,
            geometry,
            applied_geometry: geometry,
            saved_geometry: None,
            pre_fullscreen_geometry: None,
            pre_fullscreen_layer: WindowLayer::Normal,
            pre_relayout_x: 0,
            pre_relayout_y: 0,
            frame_cache_width: 0,
            frame_cache_height: 0,
            depth: 24,
            border_width: 0,
            gravity: 0, // NorthWestGravity
            win_workspace: 0,
            blink_iterations: 0,
            button_status: [0; 7],
            struts: [0; 12],
            hostname: String::new(),
            name: String::new(),
            user_time: 0,
            pid: 0,
            ping_time: 0,
            flags: ClientFlags::empty(),
            wm_flags: WmFlags::empty(),
            xfwm_flags: XfwmFlags::default(),
            fullscreen_monitors: None,
            frame_extents: [0; 4],
            tile_mode: TilePosition::None,
            opacity: 0xFFFFFFFF, // Opaque
            opacity_applied: 0xFFFFFFFF,
            opacity_flags: 0,
            startup_id: None,
            xsync_counter: None,
            xsync_value: None,
            next_xsync_value: None,
            xsync_alarm: None,
            xsync_timeout_id: None,
            cmap_windows: Vec::new(),
            cmap: None,
            ncmap: 0,
            size_hints: None,
            wm_hints: None,
            class_hint: None,
            mwm_hints: None,
        }
    }
    
    /// Get window ID (for compatibility)
    pub fn id(&self) -> u32 {
        self.window
    }
    
    /// Get window ID as field (for compatibility with old code)
    pub fn get_id(&self) -> u32 {
        self.window
    }
    
    /// Check if window is maximized
    pub fn is_maximized(&self) -> bool {
        self.flags.contains(ClientFlags::MAXIMIZED_VERT) && self.flags.contains(ClientFlags::MAXIMIZED_HORIZ)
    }
    
    /// Check if window is fullscreen
    pub fn is_fullscreen(&self) -> bool {
        self.flags.contains(ClientFlags::FULLSCREEN)
    }
    
    /// Check if window is minimized/iconified
    pub fn is_minimized(&self) -> bool {
        self.flags.contains(ClientFlags::ICONIFIED)
    }
    
    /// Check if window is shaded
    pub fn is_shaded(&self) -> bool {
        self.flags.contains(ClientFlags::SHADED)
    }
    
    /// Check if window is sticky (on all workspaces)
    pub fn is_sticky(&self) -> bool {
        self.flags.contains(ClientFlags::STICKY) || self.win_workspace == 0xFFFFFFFF
    }
    
    /// Calculate frame geometry
    pub fn frame_geometry(&self) -> Geometry {
        if self.is_fullscreen() {
            return self.geometry;
        }
        
        if let Some(_frame) = &self.frame {
            // Frame extends beyond client by frame extents
            Geometry {
                x: self.geometry.x - self.frame_extents[0],
                y: self.geometry.y - self.frame_extents[2],
                width: self.geometry.width + (self.frame_extents[0] + self.frame_extents[1]) as u32,
                height: self.geometry.height + (self.frame_extents[2] + self.frame_extents[3]) as u32,
            }
        } else {
            self.geometry
        }
    }
}

// Compatibility with existing code
impl Client {
    /// Get geometry (for compatibility with old code)
    pub fn geometry(&self) -> Geometry {
        self.geometry
    }
    
    /// Get state flags (for compatibility)
    pub fn state(&self) -> crate::shared::window_state::WindowFlags {
        crate::shared::window_state::WindowFlags {
            maximized: self.is_maximized(),
            minimized: self.is_minimized(),
            fullscreen: self.is_fullscreen(),
            shaded: self.is_shaded(),
            sticky: self.is_sticky(),
            modal: self.flags.contains(ClientFlags::STATE_MODAL),
            skip_pager: self.flags.contains(ClientFlags::SKIP_PAGER),
            skip_taskbar: self.flags.contains(ClientFlags::SKIP_TASKBAR),
            above: self.flags.contains(ClientFlags::ABOVE),
            below: self.flags.contains(ClientFlags::BELOW),
            demands_attention: self.flags.contains(ClientFlags::DEMANDS_ATTENTION),
        }
    }
    
    /// Get title (for compatibility)
    pub fn title(&self) -> &str {
        &self.name
    }
    
    /// Check if mapped (for compatibility)
    pub fn mapped(&self) -> bool {
        self.xfwm_flags.contains(XfwmFlags::VISIBLE)
    }
    
    /// Check if focused (for compatibility)
    pub fn focused(&self) -> bool {
        self.xfwm_flags.contains(XfwmFlags::FOCUS)
    }
    
    /// Get frame window ID (for compatibility)
    pub fn get_frame_window(&self) -> Option<u32> {
        self.frame.as_ref().map(|f| f.frame)
    }
    
    /// Set frame (for compatibility)
    pub fn set_frame(&mut self, frame: Option<WindowFrame>) {
        self.frame = frame;
    }
    
    /// Set mapped state (for compatibility)
    pub fn set_mapped(&mut self, mapped: bool) {
        if mapped {
            self.xfwm_flags.insert(XfwmFlags::VISIBLE);
        } else {
            self.xfwm_flags.remove(XfwmFlags::VISIBLE);
        }
    }
    
    /// Set focused state (for compatibility)
    pub fn set_focused(&mut self, focused: bool) {
        if focused {
            self.xfwm_flags.insert(XfwmFlags::FOCUS);
        } else {
            self.xfwm_flags.remove(XfwmFlags::FOCUS);
        }
    }
    
    /// Get restore geometry (for compatibility - uses saved_geometry)
    pub fn restore_geometry(&self) -> Option<Geometry> {
        self.saved_geometry
    }
    
    /// Set restore geometry (for compatibility - uses saved_geometry)
    pub fn set_restore_geometry(&mut self, geom: Option<Geometry>) {
        self.saved_geometry = geom;
    }
    
    /// Set state flags (for compatibility)
    pub fn set_state(&mut self, state: crate::shared::window_state::WindowFlags) {
        if state.maximized {
            self.flags.insert(ClientFlags::MAXIMIZED_VERT);
            self.flags.insert(ClientFlags::MAXIMIZED_HORIZ);
        } else {
            self.flags.remove(ClientFlags::MAXIMIZED_VERT);
            self.flags.remove(ClientFlags::MAXIMIZED_HORIZ);
        }
        if state.minimized {
            self.flags.insert(ClientFlags::ICONIFIED);
        } else {
            self.flags.remove(ClientFlags::ICONIFIED);
        }
        if state.fullscreen {
            self.flags.insert(ClientFlags::FULLSCREEN);
        } else {
            self.flags.remove(ClientFlags::FULLSCREEN);
        }
        if state.shaded {
            self.flags.insert(ClientFlags::SHADED);
        } else {
            self.flags.remove(ClientFlags::SHADED);
        }
        if state.sticky {
            self.flags.insert(ClientFlags::STICKY);
        } else {
            self.flags.remove(ClientFlags::STICKY);
        }
        if state.modal {
            self.flags.insert(ClientFlags::STATE_MODAL);
        } else {
            self.flags.remove(ClientFlags::STATE_MODAL);
        }
        if state.skip_pager {
            self.flags.insert(ClientFlags::SKIP_PAGER);
        } else {
            self.flags.remove(ClientFlags::SKIP_PAGER);
        }
        if state.skip_taskbar {
            self.flags.insert(ClientFlags::SKIP_TASKBAR);
        } else {
            self.flags.remove(ClientFlags::SKIP_TASKBAR);
        }
        if state.above {
            self.flags.insert(ClientFlags::ABOVE);
        } else {
            self.flags.remove(ClientFlags::ABOVE);
        }
        if state.below {
            self.flags.insert(ClientFlags::BELOW);
        } else {
            self.flags.remove(ClientFlags::BELOW);
        }
        if state.demands_attention {
            self.flags.insert(ClientFlags::DEMANDS_ATTENTION);
        } else {
            self.flags.remove(ClientFlags::DEMANDS_ATTENTION);
        }
    }
}

// Compatibility: Add fields that old code expects
impl Client {
    /// Compatibility: Get state as mutable reference (creates temporary)
    pub fn state_mut(&mut self) -> StateMut {
        StateMut { client: self }
    }
}

/// Helper for mutable state access
pub struct StateMut<'a> {
    client: &'a mut Client,
}

impl<'a> StateMut<'a> {
    pub fn fullscreen(&self) -> bool {
        self.client.is_fullscreen()
    }
    
    pub fn set_fullscreen(&mut self, val: bool) {
        if val {
            self.client.flags.insert(ClientFlags::FULLSCREEN);
        } else {
            self.client.flags.remove(ClientFlags::FULLSCREEN);
        }
    }
    
    pub fn maximized(&self) -> bool {
        self.client.is_maximized()
    }
    
    pub fn minimized(&self) -> bool {
        self.client.is_minimized()
    }
    
    pub fn set_minimized(&mut self, val: bool) {
        if val {
            self.client.flags.insert(ClientFlags::ICONIFIED);
        } else {
            self.client.flags.remove(ClientFlags::ICONIFIED);
        }
    }
    
    pub fn above(&self) -> bool {
        self.client.flags.contains(ClientFlags::ABOVE)
    }
    
    pub fn set_above(&mut self, val: bool) {
        if val {
            self.client.flags.insert(ClientFlags::ABOVE);
        } else {
            self.client.flags.remove(ClientFlags::ABOVE);
        }
    }
    
    pub fn below(&self) -> bool {
        self.client.flags.contains(ClientFlags::BELOW)
    }
    
    pub fn set_below(&mut self, val: bool) {
        if val {
            self.client.flags.insert(ClientFlags::BELOW);
        } else {
            self.client.flags.remove(ClientFlags::BELOW);
        }
    }
}
