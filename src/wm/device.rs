//! Device Module
//!
//! XInput2 support, multi-touch, and tablet input.
//! This matches xfwm4's device management.

use anyhow::Result;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::rust_connection::RustConnection;

use crate::wm::display::DisplayInfo;
use crate::wm::screen::ScreenInfo;

/// Input device
#[derive(Debug, Clone)]
pub struct InputDevice {
    /// Device ID
    pub device_id: u8,
    
    /// Device name
    pub name: String,
    
    /// Device type
    pub device_type: DeviceType,
}

/// Device type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    /// Master pointer
    MasterPointer,
    /// Master keyboard
    MasterKeyboard,
    /// Slave pointer
    SlavePointer,
    /// Slave keyboard
    SlaveKeyboard,
    /// Floating slave
    FloatingSlave,
}

/// Device manager
pub struct DeviceManager {
    /// Available devices
    pub devices: Vec<InputDevice>,
    
    /// XInput2 enabled
    pub xinput2_enabled: bool,
}

impl DeviceManager {
    /// Create a new device manager
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            xinput2_enabled: false,
        }
    }
    
    /// Initialize XInput2
    pub fn initialize_xinput2(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
    ) -> Result<()> {
        if display_info.extensions.have_xinput2 {
            debug!("XInput2 extension available");
            self.xinput2_enabled = true;
            
            // TODO: Query XInput2 devices
            // This requires xinput2 extension bindings
        } else {
            warn!("XInput2 extension not available");
        }
        
        Ok(())
    }
    
    /// Get device list
    pub fn get_devices(&self) -> &[InputDevice] {
        &self.devices
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}



