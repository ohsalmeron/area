//! Window decorations (titlebars, buttons) for Area WM

#![allow(dead_code)]

use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;

const TITLEBAR_HEIGHT: u16 = 28;
const BUTTON_SIZE: u16 = 20;
const BUTTON_PADDING: u16 = 4;

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
            2, // border width
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(0x2a2a2a)
                .border_pixel(0x4a90d9)
                .event_mask(
                    EventMask::SUBSTRUCTURE_REDIRECT
                        | EventMask::SUBSTRUCTURE_NOTIFY
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::POINTER_MOTION,
                ),
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
                .background_pixel(0x1e1e1e)
                .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE),
        )?;

        // Create close button (red)
        let close_x = width - BUTTON_SIZE - BUTTON_PADDING;
        conn.create_window(
            screen.root_depth,
            close_button,
            titlebar,
            close_x as i16,
            BUTTON_PADDING as i16,
            BUTTON_SIZE,
            BUTTON_SIZE,
            1,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(0xcc0000)
                .border_pixel(0x880000)
                .event_mask(EventMask::BUTTON_PRESS),
        )?;

        // Create maximize button (green)
        let max_x = close_x - BUTTON_SIZE - BUTTON_PADDING;
        conn.create_window(
            screen.root_depth,
            maximize_button,
            titlebar,
            max_x as i16,
            BUTTON_PADDING as i16,
            BUTTON_SIZE,
            BUTTON_SIZE,
            1,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(0x00cc00)
                .border_pixel(0x008800)
                .event_mask(EventMask::BUTTON_PRESS),
        )?;

        // Create minimize button (yellow)
        let min_x = max_x - BUTTON_SIZE - BUTTON_PADDING;
        conn.create_window(
            screen.root_depth,
            minimize_button,
            titlebar,
            min_x as i16,
            BUTTON_PADDING as i16,
            BUTTON_SIZE,
            BUTTON_SIZE,
            1,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(0xcccc00)
                .border_pixel(0x888800)
                .event_mask(EventMask::BUTTON_PRESS),
        )?;

        // Reparent client into frame
        conn.reparent_window(client, frame, 0, TITLEBAR_HEIGHT as i16)?;

        // Map all windows
        conn.map_window(close_button)?;
        conn.map_window(maximize_button)?;
        conn.map_window(minimize_button)?;
        conn.map_window(titlebar)?;
        conn.map_window(frame)?;

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
