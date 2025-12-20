//! Bevy-based Compositor for Area Desktop
//! 
//! This module handles rendering X11 windows as entities within the Bevy scene.

use bevy::prelude::*;
use crate::state::ShellState;
use crate::grab::X11Grabber;
use crate::wobbly::{WobblyGrid, create_wobbly_mesh};
use std::collections::HashMap;

pub struct CompositorPlugin;

impl Plugin for CompositorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CompositorState>()
            .add_systems(Update, (
                manage_window_entities,
                update_window_textures, // Removed ordering constraint for now
            ));
    }
}

#[derive(Resource, Default)]
struct CompositorState {
    /// Maps X11 Window ID to Bevy Entity
    entities: HashMap<u32, Entity>,
    /// Frame counter for throttling capture
    frame_count: u64,
}

#[derive(Component)]
pub struct WindowQuad {
    pub id: u32,
    pub image_handle: Handle<Image>,
}

fn manage_window_entities(
    mut commands: Commands,
    state: Res<ShellState>,
    mode: Res<crate::state::ShellMode>,
    mut compositor: ResMut<CompositorState>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // Only create compositor entities when shell is in fullscreen mode
    // In Normal mode (32px bar), compositor entities would be off-screen
    if *mode == crate::state::ShellMode::Normal {
        // Despawn all compositor entities when in Normal mode
        let entities_to_despawn: Vec<Entity> = compositor.entities.values().copied().collect();
        for entity in entities_to_despawn {
            commands.entity(entity).despawn();
        }
        compositor.entities.clear();
        return;
    }
    
    // 1. Spawn new windows
    for (&id, win) in &state.windows {
        // Skip ourselves (the shell)
        if win.class == "area-shell" {
            continue;
        }

        if !compositor.entities.contains_key(&id) {
            info!("Compositing new window with WOBBLE: {} ({})", win.title, id);
            
            // Create a placeholder image (transparent until captured)
            let mut image = Image::default(); 
            // Make it 1x1 transparent
            image.data = Some(vec![0, 0, 0, 0]);
            let image_handle = images.add(image);
            
            let size = Vec2::new(win.width as f32, win.height as f32);
            
            // Convert X11 coords (top-left 0,0) to Bevy coords (center 0,0, Y-up)
            // Screen is 1280x720
            let x = win.x as f32 - 1280.0 / 2.0 + size.x / 2.0;
            let y = -(win.y as f32 - 720.0 / 2.0 + size.y / 2.0);
            let _pos = Vec2::new(x, y);

            let mesh = create_wobbly_mesh(size);
            let material = materials.add(ColorMaterial {
                color: Color::WHITE,
                texture: Some(image_handle.clone()),
                ..default()
            });

            let entity = commands.spawn((
                Mesh2d(meshes.add(mesh)),
                MeshMaterial2d(material),
                Transform::from_xyz(x, y, 1.0), // Position mesh at window location
                WindowQuad {
                    id,
                    image_handle: image_handle.clone(),
                },
                WobblyGrid::new(Vec2::ZERO, size), // Physics points relative to mesh center (origin)
            )).id();
            
            compositor.entities.insert(id, entity);
        }
    }

    // 2. Remove closed windows
    let current_ids: Vec<u32> = state.windows.keys().cloned().collect();
    compositor.entities.retain(|id, entity| {
        if !current_ids.contains(id) {
            commands.entity(*entity).despawn();
            false
        } else {
            true
        }
    });

    // 3. Transform and physics updates are handled in update_window_textures system
    // This keeps systems separated: manage_window_entities handles entity lifecycle,
    // update_window_textures handles position/size updates and texture capture
}

// 4. Separate system to update WobblyGrid targets and capture textures
fn update_window_textures(
    mut compositor: ResMut<CompositorState>,
    mut grabber: ResMut<X11Grabber>,
    mut images: ResMut<Assets<Image>>,
    state: Res<ShellState>,
    mut query: Query<(&WindowQuad, &mut WobblyGrid, &mut Transform)>,
) {
    compositor.frame_count += 1;
    let throttle = 5; // Update every 5 frames

    for (quad, mut grid, mut transform) in &mut query {
        // Update Transform and Physics Target from Window State
        if let Some(win) = state.windows.get(&quad.id) {
            let size = Vec2::new(win.width as f32, win.height as f32);
            let world_x = win.x as f32 - 1280.0 / 2.0 + size.x / 2.0;
            let world_y = -(win.y as f32 - 720.0 / 2.0 + size.y / 2.0);
            
            let is_focused = state.focused == Some(quad.id);
            let z = if is_focused { 5.0 } else { 1.0 };
            
            // Check if window is being dragged
            let is_dragging = state.dragging_windows.contains(&quad.id);
            
            if is_dragging {
                // During drag: Trigger wobble by moving physics points
                // Calculate movement delta from current Transform position
                let current_pos = Vec2::new(transform.translation.x, transform.translation.y);
                let target_pos = Vec2::new(world_x, world_y);
                let delta = target_pos - current_pos;
                
                // Update physics target to the delta (in local space)
                // This will cause physics to animate towards the new position
                // The physics points will wobble as they animate
                grid.target_pos = delta;
                
                // Update Transform to new position (physics will deform mesh around this)
                transform.translation = Vec3::new(world_x, world_y, z);
            } else {
                // Not dragging: Update Transform immediately (no wobble)
                transform.translation = Vec3::new(world_x, world_y, z);
                
                // Reset physics target to center (no movement to animate)
                grid.target_pos = Vec2::ZERO;
            }
            
            // Always update target size (for resize wobble)
            grid.target_size = size;
        }

        // Capture Texture
        // Note: quad.id is the frame ID (sent by WM), which contains the client window
        // Capturing the frame will show the whole window including decorations
        // Only capture if visible and frame count matches
        if compositor.frame_count % throttle == 0 {
             if let Some(image) = grabber.capture_window(quad.id) {
                // Resize check? X11Grabber returns new image size.
                // Update image asset
                if let Some(asset) = images.get_mut(&quad.image_handle) {
                    *asset = image;
                }
            }
        }
    }
}
