use bevy::prelude::*;
use avian2d::prelude::*;

use crate::common::test_utils::run_system_once;

fn write_collision(world: &mut World, a: Entity, b: Entity) {
    world.write_message(CollisionStart { collider1: a, collider2: b, body1: None, body2: None });
}

#[test]
fn hit_enemy_despawns_both() {
    let mut world = World::new();
    world.init_resource::<Messages<CollisionStart>>();

    let bullet = world.spawn(super::Bullet { damage: 1 }).id();
    let enemy = world.spawn(crate::plugins::enemies::Enemy).id();

    write_collision(&mut world, bullet, enemy);

    run_system_once(&mut world, super::systems::process_bullet_hits);

    assert!(world.get_entity(bullet).is_err());
    assert!(world.get_entity(enemy).is_err());
}
