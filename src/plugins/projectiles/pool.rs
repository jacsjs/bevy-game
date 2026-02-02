use avian2d::prelude::*;
use bevy::prelude::*;

use super::components::{Bullet, BulletState, PooledBullet};
use super::layers::Layer;

#[derive(Resource, Debug)]
pub struct BulletPool {
    pub free: Vec<Entity>,
    pub capacity: usize,
}

impl BulletPool {
    pub fn new(capacity: usize) -> Self {
        Self {
            free: Vec::with_capacity(capacity),
            capacity,
        }
    }
}

#[inline]
fn active_bullet_layers() -> CollisionLayers {
    CollisionLayers::new(Layer::PlayerBullet, [Layer::World, Layer::Enemy])
}

/// “Disabled” without structural changes: empty filters means we collide with nothing.
#[inline]
fn inactive_bullet_layers() -> CollisionLayers {
    CollisionLayers::new(Layer::PlayerBullet, [] as [Layer; 0])
}

/// Pre-spawn pooled bullets (inactive).
///
/// Option A: keep physics components present, but set collision layers so inactive bullets
/// never collide (and therefore never generate collision events).
pub fn init_bullet_pool(mut commands: Commands, mut pool: ResMut<BulletPool>) {
    pool.free.clear();
    let cap = pool.capacity;
    pool.free.reserve(cap);

    let restitution = Restitution::new(0.95).with_combine_rule(CoefficientCombine::Max);
    let friction = Friction::ZERO;

    for _ in 0..cap {
        let e = commands
            .spawn((
                Name::new("Bullet(Pooled)"),
                PooledBullet,
                BulletState::Inactive,
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
                inactive_bullet_layers(),
                restitution,
                friction,
                LinearVelocity(Vec2::ZERO),
                // Keep this always; inactive bullets won’t collide anyway because layers are empty.
                CollisionEventsEnabled,
            ))
            .id();

        pool.free.push(e);
    }
}

/// Activate a bullet from the pool (no structural toggles).
pub fn acquire_bullet(
    commands: &mut Commands,
    pool: &mut BulletPool,
    pos: Vec2,
    vel: Vec2,
    damage: i32,
) -> Option<Entity> {
    let e = pool.free.pop()?;

    commands
        .entity(e)
        .insert(Visibility::Visible)
        .insert(Transform::from_translation(pos.extend(2.0)))
        .insert(LinearVelocity(vel))
        .insert(Bullet::activate(damage))
        .insert(BulletState::Active)
        // Overwrite layers to enable collisions
        .insert(active_bullet_layers());

    Some(e)
}

/// Commit return-to-pool (no structural toggles).
///
/// This mutates component values directly, avoiding archetype moves.
pub fn return_to_pool_commit(
    mut pool: ResMut<BulletPool>,
    mut q: Query<(
        Entity,
        &mut BulletState,
        &mut CollisionLayers,
        &mut LinearVelocity,
        &mut Visibility,
    ), With<PooledBullet>>,
) {
    for (e, mut state, mut layers, mut vel, mut vis) in &mut q {
        if *state != BulletState::PendingReturn {
            continue;
        }

        *state = BulletState::Inactive;
        *layers = inactive_bullet_layers();
        *vel = LinearVelocity(Vec2::ZERO);
        *vis = Visibility::Hidden;

        pool.free.push(e);
    }
}