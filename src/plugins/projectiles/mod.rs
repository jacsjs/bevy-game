//! Projectiles plugin (v3): **Message-based producer → consumer** spawning + data-driven pooling.
//!
//! # Philosophy: invariants first
//! This module tree is intentionally designed to **push correctness checks to boundaries** and
//! keep **hot paths** (allocation, collision resolve, return commit) as straight-line as possible.
//!
//! In an ECS, you can't make "this entity exists and has these components" a compile-time fact.
//! But you *can*:
//! - encode **meaning** with types (newtypes / enums),
//! - validate invariants once (spawn / state transition),
//! - and then treat violations as bugs (fail-fast `expect()`),
//! which removes a lot of runtime branching from hot loops.
//!
//! # Data flow (big picture)
//! ```text
//!   Update schedule (variable dt)
//!┌────────────────────────────────────────────────────────────────────────────┐
//!│  (A) Aim Update (normalize cursor → world space)                           │
//!│      - reads: Window cursor position, MainCameraEntity                     │
//!│      - writes: Aim { world_cursor: Option<Vec2> }                          │
//!│                                                                            │
//!│  (B) Producer: request_player_bullets                                      │
//!│      - reads: MouseButton input, PlayerEntity, Aim, Player Transform       │
//!│      - writes: SpawnBulletRequest message                                  │
//!│                                                                            │
//!│  (C) Consumer: allocate_bullets_from_pool                                  │
//!│      - reads: SpawnBulletRequest messages                                  │
//!│      - mutates: BulletPool.free (Vec<BulletEntity>)                        │
//!│      - mutates: BulletState, Bullet, Transform, Velocity, Visibility,      │
//!│                 CollisionLayers                                            │
//!└────────────────────────────────────────────────────────────────────────────┘
//!                │
//!                v
//!FixedPostUpdate (fixed dt)
//!┌────────────────────────────────────────────────────────────────────────────┐
//!│  (D) Physics emits CollisionStart messages (Avian)                         │
//!│                                                                            │
//!│  (E) Resolve collisions: process_player_bullet_collisions                  │
//!│      - reads: CollisionStart messages                                      │
//!│      - reads: layers/armour/health                                         │
//!│      - mutates: BulletState -> PendingReturn                               │
//!│      - dedupe: CollisionStamp + CollisionEpoch (no HashSet)                │
//!│                                                                            │
//!│  (F) Commit returns: return_to_pool_commit                                 │
//!│      - reads: bullets with PendingReturn                                   │
//!│      - writes invariants for Inactive state                                │
//!│      - mutates: BulletPool.free.push(BulletEntity)                         │
//!└────────────────────────────────────────────────────────────────────────────┘
//!
//! Feedback loop:
//!   commit pushes BulletEntity back into BulletPool.free
//!   allocator pops BulletEntity from BulletPool.free
//! ```
//!
//! # Why "Messages" instead of direct pool access?
//! Producers do **not** borrow `ResMut<BulletPool>`.
//! They only enqueue intent (SpawnBulletRequest).
//! The allocator is the **single writer** that mutates the pool.
//! This improves decoupling and keeps pool mutation localized.
//!
//! # Where do we still branch?
//! - Real-world input: cursor can be missing (outside window) → Aim becomes None.
//! - Capacity: pool can be empty → allocator drops request (capacity decision).
//! Everything else is treated as an invariant violation.
//! BulletState (explicit enum)

pub mod layers;
pub mod components;
pub mod pool;
pub mod collision;

// v3 message-based spawn pipeline
pub mod messages;
pub mod request;
pub mod allocator;
pub mod commit;

use bevy::prelude::*;
use bevy::ecs::message::Messages;
use avian2d::collision::narrow_phase::CollisionEventSystems;

use crate::common::state::GameState;

pub struct ProjectilesPlugin;

/// Maintain spawn request message buffers.
///
/// Messages are double-buffered; `update()` advances buffers.
fn update_spawn_messages(mut msgs: ResMut<Messages<messages::SpawnBulletRequest>>) {
    msgs.update();
}

impl Plugin for ProjectilesPlugin {
    fn build(&self, app: &mut App) {
        // Pool + pre-spawn
        app.insert_resource(pool::BulletPool::new(512))
            .insert_resource(components::CollisionEpoch::default())
            .insert_resource(components::Aim::default())
            .add_systems(Startup, pool::init_bullet_pool);

        // Message storage for spawn requests.
        app.init_resource::<Messages<messages::SpawnBulletRequest>>();
        app.add_systems(PostUpdate, update_spawn_messages);

        // Update-phase pipeline: aim -> request -> allocate
        app.add_systems(
            Update,
            request::update_aim_from_cursor
                .run_if(in_state(GameState::InGame)),
        );

        app.add_systems(
            Update,
            (
                request::request_player_bullets,
                allocator::allocate_bullets_from_pool.after(request::request_player_bullets),
            )
                .run_if(in_state(GameState::InGame)),
        );

        // Fixed collision pipeline
        app.add_systems(
            FixedPostUpdate,
            collision::process_player_bullet_collisions
                .after(CollisionEventSystems)
                .run_if(in_state(GameState::InGame)),
        )
        .add_systems(
            FixedPostUpdate,
            commit::return_to_pool_commit
                .after(collision::process_player_bullet_collisions)
                .run_if(in_state(GameState::InGame)),
        );
    }
}
