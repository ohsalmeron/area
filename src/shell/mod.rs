//! Shell UI Module
//!
//! Built-in desktop shell elements rendered directly by the compositor.
//! This includes the panel, logout dialog, and other shell UI.

pub mod panel;
pub mod logout;
pub mod render;

use anyhow::Result;

/// Shell state
pub struct Shell {
    /// Panel state
    pub panel: panel::Panel,
    
    /// Logout dialog state
    pub logout_dialog: logout::LogoutDialog,
}

impl Shell {
    /// Create a new shell
    pub fn new(screen_width: u16, screen_height: u16, panel_config: crate::config::PanelConfig) -> Self {
        Self {
            panel: panel::Panel::new(screen_width, screen_height, panel_config),
            logout_dialog: logout::LogoutDialog::new(),
        }
    }
    
    /// Handle mouse click
    pub async fn handle_click(&mut self, x: i16, y: i16, power: &Option<crate::dbus::power::PowerService>) -> Result<()> {
        // Check if click is on logout dialog first (it's on top)
        if self.logout_dialog.visible {
            if self.logout_dialog.handle_click(x, y, power).await? {
                return Ok(());
            }
        }
        
        // Check if click is on panel
        match self.panel.handle_click(x, y, &mut self.logout_dialog)? {
            panel::PanelClickAction::LaunchApp | panel::PanelClickAction::Logout => {
                return Ok(());
            }
            panel::PanelClickAction::None => {}
        }
        
        Ok(())
    }
    
    /// Update shell state (called every frame)
    /// Future: Add animations, clock updates, etc. here
    pub fn update(&self) {
        // Panel and logout dialog updates will be added here when needed
    }
    
    /// Update screen size (called when screen resolution changes)
    pub fn set_screen_size(&mut self, width: u16, height: u16) {
        self.panel.set_screen_size(width, height);
        self.logout_dialog.set_screen_size(width, height);
    }
}



