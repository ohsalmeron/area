//! Window decorations (titlebars, buttons) for Area WM


use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

const TITLEBAR_HEIGHT: u16 = 32;
const BUTTON_SIZE: u16 = 16;
const BUTTON_PADDING: u16 = 8;
const BORDER_WIDTH: u16 = 2;

// Nord Theme Colors
const COLOR_BG: u32 = 0x2e3440;      // Polar Night Darkest
const COLOR_TITLEBAR: u32 = 0x3b4252; // Polar Night Lighter
const COLOR_BORDER: u32 = 0x5e81ac;   // Frost Blue
const COLOR_CLOSE: u32 = 0xbf616a;    // Aurora Red
const COLOR_MAX: u32 = 0xa3be8c;      // Aurora Green
const COLOR_MIN: u32 = 0xebcb8b;      // Aurora Yellow

/// Represents a window frame with decorations
#[derive(Debug, Clone)]
pub struct WindowFrame {
    pub client: Window,
    pub frame: Window,
    pub titlebar: Window,
    pub close_button: Window,
    pub maximize_button: Window,
    pub minimize_button: Window,
}

impl WindowFrame {
    /// Reconstruct a WindowFrame from stored IDs
    pub fn from_state(client: Window, state: &crate::shared::window_state::WindowFrame) -> Self {
        Self {
            client,
            frame: state.frame,
            titlebar: state.titlebar,
            close_button: state.close_button,
            maximize_button: state.maximize_button,
            minimize_button: state.minimize_button,
        }
    }

    /// Create a new window frame for a client window
    pub fn new(
        conn: &RustConnection,
        screen: &Screen,
        client: Window,
        x: i16,
        y: i16,
        width: u16,
        height: u16,
    ) -> Result<Self> {
        let frame = conn.generate_id()?;
        let titlebar = conn.generate_id()?;
        let close_button = conn.generate_id()?;
        let maximize_button = conn.generate_id()?;
        let minimize_button = conn.generate_id()?;

        // Create frame window
        conn.create_window(
            screen.root_depth,
            frame,
            screen.root,
            x,
            y,
            width,
            height + TITLEBAR_HEIGHT,
            BORDER_WIDTH, 
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(COLOR_BG)
                .border_pixel(COLOR_BORDER)
                .event_mask(
                    EventMask::SUBSTRUCTURE_REDIRECT
                        | EventMask::SUBSTRUCTURE_NOTIFY
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::POINTER_MOTION,
                )
                .override_redirect(1),
        )?;

        // Create titlebar
        conn.create_window(
            screen.root_depth,
            titlebar,
            frame,
            0,
            0,
            width,
            TITLEBAR_HEIGHT,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(COLOR_TITLEBAR)
                .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE),
        )?;

        // Create close button
        // Use i32 for calculations to avoid underflow on small windows
        let width_i32 = width as i32;
        let btn_size = BUTTON_SIZE as i32;
        let pad = BUTTON_PADDING as i32;

        let close_x = width_i32 - btn_size - pad;
        let btn_y = (TITLEBAR_HEIGHT - BUTTON_SIZE) / 2;
        conn.create_window(
            screen.root_depth,
            close_button,
            titlebar,
            close_x as i16,
            btn_y as i16,
            BUTTON_SIZE,
            BUTTON_SIZE,
            0, // No border for buttons (flat look)
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(COLOR_CLOSE)
                .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE),
        )?;

        // Create maximize button
        let max_x = close_x - btn_size - pad;
        conn.create_window(
            screen.root_depth,
            maximize_button,
            titlebar,
            max_x as i16,
            btn_y as i16,
            BUTTON_SIZE,
            BUTTON_SIZE,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(COLOR_MAX)
                .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE),
        )?;

        // Create minimize button
        let min_x = max_x - btn_size - pad;
        conn.create_window(
            screen.root_depth,
            minimize_button,
            titlebar,
            min_x as i16,
            btn_y as i16,
            BUTTON_SIZE,
            BUTTON_SIZE,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(COLOR_MIN)
                .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE),
        )?;

        // Reparent client into frame
        conn.reparent_window(client, frame, 0, TITLEBAR_HEIGHT as i16)?;
        
        // Map all windows (frame first, then client)
        conn.map_window(frame)?;
        conn.map_window(close_button)?;
        conn.map_window(maximize_button)?;
        conn.map_window(minimize_button)?;
        conn.map_window(titlebar)?;
        // Map the client window so it's visible
        conn.map_window(client)?;

        Ok(Self {
            client,
            frame,
            titlebar,
            close_button,
            maximize_button,
            minimize_button,
        })
    }

    /// Check if a window ID belongs to this frame
    pub fn contains(&self, window: Window) -> bool {
        window == self.frame
            || window == self.titlebar
            || window == self.close_button
            || window == self.maximize_button
            || window == self.minimize_button
    }

    /// Get the button type if window is a button
    pub fn get_button_type(&self, window: Window) -> Option<ButtonType> {
        if window == self.close_button {
            Some(ButtonType::Close)
        } else if window == self.maximize_button {
            Some(ButtonType::Maximize)
        } else if window == self.minimize_button {
            Some(ButtonType::Minimize)
        } else {
            None
        }
    }

    /// Resize the frame and client
    pub fn resize(&self, conn: &RustConnection, width: u16, height: u16) -> Result<()> {
        conn.configure_window(
            self.frame,
            &ConfigureWindowAux::new()
                .width(width as u32)
                .height((height + TITLEBAR_HEIGHT) as u32),
        )?;
        conn.configure_window(
            self.titlebar,
            &ConfigureWindowAux::new().width(width as u32),
        )?;
        conn.configure_window(
            self.client,
            &ConfigureWindowAux::new()
                .width(width as u32)
                .height(height as u32),
        )?;

        // Reposition buttons
        let close_x = width - BUTTON_SIZE - BUTTON_PADDING;
        let max_x = close_x - BUTTON_SIZE - BUTTON_PADDING;
        let min_x = max_x - BUTTON_SIZE - BUTTON_PADDING;

        conn.configure_window(
            self.close_button,
            &ConfigureWindowAux::new().x(close_x as i32),
        )?;
        conn.configure_window(
            self.maximize_button,
            &ConfigureWindowAux::new().x(max_x as i32),
        )?;
        conn.configure_window(
            self.minimize_button,
            &ConfigureWindowAux::new().x(min_x as i32),
        )?;

        Ok(())
    }

    /// Move the frame
    pub fn move_to(&self, conn: &RustConnection, x: i16, y: i16) -> Result<()> {
        conn.configure_window(
            self.frame,
            &ConfigureWindowAux::new().x(x as i32).y(y as i32),
        )?;
        Ok(())
    }

    /// Destroy the frame and unparent the client
    pub fn destroy(&self, conn: &RustConnection, root: Window) -> Result<()> {
        conn.reparent_window(self.client, root, 0, 0)?;
        conn.destroy_window(self.frame)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonType {
    Close,
    Maximize,
    Minimize,
}
