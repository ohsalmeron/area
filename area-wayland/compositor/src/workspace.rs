//! Workspace management
//!
//! Handles virtual desktops and workspace switching.

use tracing::debug;

/// A workspace represents a virtual desktop
///
/// PURPOSE: Virtual desktop management. Will be used when workspace switching is implemented.
/// PLAN: Track views per workspace, implement workspace switching via keybinds/protocol.
#[allow(dead_code)]
pub struct Workspace {
    id: u32,
    name: String,
}

impl Workspace {
    #[allow(dead_code)]
    pub fn new(id: u32, name: String) -> Self {
        Self { id, name }
    }

    #[allow(dead_code)]
    pub fn id(&self) -> u32 {
        self.id
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Workspace manager
///
/// PURPOSE: Manages multiple workspaces/virtual desktops. Will be used when workspace switching is implemented.
/// PLAN: Track current workspace, handle workspace switching, manage views per workspace.
#[allow(dead_code)]
pub struct WorkspaceManager {
    workspaces: Vec<Workspace>,
    current: usize,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        let workspaces = vec![Workspace::new(1, "Workspace 1".to_string())];
        
        Self {
            workspaces,
            current: 0,
        }
    }

    #[allow(dead_code)]
    pub fn current_workspace(&self) -> &Workspace {
        &self.workspaces[self.current]
    }

    #[allow(dead_code)]
    pub fn switch_to(&mut self, index: usize) {
        if index < self.workspaces.len() {
            debug!("Switching to workspace {}", index);
            self.current = index;
        }
    }
}
