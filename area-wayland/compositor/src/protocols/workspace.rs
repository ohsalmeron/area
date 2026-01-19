//! Workspace protocols
//!
//! Implements ext-workspace-manager-v1 and related protocols.

use tracing::debug;

/// Workspace protocol manager
///
/// PURPOSE: ext-workspace-manager-v1 protocol. Will be used when clients need workspace info.
/// PLAN: Expose workspaces via protocol, handle workspace switch requests from clients.
#[allow(dead_code)]
pub struct WorkspaceProtocolManager {
    // TODO: Store workspace protocol state
}

impl Default for WorkspaceProtocolManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceProtocolManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        debug!("Initializing workspace protocols");
        Self {}
    }

    /// Handle workspace switch request
    #[allow(dead_code)]
    pub fn handle_workspace_switch(&mut self, _workspace_id: u32) {
        debug!("Workspace switch requested");
        // TODO: Implement workspace switching via protocol
    }
}
