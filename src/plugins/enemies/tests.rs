use crate::common::test_utils::run_system_once;
use crate::plugins::enemies;
use bevy::prelude::*;

#[test]
fn spawns_three_targets() {
    let mut world = World::new();
    run_system_once(&mut world, super::spawn_targets);
    assert_eq!(world.query::<&enemies::Enemy>().iter(&world).count(), 3);
}
