//! Input device management using XInput extension
//!
//! Handles mouse acceleration, profiles, and other input device settings.

use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as XInputExt};
use x11rb::protocol::xproto::{Atom, PropMode, ConnectionExt as XProtoExt};
use x11rb::rust_connection::RustConnection;
use crate::config::MouseConfig;

/// Input device manager
pub struct InputManager {
    conn: Arc<RustConnection>,
    float_atom: Atom,
    accel_speed_atom: Option<Atom>,
    accel_profile_atom: Option<Atom>,
    left_handed_atom: Option<Atom>,
}

impl InputManager {
    /// Create a new InputManager
    pub fn new(conn: Arc<RustConnection>) -> Result<Self> {
        // Query XInput extension version
        use x11rb::protocol::xinput::xi_query_version;
        let version_cookie = xi_query_version(conn.as_ref(), 2, 3)?;
        let version_reply = version_cookie.reply()
            .context("Failed to query XInput version")?;
        
        info!("XInput extension version: {}.{}", 
            version_reply.major_version, version_reply.minor_version);
        
        // Intern atoms we'll need
        let float_atom = conn.as_ref().intern_atom(false, b"FLOAT")?
            .reply()
            .context("Failed to intern FLOAT atom")?
            .atom;
        
        // Try to intern libinput atoms (may fail if libinput not available)
        let accel_speed_atom = conn.as_ref().intern_atom(false, b"libinput Accel Speed")
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.atom);
        
        let accel_profile_atom = conn.as_ref().intern_atom(false, b"libinput Accel Profile Enabled")
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.atom);
        
        let left_handed_atom = conn.as_ref().intern_atom(false, b"libinput Left Handed Enabled")
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .map(|reply| reply.atom);
        
        if accel_speed_atom.is_none() {
            warn!("libinput atoms not available - input configuration may not work");
        }
        
        Ok(Self {
            conn,
            float_atom,
            accel_speed_atom,
            accel_profile_atom,
            left_handed_atom,
        })
    }
    
    /// List all pointer devices
    pub fn list_pointer_devices(&self) -> Result<Vec<u16>> {
        let devices_cookie = self.conn.as_ref().xinput_list_input_devices()?;
        let devices_reply = devices_cookie.reply()
            .context("Failed to list input devices")?;
        
        let mut pointer_devices = Vec::new();
        
        // Note: xinput_list_input_devices returns old XInput format (DeviceInfo)
        // For XI2 devices, we need to use xi_query_device, but that requires device IDs
        // For now, we'll try to use the old API and convert device_id (u8) to u16
        // TODO: Migrate to XI2 API (xi_query_device) for better device enumeration
        for device_info in &devices_reply.devices {
            // Check if device is a pointer using device_use field
            let is_pointer = device_info.device_use == xinput::DeviceUse::IS_X_POINTER
                || device_info.device_use == xinput::DeviceUse::IS_X_EXTENSION_POINTER;
            
            if is_pointer {
                // device_id is u8, convert to u16 for consistency
                let device_id = device_info.device_id as u16;
                pointer_devices.push(device_id);
                debug!("Found pointer device: id={}, use={:?}", 
                    device_id,
                    device_info.device_use);
            }
        }
        
        info!("Found {} pointer device(s)", pointer_devices.len());
        Ok(pointer_devices)
    }
    
    /// Set libinput acceleration speed for a device
    pub fn set_libinput_accel_speed(&self, device_id: u16, speed: f32) -> Result<()> {
        let accel_atom = match self.accel_speed_atom {
            Some(atom) => atom,
            None => {
                warn!("libinput Accel Speed atom not available, skipping");
                return Ok(());
            }
        };
        
        // Convert f32 to u32 bits for FLOAT property
        let speed_u32 = speed.to_bits();
        
        debug!("Setting libinput Accel Speed to {} for device {}", speed, device_id);
        
        use x11rb::protocol::xinput::{xi_change_property, XIChangePropertyAux};
        let aux = XIChangePropertyAux::Data32(vec![speed_u32]);
        xi_change_property(
            self.conn.as_ref(),
            device_id,
            PropMode::REPLACE,
            accel_atom,
            self.float_atom,
            1,  // Number of items
            &aux,
        )?;
        
        self.conn.as_ref().flush()?;
        Ok(())
    }
    
    /// Set libinput acceleration profile for a device
    pub fn set_libinput_accel_profile(&self, device_id: u16, profile: &str) -> Result<()> {
        let profile_atom = match self.accel_profile_atom {
            Some(atom) => atom,
            None => {
                warn!("libinput Accel Profile Enabled atom not available, skipping");
                return Ok(());
            }
        };
        
        // Profile is represented as array of 3 booleans: [adaptive, flat, custom]
        let profile_array: [u8; 3] = match profile {
            "adaptive" => [1, 0, 0],
            "flat" => [0, 1, 0],
            "custom" => [0, 0, 1],
            _ => {
                warn!("Unknown acceleration profile: {}, using adaptive", profile);
                [1, 0, 0]
            }
        };
        
        debug!("Setting libinput Accel Profile to {:?} for device {}", profile_array, device_id);
        
        // Intern INTEGER atom
        let integer_atom = self.conn.as_ref().intern_atom(false, b"INTEGER")?
            .reply()
            .context("Failed to intern INTEGER atom")?
            .atom;
        
        use x11rb::protocol::xinput::{xi_change_property, XIChangePropertyAux};
        let aux = XIChangePropertyAux::Data8(profile_array.to_vec());
        xi_change_property(
            self.conn.as_ref(),
            device_id,
            PropMode::REPLACE,
            profile_atom,
            integer_atom,
            3, // Number of items
            &aux,
        )?;
        
        self.conn.as_ref().flush()?;
        Ok(())
    }
    
    /// Set libinput left-handed mode for a device
    pub fn set_libinput_left_handed(&self, device_id: u16, enabled: bool) -> Result<()> {
        let left_handed_atom = match self.left_handed_atom {
            Some(atom) => atom,
            None => {
                warn!("libinput Left Handed Enabled atom not available, skipping");
                return Ok(());
            }
        };
        
        let enabled_byte = if enabled { 1u8 } else { 0u8 };
        
        debug!("Setting libinput Left Handed Enabled to {} for device {}", enabled, device_id);
        
        // Intern INTEGER atom
        let integer_atom = self.conn.as_ref().intern_atom(false, b"INTEGER")?
            .reply()
            .context("Failed to intern INTEGER atom")?
            .atom;
        
        use x11rb::protocol::xinput::{xi_change_property, XIChangePropertyAux};
        let aux = XIChangePropertyAux::Data8(vec![enabled_byte]);
        xi_change_property(
            self.conn.as_ref(),
            device_id,
            PropMode::REPLACE,
            left_handed_atom,
            integer_atom,
            1, // Number of items
            &aux,
        )?;
        
        self.conn.as_ref().flush()?;
        Ok(())
    }
    
    /// Apply mouse configuration to all pointer devices
    pub fn apply_mouse_config(&self, config: &MouseConfig) -> Result<()> {
        let devices = self.list_pointer_devices()?;
        
        if devices.is_empty() {
            warn!("No pointer devices found, skipping mouse configuration");
            return Ok(());
        }
        
        let mut success_count = 0;
        let mut fail_count = 0;
        
        for device_id in devices {
            // Apply acceleration speed
            if let Some(speed) = config.accel_speed {
                if let Err(e) = self.set_libinput_accel_speed(device_id, speed) {
                    warn!("Failed to set acceleration speed for device {}: {}", device_id, e);
                    fail_count += 1;
                    continue;
                }
                success_count += 1;
            }
            
            // Apply acceleration profile
            if let Some(ref profile) = config.accel_profile {
                if let Err(e) = self.set_libinput_accel_profile(device_id, profile) {
                    warn!("Failed to set acceleration profile for device {}: {}", device_id, e);
                    fail_count += 1;
                    continue;
                }
            }
            
            // Apply left-handed mode
            if let Some(left_handed) = config.left_handed {
                if let Err(e) = self.set_libinput_left_handed(device_id, left_handed) {
                    warn!("Failed to set left-handed mode for device {}: {}", device_id, e);
                    fail_count += 1;
                    continue;
                }
            }
        }
        
        if success_count > 0 {
            info!("Applied mouse configuration to {} device(s)", success_count);
        }
        if fail_count > 0 {
            warn!("Failed to apply configuration to {} device(s)", fail_count);
        }
        
        Ok(())
    }
}

