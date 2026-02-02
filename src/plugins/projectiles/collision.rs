use avian2d::prelude::*;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;

use super::components::{Armour, Bullet, BulletState, Health, PooledBullet};
use super::layers::Layer;

#[derive(Clone, Copy, Debug)]
struct CollisionTarget {
    collider: Entity,
    body: Option<Entity>,
}

impl CollisionTarget {
    #[inline]
    fn gameplay_owner(self) -> Entity {
        self.body.unwrap_or(self.collider)
    }
}

#[inline]
fn targets(ev: &CollisionStart) -> (CollisionTarget, CollisionTarget) {
    (
        CollisionTarget { collider: ev.collider1, body: ev.body1 },
        CollisionTarget { collider: ev.collider2, body: ev.body2 },
    )
}

#[inline]
fn is_in_layer(layers: &CollisionLayers, layer: Layer) -> bool {
    layers.memberships.has_all(layer)
}

pub fn process_player_bullet_collisions(
    mut started: MessageReader<CollisionStart>,
    q_is_bullet: Query<(), With<PooledBullet>>,
    mut q_bullet_data: Query<(&mut Bullet, &mut BulletState), With<PooledBullet>>,
    q_layers: Query<&CollisionLayers>,
    mut q_armour: Query<&mut Armour>,
    mut q_health: Query<&mut Health>,
    mut seen: Local<HashSet<Entity>>,
) {
    seen.clear();

    for ev in started.read() {
        let (t1, t2) = targets(ev);

        let b1 = q_is_bullet.contains(t1.collider);
        let b2 = q_is_bullet.contains(t2.collider);
        if !(b1 ^ b2) { continue; }
        let (bullet_side, other_side) = if b1 { (t1, t2) } else { (t2, t1) };

        if !seen.insert(bullet_side.collider) { continue; }

        let Ok(other_layers) = q_layers.get(other_side.collider) else { continue; };
        let Ok((mut bullet, mut state)) = q_bullet_data.get_mut(bullet_side.collider) else { continue; };

        if *state != BulletState::Active { continue; }

        if is_in_layer(other_layers, Layer::World) {
            bullet.wall_bounces_left = bullet.wall_bounces_left.saturating_sub(1);
            if bullet.wall_bounces_left == 0 { *state = BulletState::PendingReturn; }
            continue;
        }

        if is_in_layer(other_layers, Layer::Enemy) {
            let enemy_entity = other_side.gameplay_owner();

            if let Ok(mut armour) = q_armour.get_mut(enemy_entity) {
                if armour.hits_remaining > 0 {
                    armour.hits_remaining = armour.hits_remaining.saturating_sub(1);
                    continue;
                }
            }

            if let Ok(mut hp) = q_health.get_mut(enemy_entity) {
                hp.hp -= bullet.damage;
            }

            *state = BulletState::PendingReturn;
        }
    }
}
