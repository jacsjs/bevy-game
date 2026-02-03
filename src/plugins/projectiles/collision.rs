//! Collision resolve: apply gameplay rules and mark bullets for return.
//!
//! # Hot path design
//! - No HashSet dedupe: we use `CollisionStamp` + `CollisionEpoch`.
//! - Fail-fast for impossible states: if a collider is a pooled bullet, it must have bullet data.
//!
//! # Rule summary
//! - World: decrement wall bounce budget; at 0 => PendingReturn
//! - Enemy: armour gate; if armour up => wear; else apply damage and PendingReturn

use avian2d::prelude::*;
use bevy::prelude::*;

use super::components::{Armour, Bullet, BulletState, CollisionEpoch, CollisionStamp, Health, PooledBullet};
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
    mut epoch: ResMut<CollisionEpoch>,
    q_is_bullet: Query<(), With<PooledBullet>>,
    mut q_bullet: Query<(&mut Bullet, &mut BulletState, &mut CollisionStamp), With<PooledBullet>>,
    q_layers: Query<&CollisionLayers>,
    mut q_armour: Query<&mut Armour>,
    mut q_health: Query<&mut Health>,
) {
    epoch.0 = epoch.0.wrapping_add(1);
    let cur_epoch = epoch.0;

    for ev in started.read() {
        let (t1, t2) = targets(ev);

        let b1 = q_is_bullet.contains(t1.collider);
        let b2 = q_is_bullet.contains(t2.collider);
        if !(b1 ^ b2) { continue; }
        let (bullet_side, other_side) = if b1 { (t1, t2) } else { (t2, t1) };

        let (mut bullet, mut state, mut stamp) =
            q_bullet.get_mut(bullet_side.collider)
                .expect("Bullet collider missing required pooled bullet components");

        // Dedupe per bullet per resolve run
        if stamp.last_epoch == cur_epoch { continue; }
        stamp.last_epoch = cur_epoch;

        let other_layers = q_layers.get(other_side.collider)
            .expect("Collider missing CollisionLayers");

        if *state != BulletState::Active { continue; }

        if is_in_layer(other_layers, Layer::World) {
            bullet.wall_bounces_left = bullet.wall_bounces_left.saturating_sub(1);
            if bullet.wall_bounces_left == 0 {
                *state = BulletState::PendingReturn;
            }
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
