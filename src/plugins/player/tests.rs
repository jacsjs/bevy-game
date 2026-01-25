use bevy::prelude::*;
use avian2d::prelude::*;

use crate::common::test_utils::run_system_once;
use crate::common::tunables::Tunables;

#[test]
fn spawn_creates_player() {
    let mut world = World::new();
    run_system_once(&mut world, super::spawn);
    assert!(world.query::<&super::Player>().iter(&world).next().is_some());
}

#[test]
fn apply_movement_sets_velocity() {
    let mut world = World::new();
    world.insert_resource(Tunables { pixels_per_meter: 20.0, player_speed: 100.0, bullet_speed: 0.0 });
    world.insert_resource(super::PlayerInput { move_axis: Vec2::new(1.0, 0.0) });
    world.spawn((super::Player, LinearVelocity::ZERO));

    run_system_once(&mut world, super::apply_movement);

    let v = world.query::<&LinearVelocity>().iter(&world).next().unwrap();
    assert_eq!(v.0, Vec2::new(100.0, 0.0));
}
