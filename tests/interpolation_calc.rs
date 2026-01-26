mod common;

use avian2d::prelude::*;
use bevy::prelude::*;
use std::time::Duration;

#[derive(Component)]
struct Thing;

#[derive(Resource, Default)]
struct StepCounter(u32);

// Discrete “physics tick” motion: jump 10 units each fixed tick
fn fixed_step_motion(mut step: ResMut<StepCounter>, mut q: Query<&mut Transform, With<Thing>>) {
    step.0 += 1;
    let target_x = step.0 as f32 * 10.0;
    if let Ok(mut tf) = q.single_mut() {
        tf.translation.x = target_x;
    }
}

#[test]
fn interpolation_smooths_between_fixed_steps() {
    // Configure your headless game (states + gameplay plugins)
    let mut app = common::app_headless();

    // Make fixed timestep coarse so smoothing is obvious.
    app.insert_resource(Time::<Fixed>::from_seconds(0.5));

    app.init_resource::<StepCounter>();

    // Spawn a thing that opts into interpolation
    app.world_mut().spawn((
        Thing,
        Transform::from_xyz(0.0, 0.0, 0.0),
        TranslationInterpolation,
    ));

    // Apply discrete steps in fixed schedule
    app.add_systems(FixedPostUpdate, fixed_step_motion);

    // Run a first update so schedules initialize
    app.update();

    // Run a fixed tick once (should jump to x=10)
    app.world_mut().run_schedule(FixedPostUpdate);
    let x_after_step = app
        .world_mut()
        .query::<&Transform>()
        .single(app.world())
        .unwrap()
        .translation
        .x;
    assert_eq!(x_after_step, 10.0);

    // Now simulate several “render frames” without another fixed step.
    // We advance virtual time in small increments so the interpolation has “in-between” time to work with.
    // (You can tune this depending on your harness.)
    for _ in 0..5 {
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(Duration::from_millis(50));
        app.update();
    }

    let x_after_frames = app
        .world_mut()
        .query::<&Transform>()
        .single(app.world())
        .unwrap()
        .translation
        .x;

    // With interpolation enabled, the rendered transform should have moved
    // even though we did not run another fixed step.
    assert!(
        x_after_frames > 0.0 && x_after_frames < 10.0,
        "Expected interpolated x to be between 0 and 10, got {x_after_frames}"
    );
}
