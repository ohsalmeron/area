//! Placement Module
//!
//! Window placement algorithms: smart placement, mouse placement, center, etc.
//! This matches xfwm4's window placement system.

use anyhow::Result;
use tracing::debug;
use x11rb::connection::Connection;
use x11rb::rust_connection::RustConnection;

use crate::shared::Geometry;
use crate::wm::client::Client;
use crate::wm::screen::ScreenInfo;

/// Placement policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PlacementPolicy {
    /// Smart placement (avoid overlapping)
    Smart,
    /// Center placement
    Center,
    /// Mouse placement (at cursor)
    Mouse,
    /// Random placement
    Random,
    /// Respect initial position hints
    RespectInitialPosition,
}

/// Placement manager
pub struct PlacementManager {
    /// Current placement policy
    pub policy: PlacementPolicy,
    
    /// Smart placement grid
    pub smart_grid: Vec<(i32, i32)>,
}

impl PlacementManager {
    /// Create a new placement manager
    pub fn new(policy: PlacementPolicy) -> Self {
        Self {
            policy,
            smart_grid: Vec::new(),
        }
    }
    
    /// Place a window
    pub fn place_window(
        &mut self,
        conn: &RustConnection,
        screen_info: &ScreenInfo,
        client: &mut Client,
        mouse_x: Option<i16>,
        mouse_y: Option<i16>,
        existing_clients: &std::collections::HashMap<u32, Client>,
    ) -> Result<Geometry> {
        let work_area = &screen_info.work_area;
        let mut geometry = client.geometry;
        
        match self.policy {
            PlacementPolicy::Smart => {
                geometry = self.place_smart(screen_info, &geometry, existing_clients)?;
            }
            PlacementPolicy::Center => {
                geometry = self.place_center(screen_info, &geometry)?;
            }
            PlacementPolicy::Mouse => {
                geometry = self.place_mouse(screen_info, &geometry, mouse_x, mouse_y)?;
            }
            PlacementPolicy::Random => {
                geometry = self.place_random(screen_info, &geometry)?;
            }
            PlacementPolicy::RespectInitialPosition => {
                // Use existing geometry (already set from hints)
                // Just constrain to work area
                geometry.x = geometry.x.max(work_area.x);
                geometry.y = geometry.y.max(work_area.y);
            }
        }
        
        // Constrain to work area
        geometry.x = geometry.x.max(work_area.x);
        geometry.y = geometry.y.max(work_area.y);
        geometry.width = geometry.width.min(work_area.width);
        geometry.height = geometry.height.min(work_area.height);
        
        client.geometry = geometry;
        
        Ok(geometry)
    }
    
    /// Smart placement (avoid overlapping windows)
    fn place_smart(
        &self,
        screen_info: &ScreenInfo,
        geometry: &Geometry,
        existing_clients: &std::collections::HashMap<u32, Client>,
    ) -> Result<Geometry> {
        let work_area = &screen_info.work_area;
        let mut best_x = work_area.x;
        let mut best_y = work_area.y;
        let mut best_score = i32::MAX;
        
        // Try positions in a grid
        let step_x = (geometry.width / 4).max(10) as i32;
        let step_y = (geometry.height / 4).max(10) as i32;
        
        for y in (work_area.y..work_area.y + work_area.height as i32).step_by(step_y as usize) {
            for x in (work_area.x..work_area.x + work_area.width as i32).step_by(step_x as usize) {
                let test_geom = Geometry {
                    x,
                    y,
                    width: geometry.width,
                    height: geometry.height,
                };
                
                // Check for overlaps
                let mut overlaps = false;
                for client in existing_clients.values() {
                    if client.mapped() && self.geometries_overlap(&test_geom, &client.geometry) {
                        overlaps = true;
                        break;
                    }
                }
                
                if !overlaps {
                    // Score based on distance from top-left (prefer top-left positions)
                    let score = x + y;
                    if score < best_score {
                        best_score = score;
                        best_x = x;
                        best_y = y;
                    }
                }
            }
        }
        
        Ok(Geometry {
            x: best_x,
            y: best_y,
            width: geometry.width,
            height: geometry.height,
        })
    }
    
    /// Center placement
    fn place_center(
        &self,
        screen_info: &ScreenInfo,
        geometry: &Geometry,
    ) -> Result<Geometry> {
        let work_area = &screen_info.work_area;
        
        Ok(Geometry {
            x: work_area.x + (work_area.width as i32 - geometry.width as i32) / 2,
            y: work_area.y + (work_area.height as i32 - geometry.height as i32) / 2,
            width: geometry.width,
            height: geometry.height,
        })
    }
    
    /// Mouse placement (at cursor position)
    fn place_mouse(
        &self,
        screen_info: &ScreenInfo,
        geometry: &Geometry,
        mouse_x: Option<i16>,
        mouse_y: Option<i16>,
    ) -> Result<Geometry> {
        let work_area = &screen_info.work_area;
        
        let x = mouse_x.map(|x| x as i32).unwrap_or(work_area.x);
        let y = mouse_y.map(|y| y as i32).unwrap_or(work_area.y);
        
        // Center window on cursor
        Ok(Geometry {
            x: x - (geometry.width as i32 / 2),
            y: y - (geometry.height as i32 / 2),
            width: geometry.width,
            height: geometry.height,
        })
    }
    
    /// Random placement
    fn place_random(
        &self,
        screen_info: &ScreenInfo,
        geometry: &Geometry,
    ) -> Result<Geometry> {
        let work_area = &screen_info.work_area;
        
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::time::{SystemTime, UNIX_EPOCH};
        
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        let hash = hasher.finish();
        
        let max_x = work_area.width.saturating_sub(geometry.width);
        let max_y = work_area.height.saturating_sub(geometry.height);
        
        Ok(Geometry {
            x: work_area.x + ((hash % max_x as u64) as i32),
            y: work_area.y + (((hash >> 32) % max_y as u64) as i32),
            width: geometry.width,
            height: geometry.height,
        })
    }
    
    /// Check if two geometries overlap
    fn geometries_overlap(&self, a: &Geometry, b: &Geometry) -> bool {
        !(a.x + a.width as i32 <= b.x ||
          b.x + b.width as i32 <= a.x ||
          a.y + a.height as i32 <= b.y ||
          b.y + b.height as i32 <= a.y)
    }
}

impl Default for PlacementManager {
    fn default() -> Self {
        Self::new(PlacementPolicy::Smart)
    }
}

