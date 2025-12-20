//! Wobbly Windows Physics Engine
//! Ported and adapted from Compiz's wobbly.c

use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;

pub struct WobblyPlugin;

impl Plugin for WobblyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (update_wobbly_physics, update_wobbly_meshes, spawn_debug_wobbly, handle_wobbly_drag));
    }
}

// Constants from Compiz
const GRID_WIDTH: usize = 4;
const GRID_HEIGHT: usize = 4;
const MASS: f32 = 15.0;
const FRICTION: f32 = 0.5; // Compiz has complex friction, we'll simplify
const SPRING_K: f32 = 300.0; // Stiffness

#[derive(Clone, Copy, Default, Debug)]
pub struct WobblyPoint {
    pub position: Vec2,
    pub velocity: Vec2,
    pub force: Vec2,
    pub _dragging: bool,
}

#[derive(Component)]
pub struct WobblyGrid {
    pub points: Vec<WobblyPoint>, // Flat grid GRID_WIDTH * GRID_HEIGHT
    pub target_size: Vec2,
    pub target_pos: Vec2,
}

impl Default for WobblyGrid {
    fn default() -> Self {
        Self {
            points: vec![WobblyPoint::default(); GRID_WIDTH * GRID_HEIGHT],
            target_size: Vec2::new(100.0, 100.0),
            target_pos: Vec2::ZERO,
        }
    }
}

impl WobblyGrid {
    pub fn new(pos: Vec2, size: Vec2) -> Self {
        let mut physics = Self {
            target_size: size,
            target_pos: pos,
            ..default()
        };
        physics.init_points(pos, size);
        physics
    }

    fn init_points(&mut self, pos: Vec2, size: Vec2) {
        for y in 0..GRID_HEIGHT {
            for x in 0..GRID_WIDTH {
                let px = pos.x + size.x * (x as f32 / (GRID_WIDTH - 1) as f32);
                let py = pos.y + size.y * (y as f32 / (GRID_HEIGHT - 1) as f32);
                let idx = y * GRID_WIDTH + x;
                self.points[idx].position = Vec2::new(px, py);
                self.points[idx].velocity = Vec2::ZERO;
            }
        }
    }
}

fn update_wobbly_physics(
    time: Res<Time>,
    mut query: Query<&mut WobblyGrid>,
) {
    let dt = time.delta_secs();
    if dt == 0.0 { return; }

    for mut grid in &mut query {
        let size = grid.target_size;
        let pos = grid.target_pos;

        // 1. Calculate ideal positions (anchors) and Spring forces
        for y in 0..GRID_HEIGHT {
            for x in 0..GRID_WIDTH {
                let idx = y * GRID_WIDTH + x;
                
                // Where the point SHOULD be if rigid
                let target_x = pos.x + size.x * (x as f32 / (GRID_WIDTH - 1) as f32);
                let target_y = pos.y + size.y * (y as f32 / (GRID_HEIGHT - 1) as f32);
                let target = Vec2::new(target_x, target_y);
                
                // Spring force pulling towards target
                let delta = target - grid.points[idx].position;
                let force = delta * SPRING_K;
                
                grid.points[idx].force = force;
            }
        }

        // 2. Integration (Euler)
        for point in grid.points.iter_mut() {
            let accel = point.force / MASS;
            point.velocity = (point.velocity + accel * dt) * (1.0 - FRICTION * dt * 5.0); // Damping
            point.position += point.velocity * dt;
        }
    }
}


fn update_wobbly_meshes(
    mut meshes: ResMut<Assets<Mesh>>,
    query: Query<(&WobblyGrid, &Mesh2d)>,
) {
    for (grid, mesh_handle) in &query {
        if let Some(mesh) = meshes.get_mut(&mesh_handle.0) {
            if let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
            {
                // Map grid points to mesh vertices 1:1
                // We assume the mesh was created with create_wobbly_mesh
                // which matches GRID_WIDTH x GRID_HEIGHT
                if positions.len() == grid.points.len() {
                    for (i, point) in grid.points.iter().enumerate() {
                        positions[i] = [point.position.x, point.position.y, 0.0];
                    }
                }
            }
        }
    }
}

/// Helper to create a subdivided plane mesh for the wobbly effect
pub fn create_wobbly_mesh(size: Vec2) -> Mesh {
    let mut mesh = Mesh::new(bevy::render::render_resource::PrimitiveTopology::TriangleList, bevy::render::render_asset::RenderAssetUsages::default());

    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    // Generate vertices for grid
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let px = (x as f32 / (GRID_WIDTH - 1) as f32) * size.x - size.x / 2.0; // Centered
            let py = (y as f32 / (GRID_HEIGHT - 1) as f32) * size.y - size.y / 2.0;
            
            positions.push([px, py, 0.0]);
            
            let u = x as f32 / (GRID_WIDTH - 1) as f32;
            let v = 1.0 - (y as f32 / (GRID_HEIGHT - 1) as f32); // Flip V for Bevy/Wgpu
            uvs.push([u, v]);
        }
    }

    // Generate indices (quads as two triangles)
    for y in 0..GRID_HEIGHT - 1 {
        for x in 0..GRID_WIDTH - 1 {
            let tl = (y * GRID_WIDTH + x) as u32;
            let tr = (y * GRID_WIDTH + x + 1) as u32;
            let bl = ((y + 1) * GRID_WIDTH + x) as u32;
            let br = ((y + 1) * GRID_WIDTH + x + 1) as u32;

            // First triangle
            indices.push(tl);
            indices.push(bl);
            indices.push(tr);

            // Second triangle
            indices.push(tr);
            indices.push(bl);
            indices.push(br);
        }
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));

    mesh
}

/// Demo: Spawn a wobbly window for testing
fn spawn_debug_wobbly(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    if keyboard.just_pressed(KeyCode::F10) {
        let size = Vec2::new(400.0, 300.0);
        let pos = Vec2::new(0.0, 0.0);

        // Spawn wobbly grid
        commands.spawn((
            Mesh2d(meshes.add(create_wobbly_mesh(size))),
            MeshMaterial2d(materials.add(Color::srgb(0.0, 0.5, 1.0))), // Blue window
            Transform::from_xyz(0.0, 0.0, 10.0), // Above UI
            WobblyGrid::new(pos, size),
            WobblyTestObject,
        ));
    }
}

#[derive(Component)]
struct WobblyTestObject;

fn handle_wobbly_drag(
    mut query: Query<&mut WobblyGrid, With<WobblyTestObject>>,
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if let Ok(window) = windows.get_single() {
        if let Some(cursor_pos) = window.cursor_position() {
            // Convert cursor to world space (centered origin)
            let world_pos = cursor_pos - Vec2::new(window.width() / 2.0, window.height() / 2.0);
            let world_pos = Vec2::new(world_pos.x, -world_pos.y); // Flip Y

            for mut grid in &mut query {
                if mouse.pressed(MouseButton::Left) {
                    // If dragging, pull the nearest point or center
                    // Simple version: Move the rigid target, physics follows
                    grid.target_pos = world_pos;
                }
            }
        }
    }
}
