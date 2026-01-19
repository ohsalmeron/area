//! Scene graph for managing surfaces

use tracing::debug;
use crate::view::View;

/// Scene graph for managing surfaces
///
/// PURPOSE: Scene graph for rendering order. Will be used when rendering is implemented.
/// PLAN: Track views in rendering order, manage layer-shell surfaces, handle popups.
/// Unlike wlr_scene, this is a manual implementation.
#[allow(dead_code)]
pub struct Scene {
    // TODO: Store view tree
    // TODO: Store layer-shell surfaces
    // TODO: Store popups
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

impl Scene {
    #[allow(dead_code)]
    pub fn new() -> Self {
        debug!("Creating scene graph");
        Self {}
    }

    /// Add a view to the scene
    #[allow(dead_code)]
    pub fn add_view(&mut self, _view: &View) {
        debug!("Adding view to scene");
        // TODO: Add view to scene graph
    }

    /// Remove a view from the scene
    #[allow(dead_code)]
    pub fn remove_view(&mut self, _view: &View) {
        debug!("Removing view from scene");
        // TODO: Remove view from scene graph
    }

    /// Update view position in scene
    #[allow(dead_code)]
    pub fn update_view_position(&mut self, _view: &View) {
        debug!("Updating view position in scene");
        // TODO: Update view position
    }
}
