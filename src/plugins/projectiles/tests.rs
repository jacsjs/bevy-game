//! Projectiles plugin tests (Bevy 0.18 + Avian 0.5) — **deterministic**.
//!
//! These tests avoid relying on the full physics pipeline to generate collisions.
//! Instead, they **inject `CollisionStart` messages directly** and then run the
//! projectile collision system once.
//!
//! Why? In Bevy 0.18, messages are buffered and updated by a separate message update system.
//! When running only a single schedule manually in tests, you can end up not seeing messages
//! unless you also run the message update system / relevant schedules.
//! Injecting messages makes these tests stable and fast.
//!
//! This matches Bevy's intended usage: Messages can be written and read via MessageReader/Writer,
//! and the World can write messages directly (`world.write_message(...)`).
//! See: Message / MessageReader / Messages docs. 
//!
//! ## Enable
//! Put this file next to `src/plugins/projectiles/mod.rs` and add:
//! ```rust
//! #[cfg(test)]
//! mod tests;
//! ```

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

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

/// Runs `f(commands, pool)` while temporarily removing BulletPool from the World.
fn with_commands_and_pool<T>(world: &mut World, f: impl FnOnce(&mut Commands, &mut pool::BulletPool) -> T) -> T {
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
fn write_collision_start(world: &mut World, collider1: Entity, collider2: Entity, body1: Option<Entity>, body2: Option<Entity>) {
    ensure_collisionstart_messages(world);
    world.write_message(CollisionStart { collider1, collider2, body1, body2 });
}

/// Optional: call Messages::update() once.
/// In these tests it isn't strictly necessary, but it mirrors "once per frame" semantics.
fn update_messages(world: &mut World) {
    world.resource_mut::<Messages<CollisionStart>>().update();
}

// -----------------------------------------------------------------------------
// Pooling unit tests (pure ECS)
// -----------------------------------------------------------------------------

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

    // Inactive state: hidden + disabled + events enabled
    let mut q = world.query::<(
        &components::PooledBullet,
        &Visibility,
        &RigidBodyDisabled,
        &ColliderDisabled,
        &CollisionLayers,
        &CollisionEventsEnabled,
        &components::Bullet,
    )>();

    for (_pb, vis, _rb_dis, _col_dis, layers, _events_enabled, bullet) in q.iter(&world) {
        assert_eq!(*vis, Visibility::Hidden);

        // Bullet membership and filters
        assert!(layers.memberships.has_all(layers::Layer::PlayerBullet));
        assert!(layers.filters.has_all(layers::Layer::World));
        assert!(layers.filters.has_all(layers::Layer::Enemy));

        // Bounce budget is reset
        assert_eq!(bullet.wall_bounces_left, components::Bullet::DEFAULT_WALL_BOUNCES);
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

    assert!(world.get::<RigidBodyDisabled>(e).is_none());
    assert!(world.get::<ColliderDisabled>(e).is_none());

    let bullet = world.get::<components::Bullet>(e).unwrap();
    assert_eq!(bullet.damage, 2);
    assert_eq!(bullet.wall_bounces_left, components::Bullet::DEFAULT_WALL_BOUNCES);
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

    world.entity_mut(e).insert(components::ReturnToPool);
    run_system_once(&mut world, pool::return_to_pool_commit);

    assert!(world.get::<components::ReturnToPool>(e).is_none());
    assert!(world.get::<RigidBodyDisabled>(e).is_some());
    assert!(world.get::<ColliderDisabled>(e).is_some());

    let vis = world.get::<Visibility>(e).unwrap();
    assert_eq!(*vis, Visibility::Hidden);

    let vel = world.get::<LinearVelocity>(e).unwrap();
    assert_eq!(vel.0, Vec2::ZERO);

    let pool_res = world.resource::<pool::BulletPool>();
    assert_eq!(pool_res.free.len(), 1);
}

// -----------------------------------------------------------------------------
// Collision system tests (inject CollisionStart messages)
// -----------------------------------------------------------------------------

#[test]
fn collision_world_decrements_bounce_budget_and_absorbs_at_zero() {
    let mut world = World::new();

    // Create a bullet collider entity (pooled bullet)
    let bullet_layers = CollisionLayers::new(layers::Layer::PlayerBullet, [layers::Layer::World]);
    let bullet = world.spawn((
        components::PooledBullet,
        components::Bullet { damage: 1, wall_bounces_left: 1 },
        bullet_layers,
    )).id();

    // Create a wall collider entity with world membership
    let wall_layers = CollisionLayers::new(layers::Layer::World, [layers::Layer::PlayerBullet]);
    let wall = world.spawn((wall_layers,)).id();

    // Inject message: bullet started colliding with wall
    write_collision_start(&mut world, bullet, wall, Some(bullet), Some(wall));
    update_messages(&mut world);

    // Run collision processing
    run_system_once(&mut world, collision::process_player_bullet_collisions);

    // Bounce budget hits 0 => ReturnToPool inserted
    assert!(world.get::<components::ReturnToPool>(bullet).is_some());
    assert_eq!(world.get::<components::Bullet>(bullet).unwrap().wall_bounces_left, 0);
}

#[test]
fn collision_enemy_with_armour_wears_armour_and_does_not_absorb_bullet() {
    let mut world = World::new();

    let bullet_layers = CollisionLayers::new(layers::Layer::PlayerBullet, [layers::Layer::Enemy]);
    let bullet = world.spawn((
        components::PooledBullet,
        components::Bullet { damage: 2, wall_bounces_left: 3 },
        bullet_layers,
    )).id();

    let enemy_layers = CollisionLayers::new(layers::Layer::Enemy, [layers::Layer::PlayerBullet]);
    let enemy = world.spawn((
        enemy_layers,
        components::Armour { hits_remaining: 2, max_hits: 2 },
        components::Health { hp: 10 },
    )).id();

    write_collision_start(&mut world, bullet, enemy, Some(bullet), Some(enemy));
    update_messages(&mut world);

    run_system_once(&mut world, collision::process_player_bullet_collisions);

    // Armour should wear by 1
    assert_eq!(world.get::<components::Armour>(enemy).unwrap().hits_remaining, 1);

    // Bullet should NOT be absorbed while armour is up
    assert!(world.get::<components::ReturnToPool>(bullet).is_none());

    // HP unchanged
    assert_eq!(world.get::<components::Health>(enemy).unwrap().hp, 10);
}

#[test]
fn collision_enemy_without_armour_absorbs_bullet_and_applies_damage() {
    let mut world = World::new();

    let bullet_layers = CollisionLayers::new(layers::Layer::PlayerBullet, [layers::Layer::Enemy]);
    let bullet = world.spawn((
        components::PooledBullet,
        components::Bullet { damage: 3, wall_bounces_left: 3 },
        bullet_layers,
    )).id();

    let enemy_layers = CollisionLayers::new(layers::Layer::Enemy, [layers::Layer::PlayerBullet]);
    let enemy = world.spawn((
        enemy_layers,
        components::Armour { hits_remaining: 0, max_hits: 2 },
        components::Health { hp: 10 },
    )).id();

    write_collision_start(&mut world, bullet, enemy, Some(bullet), Some(enemy));
    update_messages(&mut world);

    run_system_once(&mut world, collision::process_player_bullet_collisions);

    // Bullet absorbed
    assert!(world.get::<components::ReturnToPool>(bullet).is_some());

    // Damage applied
    assert_eq!(world.get::<components::Health>(enemy).unwrap().hp, 7);
}
