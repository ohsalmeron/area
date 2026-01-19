//! Output management
//!
//! Handles display outputs (monitors), multi-monitor support, and output layout.

use tracing::{debug, info};

/// An output represents a display/monitor
///
/// PURPOSE: Output/display management. Will be used when backend outputs are created.
/// PLAN: Create Output instances when DRM/winit backend reports new displays.
#[allow(dead_code)]
pub struct Output {
    id: u32,
    name: String,
    width: u32,
    height: u32,
    scale: f64,
}

impl Output {
    #[allow(dead_code)]
    pub fn new(id: u32, name: String, width: u32, height: u32, scale: f64) -> Self {
        Self {
            id,
            name,
            width,
            height,
            scale,
        }
    }

    #[allow(dead_code)]
    pub fn id(&self) -> u32 {
        self.id
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[allow(dead_code)]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[allow(dead_code)]
    pub fn scale(&self) -> f64 {
        self.scale
    }
}

/// Output manager
///
/// PURPOSE: Manages multiple outputs/displays. Will be used when backend outputs are created.
/// PLAN: Track outputs from backend, manage multi-monitor layouts, handle output hotplugging.
#[allow(dead_code)]
pub struct OutputManager {
    outputs: Vec<Output>,
    current: usize,
}

impl Default for OutputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputManager {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            outputs: Vec::new(),
            current: 0,
        }
    }

    /// Add an output
    #[allow(dead_code)]
    pub fn add_output(&mut self, output: Output) {
        info!("Adding output: {} ({}x{})", output.name(), output.width, output.height);
        self.outputs.push(output);
    }

    /// Remove an output
    #[allow(dead_code)]
    pub fn remove_output(&mut self, id: u32) {
        debug!("Removing output: {}", id);
        self.outputs.retain(|o| o.id() != id);
    }

    /// Get current output
    #[allow(dead_code)]
    pub fn current_output(&self) -> Option<&Output> {
        self.outputs.get(self.current)
    }

    /// Switch to a different output
    #[allow(dead_code)]
    pub fn switch_to(&mut self, index: usize) {
        if index < self.outputs.len() {
            self.current = index;
        }
    }
}
