//! Projectiles plugin tests (Bevy 0.18 + Avian 0.5) — **deterministic**.
//!
//! These tests avoid relying on the full physics pipeline to generate collisions.
//! Instead, they **inject `CollisionStart` messages directly** and then run the
//! projectile collision system once.
//!
//! NOTE:
//! - `ReturnToPool` marker is removed.
//! - Bullet lifecycle uses `BulletState` (Inactive / Active / PendingReturn).
//! - Pooling no longer toggles `RigidBodyDisabled` / `ColliderDisabled`.
//!   Instead, inactive bullets "collide with nothing" via empty collision filters.
use bevy::{
    ecs::{
        message::Messages,
        world::CommandQueue,
    },
    prelude::*,
};
use avian2d::prelude::*;
use crate::common::test_utils::run_system_once;
use super::{collision, components, layers, pool};

// --------------------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------------------

/// Runs `f(commands, pool)` while temporarily removing BulletPool from the World.
fn with_commands_and_pool<T>(
    world: &mut World,
    f: impl FnOnce(&mut Commands, &mut pool::BulletPool) -> T,
) -> T {
    let mut pool_res = world
        .remove_resource::<pool::BulletPool>()
        .expect("BulletPool resource must exist");

    let mut queue = CommandQueue::default();
    let result = {
        let mut commands = Commands::new(&mut queue, world);
        f(&mut commands, &mut pool_res)
    };
    queue.apply(world);
    world.insert_resource(pool_res);
    result
}

/// Ensure the Messages<CollisionStart> resource exists (needed by MessageReader in the system).
fn ensure_collisionstart_messages(world: &mut World) {
    if world.get_resource::<Messages<CollisionStart>>().is_none() {
        world.init_resource::<Messages<CollisionStart>>();
    }
}

/// Convenience: write a CollisionStart message.
fn write_collision_start(
    world: &mut World,
    collider1: Entity,
    collider2: Entity,
    body1: Option<Entity>,
    body2: Option<Entity>,
) {
    ensure_collisionstart_messages(world);
    world.write_message(CollisionStart {
        collider1,
        collider2,
        body1,
        body2,
    });
}

/// Optional: call Messages::update() once.
fn update_messages(world: &mut World) {
    world.resource_mut::<Messages<CollisionStart>>().update();
}

// --------------------------------------------------------------------------------------
// Pooling unit tests (pure ECS)
// --------------------------------------------------------------------------------------

#[test]
fn init_bullet_pool_spawns_capacity_bullets_inactive() {
    let mut world = World::new();
    world.insert_resource(pool::BulletPool::new(8));

    run_system_once(&mut world, pool::init_bullet_pool);

    // Pool contains capacity entities
    let pool_res = world.resource::<pool::BulletPool>();
    assert_eq!(pool_res.free.len(), 8);

    // Exactly 8 pooled bullets
    let count = world.query::<&components::PooledBullet>().iter(&world).count();
    assert_eq!(count, 8);

    // Inactive state: hidden + BulletState::Inactive + empty collision filters + events enabled
    let mut q = world.query::<(
        &components::PooledBullet,
        &components::BulletState,
        &Visibility,
        &CollisionLayers,
        &CollisionEventsEnabled,
        &components::Bullet,
    )>();

    for (_pb, state, vis, layers, _events_enabled, bullet) in q.iter(&world) {
        assert_eq!(*state, components::BulletState::Inactive);
        assert_eq!(*vis, Visibility::Hidden);

        // Membership includes PlayerBullet
        assert!(layers.memberships.has_all(layers::Layer::PlayerBullet));

        // Option A: inactive bullets collide with nothing -> filters should be empty
        assert!(!layers.filters.has_all(layers::Layer::World));
        assert!(!layers.filters.has_all(layers::Layer::Enemy));

        // Bounce budget is reset
        assert_eq!(
            bullet.wall_bounces_left,
            components::Bullet::DEFAULT_WALL_BOUNCES
        );
    }
}

#[test]
fn acquire_bullet_activates_and_resets_state() {
    let mut world = World::new();
    world.insert_resource(pool::BulletPool::new(1));

    run_system_once(&mut world, pool::init_bullet_pool);

    let e = with_commands_and_pool(&mut world, |commands, pool_res| {
        pool::acquire_bullet(
            commands,
            pool_res,
            Vec2::new(10.0, 20.0),
            Vec2::new(100.0, 0.0),
            2,
        )
        .expect("pool should contain a bullet")
    });

    let tf = world.get::<Transform>(e).unwrap();
    assert_eq!(tf.translation.truncate(), Vec2::new(10.0, 20.0));

    let vel = world.get::<LinearVelocity>(e).unwrap();
    assert_eq!(vel.0, Vec2::new(100.0, 0.0));

    let vis = world.get::<Visibility>(e).unwrap();
    assert_eq!(*vis, Visibility::Visible);

    let state = world.get::<components::BulletState>(e).unwrap();
    assert_eq!(*state, components::BulletState::Active);

    // Active bullets should collide with World + Enemy
    let layers = world.get::<CollisionLayers>(e).unwrap();
    assert!(layers.memberships.has_all(layers::Layer::PlayerBullet));
    assert!(layers.filters.has_all(layers::Layer::World));
    assert!(layers.filters.has_all(layers::Layer::Enemy));

    let bullet = world.get::<components::Bullet>(e).unwrap();
    assert_eq!(bullet.damage, 2);
    assert_eq!(
        bullet.wall_bounces_left,
        components::Bullet::DEFAULT_WALL_BOUNCES
    );
}

#[test]
fn return_to_pool_commit_deactivates_and_recycles() {
    let mut world = World::new();
    world.insert_resource(pool::BulletPool::new(1));

    run_system_once(&mut world, pool::init_bullet_pool);

    let e = with_commands_and_pool(&mut world, |commands, pool_res| {
        pool::acquire_bullet(
            commands,
            pool_res,
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            1,
        )
        .expect("pool should contain a bullet")
    });

    // Mark for return using BulletState (Option A)
    *world.get_mut::<components::BulletState>(e).unwrap() = components::BulletState::PendingReturn;

    run_system_once(&mut world, pool::return_to_pool_commit);

    let state = world.get::<components::BulletState>(e).unwrap();
    assert_eq!(*state, components::BulletState::Inactive);

    let vis = world.get::<Visibility>(e).unwrap();
    assert_eq!(*vis, Visibility::Hidden);

    let vel = world.get::<LinearVelocity>(e).unwrap();
    assert_eq!(vel.0, Vec2::ZERO);

    // Inactive bullets should collide with nothing (filters empty)
    let layers = world.get::<CollisionLayers>(e).unwrap();
    assert!(layers.memberships.has_all(layers::Layer::PlayerBullet));
    assert!(!layers.filters.has_all(layers::Layer::World));
    assert!(!layers.filters.has_all(layers::Layer::Enemy));

    let pool_res = world.resource::<pool::BulletPool>();
    assert_eq!(pool_res.free.len(), 1);
}

// --------------------------------------------------------------------------------------
// Collision system tests (inject CollisionStart messages)
// --------------------------------------------------------------------------------------

#[test]
fn collision_world_decrements_bounce_budget_and_absorbs_at_zero() {
    let mut world = World::new();

    // Bullet collider entity (pooled bullet) must be Active for collision logic to apply
    let bullet_layers = CollisionLayers::new(
        layers::Layer::PlayerBullet,
        [layers::Layer::World],
    );
    let bullet = world
        .spawn((
            components::PooledBullet,
            components::BulletState::Active,
            components::Bullet {
                damage: 1,
                wall_bounces_left: 1,
            },
            bullet_layers,
        ))
        .id();

    // Wall collider entity with world membership
    let wall_layers = CollisionLayers::new(
        layers::Layer::World,
        [layers::Layer::PlayerBullet],
    );
    let wall = world.spawn((wall_layers,)).id();

    // Inject message: bullet started colliding with wall
    write_collision_start(&mut world, bullet, wall, Some(bullet), Some(wall));
    update_messages(&mut world);

    // Run collision processing
    run_system_once(&mut world, collision::process_player_bullet_collisions);

    // Bounce budget hits 0 => PendingReturn
    assert_eq!(
        *world.get::<components::BulletState>(bullet).unwrap(),
        components::BulletState::PendingReturn
    );
    assert_eq!(
        world.get::<components::Bullet>(bullet).unwrap().wall_bounces_left,
        0
    );
}

#[test]
fn collision_enemy_with_armour_wears_armour_and_does_not_absorb_bullet() {
    let mut world = World::new();

    let bullet_layers = CollisionLayers::new(
        layers::Layer::PlayerBullet,
        [layers::Layer::Enemy],
    );
    let bullet = world
        .spawn((
            components::PooledBullet,
            components::BulletState::Active,
            components::Bullet {
                damage: 2,
                wall_bounces_left: 3,
            },
            bullet_layers,
        ))
        .id();

    let enemy_layers = CollisionLayers::new(
        layers::Layer::Enemy,
        [layers::Layer::PlayerBullet],
    );
    let enemy = world
        .spawn((
            enemy_layers,
            components::Armour {
                hits_remaining: 2,
                max_hits: 2,
            },
            components::Health { hp: 10 },
        ))
        .id();

    write_collision_start(&mut world, bullet, enemy, Some(bullet), Some(enemy));
    update_messages(&mut world);

    run_system_once(&mut world, collision::process_player_bullet_collisions);

    // Armour should wear by 1
    assert_eq!(
        world.get::<components::Armour>(enemy).unwrap().hits_remaining,
        1
    );

    // Bullet should NOT be absorbed while armour is up
    assert_eq!(
        *world.get::<components::BulletState>(bullet).unwrap(),
        components::BulletState::Active
    );

    // HP unchanged
    assert_eq!(
        world.get::<components::Health>(enemy).unwrap().hp,
        10
    );
}

#[test]
fn collision_enemy_without_armour_absorbs_bullet_and_applies_damage() {
    let mut world = World::new();

    let bullet_layers = CollisionLayers::new(
        layers::Layer::PlayerBullet,
        [layers::Layer::Enemy],
    );
    let bullet = world
        .spawn((
            components::PooledBullet,
            components::BulletState::Active,
            components::Bullet {
                damage: 3,
                wall_bounces_left: 3,
            },
            bullet_layers,
        ))
        .id();

    let enemy_layers = CollisionLayers::new(
        layers::Layer::Enemy,
        [layers::Layer::PlayerBullet],
    );
    let enemy = world
        .spawn((
            enemy_layers,
            components::Armour {
                hits_remaining: 0,
                max_hits: 2,
            },
            components::Health { hp: 10 },
        ))
        .id();

    write_collision_start(&mut world, bullet, enemy, Some(bullet), Some(enemy));
    update_messages(&mut world);

    run_system_once(&mut world, collision::process_player_bullet_collisions);

    // Bullet absorbed
    assert_eq!(
        *world.get::<components::BulletState>(bullet).unwrap(),
        components::BulletState::PendingReturn
    );

    // Damage applied
    assert_eq!(
        world.get::<components::Health>(enemy).unwrap().hp,
        7
    );
}