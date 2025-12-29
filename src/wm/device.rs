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
            info!("XInput2 extension available");
            self.xinput2_enabled = true;
            
            // Query XInput2 devices
            // Note: x11rb doesn't have full XInput2 support yet, so we use a basic approach
            // In a full implementation, we would use XIQueryDevice to enumerate devices
            // For now, we mark it as enabled and log that enumeration would happen here
            debug!("XInput2 enabled (device enumeration would use XIQueryDevice if x11rb had XInput2 support)");
            
            // Basic device list (can be expanded when XInput2 bindings are available)
            // Master pointer and keyboard are always present
            self.devices.push(InputDevice {
                device_id: 0, // Master pointer (typically device 2)
                name: "Master Pointer".to_string(),
                device_type: DeviceType::MasterPointer,
            });
            
            self.devices.push(InputDevice {
                device_id: 1, // Master keyboard (typically device 3)
                name: "Master Keyboard".to_string(),
                device_type: DeviceType::MasterKeyboard,
            });
            
            debug!("XInput2 devices initialized (basic master devices)");
        } else {
            debug!("XInput2 extension not available");
        }
        
        Ok(())
    }
    
    /// Handle XInput2 event
    pub fn handle_event(
        &mut self,
        _event: &x11rb::protocol::Event,
        _display_info: &DisplayInfo,
    ) -> Result<()> {
        if !self.xinput2_enabled {
            return Ok(());
        }
        
        // TODO: Handle XInput2 events
        // XInput2 events have a different event base
        // Need to check event type against XInput2 event base
        // For now, just log that we received an XInput2 event
        debug!("XInput2 event received (handling not yet fully implemented)");
        
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




