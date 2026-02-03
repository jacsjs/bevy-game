//! Projectile-related component and resource types.
//!
//! # "Type system to express invariants" (ECS edition)
//! Rust can't prove ECS world facts at compile time (entities & component presence are runtime).
//! But we can still use the type system to:
//!
//! - **Name things** (newtypes): avoid mixing raw primitives accidentally.
//! - **Encode state** (enums): avoid contradictory flag combinations.
//! - **Make intent explicit** (resource handles): avoid repeated singleton queries.
//!
//! This module provides:
//! - `BulletState`: lifecycle enum.
//! - `BulletEntity`: newtype wrapper for pooled bullet entities.
//! - `CollisionStamp` + `CollisionEpoch`: data-driven dedupe (removes HashSet from hot loop).
//! - `PlayerEntity` / `MainCameraEntity`: handles stored once at spawn time.
//! - `Aim`: normalized cursor-in-world (single source of truth).

use bevy::prelude::*;

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct Enemy;

#[derive(Component)]
pub struct PooledBullet;

/// Bullet lifecycle state: always present.
///
/// **Invariant:** pooled bullet entities always have `BulletState`.
///
/// *Why enum?*
/// - avoids contradictory booleans (e.g., `is_active` + `is_returning` simultaneously)
/// - makes lifecycle explicit and easy to reason about
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletState {
    /// In pool: hidden and non-interacting.
    Inactive,
    /// In play: visible and colliding.
    Active,
    /// Marked by collision logic; recycled by commit system.
    PendingReturn,
}

impl Default for BulletState {
    fn default() -> Self {
        Self::Inactive
    }
}

/// Bullet gameplay state.
#[derive(Component, Debug, Clone)]
pub struct Bullet {
    pub damage: i32,
    pub wall_bounces_left: u8,
}

impl Bullet {
    pub const DEFAULT_WALL_BOUNCES: u8 = 3;

    #[inline]
    pub fn reset_for_fire(&mut self, damage: i32) {
        self.damage = damage;
        self.wall_bounces_left = Self::DEFAULT_WALL_BOUNCES;
    }
}

#[derive(Component, Debug, Clone)]
pub struct Armour {
    pub hits_remaining: u16,
    pub max_hits: u16,
}

impl Armour {
    #[inline]
    pub fn is_up(&self) -> bool {
        self.hits_remaining > 0
    }

    #[inline]
    pub fn wear_one(&mut self) {
        self.hits_remaining = self.hits_remaining.saturating_sub(1);
    }
}

#[derive(Component, Debug, Clone)]
pub struct Health {
    pub hp: i32,
}

/// Newtype for pooled bullet entities.
///
/// This encodes an important invariant:
/// > `BulletPool.free` contains **only** pooled bullet entities.
///
/// It doesn't prove component presence at compile time (ECS is runtime),
/// but it prevents mixing arbitrary `Entity` values in APIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BulletEntity(pub Entity);

/// Per-bullet stamp used to dedupe collision processing without HashSet.
///
/// **Idea:** store "already processed this tick" on the bullet itself.
/// This is a data-driven alternative to allocating/clearing a HashSet every tick.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct CollisionStamp {
    pub last_epoch: u32,
}

/// Global epoch incremented once per collision-resolve run.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct CollisionEpoch(pub u32);

/// Stored handles to avoid repeated singleton query scanning.
///
/// This is the ECS analog of "validated constructor":
/// we set this once at spawn time and then `expect()` on use.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct PlayerEntity(pub Option<Entity>);

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct MainCameraEntity(pub Option<Entity>);

/// Normalized aim data: one source of truth for cursor-in-world.
///
/// 3NF intuition:
/// - camera/window math happens in one place
/// - consumers read Aim rather than recomputing
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct Aim {
    pub world_cursor: Option<Vec2>,
}
