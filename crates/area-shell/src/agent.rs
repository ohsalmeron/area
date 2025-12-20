//! Agentic overlay
//!
//! Provides context-aware suggestions based on the focused window.

use crate::ipc::IpcSender;
use crate::state::{ShellMode, ShellState};
use bevy::prelude::*;
use tracing::info;

pub struct AgentPlugin;

impl Plugin for AgentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (update_agent_overlay, handle_agent_click));
    }
}

/// Marker for the agent overlay
#[derive(Component)]
struct AgentOverlay;

/// Marker for agent buttons
#[derive(Component)]
struct AgentAction(String);

fn update_agent_overlay(
    mut commands: Commands,
    state: Res<ShellState>,
    mode: Res<ShellMode>,
    query: Query<Entity, With<AgentOverlay>>,
) {
    // Only show in Normal mode and when a window is focused
    if *mode != ShellMode::Normal || state.focused.is_none() {
        for entity in query.iter() {
            commands.entity(entity).despawn_recursive();
        }
        return;
    }

    // Only redraw on state changes
    if !state.is_changed() && !mode.is_changed() && !query.is_empty() {
        return;
    }

    // Clean up
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }

    let Some(focused) = state.focused_window() else {
        return;
    };
    
    // Position the overlay near the focused window's top-right corner
    // For now, we'll just put it in a fixed but relevant spot (center-right)
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(24.0),
                top: Val::Px(100.0),
                width: Val::Px(200.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(12.0)),
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.1, 0.1, 0.15, 0.9)),
            BorderRadius::all(Val::Px(12.0)),
            AgentOverlay,
        ))
        .with_children(|parent| {
            // Header
            parent.spawn((
                Text::new("Agent Actions"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgba(0.5, 0.8, 1.0, 1.0)),
            ));

            // Context-based actions
            let class = focused.class.to_lowercase();
            let actions = if class.contains("term") {
                vec![
                    ("Cargo Run", "cargo run"),
                    ("Git Status", "git status"),
                    ("Ls -la", "ls -la"),
                ]
            } else if class.contains("chrom") || class.contains("firefox") {
                vec![
                    ("New Tab", "xdotool key ctrl+t"),
                    ("Reload", "xdotool key ctrl+r"),
                ]
            } else {
                vec![("Open Terminal", "xfce4-terminal")]
            };

            for (label, cmd) in actions {
                parent
                    .spawn((
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(32.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.2, 0.2, 0.25, 0.8)),
                        BorderRadius::all(Val::Px(6.0)),
                        AgentAction(cmd.to_string()),
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            Text::new(label),
                            TextFont {
                                font_size: 12.0,
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
            }
        });
}

fn handle_agent_click(
    query: Query<(&Interaction, &AgentAction), Changed<Interaction>>,
    ipc: Res<IpcSender>,
) {
    for (interaction, action) in &query {
        if *interaction == Interaction::Pressed {
            info!("Agent: running action '{}'", action.0);
            ipc.send(area_ipc::ShellCommand::LaunchApp {
                command: action.0.clone(),
            });
        }
    }
}
