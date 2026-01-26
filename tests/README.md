# Performance Playbook (Bullet Scaling, Pooling, Profiling, Budgets)

This chapter is a **larger, end-to-end performance playbook** for your modular Bevy + Avian + Firefly game.
It ties together the design decisions already documented (messages for collisions, state scoping, intent input, debug HUD) and turns them into a concrete process:

- set budgets (what “good performance” means)
- measure the right metrics (FPS, frame time, entity count, bullets, collision events)
- reduce hot-path work (collision filtering, opt-in collision events)
- minimize structural churn (pooling, fewer spawns/despawns)
- iterate safely (profiling workflow + performance regression checks)

Bevy’s diagnostics system can log or expose measured values (for FPS, frame time, entity count, etc.), and `LogDiagnosticsPlugin` logs those diagnostics periodically to the console.[^bevy_log_diagnostics][^bevy_diag_toggle]
Bevy’s `Commands` are deferred structural changes applied during `ApplyDeferred`, which exists because structural changes require exclusive `World` access.[^bevy_commands][^bevy_commands_deferred_ops]
Avian collision events can be read efficiently as Bevy `Message`s, and are only generated for entities that have `CollisionEventsEnabled`.[^avian_collision_events][^avian_collision_module]

---

## 0) Guiding principles (what we optimize for)

1. **Measure first, then optimize.** “Feels slow” is not actionable; “frame time spikes when bullets exceed N” is.
2. **Optimize the hot path:** bullets, collisions, spawning/despawning, and UI updates.
3. **Prefer “do less work” over “do the same work faster”:** filtering, gating, and coarser colliders win.
4. **Keep performance changes modular and testable:** new systems/resources should be separately togglable.

---

## 1) Performance budgets (targets you can enforce)

Budgets define what you’re willing to spend per frame. They act as a “contract” for new features.

### Suggested baseline budgets (desktop, 2D)

> Adjust based on your machine and goals.

```text
Target FPS:            60 (16.67 ms/frame) minimum, 120 (8.33 ms/frame) preferred
CPU frame budget:      10–12 ms (leave headroom for GPU)
Bullet budget (Tier 1):  500 active bullets (simple physics + events)
Bullet budget (Tier 2):  5,000 active bullets (pooling + aggressive filtering)
Bullet budget (Tier 3):  20,000+ active bullets (pooling + kinematic bullets + queries)
Collision event budget:  O(bullets that can hit) not O(all contacts)
Spawn/despawn budget:    “rare” in steady-state; avoid per-frame churn

Hard rule: never regress Tier 1.
Soft rule: Tier 2 is the growth target.
Tier 3 requires architecture changes (see §6).
```

Why these budgets:

- Bullet-heavy games spend most CPU time on collision detection + entity churn.
- You want collision events only where needed (bullets, player, enemies), not for every collider.
  Avian provides `CollisionEventsEnabled` specifically to control this overhead.[^avian_collision_events][^avian_collision_module]

---

## 2) Instrumentation: the metrics you should always have

### 2.1 Core metrics

- **FPS / frame time** (smoothed and instantaneous)
- **Entity count** (catches leaks and runaway spawning)
- **Bullet count** (pooling validation)
- **CollisionStart count per second** (impact load)
- **Spawn/despawn count per second** (structural churn)

Bevy provides frame-time diagnostics and a logging plugin (`LogDiagnosticsPlugin`) that prints diagnostics to the console.[^bevy_log_diagnostics][^bevy_diag_toggle]
You can also create custom diagnostics and feed measurements each frame.[^bevy_custom_diagnostic]

### 2.2 Minimum setup: log diagnostics

```rust
use bevy::prelude::*;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};

pub fn diagnostics_plugin(app: &mut App) {
    app.add_plugins((
        FrameTimeDiagnosticsPlugin::default(),
        LogDiagnosticsPlugin::default(),
    ));
}
```

`LogDiagnosticsPlugin` logs diagnostics collected by providers like `FrameTimeDiagnosticsPlugin` and does nothing if no diagnostics are provided.[^bevy_log_diagnostics]

### 2.3 Custom diagnostics: bullets + collisions

Bevy supports custom diagnostics via `register_diagnostic(...)` and adding measurements through `Diagnostics` in a system.[^bevy_custom_diagnostic]

```rust
use bevy::prelude::*;
use bevy::diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic};

pub const BULLET_COUNT: DiagnosticPath = DiagnosticPath::const_new("bullets");
pub const COLLISION_STARTS: DiagnosticPath = DiagnosticPath::const_new("collision_starts");

#[derive(Component)]
pub struct Bullet;

pub fn perf_metrics_plugin(app: &mut App) {
    app.register_diagnostic(Diagnostic::new(BULLET_COUNT))
        .register_diagnostic(Diagnostic::new(COLLISION_STARTS))
        .add_systems(Update, (measure_bullets, measure_collisions));
}

fn measure_bullets(mut diagnostics: Diagnostics, q: Query<(), With<Bullet>>) {
    let n = q.iter().count() as f64;
    diagnostics.add_measurement(&BULLET_COUNT, || n);
}

// If you read CollisionStart as Messages, you can count them while processing.
fn measure_collisions(mut diagnostics: Diagnostics, mut starts: Local<u32>) {
    // This is just a placeholder to show a counter.
    let n = *starts as f64;
    diagnostics.add_measurement(&COLLISION_STARTS, || n);
    *starts = 0;
}
```

---

## 3) Collision strategy for bullet scaling

### 3.1 Enable collision events only where needed

Avian collision events (`CollisionStart`, `CollisionEnd`) are only generated for entities tagged with `CollisionEventsEnabled`.[^avian_collision_events][^avian_collision_module]
This is critical for bullet-hell performance because it prevents iterating every contact pair.

**Rule:**

- bullets: `CollisionEventsEnabled`
- player: `CollisionEventsEnabled`
- enemies: `CollisionEventsEnabled`
- walls/world: usually **no** collision events unless needed

```rust
use avian2d::prelude::*;
use bevy::prelude::*;

fn spawn_bullet(mut commands: Commands) {
    commands.spawn((
        Bullet,
        RigidBody::Kinematic,
        Collider::circle(2.0),
        CollisionEventsEnabled,
    ));
}
```

Avian’s docs explicitly recommend message-based reading for large numbers of collisions (e.g., bullet hits) and observer-based handling for entity-specific triggers.[^avian_collision_events]

### 3.2 Prefer MessageReader for bulk collision processing

When you have many bullet hits, reading collision events as `Message`s with `MessageReader<CollisionStart>` is efficient for bulk processing.[^avian_collision_events]

```rust
use avian2d::prelude::*;
use bevy::prelude::*;

fn process_bullet_hits(
    mut starts: MessageReader<CollisionStart>,
    bullets: Query<(), With<Bullet>>,
    enemies: Query<(), With<Enemy>>,
    mut commands: Commands,
) {
    for e in starts.read() {
        let a = e.collider1;
        let b = e.collider2;

        let bullet_enemy = (bullets.contains(a) && enemies.contains(b))
            || (bullets.contains(b) && enemies.contains(a));

        if bullet_enemy {
            commands.entity(a).despawn();
            commands.entity(b).despawn();
        }
    }
}
```

(Replace `despawn()` with pooling return once pooling is implemented; see §5.)

### 3.3 Collision filtering is non-negotiable

Avian’s collision filtering is based on `CollisionLayers` memberships and filters.
Two colliders interact only when each collider’s memberships overlap the other’s filters.[^avian_collision_layers]
This is both correctness and performance: it prevents unnecessary collision checks.

Define your project layers once and treat them as a contract:

```rust
use avian2d::prelude::*;

#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum Layer {
    #[default] Default,
    World,
    Player,
    Enemy,
    PlayerBullet,
    EnemyBullet,
}
```

`PhysicsLayer` exists as a trait specifically for layers used heavily by `CollisionLayers`, and can be derived for enums.[^avian_physics_layer][^avian_collision_layers]

**Recommended interaction matrix (rules)**

```text
PlayerBullet collides with: Enemy, World
EnemyBullet collides with: Player, World
Player collides with: World, Enemy
Enemy collides with: World, Player
Bullet vs Bullet: NO
EnemyBullet vs Enemy: NO (usually)
PlayerBullet vs Player: NO
```

Enforce these rules in tests by checking that spawned entities get the correct `CollisionLayers` configuration.

---

## 4) Structural churn: why pooling matters

### 4.1 Why spawn/despawn becomes expensive

Bevy defers structural changes through `Commands` because spawning/despawning and component insert/removal require exclusive `World` access.
Those queued commands are applied when `ApplyDeferred` runs.[^bevy_commands][^bevy_commands_deferred_ops]

This is powerful but not free:

- creating/destroying entities causes archetype churn
- large churn increases time in deferred application

Bevy even notes that deferring mutations has overhead and should only be used when it’s worth the parallelization gains.[^bevy_deferred]

### 4.2 Steady-state rule

**Rule:** In the steady-state (gameplay loop), avoid per-frame spawn/despawn.
Instead, pre-allocate and reuse bullet entities.

---

## 5) Bullet pooling: design, implementation, and tests

### 5.1 Pool design goals

- O(1) checkout/return
- bullet components reset to defaults
- no physics “ghost state” left behind
- no events generated for inactive bullets

### 5.2 Minimal pool components

```rust
use bevy::prelude::*;

#[derive(Component)]
pub struct PooledBullet;

#[derive(Component)]
pub struct ActiveBullet;

#[derive(Resource, Default)]
pub struct BulletPool {
    pub inactive: Vec<Entity>,
}
```

### 5.3 Checkout / return API

```rust
use bevy::prelude::*;
use avian2d::prelude::*;

use crate::common::layers::Layer;

pub fn checkout_bullet(commands: &mut Commands, pool: &mut BulletPool) -> Entity {
    if let Some(e) = pool.inactive.pop() {
        commands.entity(e)
            .insert((ActiveBullet, Visibility::Visible))
            .remove::<Sleeping>();
        e
    } else {
        // Create a new bullet if pool is empty.
        commands
            .spawn((
                PooledBullet,
                ActiveBullet,
                Visibility::Visible,
                RigidBody::Kinematic,
                Collider::circle(2.0),
                CollisionEventsEnabled,
                CollisionLayers::new(Layer::PlayerBullet, [Layer::Enemy, Layer::World]),
            ))
            .id()
    }
}

pub fn return_bullet(commands: &mut Commands, pool: &mut BulletPool, e: Entity) {
    commands.entity(e)
        .remove::<ActiveBullet>()
        .insert((Visibility::Hidden, LinearVelocity::ZERO));
    pool.inactive.push(e);
}
```

Notes:

- You should remove or disable anything that makes the bullet “participate” while inactive (visibility, collision events, layers, rigid body). What you choose depends on your current implementation.
- Avian colliders can be configured with collision layers and collision events as separate components, so it’s natural to enable/disable these per bullet.[^avian_collider][^avian_collision_events]

### 5.4 Pooling tests

Test the pool behavior in isolation:

```rust
use bevy::prelude::*;

#[test]
fn bullet_pool_reuses_entities() {
    let mut world = World::new();
    world.init_resource::<BulletPool>();

    // Build a mini schedule with checkout/return systems if desired.
    // Or call your functions directly if you use World insertion.
}
```

(Depending on how you structure your pool API, tests can run using `World::run_system_once` or a small schedule; keep them headless and deterministic.)

---

## 6) Scaling beyond “physics bullets”: kinematic bullets + queries (Tier 3)

Once you push beyond a few thousand active bullets, the limiting factors tend to be:

- narrow-phase contact generation
- solver bookkeeping
- event volume

Avian supports spatial queries (raycasts, shapecasts, intersections), which can be used to implement “kinematic bullets” that don’t rely on rigid-body contact generation for every bullet.[^avian_repo]

**Tier 3 approach:**

- bullets become “kinematic”: update position from velocity
- detect hits via spatial queries against a broad collision representation
- emit `HitEvent` messages manually

This reduces contact generation overhead at the cost of custom hit logic.

> You don’t need this on day 1. Pooling + collision filtering + opt-in collision events will carry you far.

---

## 7) Profiling workflow (repeatable steps)

### 7.1 The loop

1) Establish baseline metrics (FPS, bullets, collisions)
2) Add a stress mode (spawn N bullets/sec)
3) Identify bottleneck: collisions vs spawns vs rendering
4) Apply one optimization at a time
5) Verify improvement and no regression

### 7.2 Built-in tools

- Log diagnostics to the console (`LogDiagnosticsPlugin`).[^bevy_log_diagnostics]
- Add custom diagnostics (`register_diagnostic` and `Diagnostics::add_measurement`).[^bevy_custom_diagnostic]
- Disable/enable diagnostics at runtime using `DiagnosticsStore` (handy for reducing noise).[^bevy_diag_toggle]

---

## 8) Performance gates (prevent regressions)

### 8.1 “Perf smoke test” (ignored by default)

Create an integration test that runs for a few hundred ticks and asserts:

- no panics
- bullet count stays within expected bounds (pool works)
- collision starts per tick stay below a threshold for a known scenario

Mark it `#[ignore]` and run it in CI nightly or manually.

### 8.2 Track budgets in code

Keep budgets in a single resource:

```rust
use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy)]
pub struct PerfBudget {
    pub max_active_bullets: usize,
    pub max_collision_starts_per_frame: usize,
}
```

Then you can add a debug-only system that warns if budgets are exceeded.

---

## 9) Common performance pitfalls (and fixes)

### Pitfall: collision events for everything

Fix: only add `CollisionEventsEnabled` to entities that truly need events (bullets, player, enemies). Avian only generates events for those entities.[^avian_collision_events][^avian_collision_module]

### Pitfall: bullets collide with bullets

Fix: enforce collision layer rules with `CollisionLayers` so bullets don’t even get considered as contact pairs.[^avian_collision_layers]

### Pitfall: spawning/despawning bullets every frame

Fix: pooling. Bevy commands are deferred and structural changes require exclusive access; reducing churn reduces deferred application cost.[^bevy_commands][^bevy_deferred]

### Pitfall: heavy work in UI every frame

Fix: update UI on changes or at a fixed interval; keep debug HUD lightweight.

---

---

## 10) Stress test scene (repeatable performance experiments)

A stress scene is a controlled way to answer questions like:

- “How many bullets can we sustain before frame time spikes?”
- “Does pooling actually cap entity churn?”
- “Are collision events proportional to *useful* interactions?”

The goal is **repeatability**: same parameters → same load.

### 10.1 Stress mode controls (recommended)

- Toggle stress mode on/off (`F9`)
- Increase/decrease spawn rate (`[` / `]`)
- Increase/decrease max active bullets (`-` / `=`)
- Toggle collision events on bullets (`F10`)

(You can wire this into your existing Debug HUD section so you can see parameters live.)

### 10.2 Code example: StressScene resource + spawner

This spawner:

- spawns bullets at a configurable rate
- optionally uses pooling (if you already have `BulletPool`)
- emits a predictable bullet pattern (ring / spiral)

```rust
// src/plugins/stress_scene/mod.rs
use bevy::prelude::*;
use avian2d::prelude::*;

use crate::common::layers::Layer;
use crate::plugins::projectiles::pool::{BulletPool, checkout_bullet, return_bullet};
use crate::plugins::projectiles::bullets::Bullet; // your Bullet component

#[derive(Resource, Debug)]
pub struct StressConfig {
    pub enabled: bool,
    pub bullets_per_second: f32,
    pub max_active_bullets: usize,
    pub enable_collision_events: bool,
}

impl Default for StressConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bullets_per_second: 200.0,
            max_active_bullets: 5_000,
            enable_collision_events: true,
        }
    }
}

#[derive(Resource, Default)]
struct StressTimer(Timer);

pub fn plugin(app: &mut App) {
    app.init_resource::<StressConfig>()
        .init_resource::<StressTimer>()
        .add_systems(Update, (stress_controls, stress_spawn));
}

fn stress_controls(keys: Res<ButtonInput<KeyCode>>, mut cfg: ResMut<StressConfig>) {
    if keys.just_pressed(KeyCode::F9) {
        cfg.enabled = !cfg.enabled;
        info!(enabled = cfg.enabled, "stress mode toggled");
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        cfg.bullets_per_second *= 1.25;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        cfg.bullets_per_second /= 1.25;
    }
    if keys.just_pressed(KeyCode::Equal) {
        cfg.max_active_bullets = (cfg.max_active_bullets as f32 * 1.25) as usize;
    }
    if keys.just_pressed(KeyCode::Minus) {
        cfg.max_active_bullets = (cfg.max_active_bullets as f32 / 1.25).max(100.0) as usize;
    }
    if keys.just_pressed(KeyCode::F10) {
        cfg.enable_collision_events = !cfg.enable_collision_events;
    }
}

fn stress_spawn(
    time: Res<Time>,
    mut timer: ResMut<StressTimer>,
    cfg: Res<StressConfig>,
    q_active: Query<Entity, With<Bullet>>,
    mut pool: Option<ResMut<BulletPool>>,
    mut commands: Commands,
    mut angle: Local<f32>,
) {
    if !cfg.enabled {
        return;
    }

    // Cap active bullets (either pooled or despawn oldest; here we just stop spawning)
    if q_active.iter().count() >= cfg.max_active_bullets {
        return;
    }

    // Spawn `bullets_per_second` bullets
    let interval = (1.0 / cfg.bullets_per_second).max(0.0001);
    if timer.0.duration().as_secs_f32() != interval {
        timer.0 = Timer::from_seconds(interval, TimerMode::Repeating);
    }

    while timer.0.tick(time.delta()).just_finished() {
        *angle += 0.35; // spiral
        let dir = Vec2::from_angle(*angle).normalize();
        let pos = Vec2::ZERO;

        // --- checkout bullet ---
        let e = if let Some(mut pool) = pool.as_deref_mut() {
            checkout_bullet(&mut commands, &mut pool)
        } else {
            commands.spawn_empty().id()
        };

        // --- configure bullet components ---
        commands.entity(e).insert((
            Bullet, // your marker/component
            Transform::from_translation(pos.extend(0.0)),
            LinearVelocity(dir * 900.0),
            RigidBody::Kinematic,
            Collider::circle(2.0),
            CollisionLayers::new(Layer::PlayerBullet, [Layer::Enemy, Layer::World]),
        ));

        if cfg.enable_collision_events {
            commands.entity(e).insert(CollisionEventsEnabled);
        } else {
            commands.entity(e).remove::<CollisionEventsEnabled>();
        }
    }
}
```

### 10.3 Why this is a good stress harness

- It creates load in the hot path: bullet movement + collisions + messages.
- You can flip one knob at a time and observe metrics.
- It uses the exact same gameplay components (no fake code).

Avian collision events are only generated for entities with `CollisionEventsEnabled`, which is exactly what the `F10` toggle is testing.[^avian_collision_events]

---

## 11) Tracing: profiling running systems with spans

When diagnostics tell you “something is slow”, tracing tells you **what** is slow.

### 11.1 Bevy built-in tracing spans

Bevy includes built-in tracing spans for ECS systems and engine internals.
The official Bevy profiling docs recommend enabling the `trace` cargo feature and ensuring your log/tracing level is at least `info` (don’t filter out spans via `LogPlugin`).[^bevy_profiling_doc]

### 11.2 Add your own spans (recommended for hot systems)

Bevy’s profiling docs show how to add spans with `info_span!` and enter them to measure a block of work.[^bevy_profiling_doc]

```rust
use bevy::prelude::*;
use bevy::log::info_span;

fn expensive_system() {
    // Create + enter a span; it ends when the guard is dropped.
    let _span = info_span!("expensive_system").entered();

    // ...work...
}
```

The `tracing` API is based on spans (duration) and events (points in time).[^bevy_tracing_docs]

### 11.3 Capture a timeline (Chrome / Perfetto)

Bevy’s profiling guide describes capturing a Chrome-tracing JSON by building with `bevy/trace_chrome` and running your app; then open the output in Perfetto.[^bevy_profiling_doc]

Example command:

```text
cargo run --release --features bevy/trace_chrome
```

### 11.4 Make logs useful while profiling

Bevy’s `LogPlugin` uses `tracing-subscriber` and supports filtering via `filter` or the `RUST_LOG` environment variable. If `RUST_LOG` is set, it overrides the plugin’s settings.[^bevy_log_plugin]

This matters because spans are emitted at levels (commonly `info`), and if you filter them out you won’t see them in traces.[^bevy_profiling_doc]

---

## 12) Message queue insight: backlog, throughput, and “dropped messages”

Your project uses message-based collision processing (great choice for bullet scaling).
To keep it healthy, track:

- **backlog per frame** (how many messages are available when the system begins)
- **processed per frame** (how many are consumed)
- **lag** (does backlog grow over time?)

### 12.1 Message basics (why this works)

Bevy `Message`s are buffered and stored in `Messages<M>`.
They are read using `MessageReader<M>`.
This style is intended for efficient batch processing of many messages at fixed points in the schedule.[^bevy_message_trait]

The `Messages<M>` collection persists across a single frame boundary; messages not handled by the end of the following frame can be dropped silently, so backlog growth is a real signal.[^bevy_messages_struct]

### 12.2 Use `MessageReader::len()` to measure backlog

`MessageReader` provides `len()` and `is_empty()` to inspect how many messages are available without consuming them.[^bevy_message_reader]

```rust
use bevy::prelude::*;
use bevy::diagnostic::{Diagnostic, DiagnosticPath, Diagnostics, RegisterDiagnostic};
use avian2d::prelude::*;

pub const COLLISION_BACKLOG: DiagnosticPath = DiagnosticPath::const_new("collision_backlog");
pub const COLLISION_PROCESSED: DiagnosticPath = DiagnosticPath::const_new("collision_processed");

pub fn message_metrics_plugin(app: &mut App) {
    app.register_diagnostic(Diagnostic::new(COLLISION_BACKLOG))
        .register_diagnostic(Diagnostic::new(COLLISION_PROCESSED))
        .add_systems(PostUpdate, measure_collision_messages);
}

fn measure_collision_messages(
    mut diagnostics: Diagnostics,
    mut collisions: MessageReader<CollisionStart>,
) {
    let backlog = collisions.len() as f64;

    // Count processed without allocating by iterating.
    let mut processed = 0u32;
    for _ in collisions.read() {
        processed += 1;
    }

    diagnostics.add_measurement(&COLLISION_BACKLOG, || backlog);
    diagnostics.add_measurement(&COLLISION_PROCESSED, || processed as f64);
}
```

This lets you detect:

- backlog spikes (e.g., dense contact scenarios)
- steady backlog growth (consumers can’t keep up)

### 12.3 “Peek only” patterns

If you just want to know whether messages exist (e.g., play one sound), use `is_empty()` and then `clear()`.
Both methods exist on `MessageReader`.[^bevy_message_reader]

---

# Perf Regression Test Harness (`#[ignore]`) — Budget Assertions over N Ticks

This chapter adds a **repeatable performance regression harness** you can run on-demand (or in a scheduled CI job) to catch accidental slowdowns.

The key idea is simple:

1. Create a **headless** Bevy `App` with your gameplay plugins.
2. Run `app.update()` for **N ticks**.
3. Collect **performance counters** (bullets, collision-start messages, message backlog, entity churn).
4. Assert they stay within your declared **PerfBudget**.

This harness is marked `#[ignore]` so it won’t run on every `cargo test`, but it’s easy to run explicitly.

---

## 1) Why `#[ignore]`?

Perf tests are inherently machine-dependent and often slower than unit tests.
Marking them `#[ignore]` lets you:

- keep normal test runs fast
- run perf checks locally when tuning
- run perf checks in CI on a controlled runner (nightly / scheduled)

---

## 2) What to assert (practical “budget” signals)

These metrics are designed to catch *structural* regressions without requiring precise timing (which is noisy):

- **Max active bullets**: verifies pooling + caps work
- **Max CollisionStart per frame**: detects collisions becoming “too broad”
- **Max collision message backlog**: detects consumers falling behind
- **Max entity count**: detects leaks or runaway spawns

Bevy messages are buffered and read with `MessageReader`; each reader tracks its own cursor, so an instrumentation reader can read the same messages as gameplay systems without interfering.
`MessageReader` provides `len()` / `is_empty()` for backlog inspection, and `read()` to consume messages for that reader.
Messages live across a limited buffer window and can be dropped if consumers fall behind, so backlog growth is a strong signal.

References:

- `MessageReader` API (`len`, `is_empty`, `read`, `clear`) [^bevy_message_reader]
- `Messages<M>` buffer semantics and dropping behavior [^bevy_messages]

---

## 3) A minimal `PerfBudget` + `PerfCounters`

```rust
// src/common/perf.rs
use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy)]
pub struct PerfBudget {
    pub max_active_bullets: usize,
    pub max_collision_starts_per_frame: usize,
    pub max_collision_backlog: usize,
    pub max_entities: usize,
}

impl Default for PerfBudget {
    fn default() -> Self {
        Self {
            max_active_bullets: 5_000,
            max_collision_starts_per_frame: 2_000,
            max_collision_backlog: 4_000,
            max_entities: 50_000,
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct PerfCounters {
    pub max_active_bullets: usize,
    pub max_collision_starts_per_frame: usize,
    pub max_collision_backlog: usize,
    pub max_entities: usize,
}
```

---

## 4) Instrumentation system (headless-safe)

This system:

- queries bullet count
- reads collision-start messages with its **own** `MessageReader`
- uses `len()` to track backlog before consuming
- updates a `PerfCounters` resource with max values

```rust
// src/plugins/perf_harness/mod.rs
use bevy::prelude::*;
use avian2d::prelude::*;

use crate::common::perf::PerfCounters;
use crate::plugins::projectiles::bullets::Bullet;

pub fn plugin(app: &mut App) {
    app.init_resource::<PerfCounters>()
        .add_systems(PostUpdate, collect_perf_counters);
}

fn collect_perf_counters(
    mut counters: ResMut<PerfCounters>,
    q_bullets: Query<(), With<Bullet>>,
    entities: Query<Entity>,
    mut starts: MessageReader<CollisionStart>,
) {
    // Entity count
    let entity_count = entities.iter().count();
    counters.max_entities = counters.max_entities.max(entity_count);

    // Bullet count
    let bullet_count = q_bullets.iter().count();
    counters.max_active_bullets = counters.max_active_bullets.max(bullet_count);

    // Message backlog (how many CollisionStart are available to this reader)
    let backlog = starts.len();
    counters.max_collision_backlog = counters.max_collision_backlog.max(backlog);

    // Consume and count starts for this reader
    let mut starts_this_frame = 0usize;
    for _ in starts.read() {
        starts_this_frame += 1;
    }
    counters.max_collision_starts_per_frame =
        counters.max_collision_starts_per_frame.max(starts_this_frame);
}
```

Notes:

- `MessageReader` consumption is per-reader; it doesn’t prevent other systems from reading.
- For messages: keeping consumers running every frame prevents backlog growth and silent drops.

References:

- `MessageReader` API and concurrency notes [^bevy_message_reader]
- `Messages<M>` behavior (double-buffering, dropping if not handled) [^bevy_messages]

---

## 5) The `#[ignore]` integration test: run N ticks and assert budgets

Create an integration test under `tests/` so it runs like a black-box scenario.

### 5.1 Example: `tests/perf_regression.rs`

```rust
// tests/perf_regression.rs
use bevy::prelude::*;

use bevy::state::app::StatesPlugin;

use bevy_game::common::perf::{PerfBudget, PerfCounters};
use bevy_game::common::state::GameState;

// Optional: your stress plugin or a deterministic spawner
use bevy_game::plugins::stress_scene::{self, StressConfig};

#[test]
#[ignore]
fn perf_regression_bullets_and_collisions() {
    const TICKS: usize = 600; // ~10 seconds at 60hz

    let mut app = App::new();

    // Headless base + state transitions (if your plugins call init_state).
    app.add_plugins((MinimalPlugins, StatesPlugin));
    app.init_state::<GameState>();

    // Install your gameplay stack (physics, bullets, enemies, world, etc.)
    // Example: bevy_game::game::configure_headless(&mut app);

    // Install perf counter instrumentation.
    bevy_game::plugins::perf_harness::plugin(&mut app);

    // Optionally enable a stress scene (recommended for repeatability)
    stress_scene::plugin(&mut app);
    app.insert_resource(StressConfig {
        enabled: true,
        bullets_per_second: 300.0,
        max_active_bullets: 5_000,
        enable_collision_events: true,
    });

    // Set a budget for this perf scenario.
    app.insert_resource(PerfBudget {
        max_active_bullets: 5_000,
        max_collision_starts_per_frame: 2_000,
        max_collision_backlog: 4_000,
        max_entities: 50_000,
    });

    // Run N ticks.
    for _ in 0..TICKS {
        app.update();
    }

    // Assert budgets.
    let budget = *app.world().resource::<PerfBudget>();
    let counters = app.world().resource::<PerfCounters>();

    assert!(counters.max_active_bullets <= budget.max_active_bullets,
        "bullets exceeded budget: {} > {}",
        counters.max_active_bullets, budget.max_active_bullets);

    assert!(counters.max_collision_starts_per_frame <= budget.max_collision_starts_per_frame,
        "collision starts exceeded budget: {} > {}",
        counters.max_collision_starts_per_frame, budget.max_collision_starts_per_frame);

    assert!(counters.max_collision_backlog <= budget.max_collision_backlog,
        "collision backlog exceeded budget: {} > {}",
        counters.max_collision_backlog, budget.max_collision_backlog);

    assert!(counters.max_entities <= budget.max_entities,
        "entity count exceeded budget: {} > {}",
        counters.max_entities, budget.max_entities);
}
```

### 5.2 Running the perf harness

```text
cargo test --test perf_regression -- --ignored --nocapture
```

---

## 6) Making the harness robust (best practices)

### 6.1 Prefer budget-based assertions over timing

Wall-clock timing fluctuates across machines, loads, and CI.
Budget assertions (bullets, events, backlog) are stable and still catch most regressions.

### 6.2 Make the scenario deterministic

- fixed seed for procedural maps
- fixed bullet pattern (spiral/ring)
- fixed spawn rate

### 6.3 Keep the harness headless

Avoid windowing/rendering plugins in perf tests.
A headless `App` is faster and eliminates GPU variance.

---

## 7) Common pitfalls

### Pitfall: instrumentation consumes messages “too early”

Fix: keep instrumentation in `PostUpdate` or ensure it has its own `MessageReader`.
`MessageReader` is designed to support multiple readers with independent cursors.

### Pitfall: backlog grows and messages drop

Fix: make sure consumers run every frame and/or reduce message production.
`Messages<M>` is double buffered and can drop messages silently if readers don’t keep up.

References:

- `Messages<M>` behavior and dropping semantics [^bevy_messages]

---

## References

[^bevy_message_reader]: Bevy `MessageReader` docs (`len`, `is_empty`, `read`, `clear`, concurrency notes): <https://docs.rs/bevy/latest/bevy/prelude/struct.MessageReader.html>
[^bevy_messages]: Bevy `Messages<M>` docs (double buffer, update cadence, dropping behavior): <https://docs.rs/bevy/latest/bevy/ecs/message/struct.Messages.html>
[^bevy_profiling_doc]: Bevy profiling guide (built-in spans, adding spans with info_span!, trace_chrome/tracy workflows): <https://github.com/bevyengine/bevy/blob/main/docs/profiling.md>
[^bevy_tracing_docs]: Bevy tracing docs (spans/events model and instrumentation concepts): <https://docs.rs/bevy/latest/bevy/log/tracing/index.html>
[^bevy_log_plugin]: Bevy `LogPlugin` docs (filtering, RUST_LOG override, tracing subscriber backend): <https://docs.rs/bevy/latest/bevy/log/struct.LogPlugin.html>
[^bevy_message_trait]: Bevy `Message` trait docs (buffered pull-based messages, stored in Messages\<M\>): <https://docs.rs/bevy/latest/bevy/prelude/trait.Message.html>
[^bevy_messages_struct]: Bevy `Messages<M>` docs (double buffer behavior, update cadence, dropping behavior): <https://docs.rs/bevy/latest/bevy/ecs/message/struct.Messages.html>
[^bevy_log_diagnostics]: Bevy `LogDiagnosticsPlugin` docs (logs diagnostics collected by other plugins): <https://docs.rs/bevy/latest/bevy/diagnostic/struct.LogDiagnosticsPlugin.html>
[^bevy_diag_toggle]: Bevy diagnostics example (enable/disable diagnostics via DiagnosticsStore): <https://github.com/bevyengine/bevy/blob/main/examples/diagnostics/enabling_disabling_diagnostic.rs>
[^bevy_custom_diagnostic]: Bevy custom diagnostic example (register and add measurements): <https://github.com/bevyengine/bevy/blob/main/examples/diagnostics/custom_diagnostic.rs>
[^bevy_commands]: Bevy `Commands` docs (deferred command queue, applied when ApplyDeferred runs): <https://docs.rs/bevy/latest/bevy/prelude/struct.Commands.html>
[^bevy_commands_deferred_ops]: DeepWiki: Commands and Deferred Operations (why commands exist, batching, exclusive world access): <https://deepwiki.com/bevyengine/bevy/2.5-commands-and-deferred-operations>
[^bevy_deferred]: Bevy `Deferred` SystemParam docs (deferring mutations has overhead; use when worth it): <https://docs.rs/bevy/latest/bevy/prelude/struct.Deferred.html>
[^avian_collision_module]: Avian2D collision module docs (collision events, messages, Collisions param, events only for CollisionEventsEnabled): <https://docs.rs/avian2d/latest/avian2d/collision/index.html>
[^avian_collision_events]: Avian2D collision_events docs (CollisionStart/End, MessageReader vs observers, CollisionEventsEnabled): <https://docs.rs/avian2d/latest/avian2d/collision/collision_events/index.html>
[^avian_collider]: Avian2D `Collider` docs (shape constructors, configuration, multiple colliders): <https://docs.rs/avian2d/latest/avian2d/collision/collider/struct.Collider.html>
[^avian_physics_layer]: Avian2D `PhysicsLayer` docs (derive for enum-based layers): <https://docs.rs/avian2d/latest/avian2d/collision/collider/trait.PhysicsLayer.html>
[^avian_collision_layers]: Avian2D `CollisionLayers` docs (memberships/filters and compatibility rules): <https://docs.rs/avian2d/latest/avian2d/collision/collider/struct.CollisionLayers.html>
[^avian_repo]: Avian repository README (features include collision events, spatial queries, debug rendering, diagnostics): <https://github.com/avianphysics/avian>
