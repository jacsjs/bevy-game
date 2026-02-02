use avian2d::prelude::*;
use bevy::prelude::*;

use super::components::{BulletState, PooledBullet};
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
        pool.free.push(e);
    }
}
