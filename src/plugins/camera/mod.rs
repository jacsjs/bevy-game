//! Camera plugin (invariant-based edition).
//!
//! # What this module owns
//! This module owns camera follow behavior. It should NOT do input normalization.
//! Instead it consumes already-normalized facts (Aim + Player motion) and applies
//! a camera policy (look-ahead, deadzone) + integration (smoothing).
//!
//! # Dataflow
//! - `Aim` (resource) is updated elsewhere: cursor position in world-space.
//! - Player motion comes from physics (`LinearVelocity`) set by your player movement logic.
//! - This module computes a camera target each frame and eases toward it.
//!
//! # Invariants (fail-fast)
//! - There is exactly one Player while in InGame (PlayerEntity set on spawn).
//! - There is exactly one MainCamera while in InGame (MainCameraEntity set on spawn).
//! If these invariants are violated, we `expect()` and crash loudly.
//!
//! # Disjointness / aliasing constraints
//! We encode disjoint query access using `Without<...>` filters so that Bevy can prove
//! the queries cannot overlap. This avoids runtime panics caused by ambiguous aliasing.

use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;
use bevy_firefly::prelude::*;

// Controller fallback uses player velocity (Avian).
use avian2d::prelude::LinearVelocity;

use crate::common::state::GameState;
use crate::plugins::projectiles::components::{Aim, MainCameraEntity, Player, PlayerEntity};

/// Newtype: per-second responsiveness (1/seconds), non-negative by construction.
#[derive(Clone, Copy, Debug)]
pub struct ResponsivenessPerSec(pub u16);
impl ResponsivenessPerSec {
    #[inline]
    pub fn as_f32(self) -> f32 {
        self.0 as f32
    }
}

/// Newtype: distance in world units (this project uses pixels).
#[derive(Clone, Copy, Debug)]
pub struct LookAheadPixels(pub u16);
impl LookAheadPixels {
    #[inline]
    pub fn as_f32(self) -> f32 {
        self.0 as f32
    }
}

/// Newtype: dead-zone inner radius in pixels.
/// Inside this radius: camera ignores Aim look-ahead (reduces jitter).
#[derive(Clone, Copy, Debug)]
pub struct DeadZonePixels(pub u16);
impl DeadZonePixels {
    #[inline]
    pub fn as_f32(self) -> f32 {
        self.0 as f32
    }
}

/// Newtype: soft-zone width in pixels.
/// Between dead_zone and dead_zone+soft_zone: we smoothly ramp in look-ahead.
#[derive(Clone, Copy, Debug)]
pub struct SoftZonePixels(pub u16);
impl SoftZonePixels {
    #[inline]
    pub fn as_f32(self) -> f32 {
        self.0 as f32
    }
}

/// Newtype: normalized weight in [0..1].
#[derive(Clone, Copy, Debug)]
pub struct UnitF32(pub f32);
impl UnitF32 {
    #[inline]
    pub fn new_clamped(v: f32) -> Self {
        Self(v.clamp(0.0, 1.0))
    }
}

/// Main camera component: marker + configuration.
/// We keep the config on the camera entity itself (minimal components).
#[derive(Component)]
pub struct MainCamera {
    /// Snappy baseline follow rate (camera position -> target).
    pub follow_responsiveness: ResponsivenessPerSec,

    /// Softer rate for smoothing the look-ahead vector (cursor/controller direction -> look vector).
    pub look_responsiveness: ResponsivenessPerSec,

    /// Maximum look-ahead distance in pixels.
    pub look_ahead_dist: LookAheadPixels,

    /// Strength of look-ahead when fully active (0..1).
    pub look_ahead_weight: UnitF32,

    /// Strength of controller fallback look-ahead (0..1).
    /// This lets mouse aim be strong while controller look is more subtle (or vice versa).
    pub controller_look_weight: UnitF32,

    /// Aim dead-zone radius (pixels).
    pub dead_zone: DeadZonePixels,

    /// Aim soft-zone width (pixels).
    pub soft_zone: SoftZonePixels,
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::InGame), spawn_camera).add_systems(
        PostUpdate,
        follow_player
            .before(TransformSystems::Propagate)
            .run_if(in_state(GameState::InGame)),
    );
}

fn spawn_camera(mut commands: Commands) {
    let e = commands
        .spawn((
            Name::new("MainCamera"),
            Camera2d,
            MainCamera {
                // Baseline follow should be snappy.
                follow_responsiveness: ResponsivenessPerSec(12), // try 8..16

                // Look vector should be softer.
                look_responsiveness: ResponsivenessPerSec(3),    // try 2..6

                // Look-ahead tuning.
                look_ahead_dist: LookAheadPixels(180),           // try 120..220
                look_ahead_weight: UnitF32::new_clamped(0.75),   // mouse aim strength
                controller_look_weight: UnitF32::new_clamped(0.45), // controller fallback strength

                // Dead-zone tuning (bigger = less jitter close to player).
                dead_zone: DeadZonePixels(140),                 // try 80..220
                soft_zone: SoftZonePixels(220),                 // try 120..320
            },
            FireflyConfig::default(),
            Transform::from_xyz(0.0, 0.0, 999.0),
            DespawnOnExit(GameState::InGame),
        ))
        .id();

    // Boundary invariant: while in InGame, there is exactly one main camera.
    commands.insert_resource(MainCameraEntity(Some(e)));
}

/// Smoothstep on [0..1] -> [0..1] with zero slope at endpoints.
/// Used to blend look-ahead smoothly across the soft-zone.
#[inline]
fn smoothstep01(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

/// Exponential smoothing factor:
/// alpha = 1 - exp(-rate * dt)
#[inline]
fn exp_alpha(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate * dt).exp()
}

fn follow_player(
    time: Res<Time>,
    player_e: Res<PlayerEntity>,
    cam_e: Res<MainCameraEntity>,
    aim: Res<Aim>,

    // Disjointness proof: Player entities are not MainCamera entities.
    q_player: Query<(&Transform, Option<&LinearVelocity>), (With<Player>, Without<MainCamera>)>,

    // Disjointness proof: MainCamera entities are not Player entities.
    mut q_cam: Query<(&mut Transform, &MainCamera), Without<Player>>,

    // Local state: smoothed look vector (prevents jerk/jitter).
    mut smoothed_look: Local<Vec2>,
) {
    // Invariants (fail-fast).
    let player = player_e.0.expect("PlayerEntity not set");
    let cam = cam_e.0.expect("MainCameraEntity not set");

    let (tf_player, vel_opt) = q_player.get(player).expect("PlayerEntity invalid");
    let (mut tf_cam, cfg) = q_cam.get_mut(cam).expect("MainCameraEntity invalid");

    // Virtual time here (affected by slowmo/hitstop).
    // Clamp dt to avoid huge jumps after stalls/debug pauses.
    let dt = time.delta_secs().min(0.05);

    let player_pos = tf_player.translation.truncate();

    // ------------------------------------------------------------
    // 1) Compute desired look vector
    // ------------------------------------------------------------
    //
    // Priority:
    // - If Aim exists: use aim-based look-ahead with dead-zone + soft-zone blending.
    // - Else (controller/keyboard fallback): use player velocity direction.
    //
    let desired_look = if let Some(cursor) = aim.world_cursor {
        // Mouse aim look-ahead with dead-zone.
        let dir = cursor - player_pos;
        let d = dir.length();

        let r0 = cfg.dead_zone.as_f32();
        let r1 = r0 + cfg.soft_zone.as_f32();

        // Blend factor t in [0..1]
        let t = if r1 > r0 {
            ((d - r0) / (r1 - r0)).clamp(0.0, 1.0)
        } else {
            // Degenerate config: hard dead-zone.
            if d > r0 { 1.0 } else { 0.0 }
        };

        let blend = smoothstep01(t);

        let max = cfg.look_ahead_dist.as_f32();
        let clamped = dir.clamp_length_max(max);

        clamped * (cfg.look_ahead_weight.0 * blend)
    } else if let Some(vel) = vel_opt {
        // Controller/keyboard fallback: use movement direction.
        //
        // This is stable (velocity doesn't jitter like cursor near player).
        // We scale by a normalized speed factor so standing still yields no look-ahead.
        let v = vel.0;

        let speed2 = v.length_squared();
        if speed2 < 1e-4 {
            Vec2::ZERO
        } else {
            let dir = v / speed2.sqrt(); // normalize without extra branches
            let max = cfg.look_ahead_dist.as_f32();

            // Optional: scale by speed up to some cap so tiny movement gives tiny look-ahead.
            // This is a *policy* knob; keep it simple and deterministic.
            let speed = speed2.sqrt();
            let speed_cap = 300.0; // tune to match typical player speed
            let s = (speed / speed_cap).clamp(0.0, 1.0);

            dir * (max * cfg.controller_look_weight.0 * s)
        }
    } else {
        Vec2::ZERO
    };

    // ------------------------------------------------------------
    // 2) Smooth the look vector itself (fixes “jerk” and jitter)
    // ------------------------------------------------------------
    //
    // This is the clean fix for your Rust error E0502 as well:
    // we read the old value into a local and then assign the new value.
    //
    // Why the error happens:
    // - `*smoothed_look += (...) *smoothed_look ...` tries to mutably and immutably borrow
    //   the same value in one expression.
    //
    // Clean pattern:
    // - copy old into local (Vec2 is Copy)
    // - compute new
    // - write back once
    //
    let look_rate = cfg.look_responsiveness.as_f32();
    let look_alpha = exp_alpha(look_rate, dt);

    let prev_look = *smoothed_look;
    let new_look = prev_look + (desired_look - prev_look) * look_alpha;
    *smoothed_look = new_look;

    // Camera target is player position plus smoothed look-ahead.
    let target = player_pos + *smoothed_look;

    // ------------------------------------------------------------
    // 3) Smooth camera toward target (snappy baseline follow)
    // ------------------------------------------------------------
    let follow_rate = cfg.follow_responsiveness.as_f32();
    let follow_alpha = exp_alpha(follow_rate, dt);

    tf_cam.translation.x += (target.x - tf_cam.translation.x) * follow_alpha;
    tf_cam.translation.y += (target.y - tf_cam.translation.y) * follow_alpha;
}