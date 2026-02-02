use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::ecs::message::MessageReader;

use super::components::{Bullet, BulletState, PooledBullet};
use super::messages::{BulletKind, SpawnBulletRequest};
use super::pool::{active_enemy_layers, active_player_layers, BulletPool};

/// Consumer: read SpawnBulletRequest messages and activate bullets from the pool.
///
/// This is the **only** system that pops from BulletPool during spawning.
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
        let Some(e) = pool.free.pop() else { continue; };
        let Ok((mut state, mut bullet, mut tf, mut vel, mut vis, mut layers)) = q.get_mut(e) else { continue; };

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
