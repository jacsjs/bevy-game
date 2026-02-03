//! Return commit: recycle bullets back into the pool.
//!
//! This system is the "owner" of the *Inactive invariants*.
//!
//! Invariant: Inactive bullets must be:
//! - hidden
//! - velocity = 0
//! - collide with nothing (filters empty)
//!
//! Centralizing these writes here prevents inconsistencies.

use avian2d::prelude::*;
use bevy::prelude::*;

use super::components::{BulletEntity, BulletState, PooledBullet};
use super::pool::{inactive_bullet_layers, BulletPool};

pub fn return_to_pool_commit(
    mut pool: ResMut<BulletPool>,
    mut q: Query<(
        Entity,
        &mut BulletState,
        &mut Visibility,
        &mut LinearVelocity,
        &mut CollisionLayers,
    ), With<PooledBullet>>,
) {
    for (e, mut state, mut vis, mut vel, mut layers) in &mut q {
        if *state != BulletState::PendingReturn { continue; }

        *state = BulletState::Inactive;
        *vis = Visibility::Hidden;
        vel.0 = Vec2::ZERO;
        *layers = inactive_bullet_layers();

        pool.push_free(BulletEntity(e));
    }
}
