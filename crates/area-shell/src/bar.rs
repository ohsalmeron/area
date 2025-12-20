//! Top bar panel
//!
//! A Compiz-style panel with workspace switcher, window title, and clock.

use crate::ipc::IpcSender;
use crate::state::ShellState;
use bevy::prelude::*;
use tracing::debug;

/// Plugin for the top bar
pub struct BarPlugin;

impl Plugin for BarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_bar)
            .add_systems(
                Update,
                (
                    update_clock,
                    update_workspace_indicator,
                    update_window_title,
                    update_taskbar,
                    handle_workspace_click,
                    handle_terminal_click,
                    handle_taskbar_click,
                ),
            );
    }
}

/// Marker component for the bar root
#[derive(Component)]
struct BarRoot;

/// Marker for workspace buttons
#[derive(Component)]
struct WorkspaceButton(u8);

/// Marker for the terminal button
#[derive(Component)]
struct TerminalButton;

/// Marker for the active window title
#[derive(Component)]
struct WindowTitleText;

/// Marker for taskbar window buttons
#[derive(Component)]
struct TaskbarWindowButton(u32);

/// Marker for the taskbar container
#[derive(Component)]
struct TaskbarContainer;

/// Marker for the clock
#[derive(Component)]
struct ClockText;

/// Timer for clock updates
#[derive(Resource)]
struct ClockTimer(Timer);

fn setup_bar(mut commands: Commands) {
    // Clock update timer
    commands.insert_resource(ClockTimer(Timer::from_seconds(1.0, TimerMode::Repeating)));

    // Camera
    commands.spawn(Camera2d::default());

    // Bar background
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Px(32.0),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::horizontal(Val::Px(8.0)),
            ..default()
        })
        .insert(BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.9)))
        .insert(BarRoot)
        .with_children(|parent| {
            // Left: Workspace switcher
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(4.0),
                    ..default()
                })
                .with_children(|parent| {
                    for i in 0..4 {
                        parent.spawn((
                            Button,
                            Node {
                                width: Val::Px(24.0),
                                height: Val::Px(24.0),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(if i == 0 {
                                Color::srgba(0.3, 0.5, 0.9, 0.8)
                            } else {
                                Color::srgba(0.2, 0.2, 0.25, 0.8)
                            }),
                        ))
                        .insert(WorkspaceButton(i)).with_children(|parent| {
                            parent.spawn((
                                Text::new(format!("{}", i + 1)),
                                TextFont {
                                    font_size: 14.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });
                    }
                });

            // Terminal Button
            parent.spawn((
                Button,
                Node {
                    width: Val::Px(32.0),
                    height: Val::Px(24.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    margin: UiRect::left(Val::Px(8.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.2, 0.3, 0.4, 0.9)),
            ))
            .insert(TerminalButton).with_children(|parent| {
                parent.spawn((
                    Text::new("▶"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });

            // Taskbar: Window buttons
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(4.0),
                    margin: UiRect::left(Val::Px(8.0)),
                    ..default()
                })
                .insert(TaskbarContainer);

            // Center: Active window title
            parent.spawn((
                Text::new("Area Desktop"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgba(0.9, 0.9, 0.9, 1.0)),
                WindowTitleText,
            ));

            // Right: Clock
            parent.spawn((
                Text::new("00:00"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgba(0.8, 0.8, 0.8, 1.0)),
                ClockText,
            ));
        });
}

fn update_clock(
    time: Res<Time>,
    mut timer: ResMut<ClockTimer>,
    mut query: Query<&mut Text, With<ClockText>>,
) {
    if timer.0.tick(time.delta()).just_finished() {
        let now = chrono_lite_now();
        for mut text in &mut query {
            text.0 = now.clone();
        }
    }
}

fn update_workspace_indicator(
    state: Res<ShellState>,
    mut query: Query<(&WorkspaceButton, &mut BackgroundColor)>,
) {
    if !state.is_changed() {
        return;
    }

    for (button, mut bg) in &mut query {
        *bg = if button.0 == state.current_workspace {
            BackgroundColor(Color::srgba(0.3, 0.5, 0.9, 0.8))
        } else {
            BackgroundColor(Color::srgba(0.2, 0.2, 0.25, 0.8))
        };
    }
}

fn update_window_title(
    state: Res<ShellState>,
    mut query: Query<&mut Text, With<WindowTitleText>>,
) {
    if !state.is_changed() {
        return;
    }

    let title = state
        .focused_window()
        .map(|w| w.title.clone())
        .unwrap_or_else(|| "Area Desktop".to_string());

    for mut text in &mut query {
        text.0 = if title.len() > 50 {
            format!("{}...", &title[..47])
        } else {
            title.clone()
        };
    }
}

fn handle_workspace_click(
    query: Query<(&Interaction, &WorkspaceButton), Changed<Interaction>>,
    ipc: Res<IpcSender>,
) {
    for (interaction, button) in &query {
        if *interaction == Interaction::Pressed {
            debug!("Workspace {} clicked", button.0);
            ipc.send(area_ipc::ShellCommand::SwitchWorkspace { index: button.0 });
        }
    }
}


fn handle_terminal_click(
    query: Query<&Interaction, (Changed<Interaction>, With<TerminalButton>)>,
    ipc: Res<IpcSender>,
) {
    for interaction in &query {
        if *interaction == Interaction::Pressed {
            debug!("Terminal button clicked");
            ipc.send(area_ipc::ShellCommand::LaunchApp { 
                command: "wezterm start --always-new-process".to_string() 
            });
        }
    }
}

/// Update taskbar with open windows
fn update_taskbar(
    mut commands: Commands,
    state: Res<ShellState>,
    taskbar_query: Query<Entity, With<TaskbarContainer>>,
    window_buttons: Query<(Entity, &TaskbarWindowButton)>,
    mut button_colors: Query<&mut BackgroundColor, With<TaskbarWindowButton>>,
) {
    if !state.is_changed() {
        return;
    }

    // Get current window IDs from state
    let current_windows: std::collections::HashSet<u32> = state.windows.keys().copied().collect();
    
    // Get existing button window IDs
    let existing_windows: std::collections::HashSet<u32> = window_buttons
        .iter()
        .map(|(_, btn)| btn.0)
        .collect();

    // Remove buttons for closed windows
    for (entity, btn) in window_buttons.iter() {
        if !current_windows.contains(&btn.0) {
            commands.entity(entity).despawn();
        }
    }

    // Add buttons for new windows
    if let Ok(taskbar_entity) = taskbar_query.single() {
        for window_id in current_windows.iter() {
            if !existing_windows.contains(window_id) {
                if let Some(window) = state.windows.get(window_id) {
                    // Skip shell window
                    if window.class == "area-shell" {
                        continue;
                    }
                    
                    let is_focused = state.focused == Some(*window_id);
                    let title = if window.title.len() > 20 {
                        format!("{}...", &window.title[..17])
                    } else {
                        window.title.clone()
                    };

                    commands.entity(taskbar_entity).with_children(|parent| {
                        parent.spawn((
                            Button,
                            Node {
                                min_width: Val::Px(100.0),
                                max_width: Val::Px(200.0),
                                height: Val::Px(24.0),
                                padding: UiRect::horizontal(Val::Px(8.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(if is_focused {
                                Color::srgba(0.3, 0.5, 0.9, 0.8)
                            } else {
                                Color::srgba(0.2, 0.2, 0.25, 0.8)
                            }),
                        ))
                        .insert(TaskbarWindowButton(*window_id))
                        .with_children(|parent| {
                            parent.spawn((
                                Text::new(title),
                                TextFont {
                                    font_size: 12.0,
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                        });
                    });
                }
            }
        }
        
        // Update existing button colors for focus changes
        for (entity, btn) in window_buttons.iter() {
            let is_focused = state.focused == Some(btn.0);
            if let Ok(mut bg) = button_colors.get_mut(entity) {
                *bg = BackgroundColor(if is_focused {
                    Color::srgba(0.3, 0.5, 0.9, 0.8)
                } else {
                    Color::srgba(0.2, 0.2, 0.25, 0.8)
                });
            }
        }
    }
}

/// Handle clicking on taskbar window buttons
fn handle_taskbar_click(
    query: Query<(&Interaction, &TaskbarWindowButton), Changed<Interaction>>,
    ipc: Res<IpcSender>,
) {
    for (interaction, button) in &query {
        if *interaction == Interaction::Pressed {
            debug!("Taskbar: focusing window {}", button.0);
            ipc.send(area_ipc::ShellCommand::FocusWindow { id: button.0 });
        }
    }
}

/// Simple time function (avoiding chrono dependency for now)
fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Rough timezone handling (adjust for your locale)
    let offset_hours: i64 = -6; // CST
    let adjusted = secs as i64 + offset_hours * 3600;

    let hours = ((adjusted % 86400) / 3600) as u32;
    let minutes = ((adjusted % 3600) / 60) as u32;

    format!("{:02}:{:02}", hours, minutes)
}

