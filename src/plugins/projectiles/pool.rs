use avian2d::prelude::*;
use bevy::prelude::*;

use super::components::{Bullet, PooledBullet, ReturnToPool};
use super::layers::Layer;

#[derive(Resource, Debug)]
pub struct BulletPool {
    pub free: Vec<Entity>,
    pub capacity: usize,
}

impl BulletPool {
    pub fn new(capacity: usize) -> Self {
        Self { free: Vec::with_capacity(capacity), capacity }
    }
}

/// Pre-spawn pooled bullets (inactive).
///
/// Uses Avian disable markers to remove them from physics, and Visibility to hide.
pub fn init_bullet_pool(
    mut commands: Commands,
    mut pool: ResMut<BulletPool>,
) {
    pool.free.clear();
    let cap = pool.capacity;
    pool.free.reserve(cap);

    let bullet_layers = CollisionLayers::new(Layer::PlayerBullet, [Layer::World, Layer::Enemy]);

    let restitution = Restitution::new(0.95).with_combine_rule(CoefficientCombine::Max);
    let friction = Friction::ZERO;

    for _ in 0..cap {
        let e = commands.spawn((
            Name::new("Bullet(Pooled)"),
            PooledBullet,
            Bullet::activate(1),

            Sprite {
                color: Color::srgb(1.0, 0.85, 0.3),
                custom_size: Some(Vec2::splat(8.0)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 2.0),
            Visibility::Hidden,

            RigidBody::Dynamic,
            Collider::circle(4.0),
            bullet_layers,
            restitution,
            friction,
            LinearVelocity(Vec2::ZERO),

            // Opt-in events only for bullets
            CollisionEventsEnabled,

            // inactive
            RigidBodyDisabled,
            ColliderDisabled,
        )).id();

        pool.free.push(e);
    }
}

/// Activate a bullet from the pool.
pub fn acquire_bullet(
    commands: &mut Commands,
    pool: &mut BulletPool,
    pos: Vec2,
    vel: Vec2,
    damage: i32,
) -> Option<Entity> {
    let e = pool.free.pop()?;

    commands.entity(e)
        .insert(Visibility::Visible)
        .insert(Transform::from_translation(pos.extend(2.0)))
        .insert(LinearVelocity(vel))
        .insert(Bullet::activate(damage))
        .remove::<RigidBodyDisabled>()
        .remove::<ColliderDisabled>()
        .remove::<ReturnToPool>();

    Some(e)
}

/// Commit return-to-pool.
pub fn return_to_pool_commit(
    mut commands: Commands,
    mut pool: ResMut<BulletPool>,
    q: Query<Entity, With<ReturnToPool>>,
) {
    for e in &q {
        commands.entity(e)
            .insert(RigidBodyDisabled)
            .insert(ColliderDisabled)
            .insert(Visibility::Hidden)
            .insert(LinearVelocity(Vec2::ZERO))
            .remove::<ReturnToPool>();

        pool.free.push(e);
    }
}
