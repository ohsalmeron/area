//! Shell rendering utilities
//!
//! Shell UI rendering helpers (coordinates, hit testing, etc.)

/// Check if point is inside rectangle
pub fn point_in_rect(x: f32, y: f32, rect_x: f32, rect_y: f32, rect_w: f32, rect_h: f32) -> bool {
    x >= rect_x && x < rect_x + rect_w && y >= rect_y && y < rect_y + rect_h
}

