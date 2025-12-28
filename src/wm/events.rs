//! Events Module
//!
//! Handles all X11 event types, matching xfwm4's event handling system.
//! This module provides centralized event processing and routing.

use anyhow::Result;
use tracing::{debug, info, warn};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::protocol::Event;

use crate::wm::client::Client;
use crate::wm::display::DisplayInfo;
use crate::wm::screen::ScreenInfo;

/// Event handler trait for modular event processing
pub trait EventHandler {
    /// Handle a specific event type
    fn handle_event(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        event: &Event,
    ) -> Result<EventResult>;
}

/// Result of event handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    /// Event was handled successfully
    Handled,
    /// Event should be ignored
    Ignore,
    /// Event needs further processing
    Continue,
}

/// Event router - dispatches events to appropriate handlers
pub struct EventRouter {
    /// Event filter status (for event filtering system)
    pub filter_status: EventFilterStatus,
}

/// Event filter status (matches xfwm4's eventFilterStatus)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventFilterStatus {
    /// Pass event through
    Pass,
    /// Remove/ignore event
    Remove,
}

impl Default for EventFilterStatus {
    fn default() -> Self {
        Self::Pass
    }
}

impl EventRouter {
    /// Create a new event router
    pub fn new() -> Self {
        Self {
            filter_status: EventFilterStatus::default(),
        }
    }
    
    /// Route an event to the appropriate handler
    pub fn route_event(
        &mut self,
        conn: &RustConnection,
        display_info: &DisplayInfo,
        screen_info: &ScreenInfo,
        event: &Event,
    ) -> Result<EventResult> {
        match event {
            // Window management events
            Event::MapRequest(e) => {
                debug!("MapRequest: window {}", e.window);
                Ok(EventResult::Continue)
            }
            Event::MapNotify(e) => {
                debug!("MapNotify: window {}", e.window);
                Ok(EventResult::Continue)
            }
            Event::UnmapNotify(e) => {
                debug!("UnmapNotify: window {}", e.window);
                Ok(EventResult::Continue)
            }
            Event::DestroyNotify(e) => {
                debug!("DestroyNotify: window {}", e.window);
                Ok(EventResult::Continue)
            }
            Event::ConfigureRequest(e) => {
                debug!("ConfigureRequest: window {}", e.window);
                Ok(EventResult::Continue)
            }
            Event::ConfigureNotify(e) => {
                debug!("ConfigureNotify: window {}", e.window);
                Ok(EventResult::Continue)
            }
            Event::ReparentNotify(e) => {
                debug!("ReparentNotify: window {}", e.window);
                Ok(EventResult::Continue)
            }
            Event::CreateNotify(e) => {
                debug!("CreateNotify: window {}", e.window);
                Ok(EventResult::Continue)
            }
            Event::GravityNotify(e) => {
                debug!("GravityNotify: window {}", e.window);
                Ok(EventResult::Continue)
            }
            Event::CirculateRequest(e) => {
                debug!("CirculateRequest: window {}", e.window);
                Ok(EventResult::Continue)
            }
            Event::CirculateNotify(e) => {
                debug!("CirculateNotify: window {}", e.window);
                Ok(EventResult::Continue)
            }
            
            // Focus events
            Event::FocusIn(e) => {
                debug!("FocusIn: window {}", e.event);
                Ok(EventResult::Continue)
            }
            Event::FocusOut(e) => {
                debug!("FocusOut: window {}", e.event);
                Ok(EventResult::Continue)
            }
            
            // Input events
            Event::ButtonPress(e) => {
                debug!("ButtonPress: window {}, button {}", e.event, e.detail);
                Ok(EventResult::Continue)
            }
            Event::ButtonRelease(e) => {
                debug!("ButtonRelease: window {}, button {}", e.event, e.detail);
                Ok(EventResult::Continue)
            }
            Event::MotionNotify(e) => {
                // MotionNotify events are very frequent, only log at trace level
                // debug!("MotionNotify: window {}", e.event);
                Ok(EventResult::Continue)
            }
            Event::KeyPress(e) => {
                debug!("KeyPress: window {}, keycode {}", e.event, e.detail);
                Ok(EventResult::Continue)
            }
            Event::KeyRelease(e) => {
                debug!("KeyRelease: window {}, keycode {}", e.event, e.detail);
                Ok(EventResult::Continue)
            }
            Event::EnterNotify(e) => {
                debug!("EnterNotify: window {}", e.event);
                Ok(EventResult::Continue)
            }
            Event::LeaveNotify(e) => {
                debug!("LeaveNotify: window {}", e.event);
                Ok(EventResult::Continue)
            }
            
            // Property events
            Event::PropertyNotify(e) => {
                debug!("PropertyNotify: window {}, atom {}", e.window, e.atom);
                Ok(EventResult::Continue)
            }
            
            // Client messages (EWMH)
            Event::ClientMessage(e) => {
                debug!("ClientMessage: window {}, type {}", e.window, e.type_);
                Ok(EventResult::Continue)
            }
            
            // Selection events
            Event::SelectionClear(e) => {
                debug!("SelectionClear: owner {}, selection {}", e.owner, e.selection);
                Ok(EventResult::Continue)
            }
            Event::SelectionNotify(e) => {
                debug!("SelectionNotify: requestor {}, selection {}", e.requestor, e.selection);
                Ok(EventResult::Continue)
            }
            Event::SelectionRequest(e) => {
                debug!("SelectionRequest: owner {}, selection {}", e.owner, e.selection);
                Ok(EventResult::Continue)
            }
            
            // Colormap events
            Event::ColormapNotify(e) => {
                debug!("ColormapNotify: window {}", e.window);
                Ok(EventResult::Continue)
            }
            
            // Shape extension events
            Event::ShapeNotify(e) => {
                debug!("ShapeNotify: affected_window {}", e.affected_window);
                Ok(EventResult::Continue)
            }
            
            // Error events
            Event::Error(e) => {
                warn!("X11 Error: error_code={}, request_code={}, minor_code={}",
                    e.error_code, e.major_opcode, e.minor_opcode);
                Ok(EventResult::Handled)
            }
            
            // Unknown events
            _ => {
                debug!("Unknown event type: {:?}", event);
                Ok(EventResult::Ignore)
            }
        }
    }
}

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for event handling
impl EventRouter {
    /// Check if an event should be filtered
    pub fn should_filter_event(&self, event: &Event) -> bool {
        match self.filter_status {
            EventFilterStatus::Pass => false,
            EventFilterStatus::Remove => true,
        }
    }
    
    /// Get event timestamp
    pub fn get_event_timestamp(event: &Event) -> u32 {
        match event {
            Event::ButtonPress(e) => e.time,
            Event::ButtonRelease(e) => e.time,
            Event::KeyPress(e) => e.time,
            Event::KeyRelease(e) => e.time,
            Event::MotionNotify(e) => e.time,
            Event::EnterNotify(e) => e.time,
            Event::LeaveNotify(e) => e.time,
            Event::PropertyNotify(e) => e.time,
            Event::ClientMessage(e) => e.data.as_data32()[4], // timestamp is in data[4]
            _ => x11rb::CURRENT_TIME,
        }
    }
    
    /// Get event window
    pub fn get_event_window(event: &Event) -> Option<u32> {
        match event {
            Event::MapRequest(e) => Some(e.window),
            Event::MapNotify(e) => Some(e.window),
            Event::UnmapNotify(e) => Some(e.window),
            Event::DestroyNotify(e) => Some(e.window),
            Event::ConfigureRequest(e) => Some(e.window),
            Event::ConfigureNotify(e) => Some(e.window),
            Event::ReparentNotify(e) => Some(e.window),
            Event::CreateNotify(e) => Some(e.window),
            Event::GravityNotify(e) => Some(e.window),
            Event::CirculateRequest(e) => Some(e.window),
            Event::CirculateNotify(e) => Some(e.window),
            Event::FocusIn(e) => Some(e.event),
            Event::FocusOut(e) => Some(e.event),
            Event::ButtonPress(e) => Some(e.event),
            Event::ButtonRelease(e) => Some(e.event),
            Event::MotionNotify(e) => Some(e.event),
            Event::KeyPress(e) => Some(e.event),
            Event::KeyRelease(e) => Some(e.event),
            Event::EnterNotify(e) => Some(e.event),
            Event::LeaveNotify(e) => Some(e.event),
            Event::PropertyNotify(e) => Some(e.window),
            Event::ClientMessage(e) => Some(e.window),
            Event::SelectionClear(e) => Some(e.owner),
            Event::SelectionNotify(e) => Some(e.requestor),
            Event::SelectionRequest(e) => Some(e.owner),
            Event::ColormapNotify(e) => Some(e.window),
            Event::ShapeNotify(e) => Some(e.affected_window),
            Event::Error(_) => None, // Error events don't have a window
            _ => None,
        }
    }
}

