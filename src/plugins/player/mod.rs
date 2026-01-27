//! Player plugin.
//!
//! Pipeline:
//! - Update: sample input, write PlayerInput resource
//! - FixedUpdate: apply velocity to kinematic rigid body
//!
//! API note (Bevy >= 0.18):
//! - Prefer the `Single` SystemParam (and `Option<Single<...>>`) for single-entity access.
//!   `Single` fails validation if 0 or >1 entities match, and `Option<Single>` lets you
//!   explicitly handle the "missing" case without panics.

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;

use crate::{common::{state::GameState, tunables::Tunables}, plugins::projectiles::{components::Player, layers::Layer}};

#[derive(Resource, Default, Debug)]
struct PlayerInput {
    move_axis: Vec2,
}

pub fn plugin(app: &mut App) {
    app.insert_resource(PlayerInput::default())
        .add_systems(OnEnter(GameState::InGame), spawn)
        // 1) sample input early to avoid phase mismatch with fixed physics
        .add_systems(PreUpdate, gather_input)
        // 2) write velocity in the same schedule physics uses, right before the step
        // Issue: https://github.com/avianphysics/avian/issues/358
        .add_systems(
            FixedPostUpdate,
            apply_movement.before(PhysicsSystems::StepSimulation),
        );
}

fn spawn(mut commands: Commands) {
    let layers = CollisionLayers::new(
        Layer::Player,
        [Layer::World, Layer::Enemy, Layer::EnemyBullet],
    );

    commands.spawn((
        Name::new("Player"),
        Player,
        Sprite {
            color: Color::srgb(0.2, 0.75, 0.9),
            custom_size: Some(Vec2::splat(26.0)),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 1.0),
        RigidBody::Dynamic,
        Collider::circle(13.0),
        layers,
        LockedAxes::ROTATION_LOCKED,
        Restitution::ZERO,
        Friction::ZERO,
        LinearVelocity::ZERO,
        // Smooth translation between fixed physics ticks
        TranslationExtrapolation,
        CollisionEventsEnabled,
        DespawnOnExit(GameState::InGame),
    ));
}

fn gather_input(keys: Option<Res<ButtonInput<KeyCode>>>, mut input: ResMut<PlayerInput>) {
    let Some(keys) = keys else {
        return;
    };

    let mut axis = Vec2::ZERO;

    if keys.pressed(KeyCode::KeyW) {
        axis.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        axis.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        axis.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        axis.x += 1.0;
    }

    input.move_axis = if axis.length_squared() > 0.0 {
        axis.normalize()
    } else {
        Vec2::ZERO
    };
}

fn apply_movement(
    tunables: Res<Tunables>,
    input: Res<PlayerInput>,
    mut q_player: Query<&mut LinearVelocity, With<Player>>,
) {
    if let Ok(mut vel) = q_player.single_mut() {
        vel.0 = input.move_axis * tunables.player_speed;
    }
}

#[cfg(test)]
mod tests;
