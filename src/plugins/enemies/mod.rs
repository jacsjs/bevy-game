//! Enemies plugin: static targets with Health + Armour + a short death state,
//! plus "game feel" global effects (screen flash, camera shake, hitstop/slowmo).
//!
//! ---------------------------
//! HOW THIS IS DESIGNED (ECS)
//! ---------------------------
//! Think of the ECS world like a normalized in-memory database:
//!
//! 1) FACTS ("truth") live in components/resources:
//!    - `Health`, `Armour`, `EnemyLifeState` describe gameplay reality.
//!    - `GlobalFx` describes global feedback intent (shake/flash/time effects).
//!    - `FxHandles` caches entity IDs so hot paths avoid repeated world scanning.
//!
//! 2) RULES mutate facts in predictable places:
//!    - collision system (elsewhere) updates Armour/Health.
//!    - this module reads those facts and transitions EnemyLifeState.
//!
//! 3) PRESENTATION is derived from facts:
//!    - enemy sprite colour/alpha/scale derived from ArmourFx + EnemyLifeState.
//!    - camera shake / flash overlay / time scaling derived from GlobalFx.
//!
//! ---------------------------
//! PERFORMANCE + ROBUSTNESS
//! ---------------------------
//! - We avoid per-hit structural changes (adding/removing components) because those
//!   can move entities between archetypes. Instead, ArmourFx is always present and
//!   we only mutate numbers (cheap).
//!
//! - We cache camera + overlay entity handles once and then treat them as invariants
//!   for the rest of the InGame state. Hot paths become straight-line `get_mut(entity)`
//!   instead of scanning queries every frame.
//!
//! - We avoid despawning physics entities inside the fixed physics step.
//!   Instead, we mark `PendingDespawn` and despawn later in PostUpdate.
//!   This prevents "deferred command" interactions where other systems may still
//!   have queued work for the entity.
//!
//! ---------------------------
//! TIME MODEL (HITSTOP / SLOWMO)
//! ---------------------------
//! We implement hitstop/slowmo by changing the speed of virtual game time.
//! - hitstop: speed = 0 for a short real-time window
//! - slowmo: speed interpolates from min_speed back to 1 over a real-time tail
//!
//! The timers tick using real (wall-clock) time so they still progress even while
//! virtual time is frozen.

use avian2d::prelude::*;
use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;
use bevy::time::{Fixed, Real, Virtual};
use bevy_firefly::prelude::Occluder2d;

use crate::common::state::GameState;
use crate::plugins::projectiles::components::{Armour, Enemy, Health};
use crate::plugins::projectiles::layers::Layer;

// We prefer using a specific camera marker for determinism.
// If your project always spawns exactly one main camera, caching it is ideal.
use crate::plugins::camera::MainCamera;

// -----------------------------------------------------------------------------
// Newtypes (encode meaning / prevent mixing units / keep hot code straight-line)
// -----------------------------------------------------------------------------

/// Newtype for values that are conceptually normalized to [0..1].
///
/// Why this matters:
/// - Avoids mixing arbitrary floats with normalized intensities.
/// - Centralizes clamping rules.
/// - Makes intent obvious at call sites.
/// - Allows "branch reduction" in hot code: clamp once on write, not everywhere.
#[derive(Clone, Copy, Debug, Default)]
struct UnitF32(f32);

impl UnitF32 {
    #[inline]
    fn new_clamped(v: f32) -> Self {
        Self(v.clamp(0.0, 1.0))
    }
    #[inline]
    fn get(self) -> f32 {
        self.0
    }
    #[inline]
    fn add_clamped(&mut self, dv: f32) {
        self.0 = (self.0 + dv).clamp(0.0, 1.0);
    }
    #[inline]
    fn decay_to_zero(&mut self, rate_per_sec: f32, dt: f32) {
        self.0 = (self.0 - rate_per_sec * dt).max(0.0);
    }
}

/// Newtype for real-time seconds (wall-clock durations).
///
/// We use real-time for global FX timers (hitstop/slowmo/flash tail) because we want
/// these timers to keep progressing even when virtual game time is slowed or frozen.
#[derive(Clone, Copy, Debug, Default)]
struct RealSeconds(f32);

impl RealSeconds {
    #[inline]
    fn new(v: f32) -> Self {
        Self(v.max(0.0))
    }
    #[inline]
    fn get(self) -> f32 {
        self.0
    }
    #[inline]
    fn set_max(&mut self, v: f32) {
        self.0 = self.0.max(v.max(0.0));
    }
    #[inline]
    fn tick_down(&mut self, dt: f32) {
        self.0 = (self.0 - dt).max(0.0);
    }
    #[inline]
    fn is_positive(self) -> bool {
        self.0 > 0.0
    }
}

// -----------------------------------------------------------------------------
// Components
// -----------------------------------------------------------------------------

/// Enemy lifecycle state machine.
///
/// This state machine is intentionally small:
/// - Alive: normal gameplay.
/// - Dying: short transition animation.
/// - Dead: terminal marker to stop further state transitions.
///
/// Why keep this explicit?
/// - It prevents "contradictory flag" bugs.
/// - It gives a single place to attach animation logic later (sprite sheets, clips).
#[derive(Component, Debug, Clone)]
pub enum EnemyLifeState {
    Alive,
    Dying { timer: Timer },
    Dead,
}

/// Marker: enemy should be removed from the world.
///
/// We don't despawn immediately in the fixed step; we mark and despawn later.
/// This keeps structural changes centralized and avoids ordering hazards.
#[derive(Component, Debug, Clone, Copy)]
pub struct PendingDespawn;

/// Presentation-only armour FX state.
///
/// Important separation:
/// - `Armour` is gameplay truth.
/// - `ArmourFx` is just "how it looks/feels".
///
/// We keep this component always present to avoid structural churn per hit.
/// That keeps performance stable and makes behaviour easy to reason about.
#[derive(Component, Debug, Clone)]
pub struct ArmourFx {
    last_hits_remaining: u16,
    hit_flash: UnitF32,
    break_pulse: UnitF32,
    crackle_remaining: RealSeconds,
    crackle_phase: f32,
}

impl ArmourFx {
    fn new(initial_hits: u16) -> Self {
        Self {
            last_hits_remaining: initial_hits,
            hit_flash: UnitF32::default(),
            break_pulse: UnitF32::default(),
            crackle_remaining: RealSeconds::default(),
            crackle_phase: 0.0,
        }
    }

    /// Used to skip extra colour math when no effect is active.
    #[inline]
    fn any_active(&self) -> bool {
        self.hit_flash.get() > 0.001
            || self.break_pulse.get() > 0.001
            || self.crackle_remaining.is_positive()
    }
}

/// Marker for the fullscreen flash overlay entity.
#[derive(Component, Debug, Clone, Copy)]
struct ScreenFlashOverlay;

// -----------------------------------------------------------------------------
// Resources (normalized global FX truth + cached handles)
// -----------------------------------------------------------------------------

/// Cached entity handles for fast access.
///
/// This is a classic "boundary invariant" pattern:
/// - find the camera once
/// - spawn/find the overlay once
/// - hot loop uses `get_mut(entity)` instead of scanning queries.
///
/// Also stores `prev_shake_offset` so the shake does not accumulate drift.
#[derive(Resource, Debug, Default, Clone, Copy)]
struct FxHandles {
    camera: Option<Entity>,
    overlay: Option<Entity>,
    prev_shake_offset: Vec2,
}

/// Global FX state.
///
/// This resource is the single source of truth for global presentation effects.
/// Producers (like armour break) write intent here. Consumer applies effects to:
/// - time scaling
/// - camera transform
/// - overlay alpha
///
/// Keeping it centralized prevents "multiple systems fighting the camera" or
/// "time scaling applied inconsistently".
#[derive(Resource, Debug)]
pub struct GlobalFx {
    // Camera shake parameters:
    // trauma is intensity [0..1], shake_phase is a deterministic oscillator.
    trauma: UnitF32,
    shake_phase: f32,

    // White flash intensity [0..1].
    flash: UnitF32,

    // Time control timers (real time):
    // hitstop freezes virtual time briefly, slowmo smoothly returns to normal.
    hitstop: RealSeconds,
    slowmo_remaining: RealSeconds,
    slowmo_duration: RealSeconds,
    slowmo_min_speed: f32, // [0..1]
}

impl Default for GlobalFx {
    fn default() -> Self {
        Self {
            trauma: UnitF32::default(),
            shake_phase: 0.0,
            flash: UnitF32::default(),
            hitstop: RealSeconds::default(),
            slowmo_remaining: RealSeconds::default(),
            slowmo_duration: RealSeconds::new(1.0),
            slowmo_min_speed: 0.22,
        }
    }
}

impl GlobalFx {
    /// Armour break "feel preset".
    ///
    /// This function packages multiple sensory cues together:
    /// - shake (visceral)
    /// - flash (readability)
    /// - hitstop (impact)
    /// - slowmo tail (emphasis)
    ///
    /// This is a scalable approach: later you can add more presets (explosion, crit, boss hit).
    fn trigger_armour_break(&mut self) {
        self.trauma.add_clamped(0.95);
        self.flash = UnitF32::new_clamped(1.0);

        self.hitstop.set_max(0.09);

        self.slowmo_duration = RealSeconds::new(1.0);
        self.slowmo_remaining.set_max(self.slowmo_duration.get());
        self.slowmo_min_speed = 0.22;
    }
}

// -----------------------------------------------------------------------------
// Plugin wiring
// -----------------------------------------------------------------------------

/// Register enemy systems.
///
/// Schedules:
/// - FixedPostUpdate: consume combat results (armour/health) and update enemy-local states.
/// - PostUpdate: apply global effects and do structural cleanup.
///
/// The separation keeps "simulation" stable and "presentation" smooth.
pub fn plugin(app: &mut App) {
    app.insert_resource(GlobalFx::default());
    app.insert_resource(FxHandles::default());

    // Spawn enemies once per entry into InGame.
    app.add_systems(OnEnter(GameState::InGame), spawn_targets);

    // Fixed-step lifecycle:
    // - death trigger runs after collision resolution so it sees updated Health.
    // - death progress animates and marks PendingDespawn when complete.
    app.add_systems(
        FixedPostUpdate,
        enemy_death_trigger
            .after(crate::plugins::projectiles::collision::process_player_bullet_collisions)
            .run_if(in_state(GameState::InGame)),
    );

    app.add_systems(
        FixedPostUpdate,
        enemy_death_progress
            .after(enemy_death_trigger)
            .run_if(in_state(GameState::InGame)),
    );

    // Fixed-step armour visuals:
    // - read Armour changes
    // - update local ArmourFx
    // - trigger global effects on break
    app.add_systems(
        FixedPostUpdate,
        armour_fx_update
            .after(crate::plugins::projectiles::collision::process_player_bullet_collisions)
            .after(enemy_death_trigger)
            .run_if(in_state(GameState::InGame)),
    );

    // PostUpdate boundary: ensure camera/overlay handles exist.
    // After this, apply_global_fx can run straight-line and fast.
    app.add_systems(
        PostUpdate,
        ensure_fx_handles.run_if(in_state(GameState::InGame)),
    );

    // PostUpdate hot path: apply global effects.
    app.add_systems(
        PostUpdate,
        apply_global_fx
            .after(ensure_fx_handles)
            .run_if(in_state(GameState::InGame)),
    );

    // PostUpdate structural cleanup: despawn after fixed-step work is done.
    app.add_systems(
        PostUpdate,
        despawn_marked_enemies.run_if(in_state(GameState::InGame)),
    );
}

// -----------------------------------------------------------------------------
// Spawn
// -----------------------------------------------------------------------------

/// Collision layers for an enemy that should no longer interact with anything.
///
/// We keep membership as "Enemy" but clear filters:
/// - avoids structural changes
/// - stops new collision interactions immediately
#[inline]
fn non_interacting_enemy_layers() -> CollisionLayers {
    CollisionLayers::new(Layer::Enemy, [] as [Layer; 0])
}

/// Spawn a few stationary targets.
///
/// This is intentionally asset-free: plain sprites and simple colliders.
fn spawn_targets(mut commands: Commands) {
    // Enemy collision intent:
    // - enemy collides with world, player, and player bullets.
    let enemy_layers = CollisionLayers::new(
        Layer::Enemy,
        [Layer::World, Layer::Player, Layer::PlayerBullet],
    );

    let initial_armour: u16 = 3;
    let initial_hp: i32 = 5;

    for (i, x) in [-200.0, 0.0, 200.0].into_iter().enumerate() {
        commands.spawn((
            Name::new(format!("EnemyTarget{i}")),
            Enemy,
            Armour {
                hits_remaining: initial_armour,
                max_hits: initial_armour,
            },
            Health { hp: initial_hp },
            EnemyLifeState::Alive,
            ArmourFx::new(initial_armour),
            Sprite {
                color: Color::srgb(0.9, 0.25, 0.25),
                custom_size: Some(Vec2::splat(32.0)),
                ..default()
            },
            Transform::from_xyz(x, 120.0, 1.0),
            RigidBody::Static,
            Collider::circle(16.0),
            enemy_layers,
            Occluder2d::circle(16.0),
            DespawnOnExit(GameState::InGame),
        ));
    }
}

// -----------------------------------------------------------------------------
// Rules: enemy death lifecycle
// -----------------------------------------------------------------------------

/// Transition Alive -> Dying when HP drops to 0.
///
/// Note: this system does not despawn.
/// It only transitions state and enforces "dying invariants" (stop collision interaction).
fn enemy_death_trigger(
    mut q: Query<(
        &Health,
        &mut EnemyLifeState,
        &mut CollisionLayers,
        &mut Sprite,
        &mut Transform,
    ), (With<Enemy>, Without<PendingDespawn>)>,
) {
    for (hp, mut life, mut layers, mut sprite, mut tf) in &mut q {
        if !matches!(*life, EnemyLifeState::Alive) {
            continue;
        }

        if hp.hp <= 0 {
            *life = EnemyLifeState::Dying {
                timer: Timer::from_seconds(0.35, TimerMode::Once),
            };
            *layers = non_interacting_enemy_layers();

            // Immediate readability: a neutral tint and reset scale.
            sprite.color = Color::srgba(0.8, 0.8, 0.8, 1.0);
            tf.scale = Vec3::ONE;
        }
    }
}

/// Animate Dying state and mark PendingDespawn once finished.
///
/// This keeps despawning centralized and delayed.
fn enemy_death_progress(
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut EnemyLifeState, &mut Sprite, &mut Transform), (With<Enemy>, Without<PendingDespawn>)>,
) {
    for (e, mut life, mut sprite, mut tf) in &mut q {
        let EnemyLifeState::Dying { timer } = &mut *life else {
            continue;
        };

        timer.tick(time.delta());

        // Normalized [0..1] for simple animation curves.
        let dur = timer.duration().as_secs_f32().max(0.0001);
        let t = (timer.elapsed_secs() / dur).clamp(0.0, 1.0);

        // Simple asset-free death animation.
        tf.scale = Vec3::splat(1.0 - t);

        let mut c = sprite.color.to_srgba();
        c.alpha = 1.0 - t;
        sprite.color = c.into();

        if timer.is_finished() {
            *life = EnemyLifeState::Dead;
            commands.entity(e).insert(PendingDespawn);
        }
    }
}

// -----------------------------------------------------------------------------
// Presentation: armour FX + triggers global FX on break
// -----------------------------------------------------------------------------

/// Update local armour visuals and trigger global effects on armour break.
///
/// This system reads gameplay truth (`Armour`) and writes presentation state (`ArmourFx`).
/// It also triggers global feedback via `GlobalFx` when a break is detected.
fn armour_fx_update(
    fixed_time: Res<Time<Fixed>>,
    mut global_fx: ResMut<GlobalFx>,
    mut q: Query<(&Armour, &mut ArmourFx, &mut Sprite, &EnemyLifeState), (With<Enemy>, Without<PendingDespawn>)>,
) {
    // Using Fixed time means hitstop/slowmo affects these visuals too.
    let dt = fixed_time.delta_secs();

    for (armour, mut fx, mut sprite, life) in &mut q {
        if !matches!(life, EnemyLifeState::Alive) {
            continue;
        }

        let new_hits = armour.hits_remaining;
        let old_hits = fx.last_hits_remaining;

        // Detect armour changes without events:
        // - compare last seen value to current value
        // - update last seen
        if new_hits < old_hits {
            fx.hit_flash = UnitF32::new_clamped(1.0);

            // Break when it crosses to 0.
            if new_hits == 0 && old_hits > 0 {
                fx.break_pulse = UnitF32::new_clamped(1.0);
                fx.crackle_remaining = RealSeconds::new(0.32);
                fx.crackle_phase = 0.0;

                global_fx.trigger_armour_break();
            }
        }

        fx.last_hits_remaining = new_hits;

        // Decay local FX toward zero.
        fx.hit_flash.decay_to_zero(8.0, dt);
        fx.break_pulse.decay_to_zero(3.2, dt);
        if fx.crackle_remaining.is_positive() {
            fx.crackle_remaining.tick_down(dt);
            fx.crackle_phase += dt;
        }

        // Base colour communicates armour state.
        let base = if new_hits > 0 {
            Color::srgb(0.35, 0.65, 1.0)
        } else {
            Color::srgb(0.9, 0.25, 0.25)
        };

        // Skip extra math when nothing is active.
        if !fx.any_active() {
            sprite.color = base;
            continue;
        }

        // Compose colour as base + layered flashes.
        let mut out = base.to_srgba();

        // Hit flash pushes towards white.
        let hf = fx.hit_flash.get();
        out.red = (out.red + hf * 0.55).min(1.0);
        out.green = (out.green + hf * 0.55).min(1.0);
        out.blue = (out.blue + hf * 0.55).min(1.0);

        // Break pulse pushes towards cyan.
        let bp = fx.break_pulse.get();
        out.red = (out.red + bp * 0.08).min(1.0);
        out.green = (out.green + bp * 0.30).min(1.0);
        out.blue = (out.blue + bp * 0.70).min(1.0);

        // Crackle: deterministic flicker for extra "shatter" feel.
        let cr = fx.crackle_remaining.get();
        if cr > 0.0 {
            let r = (cr / 0.32).clamp(0.0, 1.0);
            let amp = r * r;

            let s1 = (fx.crackle_phase * 48.0 * std::f32::consts::TAU).sin();
            let s2 = (fx.crackle_phase * 73.0 * std::f32::consts::TAU).sin();
            let flicker = (0.5 + 0.5 * (0.6 * s1 + 0.4 * s2)).clamp(0.0, 1.0);

            out.red = (out.red + amp * flicker * 0.05).min(1.0);
            out.green = (out.green + amp * flicker * 0.18).min(1.0);
            out.blue = (out.blue + amp * flicker * 0.35).min(1.0);
        }

        out.alpha = 1.0;
        sprite.color = out.into();
    }
}

// -----------------------------------------------------------------------------
// PostUpdate boundary: establish invariants (cache camera + spawn overlay once)
// -----------------------------------------------------------------------------

/// Ensure `FxHandles` has valid entity IDs for camera and overlay.
///
/// Once these are set, the hot path can assume they exist during InGame.
/// If they are missing (startup ordering), we simply keep trying until found.
fn ensure_fx_handles(
    mut commands: Commands,
    mut handles: ResMut<FxHandles>,
    q_main_cam: Query<Entity, With<MainCamera>>,
    q_any_cam: Query<Entity, With<Camera2d>>,
    q_overlay: Query<Entity, With<ScreenFlashOverlay>>,
) {
    // If already cached, nothing to do.
    if handles.camera.is_some() && handles.overlay.is_some() {
        return;
    }

    // Cache camera entity.
    if handles.camera.is_none() {
        handles.camera = q_main_cam.single().ok().or_else(|| q_any_cam.iter().next());
    }

    // Cache overlay entity; spawn it if it doesn't exist yet.
    if handles.overlay.is_none() {
        handles.overlay = q_overlay.single().ok().or_else(|| {
            let e = commands
                .spawn((
                    ScreenFlashOverlay,
                    Sprite {
                        color: Color::srgba(1.0, 1.0, 1.0, 0.0),
                        custom_size: Some(Vec2::splat(5000.0)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, 10_000.0),
                    Visibility::Hidden,
                ))
                .id();
            Some(e)
        });
    }
}

// -----------------------------------------------------------------------------
// PostUpdate hot path: apply global effects
// -----------------------------------------------------------------------------

/// Quintic easing (0..1 -> 0..1) for very smooth slowmo fade-back.
///
/// Compared to linear interpolation, this avoids sharp acceleration changes and
/// feels more "cinematic" when returning to normal speed.
#[inline]
fn smootherstep(x: f32) -> f32 {
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

/// Apply global effects from `GlobalFx`.
///
/// This system is intentionally centralized:
/// - it is the only writer to camera shake transform adjustments
/// - it is the only writer to overlay alpha
/// - it is the only writer to virtual time speed
///
/// That prevents subtle "systems fighting each other" bugs.
fn apply_global_fx(
    real_time: Res<Time<Real>>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut fx: ResMut<GlobalFx>,
    mut handles: ResMut<FxHandles>,

    // These two queries both touch Transform mutably, but are guaranteed disjoint:
    // - camera entities have Camera2d and not ScreenFlashOverlay
    // - overlay entity has ScreenFlashOverlay and not Camera2d
    mut q_cam_tf: Query<&mut Transform, (With<Camera2d>, Without<ScreenFlashOverlay>)>,
    mut q_overlay: Query<(&mut Transform, &mut Sprite, &mut Visibility), (With<ScreenFlashOverlay>, Without<Camera2d>)>,
) {
    let dt = real_time.delta_secs();

    // If invariants aren't established yet, do nothing this frame.
    let Some(cam_e) = handles.camera else { return; };
    let Some(overlay_e) = handles.overlay else { return; };

    // -----------------
    // TIME CONTROL
    // -----------------
    // hitstop has priority over slowmo.
    if fx.hitstop.is_positive() {
        fx.hitstop.tick_down(dt);
        virtual_time.set_relative_speed(0.0);
    } else if fx.slowmo_remaining.is_positive() {
        fx.slowmo_remaining.tick_down(dt);

        let dur = fx.slowmo_duration.get().max(0.0001);
        let progress = 1.0 - (fx.slowmo_remaining.get() / dur).clamp(0.0, 1.0);
        let eased = smootherstep(progress);

        let min = fx.slowmo_min_speed.clamp(0.0, 1.0);
        let speed = min + (1.0 - min) * eased;
        virtual_time.set_relative_speed(speed);
    } else {
        virtual_time.set_relative_speed(1.0);
    }

    // -----------------
    // CAMERA SHAKE
    // -----------------
    // Remove last frame's offset first to prevent drift.
    if let Ok(mut cam_tf) = q_cam_tf.get_mut(cam_e) {
        cam_tf.translation.x -= handles.prev_shake_offset.x;
        cam_tf.translation.y -= handles.prev_shake_offset.y;
    }
    handles.prev_shake_offset = Vec2::ZERO;

    // Decay trauma for a long-ish tail.
    fx.shake_phase += dt;
    fx.trauma.decay_to_zero(0.9, dt);

    if fx.trauma.get() > 0.0 {
        let strength = fx.trauma.get() * fx.trauma.get();
        let amp = 42.0 * strength;

        // Deterministic pseudo-noise (no RNG needed).
        let x = (fx.shake_phase * 37.0 * std::f32::consts::TAU).sin()
            + 0.5 * (fx.shake_phase * 61.0 * std::f32::consts::TAU).sin();
        let y = (fx.shake_phase * 41.0 * std::f32::consts::TAU).cos()
            + 0.5 * (fx.shake_phase * 53.0 * std::f32::consts::TAU).cos();

        let offset = Vec2::new(x, y).clamp_length_max(1.0) * amp;

        if let Ok(mut cam_tf) = q_cam_tf.get_mut(cam_e) {
            cam_tf.translation.x += offset.x;
            cam_tf.translation.y += offset.y;
            handles.prev_shake_offset = offset;
        }
    }

    // -----------------
    // WHITE FLASH OVERLAY
    // -----------------
    // Decay flash quickly for a snappy effect.
    fx.flash.decay_to_zero(3.0, dt);

    if let Ok((mut tf, mut sprite, mut vis)) = q_overlay.get_mut(overlay_e) {
        // Center overlay on camera so it behaves like a screen-space flash.
        if let Ok(cam_tf) = q_cam_tf.get(cam_e) {
            tf.translation.x = cam_tf.translation.x;
            tf.translation.y = cam_tf.translation.y;
        }
        tf.translation.z = 10_000.0;

        if fx.flash.get() > 0.001 {
            *vis = Visibility::Visible;
            let mut c = sprite.color.to_srgba();
            c.alpha = (fx.flash.get() * 0.85).clamp(0.0, 0.85);
            sprite.color = c.into();
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

// -----------------------------------------------------------------------------
// Cleanup (PostUpdate)
// -----------------------------------------------------------------------------

/// Despawn enemies marked for removal.
///
/// Centralizing despawn in one system keeps structural changes predictable.
fn despawn_marked_enemies(mut commands: Commands, q: Query<Entity, With<PendingDespawn>>) {
    for e in &q {
        commands.entity(e).despawn();
    }
}


#[cfg(test)]
mod tests;