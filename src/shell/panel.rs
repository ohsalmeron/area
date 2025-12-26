//! Panel (top/bottom bar) implementation

use anyhow::Result;
use crate::shell::logout::LogoutDialog;
use crate::shell::render;

/// Panel configuration
const PANEL_HEIGHT: f32 = 40.0;
const BUTTON_WIDTH: f32 = 80.0;
const BUTTON_HEIGHT: f32 = 30.0;
const BUTTON_PADDING: f32 = 5.0;

/// Panel state
pub struct Panel {
    /// Screen dimensions
    screen_width: u16,
    screen_height: u16,
    
    /// Panel position (true = top, false = bottom)
    position_top: bool,
    
    /// Logout button position
    logout_button_x: f32,
    logout_button_y: f32,
}

impl Panel {
    pub fn new(screen_width: u16, screen_height: u16) -> Self {
        // Position panel at top
        let position_top = true;
        let y = if position_top { 0.0 } else { screen_height as f32 - PANEL_HEIGHT };
        
        // Position logout button on the right
        let logout_button_x = screen_width as f32 - BUTTON_WIDTH - BUTTON_PADDING;
        let logout_button_y = y + (PANEL_HEIGHT - BUTTON_HEIGHT) / 2.0;
        
        Self {
            screen_width,
            screen_height,
            position_top,
            logout_button_x,
            logout_button_y,
        }
    }
    
    /// Handle mouse click on panel
    pub fn handle_click(&self, x: i16, y: i16, logout_dialog: &mut LogoutDialog) -> Result<bool> {
        let fx = x as f32;
        let fy = y as f32;
        
        // Check if click is on logout button
        if render::point_in_rect(
            fx,
            fy,
            self.logout_button_x,
            self.logout_button_y,
            BUTTON_WIDTH,
            BUTTON_HEIGHT,
        ) {
            logout_dialog.show();
            return Ok(true);
        }
        
        Ok(false)
    }
    
    /// Update panel state
    pub fn update(&mut self) {
        // Update clock, etc. in the future
    }
    
    /// Render the panel using the renderer
    pub fn render(&self, renderer: &crate::compositor::renderer::Renderer, screen_width: f32, screen_height: f32) {
        let y = if self.position_top { 0.0 } else { self.screen_height as f32 - PANEL_HEIGHT };
        
        // Render panel background
        renderer.render_rectangle(
            0.0,
            y,
            self.screen_width as f32,
            PANEL_HEIGHT,
            screen_width,
            screen_height,
            0.2,  // r
            0.2,  // g
            0.2,  // b
            0.9,  // a (semi-transparent)
        );
        
        // Render logout button background
        renderer.render_rectangle(
            self.logout_button_x,
            self.logout_button_y,
            BUTTON_WIDTH,
            BUTTON_HEIGHT,
            screen_width,
            screen_height,
            0.4,  // r
            0.2,  // g
            0.2,  // b
            0.9,  // a
        );
        
        // Render logout button border (simple approach: render 4 rectangles)
        let border_width = 2.0;
        renderer.render_rectangle(
            self.logout_button_x,
            self.logout_button_y,
            BUTTON_WIDTH,
            border_width,
            screen_width,
            screen_height,
            0.6, 0.3, 0.3, 1.0,  // top border
        );
        renderer.render_rectangle(
            self.logout_button_x,
            self.logout_button_y + BUTTON_HEIGHT - border_width,
            BUTTON_WIDTH,
            border_width,
            screen_width,
            screen_height,
            0.6, 0.3, 0.3, 1.0,  // bottom border
        );
        renderer.render_rectangle(
            self.logout_button_x,
            self.logout_button_y,
            border_width,
            BUTTON_HEIGHT,
            screen_width,
            screen_height,
            0.6, 0.3, 0.3, 1.0,  // left border
        );
        renderer.render_rectangle(
            self.logout_button_x + BUTTON_WIDTH - border_width,
            self.logout_button_y,
            border_width,
            BUTTON_HEIGHT,
            screen_width,
            screen_height,
            0.6, 0.3, 0.3, 1.0,  // right border
        );
        
        // TODO: Render "Logout" text on button
        // For now, the button is just a red rectangle
    }
    
    /// Get panel height
    pub fn height(&self) -> f32 {
        PANEL_HEIGHT
    }
    
    /// Check if point is on panel
    pub fn contains_point(&self, _x: i16, y: i16) -> bool {
        let panel_y = if self.position_top { 0.0 } else { self.screen_height as f32 - PANEL_HEIGHT };
        let fy = y as f32;
        fy >= panel_y && fy < panel_y + PANEL_HEIGHT
    }
    
    /// Update screen size (called when screen resolution changes)
    pub fn set_screen_size(&mut self, width: u16, height: u16) {
        self.screen_width = width;
        self.screen_height = height;
        
        // Recalculate button positions
        let y = if self.position_top { 0.0 } else { height as f32 - PANEL_HEIGHT };
        self.logout_button_x = width as f32 - BUTTON_WIDTH - BUTTON_PADDING;
        self.logout_button_y = y + (PANEL_HEIGHT - BUTTON_HEIGHT) / 2.0;
    }
}

