//! Collision processing using Avian collision layers.
//!
//! ## Why layers, not markers?
//! All collidable entities already have `CollisionLayers`. We classify hits by checking
//! `CollisionLayers.memberships` (a bitmask) instead of adding redundant marker components.
//!
//! ## Hierarchy-ready (hitboxes)
//! Avian's `CollisionStart` includes both collider entities (`collider1/collider2`) and the
//! rigid bodies they are attached to (`body1/body2`). If you later move colliders to children,
//! use `bodyX` as the gameplay owner.

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::platform::collections::HashSet;

use super::layers::Layer;
use super::components::{Armour, Bullet, Health, PooledBullet, ReturnToPool};

/// Resolved collision participant.
///
/// - `collider`: the collider entity reported by the event
/// - `body`: optional rigid body entity the collider is attached to
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

/// Player-bullet collision processing.
///
/// Rules (Phase A + Phase B):
/// - World: decrement `wall_bounces_left`; when it reaches 0 => `ReturnToPool`
/// - Enemy: Armour gate: if armour hits > 0 => wear 1 and bullet continues
///          else => apply damage and `ReturnToPool`
///
/// Scheduling:
/// - Run this after Avian's collision events are triggered (see docs: `CollisionEventSystems`).
/// - Then run your `return_to_pool_commit` after this.
pub fn process_player_bullet_collisions(
    mut commands: Commands,
    mut started: MessageReader<CollisionStart>,

    // NOTE: if bullets become child-colliders later, move Bullet component to body and
    // use CollisionTarget::gameplay_owner() for bullet ownership as well.
    mut q_bullets: Query<&mut Bullet, With<PooledBullet>>,

    // Read layers from collider entities.
    q_layers: Query<&CollisionLayers>,

    // Gameplay state (on body entities).
    mut q_armour: Query<&mut Armour>,
    mut q_health: Query<&mut Health>,

    // Efficient per-frame dedupe with allocation reuse.
    mut seen: Local<HashSet<Entity>>,
) {
    seen.clear();

    for ev in started.read() {
        let (t1, t2) = targets(&ev);

        // Identify bullet side by presence of Bullet on the collider entity.
        let (bullet_side, other_side) = if q_bullets.get_mut(t1.collider).is_ok() {
            (t1, t2)
        } else if q_bullets.get_mut(t2.collider).is_ok() {
            (t2, t1)
        } else {
            continue;
        };

        // Deduplicate per bullet collider.
        if !seen.insert(bullet_side.collider) {
            continue;
        }

        let Ok(other_layers) = q_layers.get(other_side.collider) else {
            continue;
        };

        let Ok(mut bullet) = q_bullets.get_mut(bullet_side.collider) else {
            continue;
        };

        // WORLD: bounce budget
        if is_in_layer(other_layers, Layer::World) {
            bullet.wall_bounces_left = bullet.wall_bounces_left.saturating_sub(1);
            if bullet.wall_bounces_left == 0 {
                commands.entity(bullet_side.collider).insert(ReturnToPool);
            }
            continue;
        }

        // ENEMY: armour gate -> damage
        if is_in_layer(other_layers, Layer::Enemy) {
            let enemy_entity = other_side.gameplay_owner();

            if let Ok(mut armour) = q_armour.get_mut(enemy_entity) {
                if armour.hits_remaining > 0 {
                    armour.hits_remaining = armour.hits_remaining.saturating_sub(1);
                    // Bullet continues (ricochet) while armour is up.
                    continue;
                }
            }

            if let Ok(mut hp) = q_health.get_mut(enemy_entity) {
                hp.hp -= bullet.damage;
            }

            commands.entity(bullet_side.collider).insert(ReturnToPool);
            continue;
        }

        // Else ignore.
    }
}
