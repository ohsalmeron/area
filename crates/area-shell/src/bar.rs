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
                    handle_workspace_click,
                    handle_terminal_click,
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
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(32.0),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.9)),
            BarRoot,
        ))
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
                                ..default()
                            },
                            BackgroundColor(if i == 0 {
                                Color::srgba(0.3, 0.5, 0.9, 0.8)
                            } else {
                                Color::srgba(0.2, 0.2, 0.25, 0.8)
                            }),
                            BorderRadius::all(Val::Px(4.0)),
                            WorkspaceButton(i),
                        )).with_children(|parent| {
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
                    ..default()
                },
                BackgroundColor(Color::srgba(0.2, 0.3, 0.4, 0.9)),
                BorderRadius::all(Val::Px(4.0)),
                TerminalButton,
            )).with_children(|parent| {
                parent.spawn((
                    Text::new("▶"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });

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

