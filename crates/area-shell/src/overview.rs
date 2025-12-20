//! Overview mode
//!
//! Compiz Expo-style overview showing all windows in a grid.
//! Will eventually include smooth zoom animations and window thumbnails.

use crate::ipc::IpcSender;
use crate::state::{ShellMode, ShellState};
use bevy::prelude::*;
use tracing::info;

/// Plugin for overview mode
pub struct OverviewPlugin;

impl Plugin for OverviewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (toggle_overview, render_overview, handle_overview_clicks, fetch_thumbnails));
    }
}

/// Marker for overview UI elements
#[derive(Component)]
struct OverviewUI;

/// Marker for window tiles in overview
#[derive(Component)]
struct WindowTile(u32);

/// Toggle overview mode with keyboard
fn toggle_overview(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<ShellMode>,
) {
    // Super key (on some systems this might not work due to WM grabbing it)
    // For now, use F9 as a fallback
    if keyboard.just_pressed(KeyCode::F9) {
        *mode = match *mode {
            ShellMode::Overview => {
                info!("Exiting overview mode");
                ShellMode::Normal
            }
            _ => {
                info!("Entering overview mode");
                ShellMode::Overview
            }
        };
    }
}


/// Marker for entities that need a thumbnail fetched
#[derive(Component)]
struct NeedsThumbnail;

/// Marker for the image node within a window tile
#[derive(Component)]
struct ThumbnailImage;

/// Render overview when active
fn render_overview(
    mut commands: Commands,
    mode: Res<ShellMode>,
    state: Res<ShellState>,
    overview_query: Query<Entity, With<OverviewUI>>,
    _ipc: Res<IpcSender>,
) {
    // Only update when mode changes
    if !mode.is_changed() {
        return;
    }

    // Clean up existing overview UI
    for entity in overview_query.iter() {
        commands.entity(entity).despawn();
    }

    if *mode != ShellMode::Overview {
        return;
    }

    info!("Rendering overview with {} windows", state.windows.len());

    // Create overview background
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(32.0), // Below bar
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            align_content: AlignContent::Center,
            padding: UiRect::all(Val::Px(32.0)),
            row_gap: Val::Px(16.0),
            column_gap: Val::Px(16.0),
            ..default()
        })
        .insert(BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0))) // Start transparent
        .insert(crate::animation::UiAnimation {
            target_opacity: Some(0.8),
            ..default()
        })
        .insert(OverviewUI)
        .with_children(|parent| {
            // Create tiles for each window
            for window in state.windows.values() {
                let tile_width = 300.0;
                let tile_height = 200.0;

                parent
                    .spawn((
                        Button,
                        Node {
                            width: Val::Px(tile_width),
                            height: Val::Px(tile_height),
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::End,
                            align_items: AlignItems::Center,
                            padding: UiRect::all(Val::Px(8.0)),
                            border_radius: BorderRadius::all(Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.15, 0.15, 0.2, 0.0)), // Start transparent
                        Transform::from_scale(Vec3::splat(0.8)), // Start small
                    ))
                    .insert(WindowTile(window.id))
                    .insert(NeedsThumbnail) // Mark for fetching
                    .insert(crate::animation::UiAnimation {
                        target_opacity: Some(0.95),
                        target_scale: Some(Vec3::ONE),
                        speed: 8.0,
                    })
                    .with_children(|parent| {
                        // Window preview image (initially empty/placeholder)
                        parent.spawn(Node {
                            width: Val::Percent(100.0),
                            height: Val::Percent(80.0),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        })
                        .insert(BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 1.0)))
                        .insert(ThumbnailImage);

                        // Window title
                        let title = if window.title.len() > 30 {
                            format!("{}...", &window.title[..27])
                        } else {
                            window.title.clone()
                        };

                        parent.spawn((
                            Text::new(title),
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

fn fetch_thumbnails(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut grabber: ResMut<crate::grab::X11Grabber>,
    mut query: Query<(Entity, &WindowTile, &Children), With<NeedsThumbnail>>,
    image_query: Query<Entity, With<ThumbnailImage>>,
) {
    for (entity, tile, children) in &mut query {
        if let Some(image) = grabber.capture_window(tile.0) {
            let handle = images.add(image);
            
            // Find the child that is the thumbnail image node
            for child in children.iter() {
                if image_query.contains(child) {
                    commands.entity(child).insert(ImageNode::new(handle.clone()));
                    commands.entity(child).remove::<BackgroundColor>(); // Remove placeholder color
                    break;
                }
            }
        }
        // Remove marker so we don't fetch again
        commands.entity(entity).remove::<NeedsThumbnail>();
    }
}

/// Handle window clicks in overview
fn handle_overview_clicks(
    query: Query<(&Interaction, &WindowTile), Changed<Interaction>>,
    mut mode: ResMut<ShellMode>,
    ipc: Res<IpcSender>,
) {
    for (interaction, tile) in &query {
        if *interaction == Interaction::Pressed {
            info!("Overview: focusing window {}", tile.0);
            ipc.send(area_ipc::ShellCommand::FocusWindow { id: tile.0 });
            *mode = ShellMode::Normal;
        }
    }
}

