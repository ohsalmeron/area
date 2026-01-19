//! View management
//!
//! Views represent managed windows (XDG toplevels).

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use tracing::debug;

/// Window geometry
///
/// PURPOSE: Represents window position and size. Will be used when XDG toplevel handling is implemented.
/// PLAN: Integrate with XdgToplevel surface commits to track window geometry.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Geometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// A view represents a managed window
///
/// PURPOSE: Core window management structure. Will be created when XDG toplevels are mapped.
/// PLAN: Create View instances in XdgToplevel request handlers when surfaces are committed.
#[allow(dead_code)]
pub struct View {
    /// Wayland surface (stored for future use when rendering is implemented)
    #[allow(dead_code)]
    surface: WlSurface,
    /// Window geometry
    geometry: Geometry,
    /// Whether the view is mapped (visible)
    mapped: bool,
    /// Whether the view has focus
    focused: bool,
    /// Workspace this view is on
    workspace_id: u32,
}

impl View {
    /// Create a new view
    #[allow(dead_code)]
    pub fn new(surface: WlSurface, workspace_id: u32) -> Self {
        Self {
            surface,
            geometry: Geometry {
                x: 100,
                y: 100,
                width: 800,
                height: 600,
            },
            mapped: false,
            focused: false,
            workspace_id,
        }
    }

    /// Map the view (make it visible)
    #[allow(dead_code)]
    pub fn map(&mut self) {
        debug!("Mapping view");
        self.mapped = true;
    }

    /// Unmap the view (hide it)
    #[allow(dead_code)]
    pub fn unmap(&mut self) {
        debug!("Unmapping view");
        self.mapped = false;
    }

    /// Set focus on this view
    #[allow(dead_code)]
    pub fn set_focus(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Update view geometry
    #[allow(dead_code)]
    pub fn set_geometry(&mut self, geometry: Geometry) {
        self.geometry = geometry;
    }

    /// Get view geometry
    #[allow(dead_code)]
    pub fn geometry(&self) -> Geometry {
        self.geometry
    }

    /// Check if view is mapped
    #[allow(dead_code)]
    pub fn is_mapped(&self) -> bool {
        self.mapped
    }

    /// Check if view has focus
    #[allow(dead_code)]
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Get workspace ID
    #[allow(dead_code)]
    pub fn workspace_id(&self) -> u32 {
        self.workspace_id
    }

    /// Set workspace ID
    #[allow(dead_code)]
    pub fn set_workspace(&mut self, workspace_id: u32) {
        self.workspace_id = workspace_id;
    }
}
