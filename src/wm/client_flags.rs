//! Client Flags
//!
//! Bitfield flags for client state, matching xfwm4's flag system.

use bitflags::bitflags;

bitflags! {
    /// XFWM flags - Window manager internal flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct XfwmFlags: u64 {
        const HAS_BORDER          = 1 << 0;
        const HAS_MENU            = 1 << 1;
        const HAS_MAXIMIZE        = 1 << 2;
        const HAS_CLOSE           = 1 << 3;
        const HAS_HIDE            = 1 << 4;
        const HAS_MOVE            = 1 << 5;
        const HAS_RESIZE          = 1 << 6;
        const HAS_STICK           = 1 << 7;
        const FOCUS               = 1 << 8;
        const IS_RESIZABLE        = 1 << 9;
        const MAP_PENDING         = 1 << 10;
        const VISIBLE             = 1 << 11;
        const MANAGED             = 1 << 13;
        const SESSION_MANAGED     = 1 << 14;
        const WORKSPACE_SET       = 1 << 15;
        const WAS_SHOWN           = 1 << 16;
        const DRAW_ACTIVE         = 1 << 17;
        const SEEN_ACTIVE         = 1 << 18;
        const FIRST_MAP           = 1 << 19;
        const SAVED_POS           = 1 << 20;
        const MOVING_RESIZING     = 1 << 21;
        const NEEDS_REDRAW        = 1 << 22;
        const OPACITY_LOCKED      = 1 << 23;
    }
}

impl Default for XfwmFlags {
    fn default() -> Self {
        Self::HAS_BORDER
            | Self::HAS_MENU
            | Self::HAS_MAXIMIZE
            | Self::HAS_STICK
            | Self::HAS_HIDE
            | Self::HAS_CLOSE
            | Self::HAS_MOVE
            | Self::HAS_RESIZE
            | Self::FIRST_MAP
            | Self::NEEDS_REDRAW
    }
}

bitflags! {
    /// CLIENT flags - Window state flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ClientFlags: u64 {
        const HAS_STRUT            = 1 << 0;
        const HAS_STRUT_PARTIAL    = 1 << 1;
        const HAS_USER_TIME        = 1 << 2;
        const HAS_STARTUP_TIME     = 1 << 3;
        const ABOVE                = 1 << 4;
        const BELOW                = 1 << 5;
        const FULLSCREEN           = 1 << 6;
        const ICONIFIED            = 1 << 7;
        const MAXIMIZED_VERT       = 1 << 8;
        const MAXIMIZED_HORIZ      = 1 << 9;
        const SHADED               = 1 << 10;
        const SKIP_PAGER           = 1 << 11;
        const SKIP_TASKBAR         = 1 << 12;
        const STATE_MODAL          = 1 << 13;
        const STICKY               = 1 << 15;
        const NAME_CHANGED         = 1 << 16;
        const DEMANDS_ATTENTION    = 1 << 17;
        const HAS_SHAPE            = 1 << 18;
        const FULLSCREEN_MONITORS  = 1 << 19;
        const HAS_FRAME_EXTENTS    = 1 << 20;
        const HIDE_TITLEBAR        = 1 << 21;
        const XSYNC_WAITING        = 1 << 22;
        const XSYNC_ENABLED        = 1 << 23;
        const XSYNC_EXT_COUNTER    = 1 << 24;
        const RESTORE_SIZE_POS     = 1 << 25;
    }
}

impl ClientFlags {
    pub fn maximized() -> Self {
        Self::MAXIMIZED_VERT | Self::MAXIMIZED_HORIZ
    }
    
    pub fn is_maximized(&self) -> bool {
        self.contains(Self::MAXIMIZED_VERT) && self.contains(Self::MAXIMIZED_HORIZ)
    }
}

bitflags! {
    /// WM flags - Window manager protocol flags
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct WmFlags: u32 {
        const DELETE       = 1 << 0;
        const INPUT        = 1 << 1;
        const TAKEFOCUS    = 1 << 2;
        const CONTEXT_HELP = 1 << 3;
        const URGENT       = 1 << 4;
        const PING         = 1 << 5;
    }
}

/// Window type (EWMH _NET_WM_WINDOW_TYPE)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    Normal,
    Desktop,
    Dock,
    Dialog,
    ModalDialog,
    Toolbar,
    Menu,
    Utility,
    Splashscreen,
    Notification,
    DropdownMenu,
    PopupMenu,
    Tooltip,
    Combo,
    Dnd,
}

impl WindowType {
    pub fn from_atom(atom: u32, atoms: &crate::wm::ewmh::Atoms) -> Self {
        if atom == atoms._net_wm_window_type_desktop {
            Self::Desktop
        } else if atom == atoms._net_wm_window_type_dock {
            Self::Dock
        } else if atom == atoms._net_wm_window_type_dialog {
            Self::Dialog
        } else if atom == atoms._net_wm_window_type_utility {
            Self::Utility
        } else if atom == atoms._net_wm_window_type_toolbar {
            Self::Toolbar
        } else if atom == atoms._net_wm_window_type_splash {
            Self::Splashscreen
        } else if atom == atoms._net_wm_window_type_menu {
            Self::Menu
        } else if atom == atoms._net_wm_window_type_notification {
            Self::Notification
        } else {
            Self::Normal
        }
    }
}

/// Window layer (for stacking)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WindowLayer {
    Desktop = 0,
    Below = 1,
    Normal = 2,
    Above = 3,
    Fullscreen = 4,
}

/// Tile position type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TilePosition {
    None,
    Left,
    Right,
    Down,
    Up,
    DownLeft,
    DownRight,
    UpLeft,
    UpRight,
}

