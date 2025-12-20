//! App launcher
//!
//! Spotlight-style launcher with fuzzy search.

use crate::ipc::IpcSender;
use crate::state::ShellMode;
use bevy::prelude::*;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use std::fs;
use std::path::PathBuf;
use tracing::info;

/// Plugin for the app launcher
pub struct LauncherPlugin;

impl Plugin for LauncherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LauncherState>()
            .add_systems(Startup, scan_applications)
            .add_systems(
                Update,
                (
                    toggle_launcher,
                    render_launcher,
                    handle_launcher_input,
                    handle_app_click,
                ),
            );
    }
}

/// Launcher state
#[derive(Resource, Default)]
pub struct LauncherState {
    /// All discovered applications
    pub apps: Vec<AppEntry>,
    /// Current search query
    pub query: String,
    /// Filtered results
    pub results: Vec<usize>,
    /// Selected index
    pub selected: usize,
}

/// An application entry from .desktop files
#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub exec: String,
    pub _icon: Option<String>,
    pub _comment: Option<String>,
}

/// Marker for launcher UI
#[derive(Component)]
struct LauncherUI;

/// Marker for search input display
#[derive(Component)]
struct SearchText;

/// Marker for app result buttons
#[derive(Component)]
struct AppButton(usize);

/// Scan for .desktop files
fn scan_applications(mut state: ResMut<LauncherState>) {
    let dirs = [
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        dirs::data_dir()
            .map(|p| p.join("applications"))
            .unwrap_or_default(),
    ];

    for dir in dirs {
        if !dir.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "desktop").unwrap_or(false) {
                    if let Some(app) = parse_desktop_file(&path) {
                        state.apps.push(app);
                    }
                }
            }
        }
    }

    // Sort by name
    state.apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    info!("Found {} applications", state.apps.len());

    // Initialize results to all apps
    state.results = (0..state.apps.len()).collect();
}

/// Parse a .desktop file
fn parse_desktop_file(path: &PathBuf) -> Option<AppEntry> {
    let content = fs::read_to_string(path).ok()?;

    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut comment = None;
    let mut no_display = false;
    let mut hidden = false;

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with("Name=") && name.is_none() {
            name = Some(line[5..].to_string());
        } else if line.starts_with("Exec=") {
            // Remove field codes like %u, %U, %f, etc.
            let mut cmd = line[5..].to_string();
            for code in &["%u", "%U", "%f", "%F", "%i", "%c", "%k"] {
                cmd = cmd.replace(code, "");
            }
            exec = Some(cmd.trim().to_string());
        } else if line.starts_with("Icon=") {
            icon = Some(line[5..].to_string());
        } else if line.starts_with("Comment=") && comment.is_none() {
            comment = Some(line[8..].to_string());
        } else if line == "NoDisplay=true" {
            no_display = true;
        } else if line == "Hidden=true" {
            hidden = true;
        }
    }

    if no_display || hidden {
        return None;
    }

    Some(AppEntry {
        name: name?,
        exec: exec?,
        _icon: icon,
        _comment: comment,
    })
}

/// Toggle launcher with keyboard
fn toggle_launcher(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<ShellMode>,
    mut state: ResMut<LauncherState>,
) {
    // Super+Space or F10 to toggle
    if keyboard.just_pressed(KeyCode::F10) {
        *mode = match *mode {
            ShellMode::Launcher => {
                info!("Closing launcher");
                state.query.clear();
                state.results = (0..state.apps.len()).collect();
                state.selected = 0;
                ShellMode::Normal
            }
            _ => {
                info!("Opening launcher");
                ShellMode::Launcher
            }
        };
    }

    // Escape to close
    if *mode == ShellMode::Launcher && keyboard.just_pressed(KeyCode::Escape) {
        *mode = ShellMode::Normal;
        state.query.clear();
        state.results = (0..state.apps.len()).collect();
        state.selected = 0;
    }
}

/// Handle keyboard input in launcher
fn handle_launcher_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<ShellMode>,
    mut state: ResMut<LauncherState>,
    ipc: Res<IpcSender>,
    mut keyboard_events: MessageReader<KeyboardInput>,
) {
    if *mode != ShellMode::Launcher {
        return;
    }

    // Handle character input from keyboard events
    for event in keyboard_events.read() {
        if event.state != ButtonState::Pressed {
            continue;
        }
        
        // Map keycodes to characters
        if let Some(c) = keycode_to_char(event.key_code, keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight)) {
            state.query.push(c);
            filter_apps(&mut state);
        }
    }

    // Backspace
    if keyboard.just_pressed(KeyCode::Backspace) {
        state.query.pop();
        filter_apps(&mut state);
    }

    // Arrow navigation
    if keyboard.just_pressed(KeyCode::ArrowDown) {
        if state.selected < state.results.len().saturating_sub(1) {
            state.selected += 1;
        }
    }
    if keyboard.just_pressed(KeyCode::ArrowUp) {
        state.selected = state.selected.saturating_sub(1);
    }

    // Enter to launch
    if keyboard.just_pressed(KeyCode::Enter) && !state.results.is_empty() {
        if let Some(&idx) = state.results.get(state.selected) {
            if let Some(app) = state.apps.get(idx) {
                info!("Launching: {}", app.exec);
                ipc.send(area_ipc::ShellCommand::LaunchApp {
                    command: app.exec.clone(),
                });
                *mode = ShellMode::Normal;
                state.query.clear();
                state.results = (0..state.apps.len()).collect();
                state.selected = 0;
            }
        }
    }
}

/// Convert keycode to character (simple mapping)
fn keycode_to_char(key: KeyCode, shift: bool) -> Option<char> {
    match key {
        KeyCode::KeyA => Some(if shift { 'A' } else { 'a' }),
        KeyCode::KeyB => Some(if shift { 'B' } else { 'b' }),
        KeyCode::KeyC => Some(if shift { 'C' } else { 'c' }),
        KeyCode::KeyD => Some(if shift { 'D' } else { 'd' }),
        KeyCode::KeyE => Some(if shift { 'E' } else { 'e' }),
        KeyCode::KeyF => Some(if shift { 'F' } else { 'f' }),
        KeyCode::KeyG => Some(if shift { 'G' } else { 'g' }),
        KeyCode::KeyH => Some(if shift { 'H' } else { 'h' }),
        KeyCode::KeyI => Some(if shift { 'I' } else { 'i' }),
        KeyCode::KeyJ => Some(if shift { 'J' } else { 'j' }),
        KeyCode::KeyK => Some(if shift { 'K' } else { 'k' }),
        KeyCode::KeyL => Some(if shift { 'L' } else { 'l' }),
        KeyCode::KeyM => Some(if shift { 'M' } else { 'm' }),
        KeyCode::KeyN => Some(if shift { 'N' } else { 'n' }),
        KeyCode::KeyO => Some(if shift { 'O' } else { 'o' }),
        KeyCode::KeyP => Some(if shift { 'P' } else { 'p' }),
        KeyCode::KeyQ => Some(if shift { 'Q' } else { 'q' }),
        KeyCode::KeyR => Some(if shift { 'R' } else { 'r' }),
        KeyCode::KeyS => Some(if shift { 'S' } else { 's' }),
        KeyCode::KeyT => Some(if shift { 'T' } else { 't' }),
        KeyCode::KeyU => Some(if shift { 'U' } else { 'u' }),
        KeyCode::KeyV => Some(if shift { 'V' } else { 'v' }),
        KeyCode::KeyW => Some(if shift { 'W' } else { 'w' }),
        KeyCode::KeyX => Some(if shift { 'X' } else { 'x' }),
        KeyCode::KeyY => Some(if shift { 'Y' } else { 'y' }),
        KeyCode::KeyZ => Some(if shift { 'Z' } else { 'z' }),
        KeyCode::Digit0 => Some(if shift { ')' } else { '0' }),
        KeyCode::Digit1 => Some(if shift { '!' } else { '1' }),
        KeyCode::Digit2 => Some(if shift { '@' } else { '2' }),
        KeyCode::Digit3 => Some(if shift { '#' } else { '3' }),
        KeyCode::Digit4 => Some(if shift { '$' } else { '4' }),
        KeyCode::Digit5 => Some(if shift { '%' } else { '5' }),
        KeyCode::Digit6 => Some(if shift { '^' } else { '6' }),
        KeyCode::Digit7 => Some(if shift { '&' } else { '7' }),
        KeyCode::Digit8 => Some(if shift { '*' } else { '8' }),
        KeyCode::Digit9 => Some(if shift { '(' } else { '9' }),
        KeyCode::Space => Some(' '),
        KeyCode::Minus => Some(if shift { '_' } else { '-' }),
        _ => None,
    }
}

/// Filter apps based on query
fn filter_apps(state: &mut LauncherState) {
    if state.query.is_empty() {
        state.results = (0..state.apps.len()).collect();
    } else {
        let query = state.query.to_lowercase();
        state.results = state
            .apps
            .iter()
            .enumerate()
            .filter(|(_, app)| {
                app.name.to_lowercase().contains(&query)
                    || app.exec.to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();
    }
    state.selected = 0;
}

/// Render launcher UI
fn render_launcher(
    mut commands: Commands,
    mode: Res<ShellMode>,
    state: Res<LauncherState>,
    launcher_query: Query<Entity, With<LauncherUI>>,
) {
    if !mode.is_changed() && !state.is_changed() {
        return;
    }

    // Clean up existing UI
    for entity in launcher_query.iter() {
        commands.entity(entity).despawn();
    }

    if *mode != ShellMode::Launcher {
        return;
    }

    // Create launcher overlay
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexStart,
                padding: UiRect::top(Val::Px(100.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
            LauncherUI,
        ))
        .with_children(|parent| {
            // Search box container
            parent
                .spawn((
                    Node {
                        width: Val::Px(500.0),
                        flex_direction: FlexDirection::Column,
                        ..default()
                    },
                ))
                .with_children(|parent| {
                    // Search input
                    parent.spawn(Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(48.0),
                        padding: UiRect::all(Val::Px(12.0)),
                        margin: UiRect::bottom(Val::Px(8.0)),
                        justify_content: JustifyContent::FlexStart,
                        align_items: AlignItems::Center,
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    })
                    .insert(BackgroundColor(Color::srgba(0.15, 0.15, 0.2, 0.95))).with_children(|parent| {
                        let display_text = if state.query.is_empty() {
                            "Type to search...".to_string()
                        } else {
                            state.query.clone()
                        };

                        parent.spawn((
                            Text::new(display_text),
                            TextFont {
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(if state.query.is_empty() {
                                Color::srgba(0.5, 0.5, 0.5, 1.0)
                            } else {
                                Color::WHITE
                            }),
                            SearchText,
                        ));
                    });

                    // Results list
                    parent
                        .spawn(Node {
                            width: Val::Percent(100.0),
                            max_height: Val::Px(400.0),
                            flex_direction: FlexDirection::Column,
                            overflow: Overflow::clip_y(),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        })
                        .insert(BackgroundColor(Color::srgba(0.12, 0.12, 0.15, 0.95)))
                        .with_children(|parent| {
                            for (i, &idx) in state.results.iter().take(10).enumerate() {
                                if let Some(app) = state.apps.get(idx) {
                                    let is_selected = i == state.selected;

                                    parent
                                        .spawn((
                                            Button,
                                            Node {
                                                width: Val::Percent(100.0),
                                                height: Val::Px(40.0),
                                                padding: UiRect::horizontal(Val::Px(12.0)),
                                                justify_content: JustifyContent::FlexStart,
                                                align_items: AlignItems::Center,
                                                ..default()
                                            },
                                            BackgroundColor(if is_selected {
                                                Color::srgba(0.3, 0.5, 0.9, 0.8)
                                            } else {
                                                Color::NONE
                                            }),
                                        ))
                                        .insert(AppButton(idx))
                                        .with_children(|parent| {
                                            parent.spawn((
                                                Text::new(&app.name),
                                                TextFont {
                                                    font_size: 14.0,
                                                    ..default()
                                                },
                                                TextColor(Color::WHITE),
                                            ));
                                        });
                                }
                            }
                        });
                });
        });
}

/// Handle clicking on app results
fn handle_app_click(
    query: Query<(&Interaction, &AppButton), Changed<Interaction>>,
    state: Res<LauncherState>,
    ipc: Res<IpcSender>,
    mut mode: ResMut<ShellMode>,
) {
    for (interaction, button) in &query {
        if *interaction == Interaction::Pressed {
            if let Some(app) = state.apps.get(button.0) {
                info!("Launching via click: {}", app.exec);
                ipc.send(area_ipc::ShellCommand::LaunchApp {
                    command: app.exec.clone(),
                });
                *mode = ShellMode::Normal;
            }
        }
    }
}
