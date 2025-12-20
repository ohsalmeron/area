//! Animation utilities for the shell

use bevy::prelude::*;

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (animate_nodes, animate_transforms));
    }
}

/// Component to animate a UI node's properties
#[derive(Component)]
pub struct UiAnimation {
    pub target_opacity: Option<f32>,
    pub target_scale: Option<Vec3>,
    pub speed: f32,
}

impl Default for UiAnimation {
    fn default() -> Self {
        Self {
            target_opacity: None,
            target_scale: None,
            speed: 5.0,
        }
    }
}

fn animate_nodes(
    time: Res<Time>,
    mut query: Query<(&mut BackgroundColor, &UiAnimation)>,
) {
    for (mut bg, anim) in &mut query {
        if let Some(target) = anim.target_opacity {
            let current = bg.0.alpha();
            let new_alpha = current + (target - current) * anim.speed * time.delta_secs();
            bg.0.set_alpha(new_alpha);
        }
    }
}

fn animate_transforms(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &UiAnimation)>,
) {
    for (mut transform, anim) in &mut query {
        if let Some(target) = anim.target_scale {
            transform.scale = transform.scale.lerp(target, anim.speed * time.delta_secs());
        }
    }
}
