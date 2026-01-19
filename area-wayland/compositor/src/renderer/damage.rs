//! Damage tracking for efficient rendering

use tracing::debug;

/// Damage region
///
/// PURPOSE: Tracks damaged screen regions for efficient rendering. Will be used when rendering is implemented.
/// PLAN: Mark regions when surfaces commit, use for partial screen updates.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Damage {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Damage tracker for efficient rendering
///
/// PURPOSE: Tracks damaged screen regions for efficient rendering. Will be used when rendering is implemented.
/// PLAN: Collect damage from surface commits, clear after rendering, optimize redraw regions.
#[allow(dead_code)]
pub struct DamageTracker {
    damaged_regions: Vec<Damage>,
}

impl Default for DamageTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl DamageTracker {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            damaged_regions: Vec::new(),
        }
    }

    /// Mark a region as damaged
    #[allow(dead_code)]
    pub fn mark_damaged(&mut self, x: i32, y: i32, width: u32, height: u32) {
        debug!("Marking damage: ({}, {}) {}x{}", x, y, width, height);
        self.damaged_regions.push(Damage {
            x,
            y,
            width,
            height,
        });
    }

    /// Get all damaged regions
    #[allow(dead_code)]
    pub fn get_damaged_regions(&self) -> &[Damage] {
        &self.damaged_regions
    }

    /// Clear all damage
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.damaged_regions.clear();
    }
}
