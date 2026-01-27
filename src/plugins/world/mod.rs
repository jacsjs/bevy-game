//! World plugin: spawns arena walls.

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;

use crate::common::state::GameState;
use crate::plugins::projectiles::layers::Layer;

const TILE: i32 = 64;
const HALF_W: i32 = TILE * 16;
const HALF_H: i32 = TILE * 9;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::InGame), spawn_arena);
    app.add_systems(OnEnter(GameState::InGame), spawn_floor);
}

fn spawn_arena(mut commands: Commands) {
    let wall_color = Color::srgb(0.25, 0.27, 0.33);
    let thickness = 30.0;

    let wall_layers = CollisionLayers::new(
        Layer::World,
        [
            Layer::Player,
            Layer::Enemy,
            Layer::PlayerBullet,
            Layer::EnemyBullet,
        ],
    );

    let mut spawn_wall = |name: String, pos: Vec3, size: Vec2| {
        commands.spawn((
            Name::new(name),
            Sprite {
                color: wall_color,
                custom_size: Some(size),
                ..default()
            },
            Transform::from_translation(pos),
            RigidBody::Static,
            Collider::rectangle(size.x, size.y),
            wall_layers,
            DespawnOnExit(GameState::InGame),
        ));
    };

    spawn_wall(
        "WallTop".into(),
        Vec3::new(0.0, HALF_H as f32 + thickness * 0.5, 0.0),
        Vec2::new(HALF_W as f32 * 2.0 + thickness * 2.0, thickness),
    );
    spawn_wall(
        "WallBottom".into(),
        Vec3::new(0.0, -HALF_H as f32 - thickness * 0.5, 0.0),
        Vec2::new(HALF_W as f32 * 2.0 + thickness * 2.0, thickness),
    );
    spawn_wall(
        "WallLeft".into(),
        Vec3::new(-HALF_W as f32 - thickness * 0.5, 0.0, 0.0),
        Vec2::new(thickness, HALF_H as f32 * 2.0),
    );
    spawn_wall(
        "WallRight".into(),
        Vec3::new(HALF_W as f32 + thickness * 0.5, 0.0, 0.0),
        Vec2::new(thickness, HALF_H as f32 * 2.0),
    );
}

/// Spawn a simple floor grid.
///
/// We intentionally build the floor from solid-color sprites so the project has no assets.
/// Later, you can replace these with textured tiles or a proper tilemap.
fn spawn_floor(mut commands: Commands) {
    // Rust-idiomatic iteration: generate positions via iterators.
    (-(HALF_H / TILE)..=HALF_H / TILE)
        .flat_map(|y| (-(HALF_W / TILE)..=HALF_W / TILE).map(move |x| (x, y)))
        .for_each(|(x, y)| {
            let world_pos = Vec3::new(x as f32 * TILE as f32, y as f32 * TILE as f32, 0.0);
            let color = if (x + y) % 2 == 0 {
                Color::srgb(0.14, 0.14, 0.16)
            } else {
                Color::srgb(0.12, 0.12, 0.14)
            };

            commands.spawn((
                Sprite::from_color(color, Vec2::splat(TILE as f32)),
                Transform::from_translation(world_pos),
            ));
        });
}

#[cfg(test)]
mod tests;
