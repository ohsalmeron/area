//! Logout dialog implementation

use anyhow::Result;
use std::process::Command;
use crate::shell::render;

/// Dialog configuration
const DIALOG_WIDTH: f32 = 300.0;
const DIALOG_HEIGHT: f32 = 150.0;
const BUTTON_WIDTH: f32 = 100.0;
const BUTTON_HEIGHT: f32 = 35.0;
const BUTTON_SPACING: f32 = 20.0;

/// Logout dialog state
pub struct LogoutDialog {
    /// Is dialog visible?
    pub visible: bool,
    
    /// Dialog position (centered)
    dialog_x: f32,
    dialog_y: f32,
    
    /// Logout button position
    logout_button_x: f32,
    logout_button_y: f32,
    
    /// Cancel button position
    cancel_button_x: f32,
    cancel_button_y: f32,
    
    /// Screen dimensions (for centering)
    screen_width: u16,
    screen_height: u16,
}

impl LogoutDialog {
    pub fn new() -> Self {
        // Will be initialized when shown
        Self {
            visible: false,
            dialog_x: 0.0,
            dialog_y: 0.0,
            logout_button_x: 0.0,
            logout_button_y: 0.0,
            cancel_button_x: 0.0,
            cancel_button_y: 0.0,
            screen_width: 1920,
            screen_height: 1080,
        }
    }
    
    /// Show the dialog
    pub fn show(&mut self) {
        self.visible = true;
        self.update_positions();
    }
    
    /// Hide the dialog
    pub fn hide(&mut self) {
        self.visible = false;
    }
    
    /// Update button positions (call when screen size changes)
    pub fn update_positions(&mut self) {
        // Center dialog
        self.dialog_x = (self.screen_width as f32 - DIALOG_WIDTH) / 2.0;
        self.dialog_y = (self.screen_height as f32 - DIALOG_HEIGHT) / 2.0;
        
        // Position buttons
        let button_y = self.dialog_y + DIALOG_HEIGHT - BUTTON_HEIGHT - 20.0;
        let total_buttons_width = BUTTON_WIDTH * 2.0 + BUTTON_SPACING;
        let start_x = self.dialog_x + (DIALOG_WIDTH - total_buttons_width) / 2.0;
        
        self.logout_button_x = start_x;
        self.logout_button_y = button_y;
        
        self.cancel_button_x = start_x + BUTTON_WIDTH + BUTTON_SPACING;
        self.cancel_button_y = button_y;
    }
    
    /// Set screen dimensions
    pub fn set_screen_size(&mut self, width: u16, height: u16) {
        self.screen_width = width;
        self.screen_height = height;
        if self.visible {
            self.update_positions();
        }
    }
    
    /// Handle mouse click
    pub fn handle_click(&mut self, x: i16, y: i16) -> Result<bool> {
        if !self.visible {
            return Ok(false);
        }
        
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
            self.perform_logout()?;
            return Ok(true);
        }
        
        // Check if click is on cancel button
        if render::point_in_rect(
            fx,
            fy,
            self.cancel_button_x,
            self.cancel_button_y,
            BUTTON_WIDTH,
            BUTTON_HEIGHT,
        ) {
            self.hide();
            return Ok(true);
        }
        
        // Check if click is outside dialog (close dialog)
        if !render::point_in_rect(
            fx,
            fy,
            self.dialog_x,
            self.dialog_y,
            DIALOG_WIDTH,
            DIALOG_HEIGHT,
        ) {
            self.hide();
            return Ok(true);
        }
        
        Ok(false)
    }
    
    /// Perform logout
    fn perform_logout(&self) -> Result<()> {
        // Try loginctl first (most reliable for systemd sessions)
        let result = Command::new("loginctl")
            .arg("terminate-session")
            .arg("")
            .output();
        
        match result {
            Ok(output) if output.status.success() => {
                // Success - session will terminate
                tracing::info!("Logout command executed successfully");
            }
            Ok(_) | Err(_) => {
                // loginctl failed, try systemctl as fallback
                let fallback = Command::new("systemctl")
                    .arg("--user")
                    .arg("stop")
                    .arg("area-desktop.target")
                    .output();
                
                match fallback {
                    Ok(_) => {
                        tracing::info!("Logout fallback command executed");
                    }
                    Err(e) => {
                        tracing::error!("Failed to logout: {}", e);
                        return Err(anyhow::anyhow!("Failed to logout: {}", e));
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Update dialog state
    pub fn update(&mut self) {
        // Future: animations, etc.
    }
    
    /// Render the dialog using the renderer
    pub fn render(&self, renderer: &crate::compositor::renderer::Renderer, screen_width: f32, screen_height: f32) {
        if !self.visible {
            return;
        }
        
        let border_width = 2.0;
        
        // Render dialog background
        renderer.render_rectangle(
            self.dialog_x,
            self.dialog_y,
            DIALOG_WIDTH,
            DIALOG_HEIGHT,
            screen_width,
            screen_height,
            0.15, 0.15, 0.15, 0.95,
        );
        
        // Render dialog border
        renderer.render_rectangle(self.dialog_x, self.dialog_y, DIALOG_WIDTH, border_width, screen_width, screen_height, 0.4, 0.4, 0.4, 1.0); // top
        renderer.render_rectangle(self.dialog_x, self.dialog_y + DIALOG_HEIGHT - border_width, DIALOG_WIDTH, border_width, screen_width, screen_height, 0.4, 0.4, 0.4, 1.0); // bottom
        renderer.render_rectangle(self.dialog_x, self.dialog_y, border_width, DIALOG_HEIGHT, screen_width, screen_height, 0.4, 0.4, 0.4, 1.0); // left
        renderer.render_rectangle(self.dialog_x + DIALOG_WIDTH - border_width, self.dialog_y, border_width, DIALOG_HEIGHT, screen_width, screen_height, 0.4, 0.4, 0.4, 1.0); // right
        
        // Render logout button (red)
        renderer.render_rectangle(
            self.logout_button_x,
            self.logout_button_y,
            BUTTON_WIDTH,
            BUTTON_HEIGHT,
            screen_width,
            screen_height,
            0.6, 0.2, 0.2, 0.9,
        );
        renderer.render_rectangle(self.logout_button_x, self.logout_button_y, BUTTON_WIDTH, border_width, screen_width, screen_height, 0.8, 0.3, 0.3, 1.0); // top border
        renderer.render_rectangle(self.logout_button_x, self.logout_button_y + BUTTON_HEIGHT - border_width, BUTTON_WIDTH, border_width, screen_width, screen_height, 0.8, 0.3, 0.3, 1.0); // bottom border
        renderer.render_rectangle(self.logout_button_x, self.logout_button_y, border_width, BUTTON_HEIGHT, screen_width, screen_height, 0.8, 0.3, 0.3, 1.0); // left border
        renderer.render_rectangle(self.logout_button_x + BUTTON_WIDTH - border_width, self.logout_button_y, border_width, BUTTON_HEIGHT, screen_width, screen_height, 0.8, 0.3, 0.3, 1.0); // right border
        
        // Render cancel button (gray)
        renderer.render_rectangle(
            self.cancel_button_x,
            self.cancel_button_y,
            BUTTON_WIDTH,
            BUTTON_HEIGHT,
            screen_width,
            screen_height,
            0.3, 0.3, 0.3, 0.9,
        );
        renderer.render_rectangle(self.cancel_button_x, self.cancel_button_y, BUTTON_WIDTH, border_width, screen_width, screen_height, 0.5, 0.5, 0.5, 1.0); // top border
        renderer.render_rectangle(self.cancel_button_x, self.cancel_button_y + BUTTON_HEIGHT - border_width, BUTTON_WIDTH, border_width, screen_width, screen_height, 0.5, 0.5, 0.5, 1.0); // bottom border
        renderer.render_rectangle(self.cancel_button_x, self.cancel_button_y, border_width, BUTTON_HEIGHT, screen_width, screen_height, 0.5, 0.5, 0.5, 1.0); // left border
        renderer.render_rectangle(self.cancel_button_x + BUTTON_WIDTH - border_width, self.cancel_button_y, border_width, BUTTON_HEIGHT, screen_width, screen_height, 0.5, 0.5, 0.5, 1.0); // right border
        
        // TODO: Render text ("Logout", "Cancel", "Are you sure?")
        // For now, buttons are just colored rectangles
    }
}

