//! Unit tests for the enemies module.
//!
//! ## How to enable
//! This file is intended to be compiled as a submodule of `src/plugins/enemies/mod.rs`.
//! Add this line near the bottom of `mod.rs` if it isn't already there:
//!
//! ```rust
//! #[cfg(test)]
//! mod tests;
//! ```
//!
//! Why submodule tests?
//! - They can access private helpers (newtypes, easing function, internal resources)
//!   without making them public.

#![cfg(test)]

use super::*;

use bevy::ecs::system::RunSystemOnce;
use std::time::{Duration, Instant};

// -----------------------------------------------------------------------------
// Test utilities
// -----------------------------------------------------------------------------

/// Helper: create a `Time<Fixed>` with a specific delta for a single system run.
fn fixed_time_with_delta(dt: f32) -> Time<Fixed> {
    let mut t = Time::<Fixed>::default();
    t.advance_by(Duration::from_secs_f32(dt));
    t
}

/// Helper: create a `Time<Real>` and advance by delta using test-friendly API.
fn real_time_with_delta(dt: f32) -> Time<Real> {
    let mut t = Time::<Real>::new(Instant::now());
    t.update_with_duration(Duration::from_secs_f32(dt));
    t
}

/// Tiny deterministic PRNG for property-style tests (xorshift64*).
///
/// This avoids pulling in an external property-testing dependency, while still
/// allowing us to run many randomized cases deterministically.
#[derive(Clone, Copy)]
struct TestRng(u64);

impl TestRng {
    fn new(seed: u64) -> Self { Self(seed) }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    #[inline]
    fn next_f32(&mut self) -> f32 {
        // 24 random bits -> float in [0,1)
        let v = (self.next_u64() >> 40) as u32;
        (v as f32) / ((1u32 << 24) as f32)
    }

    #[inline]
    fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        debug_assert!(hi >= lo);
        lo + (hi - lo) * self.next_f32()
    }
}

// -----------------------------------------------------------------------------
// Newtype/unit tests
// -----------------------------------------------------------------------------

#[test]
fn unitf32_clamps_add_and_decay() {
    let mut x = UnitF32::new_clamped(0.9);
    x.add_clamped(0.5);
    assert!((0.0..=1.0).contains(&x.get()));
    assert_eq!(x.get(), 1.0);

    x.decay_to_zero(10.0, 0.2);
    assert_eq!(x.get(), 0.0);

    // Decaying beyond zero stays at zero.
    x.decay_to_zero(10.0, 1.0);
    assert_eq!(x.get(), 0.0);
}

#[test]
fn realseconds_never_negative_and_set_max() {
    let mut t = RealSeconds::new(0.1);
    t.tick_down(10.0);
    assert_eq!(t.get(), 0.0);

    t.set_max(0.5);
    assert_eq!(t.get(), 0.5);

    // set_max should not reduce.
    t.set_max(0.1);
    assert_eq!(t.get(), 0.5);
}

#[test]
fn smootherstep_endpoints_monotonic_and_range() {
    assert!((smootherstep(0.0) - 0.0).abs() < 1e-6);
    assert!((smootherstep(1.0) - 1.0).abs() < 1e-6);

    let mut prev = smootherstep(0.0);
    for i in 1..=1000 {
        let x = i as f32 / 1000.0;
        let y = smootherstep(x);
        assert!(y >= -1e-6 && y <= 1.0 + 1e-6);
        assert!(y + 1e-6 >= prev);
        prev = y;
    }
}

// -----------------------------------------------------------------------------
// Property-style decay tests
// -----------------------------------------------------------------------------

#[test]
fn unitf32_decay_properties_many_random_cases() {
    let mut rng = TestRng::new(0xC0FFEE_1234_5678);

    for _case in 0..20_000 {
        let start = rng.range_f32(0.0, 1.0);
        let rate = rng.range_f32(0.0, 50.0);
        let dt = rng.range_f32(0.0, 1.0);

        let mut x = UnitF32::new_clamped(start);
        let before = x.get();
        x.decay_to_zero(rate, dt);
        let after = x.get();

        // Always stays clamped.
        assert!((0.0..=1.0).contains(&after));

        // Never increases.
        assert!(after <= before + 1e-6);

        // Matches saturating linear decay.
        let expected = (before - rate * dt).max(0.0);
        assert!((after - expected).abs() < 1e-5);
    }
}

#[test]
fn unitf32_decay_reaches_zero_given_enough_time() {
    let mut rng = TestRng::new(0xDEADBEEF_00AA_55FF);

    for _case in 0..5000 {
        let start = rng.range_f32(0.01, 1.0);
        let rate = rng.range_f32(0.1, 50.0);

        // Ensure total time is beyond time-to-zero.
        let total = (start / rate) * rng.range_f32(1.0, 3.0) + 0.001;

        let steps = (rng.next_u64() % 20 + 1) as usize;
        let mut remaining = total;

        let mut x = UnitF32::new_clamped(start);
        for i in 0..steps {
            let dt = if i + 1 == steps {
                remaining
            } else {
                let chunk = rng.range_f32(0.0, remaining);
                remaining -= chunk;
                chunk
            };
            x.decay_to_zero(rate, dt);
        }

        assert!(x.get() <= 1e-6);
    }
}

#[test]
fn unitf32_decay_is_consistent_over_step_splitting() {
    let mut rng = TestRng::new(0x12345678_9ABCDEF0);

    for _case in 0..10_000 {
        let start = rng.range_f32(0.0, 1.0);
        let rate = rng.range_f32(0.0, 50.0);
        let total = rng.range_f32(0.0, 1.0);

        // One big step.
        let mut a = UnitF32::new_clamped(start);
        a.decay_to_zero(rate, total);

        // Many steps summing to total.
        let mut b = UnitF32::new_clamped(start);
        let steps = (rng.next_u64() % 10 + 1) as usize;
        let mut rem = total;
        for i in 0..steps {
            let dt = if i + 1 == steps {
                rem
            } else {
                let chunk = rng.range_f32(0.0, rem);
                rem -= chunk;
                chunk
            };
            b.decay_to_zero(rate, dt);
        }

        assert!((a.get() - b.get()).abs() < 1e-5);
    }
}

#[test]
fn realseconds_tick_down_properties_many_random_cases() {
    let mut rng = TestRng::new(0xFEEDFACE_F00DBAAD);

    for _case in 0..20_000 {
        let start = rng.range_f32(0.0, 10.0);
        let dt = rng.range_f32(0.0, 10.0);

        let mut t = RealSeconds::new(start);
        let before = t.get();
        t.tick_down(dt);
        let after = t.get();

        assert!(after >= 0.0);
        assert!(after <= before + 1e-6);
        let expected = (before - dt).max(0.0);
        assert!((after - expected).abs() < 1e-6);
    }
}

// -----------------------------------------------------------------------------
// Pure logic tests (GlobalFx)
// -----------------------------------------------------------------------------

#[test]
fn globalfx_trigger_armour_break_sets_expected_fields_and_clamps() {
    let mut fx = GlobalFx::default();

    assert_eq!(fx.trauma.get(), 0.0);
    assert_eq!(fx.flash.get(), 0.0);
    assert_eq!(fx.hitstop.get(), 0.0);
    assert_eq!(fx.slowmo_remaining.get(), 0.0);

    fx.trigger_armour_break();

    assert!(fx.trauma.get() > 0.0);
    assert!(fx.flash.get() > 0.0);
    assert!(fx.hitstop.get() > 0.0);
    assert!(fx.slowmo_remaining.get() > 0.0);

    assert!(fx.trauma.get() <= 1.0);
    assert!(fx.flash.get() <= 1.0);

    for _ in 0..20 {
        fx.trigger_armour_break();
    }

    assert_eq!(fx.flash.get(), 1.0);
    assert_eq!(fx.trauma.get(), 1.0);
}

// -----------------------------------------------------------------------------
// ECS/system tests
// -----------------------------------------------------------------------------

#[test]
fn enemy_death_trigger_transitions_alive_to_dying_and_disables_collisions() {
    let mut world = World::new();

    // Spawn an enemy with Alive state but hp <= 0.
    // Also: seed sprite colour and non-1 scale so we can verify the system overwrites them.
    let e = world
        .spawn((
            Enemy,
            Health { hp: 0 },
            EnemyLifeState::Alive,
            Sprite { color: Color::srgba(0.1, 0.2, 0.3, 1.0), ..default() },
            Transform::from_scale(Vec3::splat(2.0)),
            CollisionLayers::new(Layer::Enemy, [Layer::World]),
        ))
        .id();

    let _ = world.run_system_once(enemy_death_trigger);

    match world.get::<EnemyLifeState>(e).unwrap() {
        EnemyLifeState::Dying { timer } => assert!(timer.duration().as_secs_f32() > 0.0),
        _ => panic!("Expected enemy to enter Dying"),
    }

    let layers = world.get::<CollisionLayers>(e).unwrap();
    assert_eq!(*layers, non_interacting_enemy_layers());

    // Dying tint.
    let sprite = world.get::<Sprite>(e).unwrap();
    let c = sprite.color.to_srgba();
    assert!((c.red - 0.8).abs() < 1e-3 && (c.green - 0.8).abs() < 1e-3 && (c.blue - 0.8).abs() < 1e-3);

    // Scale reset.
    let tf = world.get::<Transform>(e).unwrap();
    assert_eq!(tf.scale, Vec3::ONE);
}

#[test]
fn enemy_death_progress_marks_pending_despawn_and_sets_dead() {
    let mut world = World::new();

    world.insert_resource(fixed_time_with_delta(1.0));

    let e = world
        .spawn((
            Enemy,
            EnemyLifeState::Dying {
                timer: Timer::from_seconds(0.1, TimerMode::Once),
            },
            Sprite::default(),
            Transform::default(),
        ))
        .id();

    let _ = world.run_system_once(enemy_death_progress);

    assert!(world.get::<PendingDespawn>(e).is_some());
    assert!(matches!(world.get::<EnemyLifeState>(e).unwrap(), EnemyLifeState::Dead));
}

#[test]
fn armour_fx_break_triggers_global_fx_and_updates_local_fx() {
    let mut world = World::new();

    world.insert_resource(GlobalFx::default());
    world.insert_resource(fixed_time_with_delta(0.016));

    // Armour drops from 1 -> 0.
    let e = world
        .spawn((
            Enemy,
            EnemyLifeState::Alive,
            Armour { hits_remaining: 0, max_hits: 1 },
            ArmourFx::new(1),
            Sprite::default(),
        ))
        .id();

    let _ = world.run_system_once(armour_fx_update);

    let fx = world.resource::<GlobalFx>();
    assert!(fx.flash.get() > 0.0);
    assert!(fx.trauma.get() > 0.0);
    assert!(fx.hitstop.get() > 0.0);
    assert!(fx.slowmo_remaining.get() > 0.0);

    let local = world.get::<ArmourFx>(e).unwrap();
    assert!(local.any_active());
}

#[test]
fn ensure_fx_handles_caches_camera_and_spawns_overlay_when_missing() {
    let mut world = World::new();

    world.insert_resource(FxHandles::default());

    let cam = world.spawn((Camera2d, MainCamera, Transform::default())).id();

    let _ = world.run_system_once(ensure_fx_handles);

    let handles = *world.resource::<FxHandles>();
    assert_eq!(handles.camera, Some(cam));
    assert!(handles.overlay.is_some());

    let overlay = handles.overlay.unwrap();
    assert!(world.get::<ScreenFlashOverlay>(overlay).is_some());
}

#[test]
fn apply_global_fx_sets_virtual_speed_and_overlay_visibility() {
    let mut world = World::new();

    world.insert_resource(GlobalFx::default());
    world.insert_resource(FxHandles::default());
    world.insert_resource(real_time_with_delta(0.016));
    world.insert_resource(Time::<Virtual>::default());

    let cam = world.spawn((Camera2d, MainCamera, Transform::default())).id();
    let overlay = world
        .spawn((ScreenFlashOverlay, Sprite::default(), Transform::default(), Visibility::Hidden))
        .id();

    {
        let mut h = world.resource_mut::<FxHandles>();
        h.camera = Some(cam);
        h.overlay = Some(overlay);
        h.prev_shake_offset = Vec2::ZERO;
    }

    {
        let mut fx = world.resource_mut::<GlobalFx>();
        fx.hitstop = RealSeconds::new(0.1);
        fx.slowmo_duration = RealSeconds::new(1.0);
        fx.slowmo_remaining = RealSeconds::new(1.0);
        fx.slowmo_min_speed = 0.2;
        fx.flash = UnitF32::new_clamped(1.0);
        fx.trauma = UnitF32::new_clamped(0.0);
    }

    let _ = world.run_system_once(apply_global_fx);

    assert_eq!(world.resource::<Time<Virtual>>().relative_speed(), 0.0);
    assert!(matches!(*world.get::<Visibility>(overlay).unwrap(), Visibility::Visible));
}

#[test]
fn camera_shake_removes_previous_offset_when_trauma_goes_to_zero() {
    let mut world = World::new();

    world.insert_resource(GlobalFx::default());
    world.insert_resource(FxHandles::default());
    world.insert_resource(real_time_with_delta(0.016));
    world.insert_resource(Time::<Virtual>::default());

    let cam = world.spawn((Camera2d, MainCamera, Transform::default())).id();
    let overlay = world
        .spawn((ScreenFlashOverlay, Sprite::default(), Transform::default(), Visibility::Hidden))
        .id();

    {
        let mut h = world.resource_mut::<FxHandles>();
        h.camera = Some(cam);
        h.overlay = Some(overlay);
        h.prev_shake_offset = Vec2::ZERO;
    }

    // Apply shake.
    {
        let mut fx = world.resource_mut::<GlobalFx>();
        fx.trauma = UnitF32::new_clamped(1.0);
        fx.flash = UnitF32::new_clamped(0.0);
        fx.hitstop = RealSeconds::new(0.0);
        fx.slowmo_remaining = RealSeconds::new(0.0);
    }

    let _ = world.run_system_once(apply_global_fx);
    let after = world.get::<Transform>(cam).unwrap().translation;
    assert!(after.x != 0.0 || after.y != 0.0);

    // Next frame: trauma to zero, should subtract previous offset.
    {
        let mut fx = world.resource_mut::<GlobalFx>();
        fx.trauma = UnitF32::new_clamped(0.0);
    }

    world.insert_resource(real_time_with_delta(0.016));
    let _ = world.run_system_once(apply_global_fx);

    let final_pos = world.get::<Transform>(cam).unwrap().translation;
    assert!(final_pos.x.abs() < 1e-4 && final_pos.y.abs() < 1e-4);
}

// -----------------------------------------------------------------------------
// Hitstop precedence tests
// -----------------------------------------------------------------------------

#[test]
fn hitstop_precedence_over_slowmo_randomized() {
    let mut world = World::new();

    world.insert_resource(GlobalFx::default());
    world.insert_resource(FxHandles::default());
    world.insert_resource(Time::<Virtual>::default());

    // Real time resource that we advance every iteration.
    world.insert_resource(real_time_with_delta(0.016));

    // Spawn camera + overlay.
    let cam = world.spawn((Camera2d, MainCamera, Transform::default())).id();
    let overlay = world
        .spawn((ScreenFlashOverlay, Sprite::default(), Transform::default(), Visibility::Hidden))
        .id();

    {
        let mut h = world.resource_mut::<FxHandles>();
        h.camera = Some(cam);
        h.overlay = Some(overlay);
        h.prev_shake_offset = Vec2::ZERO;
    }

    let mut rng = TestRng::new(0xBADC0FFEE0DDF00D);

    for _case in 0..5000 {
        // Advance real time.
        {
            let mut real = world.resource_mut::<Time<Real>>();
            real.update_with_duration(Duration::from_secs_f32(0.016));
        }

        let hitstop = rng.range_f32(0.0, 0.2);
        let slowmo_remaining = rng.range_f32(0.0, 2.0);
        let slowmo_duration = rng.range_f32(0.05, 2.0);
        let min_speed = rng.range_f32(0.05, 0.8);

        {
            let mut fx = world.resource_mut::<GlobalFx>();
            fx.hitstop = RealSeconds::new(hitstop);
            fx.slowmo_duration = RealSeconds::new(slowmo_duration);
            fx.slowmo_remaining = RealSeconds::new(slowmo_remaining);
            fx.slowmo_min_speed = min_speed;

            fx.flash = UnitF32::new_clamped(0.0);
            fx.trauma = UnitF32::new_clamped(0.0);
        }

        let _ = world.run_system_once(apply_global_fx);

        let speed = world.resource::<Time<Virtual>>().relative_speed();

        if hitstop > 0.0 {
            assert_eq!(speed, 0.0);
        } else if slowmo_remaining > 0.0 {
            assert!(speed >= min_speed - 1e-6);
            assert!(speed <= 1.0 + 1e-6);
        } else {
            assert!((speed - 1.0).abs() < 1e-6);
        }
    }
}

#[test]
fn hitstop_keeps_speed_zero_until_timer_expires() {
    let mut world = World::new();

    world.insert_resource(GlobalFx::default());
    world.insert_resource(FxHandles::default());
    world.insert_resource(Time::<Virtual>::default());
    world.insert_resource(real_time_with_delta(0.01));

    // Spawn camera + overlay.
    let cam = world.spawn((Camera2d, MainCamera, Transform::default())).id();
    let overlay = world
        .spawn((ScreenFlashOverlay, Sprite::default(), Transform::default(), Visibility::Hidden))
        .id();

    {
        let mut h = world.resource_mut::<FxHandles>();
        h.camera = Some(cam);
        h.overlay = Some(overlay);
        h.prev_shake_offset = Vec2::ZERO;
    }

    // hitstop 0.05s, slowmo active too.
    {
        let mut fx = world.resource_mut::<GlobalFx>();
        fx.hitstop = RealSeconds::new(0.05);
        fx.slowmo_duration = RealSeconds::new(1.0);
        fx.slowmo_remaining = RealSeconds::new(1.0);
        fx.slowmo_min_speed = 0.25;
        fx.flash = UnitF32::new_clamped(0.0);
        fx.trauma = UnitF32::new_clamped(0.0);
    }

    // dt = 0.01, so hitstop should dominate for first 5 frames.
    for i in 0..10 {
        {
            let mut real = world.resource_mut::<Time<Real>>();
            real.update_with_duration(Duration::from_secs_f32(0.01));
        }

        let _ = world.run_system_once(apply_global_fx);
        let speed = world.resource::<Time<Virtual>>().relative_speed();

        if i < 5 {
            assert_eq!(speed, 0.0);
        } else {
            // After hitstop ends, slowmo should take over.
            assert!(speed >= 0.25 - 1e-6);
            assert!(speed <= 1.0 + 1e-6);
            break;
        }
    }
}