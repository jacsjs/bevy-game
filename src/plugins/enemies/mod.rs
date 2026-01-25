//! Enemies plugin: spawns static targets.

use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;
use avian2d::prelude::*;

use crate::common::{layers::Layer, state::GameState};

#[derive(Component)]
pub struct Enemy;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::InGame), spawn_targets);
}

fn spawn_targets(mut commands: Commands) {
    let enemy_layers = CollisionLayers::new(Layer::Enemy, [Layer::World, Layer::Player, Layer::PlayerBullet]);

    for (i, x) in [-200.0, 0.0, 200.0].into_iter().enumerate() {
        commands.spawn((
            Name::new(format!("EnemyTarget{i}")),
            Enemy,
            Sprite { color: Color::srgb(0.9, 0.25, 0.25), custom_size: Some(Vec2::splat(32.0)), ..default() },
            Transform::from_xyz(x, 120.0, 1.0),
            RigidBody::Static,
            Collider::circle(16.0),
            enemy_layers,
            DespawnOnExit(GameState::InGame),
        ));
    }
}

#[cfg(test)]
mod tests;