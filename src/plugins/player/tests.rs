use avian2d::prelude::*;
use bevy::prelude::*;

use crate::common::test_utils::run_system_once;
use crate::common::tunables::Tunables;

#[test]
fn spawn_creates_player() {
    let mut world = World::new();
    run_system_once(&mut world, super::spawn);
    assert!(
        world
            .query::<&super::Player>()
            .iter(&world)
            .next()
            .is_some()
    );
}

#[test]
fn apply_movement_sets_velocity() {
    let mut world = World::new();
    world.insert_resource(Tunables {
        pixels_per_meter: 20.0,
        player_speed: 100.0,
        bullet_speed: 0.0,
    });
    world.insert_resource(super::PlayerInput {
        move_axis: Vec2::new(1.0, 0.0),
    });
    world.spawn((super::Player, LinearVelocity::ZERO));

    run_system_once(&mut world, super::apply_movement);

    let v = world
        .query::<&LinearVelocity>()
        .iter(&world)
        .next()
        .unwrap();
    assert_eq!(v.0, Vec2::new(100.0, 0.0));
}

#[test]
fn spawn_enables_translation_interpolation() {
    let mut world = World::new();
    run_system_once(&mut world, super::spawn);

    // Assert that the player entity has TranslationInterpolation.
    let has = world
        .query::<(
            &super::Player,
            &avian2d::interpolation::TranslationInterpolation,
        )>()
        .iter(&world)
        .next()
        .is_some();

    assert!(
        has,
        "Player should opt-in to interpolation via TranslationInterpolation"
    );
}

#[derive(Resource, Default)]
struct ObservedVel(Option<Vec2>);

fn probe_velocity_in_step_sim(
    q: Query<&LinearVelocity, With<super::Player>>,
    mut out: ResMut<ObservedVel>,
) {
    if let Ok(v) = q.single() {
        out.0 = Some(v.0);
    }
}

#[test]
fn movement_runs_before_physics_step_simulation() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    // Add Avian (creates the PhysicsSet schedules/sets).
    // Interpolation plugin is included by default. [1](https://docs.rs/avian3d/latest/avian3d/collision/collider/struct.CollisionLayers.html)
    app.add_plugins(PhysicsPlugins::default());

    app.insert_resource(ObservedVel::default());
    app.insert_resource(Tunables {
        pixels_per_meter: 20.0,
        player_speed: 100.0,
        bullet_speed: 0.0,
    });
    app.insert_resource(super::PlayerInput {
        move_axis: Vec2::new(1.0, 0.0),
    });

    // Spawn a minimal player for this test (no need for Sprite/Collider here).
    app.world_mut().spawn((super::Player, LinearVelocity::ZERO));

    // Your system under test: should run before StepSimulation.
    app.add_systems(
        FixedPostUpdate,
        super::apply_movement.before(PhysicsSystems::StepSimulation),
    );

    // Probe system that runs *during* the StepSimulation set.
    app.add_systems(
        FixedPostUpdate,
        probe_velocity_in_step_sim.in_set(PhysicsSystems::StepSimulation),
    );

    // Run one FixedPostUpdate schedule manually (deterministic in tests).
    app.world_mut().run_schedule(FixedPostUpdate);

    let observed = app.world().resource::<ObservedVel>().0;
    assert_eq!(observed, Some(Vec2::new(100.0, 0.0)));
}
