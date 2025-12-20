//! Shell state management
//!
//! Central state that tracks windows, workspaces, and shell mode.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

/// Plugin for shell state management
pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShellState>()
            .init_resource::<ShellMode>()
            .add_message::<WindowEvent>()
            .add_message::<WorkspaceEvent>();
    }
}

/// Current shell mode
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub enum ShellMode {
    /// Normal desktop view
    #[default]
    Normal,
    /// Overview mode (all windows visible)
    Overview,
    /// Launcher open
    Launcher,
}

/// Central shell state
#[derive(Resource, Default)]
pub struct ShellState {
    /// All known windows
    pub windows: HashMap<u32, WindowState>,
    /// Currently focused window
    pub focused: Option<u32>,
    /// Current workspace
    pub current_workspace: u8,
    /// Total workspaces
    pub num_workspaces: u8,
    /// Connected to WM
    pub connected: bool,
    /// Windows currently being dragged
    pub dragging_windows: HashSet<u32>,
}

/// State for a single window
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WindowState {
    pub id: u32,
    pub title: String,
    pub class: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub workspace: u8,
}

/// Messages for window changes
#[derive(Message, Debug)]
#[allow(dead_code)]
pub enum WindowEvent {
    Opened(WindowState),
    Closed(u32),
    Focused(u32),
    TitleChanged { id: u32, title: String },
    GeometryChanged { id: u32, x: i32, y: i32, width: u32, height: u32 },
}

/// Messages for workspace changes
#[derive(Message, Debug)]
#[allow(dead_code)]
pub enum WorkspaceEvent {
    Changed { current: u8, total: u8 },
}

impl ShellState {
    /// Handle a window opened event
    pub fn on_window_opened(&mut self, window: WindowState) {
        self.windows.insert(window.id, window);
    }

    /// Handle a window closed event
    pub fn on_window_closed(&mut self, id: u32) {
        self.windows.remove(&id);
        if self.focused == Some(id) {
            self.focused = None;
        }
    }

    /// Handle a window focused event
    pub fn on_window_focused(&mut self, id: u32) {
        self.focused = Some(id);
    }

    /// Handle workspace change
    pub fn on_workspace_changed(&mut self, current: u8, total: u8) {
        self.current_workspace = current;
        self.num_workspaces = total;
    }

    /// Get windows on current workspace
    #[allow(dead_code)]
    pub fn visible_windows(&self) -> impl Iterator<Item = &WindowState> {
        let ws = self.current_workspace;
        self.windows.values().filter(move |w| w.workspace == ws)
    }

    /// Get the focused window
    pub fn focused_window(&self) -> Option<&WindowState> {
        self.focused.and_then(|id| self.windows.get(&id))
    }
}
