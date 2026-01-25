//! World plugin: spawns arena walls.

use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;
use avian2d::prelude::*;

use crate::common::{layers::Layer, state::GameState};

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::InGame), spawn_arena);
}

fn spawn_arena(mut commands: Commands) {
    let wall_color = Color::srgb(0.25, 0.27, 0.33);
    let half_w = 520.0;
    let half_h = 300.0;
    let thickness = 30.0;

    let wall_layers = CollisionLayers::new(Layer::World, [Layer::Player, Layer::Enemy, Layer::PlayerBullet, Layer::EnemyBullet]);

    let mut spawn_wall = |name: String, pos: Vec3, size: Vec2| {
        commands.spawn((
            Name::new(name),
            Sprite { color: wall_color, custom_size: Some(size), ..default() },
            Transform::from_translation(pos),
            RigidBody::Static,
            Collider::rectangle(size.x, size.y),
            wall_layers,
            DespawnOnExit(GameState::InGame),
        ));
    };

    spawn_wall("WallTop".into(), Vec3::new(0.0, half_h + thickness * 0.5, 0.0), Vec2::new(half_w * 2.0 + thickness * 2.0, thickness));
    spawn_wall("WallBottom".into(), Vec3::new(0.0, -half_h - thickness * 0.5, 0.0), Vec2::new(half_w * 2.0 + thickness * 2.0, thickness));
    spawn_wall("WallLeft".into(), Vec3::new(-half_w - thickness * 0.5, 0.0, 0.0), Vec2::new(thickness, half_h * 2.0));
    spawn_wall("WallRight".into(), Vec3::new(half_w + thickness * 0.5, 0.0, 0.0), Vec2::new(thickness, half_h * 2.0));
}

#[cfg(test)]
mod tests;