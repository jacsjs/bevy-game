//! Player plugin (invariant-based edition).
//!
//! # Goal
//! Eliminate per-tick singleton scans and branchy "maybe" access in movement logic.
//! Instead, we treat the presence of exactly one player as an **invariant** and store
//! the player entity handle once at spawn time.
//!
//! ```text
//!   OnEnter(InGame): spawn player entity -> write PlayerEntity resource
//!   PreUpdate:       gather input -> PlayerInput
//!   FixedPostUpdate: apply movement -> Query::get_mut(PlayerEntity)
//! ```

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;

use crate::{
    common::{state::GameState, tunables::Tunables},
    plugins::projectiles::{
        components::{Player, PlayerEntity},
        layers::Layer,
    },
};

#[derive(Resource, Default, Debug)]
struct PlayerInput {
    move_axis: Vec2,
}

pub fn plugin(app: &mut App) {
    app.insert_resource(PlayerInput::default())
        .add_systems(OnEnter(GameState::InGame), spawn)
        .add_systems(PreUpdate, gather_input)
        .add_systems(
            FixedPostUpdate,
            apply_movement
                .before(PhysicsSystems::StepSimulation)
                .run_if(in_state(GameState::InGame)),
        );
}

fn spawn(mut commands: Commands) {
    let layers = CollisionLayers::new(
        Layer::Player,
        [Layer::World, Layer::Enemy, Layer::EnemyBullet],
    );

    let e = commands
        .spawn((
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
            TranslationExtrapolation,
            CollisionEventsEnabled,
            DespawnOnExit(GameState::InGame),
        ))
        .id();

    // Fail-fast invariant: exactly one player while in InGame.
    commands.insert_resource(PlayerEntity(Some(e)));
}

fn gather_input(keys: Option<Res<ButtonInput<KeyCode>>>, mut input: ResMut<PlayerInput>) {
    let Some(keys) = keys else { return; };

    let mut axis = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) { axis.y += 1.0; }
    if keys.pressed(KeyCode::KeyS) { axis.y -= 1.0; }
    if keys.pressed(KeyCode::KeyA) { axis.x -= 1.0; }
    if keys.pressed(KeyCode::KeyD) { axis.x += 1.0; }

    input.move_axis = if axis.length_squared() > 0.0 {
        axis.normalize()
    } else {
        Vec2::ZERO
    };
}

fn apply_movement(
    tunables: Res<Tunables>,
    input: Res<PlayerInput>,
    player_e: Res<PlayerEntity>,
    mut q_vel: Query<&mut LinearVelocity>,
) {
    let player = player_e.0.expect("PlayerEntity not set (spawn invariant violated)");
    let mut vel = q_vel.get_mut(player).expect("PlayerEntity invalid");
    vel.0 = input.move_axis * tunables.player_speed;
}

#[cfg(test)]
mod tests;
