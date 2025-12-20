//! Bevy-based Compositor for Area Desktop
//! 
//! This module handles rendering X11 windows as entities within the Bevy scene.

use bevy::prelude::*;
use crate::state::ShellState;
use crate::grab::X11Grabber;
use std::collections::HashMap;

#[allow(dead_code)]
pub struct CompositorPlugin;

impl Plugin for CompositorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CompositorState>()
            .add_systems(Update, (
                manage_window_entities,
                update_window_textures.after(manage_window_entities),
            ));
    }
}

#[derive(Resource, Default)]
#[allow(dead_code)]
struct CompositorState {
    /// Maps X11 Window ID to Bevy Entity
    entities: HashMap<u32, Entity>,
}

#[derive(Component)]
#[allow(dead_code)]
pub struct WindowQuad {
    pub id: u32,
    pub image_handle: Handle<Image>,
}

#[allow(dead_code)]
fn manage_window_entities(
    mut commands: Commands,
    state: Res<ShellState>,
    mut compositor: ResMut<CompositorState>,
    mut images: ResMut<Assets<Image>>,
) {
    // 1. Spawn new windows
    for (&id, win) in &state.windows {
        // Skip ourselves (the shell)
        if win.class == "area-shell" {
            continue;
        }

        if !compositor.entities.contains_key(&id) {
            info!("Compositing new window: {} ({})", win.title, id);
            
            // Create a placeholder image
            let image = Image::default(); // 1x1 white
            let image_handle = images.add(image);
            
            let entity = commands.spawn((
                Sprite {
                    image: image_handle.clone(),
                    custom_size: Some(Vec2::new(win.width as f32, win.height as f32)),
                    ..default()
                },
                Transform::from_xyz(
                    win.x as f32 - 1280.0 / 2.0 + win.width as f32 / 2.0,
                    -(win.y as f32 - 720.0 / 2.0 + win.height as f32 / 2.0),
                    1.0,
                ),
                WindowQuad {
                    id,
                    image_handle: image_handle.clone(),
                },
            )).id();
            
            compositor.entities.insert(id, entity);
        }
    }

    // 2. Remove closed windows
    let current_ids: Vec<u32> = state.windows.keys().cloned().collect();
    compositor.entities.retain(|id, entity| {
        if !current_ids.contains(id) {
            commands.entity(*entity).despawn_recursive();
            false
        } else {
            true
        }
    });

    // 3. Update positions, sizes, and Z-index based on focus
    for (&id, win) in &state.windows {
        if let Some(&entity) = compositor.entities.get(&id) {
            let is_focused = state.focused == Some(id);
            let z = if is_focused { 5.0 } else { 1.0 };
            
            commands.entity(entity).insert(
                Transform::from_xyz(
                    win.x as f32 - 1280.0 / 2.0 + win.width as f32 / 2.0,
                    -(win.y as f32 - 720.0 / 2.0 + win.height as f32 / 2.0),
                    z,
                )
            );
        }
    }
}

#[allow(dead_code)]
fn update_window_textures(
    grabber: Res<X11Grabber>,
    mut images: ResMut<Assets<Image>>,
    query: Query<(&WindowQuad, &Sprite)>,
) {
    for (quad, _sprite) in &query {
        if let Some(image) = grabber.capture_window(quad.id) {
            // Update the image in assets
            // For now, let's just replace it. 
            // Better: update data in-place to avoid asset recreation overhead.
            *images.get_mut(&quad.image_handle).unwrap() = image;
            
            // Ensure sprite size matches captured image
            // sprite.custom_size = Some(Vec2::new(image.size().x as f32, image.size().y as f32));
        }
    }
}
