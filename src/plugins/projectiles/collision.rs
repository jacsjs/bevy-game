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
        CollisionTarget {
            collider: ev.collider1,
            body: ev.body1,
        },
        CollisionTarget {
            collider: ev.collider2,
            body: ev.body2,
        },
    )
}

#[inline]
fn is_in_layer(layers: &CollisionLayers, layer: Layer) -> bool {
    layers.memberships.has_all(layer)
}

pub fn process_player_bullet_collisions(
    mut started: MessageReader<CollisionStart>,
    // Fast “is this a pooled bullet?” check
    q_is_bullet: Query<(), With<PooledBullet>>,
    // Bullet data + state
    mut q_bullets: Query<(&mut Bullet, &mut BulletState), With<PooledBullet>>,
    // Read layers from collider entities
    q_layers: Query<&CollisionLayers>,
    // Gameplay state
    mut q_armour: Query<&mut Armour>,
    mut q_health: Query<&mut Health>,
    // Per-frame dedupe
    mut seen: Local<HashSet<Entity>>,
) {
    seen.clear();

    for ev in started.read() {
        let (t1, t2) = targets(ev);

        // Identify bullet side without get_mut probing
        let b1 = q_is_bullet.contains(t1.collider);
        let b2 = q_is_bullet.contains(t2.collider);
        if !(b1 ^ b2) {
            continue; // must be exactly one bullet
        }
        let (bullet_side, other_side) = if b1 { (t1, t2) } else { (t2, t1) };

        // Deduplicate per bullet collider
        if !seen.insert(bullet_side.collider) {
            continue;
        }

        let Ok(other_layers) = q_layers.get(other_side.collider) else {
            continue;
        };

        let Ok((mut bullet, mut state)) = q_bullets.get_mut(bullet_side.collider) else {
            continue;
        };

        // Ignore if somehow not active (shouldn't happen with empty filters, but safe)
        if *state != BulletState::Active {
            continue;
        }

        // WORLD: bounce budget
        if is_in_layer(other_layers, Layer::World) {
            bullet.wall_bounces_left = bullet.wall_bounces_left.saturating_sub(1);
            if bullet.wall_bounces_left == 0 {
                *state = BulletState::PendingReturn;
            }
            continue;
        }

        // ENEMY: armour gate -> damage
        if is_in_layer(other_layers, Layer::Enemy) {
            let enemy_entity = other_side.gameplay_owner();

            if let Ok(mut armour) = q_armour.get_mut(enemy_entity) {
                if armour.hits_remaining > 0 {
                    armour.hits_remaining = armour.hits_remaining.saturating_sub(1);
                    continue; // bullet continues
                }
            }

            if let Ok(mut hp) = q_health.get_mut(enemy_entity) {
                hp.hp -= bullet.damage;
            }

            *state = BulletState::PendingReturn;
            continue;
        }
    }
}