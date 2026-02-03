//! Bullet pooling.
//!
//! # Invariants
//! - `BulletPool.free` stores only `BulletEntity` (typed free list).
//! - pooled bullet entities are spawned once and never despawned individually.
//! - "inactive" bullets are hidden, have zero velocity, and collide with nothing.
//!
//! # Performance
//! - pooling avoids spawn/despawn churn
//! - Option A disable (collision filters empty) avoids structural enable/disable toggles

use avian2d::prelude::*;
use bevy::prelude::*;

use super::components::{Bullet, BulletEntity, BulletState, CollisionStamp, PooledBullet};
use super::layers::Layer;

#[derive(Resource, Debug)]
pub struct BulletPool {
    pub free: Vec<BulletEntity>,
    pub capacity: usize,
}

impl BulletPool {
    pub fn new(capacity: usize) -> Self {
        Self { free: Vec::with_capacity(capacity), capacity }
    }

    #[inline]
    pub fn pop_free(&mut self) -> Option<BulletEntity> {
        self.free.pop()
    }

    #[inline]
    pub fn push_free(&mut self, e: BulletEntity) {
        self.free.push(e)
    }
}

#[inline]
pub fn active_player_layers() -> CollisionLayers {
    CollisionLayers::new(Layer::PlayerBullet, [Layer::World, Layer::Enemy])
}

#[inline]
pub fn active_enemy_layers() -> CollisionLayers {
    CollisionLayers::new(Layer::EnemyBullet, [Layer::World, Layer::Player])
}

#[inline]
pub fn inactive_bullet_layers() -> CollisionLayers {
    CollisionLayers::new(Layer::PlayerBullet, [] as [Layer; 0])
}

pub fn init_bullet_pool(mut commands: Commands, mut pool: ResMut<BulletPool>) {
    pool.free.clear();
    let cap = pool.capacity;
    pool.free.reserve(cap);

    let restitution = Restitution::new(0.95).with_combine_rule(CoefficientCombine::Max);
    let friction = Friction::ZERO;

    for _ in 0..cap {
        let e = commands.spawn((
            Name::new("Bullet(Pooled)"),
            PooledBullet,
            BulletState::Inactive,
            Bullet { damage: 1, wall_bounces_left: Bullet::DEFAULT_WALL_BOUNCES },
            CollisionStamp::default(),
            Sprite {
                color: Color::srgb(1.0, 0.85, 0.3),
                custom_size: Some(Vec2::splat(8.0)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 2.0),
            Visibility::Hidden,
            RigidBody::Dynamic,
            Collider::circle(4.0),
            inactive_bullet_layers(),
            restitution,
            friction,
            LinearVelocity(Vec2::ZERO),
            CollisionEventsEnabled,
        )).id();

        pool.push_free(BulletEntity(e));
    }
}
