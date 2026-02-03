//! Spawn consumer: activate bullets from the pool.
//!
//! # Fail-fast invariants
//! - The pool free list contains only valid pooled bullet entities.
//! - Therefore, a pooled entity must match the bullet query.
//!
//! If this is violated, we `expect()` and crash loudly.
//! This removes branches from the hot loop and makes invariant violations obvious.

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::ecs::message::MessageReader;

use super::components::{Bullet, BulletEntity, BulletState, PooledBullet};
use super::messages::{BulletKind, SpawnBulletRequest};
use super::pool::{active_enemy_layers, active_player_layers, BulletPool};

pub fn allocate_bullets_from_pool(
    mut pool: ResMut<BulletPool>,
    mut reader: MessageReader<SpawnBulletRequest>,
    mut q: Query<(
        &mut BulletState,
        &mut Bullet,
        &mut Transform,
        &mut LinearVelocity,
        &mut Visibility,
        &mut CollisionLayers,
    ), With<PooledBullet>>,
) {
    for req in reader.read() {
        let Some(BulletEntity(e)) = pool.pop_free() else {
            // Capacity decision, not a correctness failure.
            continue;
        };

        let (mut state, mut bullet, mut tf, mut vel, mut vis, mut layers) =
            q.get_mut(e).expect("BulletPool contained an entity missing pooled bullet components");

        *state = BulletState::Active;
        bullet.reset_for_fire(req.damage);
        tf.translation = req.pos.extend(2.0);
        vel.0 = req.vel;
        *vis = Visibility::Visible;

        *layers = match req.kind {
            BulletKind::Player => active_player_layers(),
            BulletKind::Enemy => active_enemy_layers(),
        };
    }
}
