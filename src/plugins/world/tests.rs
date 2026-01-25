use bevy::prelude::*;
use avian2d::prelude::*;
use crate::common::test_utils::run_system_once;

#[test]
fn spawns_walls_on_enter() {
    let mut world = World::new();
    run_system_once(&mut world, super::spawn_arena);

    let walls = world.query::<(&Name, &RigidBody)>().iter(&world)
        .filter(|(n, rb)| n.as_str().starts_with("Wall") && matches!(**rb, RigidBody::Static))
        .count();
    assert_eq!(walls, 4);
}