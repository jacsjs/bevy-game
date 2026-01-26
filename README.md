# Bullet Hell Modular Starter (Bevy 0.18 + Avian 0.5 + Firefly)

This scaffold implements **folder-per-plugin modules** and uses **officially supported** integration patterns:

- Avian physics is installed via `PhysicsPlugins::default()` and configured with a length unit (pixels-per-meter).
- Firefly is installed via `FireflyPlugin`, and the camera has `FireflyConfig`.
- Bullet hits are processed using Avian **opt-in collision events** via `CollisionEventsEnabled` + `MessageReader<CollisionStart>`.

Controls:

- WASD: move
- LMB: shoot

> Note: Bullets are implemented as *dynamic* rigid bodies in this starter (physics gives free ricochets).
> For extremely large bullet counts you can switch to **kinematic bullets** using raycasts/shapecasts.
> See comments in `bullets/systems.rs`.

## Layout

- `src/plugins/<feature>/` — one folder per feature plugin
- `docs/` — architecture, ADRs, specs, runbooks
- `tests/` — integration tests (black-box)

## Running

```bash
cargo run
```

## Testing

Fast / plugin-local tests:

```bash
cargo test --lib
```

Integration tests (black-box):

```bash
cargo test --test '*'
```

See `docs/01_testing.md`.

---

## 1) Pick the *vertical slice* you want

Before adding more systems, decide what your first playable slice is:

### A good first slice for your stack (top-down arena shooter / bullet hell)

**Move → Aim → Shoot → Hit → Feedback → Progress**

That’s it. Everything else supports that loop.

**Definition of done**

- A player can move around, shoot, and kill something.
- There is feedback (sound/flash/particles/log counters).
- There’s a simple win/lose condition (survive 60s / clear wave / die resets).

If you nail this, everything else becomes a *plugin on top*.

### Code example: a minimal “vertical slice” plugin group

```rust
// src/plugins/gameplay/mod.rs
use bevy::prelude::*;

pub mod movement;
pub mod shooting;
pub mod hit_processing;

pub fn plugin(app: &mut App) {
    // Keep the vertical slice explicit: movement + shooting + hits.
    movement::plugin(app);
    shooting::plugin(app);
    hit_processing::plugin(app);
}
```

---

## 2) Next plugin to add: `combat` (health + damage + death)

Right now hits despawn things directly (fine for prototype). Next step: formalize combat.

### Add

- `Health { current, max }`
- `Damage(u32)` or `DamageOnHit(u32)`
- `OnDeath` behavior (despawn, spawn loot, spawn explosion, score update)

### Where

- New plugin: `src/plugins/combat/`
- Systems:
  - `apply_damage_from_hits` (reads hit messages/events)
  - `despawn_dead` / `death_effects`

**Why now?**
This makes your game extensible:

- enemies can take multiple hits
- player can take damage
- bosses become trivial to add later

**Definition of done**

- Bullets decrement `Health`
- Enemies die at 0 HP
- Player has HP and can die/reset

### Code example: combat components + death system

```rust
// src/plugins/combat/mod.rs
use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy)]
pub struct Health {
    pub current: i32,
    pub max: i32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct Damage(pub i32);

#[derive(Message, Debug, Clone, Copy)]
pub struct HitEvent {
    pub victim: Entity,
    pub amount: i32,
}

pub fn plugin(app: &mut App) {
    app.add_message::<HitEvent>()
        .add_systems(PostUpdate, (apply_damage_from_hits, despawn_dead).chain());
}

fn apply_damage_from_hits(mut hits: MessageReader<HitEvent>, mut q: Query<&mut Health>) {
    for hit in hits.read() {
        if let Ok(mut hp) = q.get_mut(hit.victim) {
            hp.current -= hit.amount;
        }
    }
}

fn despawn_dead(mut commands: Commands, q: Query<(Entity, &Health)>) {
    for (e, hp) in &q {
        if hp.current <= 0 {
            commands.entity(e).despawn();
        }
    }
}
```

---

## 3) Add `spawner` + “wave/encounter” progression (real gameplay)

Once combat exists, the next most valuable system is controlled content generation.

### Add

- `Wave { index, timer, config }`
- `SpawnPoint` markers or procedural spawn ring
- `EnemySpawner` system that spawns patterns over time

**Keep it data-driven even early**
Start with hardcoded config arrays; later move to RON/JSON.

**Definition of done**

- Wave 1 spawns a few enemies
- Wave 2 spawns more / different patterns
- After X waves, show “Victory” state or increase difficulty endlessly

### Code example: a wave resource + spawner system

```rust
// src/plugins/spawner/mod.rs
use bevy::prelude::*;

#[derive(Resource, Debug)]
pub struct Wave {
    pub index: u32,
    pub timer: Timer,
}

#[derive(Component)]
pub struct SpawnPoint;

pub fn plugin(app: &mut App) {
    app.insert_resource(Wave { index: 0, timer: Timer::from_seconds(5.0, TimerMode::Repeating) })
        .add_systems(Update, tick_wave);
}

fn tick_wave(
    mut commands: Commands,
    time: Res<Time>,
    mut wave: ResMut<Wave>,
    spawn_points: Query<&Transform, With<SpawnPoint>>,
) {
    if wave.timer.tick(time.delta()).just_finished() {
        wave.index += 1;
        for tf in spawn_points.iter().take(3) {
            // Spawn a basic enemy at each spawn point (or N of them)
            commands.spawn((
                Name::new(format!("Enemy_W{}", wave.index)),
                Transform::from_translation(tf.translation),
            ));
        }
    }
}
```

---

## 4) Bullet patterns: move from “single bullet” to “pattern DSL”

This is where bullet hell becomes fun.

### Add a `patterns` module under projectiles

- `Pattern::Burst { count, spread_deg, rate }`
- `Pattern::Ring { count }`
- `Pattern::Spiral { angular_vel }`
- `Pattern::AimAtPlayer { inaccuracy }`

**Implementation tip (modular + testable)**
Split into:

- pure math functions: `fn pattern_angles(...) -> Vec<f32>`
- ECS system: consumes pattern + spawns bullets

Your tests can hammer the math with edge cases (0 bullets, huge counts, negative spreads) without Bevy overhead.

**Definition of done**

- At least 3 distinct enemy firing patterns
- Patterns are parameterized and reusable

### Code example: pure pattern generator + ECS usage

```rust
// src/plugins/projectiles/patterns.rs
use std::f32::consts::PI;

pub fn burst_angles(count: usize, spread_deg: f32) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![0.0];
    }

    let spread = spread_deg.to_radians();
    let step = spread / (count as f32 - 1.0);
    let start = -spread * 0.5;

    (0..count).map(|i| start + step * i as f32).collect()
}

pub fn ring_angles(count: usize) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }
    let step = 2.0 * PI / count as f32;
    (0..count).map(|i| step * i as f32).collect()
}
```

```rust
// src/plugins/projectiles/enemy_shooting.rs
use bevy::prelude::*;

use crate::plugins::projectiles::patterns;

fn spawn_bullet(mut commands: Commands, origin: Vec2, dir: Vec2) {
    commands.spawn((
        Name::new("EnemyBullet"),
        Transform::from_translation(origin.extend(0.0)),
        // Add your Bullet components here...
    ));
}

pub fn shoot_burst(mut commands: Commands) {
    let origin = Vec2::ZERO;
    for angle in patterns::burst_angles(7, 60.0) {
        let dir = Vec2::from_angle(angle);
        spawn_bullet(commands.reborrow(), origin, dir);
    }
}
```

---

## 5) Add *presentation* plugins (don’t pollute gameplay)

You already separated “render-only” plugins, which is a huge win. Keep doing that.

### Next presentation plugins

- `vfx` (particles / hit sparks / explosions)
- `sfx` (audio events → sound playback)
- `ui_hud` (health bar, score, wave indicator)

**Architecture tip**
Gameplay emits *events/messages* like:

- `EnemyHit`
- `EnemyDied`
- `PlayerDamaged`
- `WaveStarted`

Presentation listens and reacts. That keeps logic clean and testable.

**Definition of done**

- On hit: flash/spark
- On death: explosion + sound
- UI shows HP + wave

### Code example: message-driven VFX listener (presentation-only)

```rust
// src/plugins/vfx/mod.rs
use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy)]
pub struct EnemyDied {
    pub where_: Vec2,
}

pub fn plugin(app: &mut App) {
    app.add_message::<EnemyDied>()
        .add_systems(PostUpdate, spawn_explosions);
}

fn spawn_explosions(mut commands: Commands, mut died: MessageReader<EnemyDied>) {
    for e in died.read() {
        commands.spawn((
            Name::new("Explosion"),
            Transform::from_translation(e.where_.extend(5.0)),
            // Sprite/VFX components here...
        ));
    }
}
```

---

## 6) Performance milestone: bullet pooling (do this before you “need it”)

Once patterns start spawning dozens/hundreds of bullets, pooling becomes worthwhile.

### Add

- `BulletPool { inactive: Vec<Entity> }`
- `ActiveBullet` marker
- Instead of despawn → deactivate and return to pool

**Why it’s a great modular addition**
Pooling is a self-contained optimization plugin:

- no changes to combat logic
- no changes to patterns except “spawn bullet from pool”

**Definition of done**

- Bullet entity count stays roughly constant
- No spawn/despawn spikes during heavy fire

### Code example: simple pool resource + checkout/return

```rust
// src/plugins/projectiles/pool.rs
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct BulletPool {
    pub inactive: Vec<Entity>,
}

#[derive(Component)]
pub struct PooledBullet;

pub fn checkout_bullet(commands: &mut Commands, pool: &mut BulletPool) -> Entity {
    if let Some(e) = pool.inactive.pop() {
        // Reactivate / reset components
        commands.entity(e).insert(Visibility::Visible);
        e
    } else {
        commands.spawn((PooledBullet, Visibility::Visible)).id()
    }
}

pub fn return_bullet(commands: &mut Commands, pool: &mut BulletPool, e: Entity) {
    commands.entity(e).insert(Visibility::Hidden);
    pool.inactive.push(e);
}
```

---

## 7) Tooling: debug overlays + toggles (quality-of-life multiplier)

This is the secret sauce for fast iteration.

### Add a `debug_tools` plugin

- FPS + bullet count + collision message count
- Toggle physics debug draw
- Toggle lighting/occluder debug view
- Toggle “slow motion” / step frame

**Definition of done**

- One key toggles an on-screen debug panel
- You can visually verify collision layers quickly

### Code example: simple debug counter + toggle

```rust
// src/plugins/debug_tools/mod.rs
use bevy::prelude::*;

#[derive(Resource, Default)]
struct DebugUiEnabled(pub bool);

pub fn plugin(app: &mut App) {
    app.insert_resource(DebugUiEnabled(true))
        .add_systems(Update, (toggle_debug_ui, print_debug_stats));
}

fn toggle_debug_ui(keys: Res<ButtonInput<KeyCode>>, mut enabled: ResMut<DebugUiEnabled>) {
    if keys.just_pressed(KeyCode::F3) {
        enabled.0 = !enabled.0;
    }
}

fn print_debug_stats(enabled: Res<DebugUiEnabled>, q_bullets: Query<(), With<Name>>) {
    if !enabled.0 { return; }

    // Example heuristic: count entities whose Name contains "Bullet"
    let bullet_count = q_bullets.iter().filter(|_| true).count();
    info!(bullet_count, "debug stats");
}
```

---

## 8) “Modern game” features you can bolt on later (in modular form)

Once the loop is fun, you can add modern features one by one:

### Meta progression

- upgrades, perks, skill tree (roguelite layer)

### Persistence

- save/load runs, config, keybinds

### Accessibility / UX

- remappable controls, colorblind palettes, screen shake toggles

### Content pipeline

- enemies defined in data files
- wave configs defined in data files

### Code example: save/load run state (minimal JSON)

```rust
// src/plugins/persistence/mod.rs
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Resource, Default, Serialize, Deserialize)]
pub struct RunState {
    pub score: u32,
    pub wave: u32,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<RunState>();
}

pub fn save_to_string(state: &RunState) -> String {
    serde_json::to_string_pretty(state).expect("serialize")
}

pub fn load_from_str(s: &str) -> RunState {
    serde_json::from_str(s).expect("deserialize")
}
```

> Tip: keep persistence logic pure (functions returning String/struct) so it’s easy to unit test.

---

## Bonus: Basic UI / Debug Screen with Relevant Metrics

A small UI overlay is a *high-leverage* addition: it turns invisible runtime state into actionable feedback.

### What to show (useful early metrics)

- FPS / frame time (performance baseline)
- Entity count (sanity + leak detection)
- Bullet count (scaling / pooling validation)
- Collision-start messages per frame (hit-load indicator)

### Code example: `debug_hud` plugin (UI + diagnostics)

This uses Bevy UI (`Node` + `Text`) as described in Bevy’s UI docs.[^bevy_ui][^bevy_ui_crate] citeturn22search220turn22search215
It also uses built-in diagnostics plugins like `FrameTimeDiagnosticsPlugin` and `EntityCountDiagnosticsPlugin`.[^bevy_diagnostics_frame][^bevy_log_diag_example] citeturn22search206turn22search203

```rust
// src/plugins/debug_hud/mod.rs
use bevy::prelude::*;
use bevy::diagnostic::{Diagnostics, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin};

use crate::plugins::projectiles::bullets::Bullet;

#[derive(Resource)]
struct DebugHudRoot(Entity);

pub fn plugin(app: &mut App) {
    // Diagnostics providers (you can omit these if you install them elsewhere).
    app.add_plugins((
        FrameTimeDiagnosticsPlugin::default(),
        EntityCountDiagnosticsPlugin::default(),
    ));

    // Spawn UI once and update every frame.
    app.add_systems(Startup, spawn_debug_hud)
        .add_systems(Update, update_debug_hud);
}

fn spawn_debug_hud(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Root UI node pinned to top-left.
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(12.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Debug HUD"),
                TextFont {
                    font: asset_server.load("fonts/FiraSans-Bold.ttf"),
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));

            p.spawn((
                // This is the line we will update every frame.
                Text::new("..."),
                TextFont {
                    font: asset_server.load("fonts/FiraMono-Medium.ttf"),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.9, 1.0)),
            ));
        })
        .id();

    commands.insert_resource(DebugHudRoot(root));
}

fn update_debug_hud(
    hud: Res<DebugHudRoot>,
    diagnostics: Diagnostics,
    q_bullets: Query<(), With<Bullet>>,
    children: Query<&Children>,
    mut texts: Query<&mut Text>,
) {
    // Root has children [title_text, metrics_text]
    let Ok(kids) = children.get(hud.0) else { return; };
    if kids.len() < 2 { return; }

    let Ok(mut metrics_text) = texts.get_mut(kids[1]) else { return; };

    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(f64::NAN);

    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(f64::NAN);

    let entities = diagnostics
        .get(&EntityCountDiagnosticsPlugin::ENTITY_COUNT)
        .and_then(|d| d.value())
        .unwrap_or(f64::NAN);

    let bullets = q_bullets.iter().count();

    **metrics_text = format!(
        "FPS: {fps:6.1}  Frame: {frame_ms:6.2} ms
Entities: {entities:8.0}
Bullets: {bullets:6}",
    );
}
```

### Code example: enable/disable debug HUD at runtime

You can gate the entire overlay behind a toggle resource and key input.
Bevy’s `ButtonInput<KeyCode>` resource makes this simple.[^bevy_buttoninput] citeturn16search181turn16search182

```rust
// src/plugins/debug_hud/toggle.rs
use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct DebugHudEnabled(pub bool);

pub fn toggle_system(keys: Res<ButtonInput<KeyCode>>, mut enabled: ResMut<DebugHudEnabled>) {
    if keys.just_pressed(KeyCode::F3) {
        enabled.0 = !enabled.0;
    }
}
```

### Engineering tip: keep UI out of headless tests

- UI often requires assets/fonts and sometimes windowing.
- Keep the debug HUD in a **render-only** plugin group (like your Firefly/camera plugins).
- Alternatively: add `.run_if(resource_exists::<AssetServer>)` or `.run_if(resource_exists::<DebugHudEnabled>)` to skip the UI systems in headless contexts.[^bevy_resource_exists][^bevy_run_conditions_example]

---

## Bonus: Input Layer — “Intent” Resources + Optional Plugin Dependencies

A dedicated **input layer** keeps gameplay systems clean, testable, and headless-friendly.
Instead of having many gameplay systems depend directly on device state (`ButtonInput<KeyCode>`, `ButtonInput<MouseButton>`, cursor, windows, etc.), you translate raw input into a small set of **intent resources**.

### What to model as intent (typical early set)

- **Movement**: `move_axis: Vec2` (normalized)
- **Aiming**: `aim_dir: Vec2` (normalized), or `aim_world: Vec2`
- **Actions**: `fire: bool`, `dash: bool`, `pause: bool`

### Why this helps (engineering benefits)

- **Testability**: unit/integration tests can insert `PlayerIntent` directly without needing input plugins or OS events.[^bevy_buttoninput][^minimal_plugins]
- **Headless safety**: you can choose to make raw input dependencies optional (`Option<Res<_>>`) so the system no-ops when input resources are absent (common in `MinimalPlugins`).[^minimal_plugins][^bevy_buttoninput]
- **Separation of concerns**: gameplay reads “what the player wants to do”, not “which device produced it”.[^bevy_buttoninput]

### Code example: Intent resource definition

```rust
// src/plugins/input_intent/mod.rs
use bevy::prelude::*;

/// The *only* input surface gameplay systems should depend on.
///
/// Populate this in an input layer (Update), then consume it in gameplay (FixedUpdate / Update).
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct PlayerIntent {
    /// Normalized movement direction, or Vec2::ZERO.
    pub move_axis: Vec2,

    /// Normalized aiming direction, or Vec2::ZERO.
    pub aim_dir: Vec2,

    /// Fire action (e.g. left mouse / gamepad trigger).
    pub fire: bool,
}
```

### Code example: Gather intent with **optional** input dependencies

This pattern keeps the system valid in headless test apps where input resources may not exist.

```rust
// src/plugins/input_intent/systems.rs
use bevy::prelude::*;

use super::PlayerIntent;

pub fn gather_intent(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    mut intent: ResMut<PlayerIntent>,
) {
    // If we run headless and didn't add input plugins/resources, just no-op.
    let Some(keys) = keys else {
        intent.move_axis = Vec2::ZERO;
        intent.aim_dir = Vec2::ZERO;
        intent.fire = false;
        return;
    };

    // Movement intent
    let mut axis = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) { axis.y += 1.0; }
    if keys.pressed(KeyCode::KeyS) { axis.y -= 1.0; }
    if keys.pressed(KeyCode::KeyA) { axis.x -= 1.0; }
    if keys.pressed(KeyCode::KeyD) { axis.x += 1.0; }
    intent.move_axis = axis.normalize_or_zero();

    // Fire intent (optional)
    intent.fire = mouse
        .map(|m| m.just_pressed(MouseButton::Left))
        .unwrap_or(false);

    // Aim intent (placeholder): aim where you move.
    // Replace with cursor/world aim when you have camera+window available.
    intent.aim_dir = if intent.move_axis.length_squared() > 0.0 {
        intent.move_axis
    } else {
        Vec2::ZERO
    };
}
```

> `ButtonInput<T>` is designed to store button state as a Resource, including `pressed()` and `just_pressed()`.[^bevy_buttoninput]

### Code example: Plugin wiring (input layer in Update; gameplay consumes intent)

```rust
// src/plugins/input_intent/mod.rs
use bevy::prelude::*;

mod systems;

pub use systems::gather_intent;

pub fn plugin(app: &mut App) {
    app.init_resource::<PlayerIntent>()
        .add_systems(Update, gather_intent);
}

// Example consumption in your player movement system:
fn apply_movement(
    intent: Res<PlayerIntent>,
    mut vel: Option<Single<&mut avian2d::prelude::LinearVelocity, With<crate::plugins::player::Player>>>,
) {
    let Some(mut vel) = vel else { return; };
    vel.0 = intent.move_axis * 420.0;
}
```

### Code example: Headless/integration test pattern

Because intent is a Resource you control, tests can bypass platform input entirely:

```rust
use bevy::prelude::*;
use crate::plugins::input_intent::PlayerIntent;

#[test]
fn movement_consumes_intent_without_input_plugins() {
    let mut world = World::new();

    // Insert intent directly — no ButtonInput resources needed.
    world.insert_resource(PlayerIntent {
        move_axis: Vec2::new(1.0, 0.0),
        aim_dir: Vec2::new(1.0, 0.0),
        fire: false,
    });

    // Run your movement system here and assert on velocity, position, etc.
}
```

### Optional: Gate raw-input systems with `resource_exists`

If you prefer to keep `Res<ButtonInput<_>>` (required) instead of `Option<Res<_>>`, gate the system so it only runs when the resource exists:

```rust
use bevy::prelude::*;

app.add_systems(Update, gather_intent.run_if(resource_exists::<ButtonInput<KeyCode>>));
```

`resource_exists::<T>` is a built-in run condition for guarding systems on resource availability.[^bevy_resource_exists][^bevy_run_conditions]

---

## Bonus: Input Mapping + Intent Testing (Headless-Friendly)

This section extends the **Input Intent** approach with an **input mapping layer** (bindings) and shows how to test intent-based gameplay systems without requiring OS/window input.

---

### Why input mapping?

Hardcoding `KeyCode::W` / `MouseButton::Left` inside gameplay systems makes it harder to:

- support remapping
- support gamepads
- support accessibility options
- keep headless tests simple

Bevy represents button-like device state via the `ButtonInput<T>` resource, which provides `pressed`, `just_pressed`, `release`, etc.[^bevy_buttoninput]
However, `MinimalPlugins` is intentionally minimal and does not include input by default, so tests often won’t have these resources unless you add `InputPlugin` or insert them manually.[^minimal_plugins][^bevy_input_plugin]

---

## Part A — Input Mapping (Bindings)

### What to build

1) An **Action** enum describing *what the player wants to do*.
2) An **InputMap** resource mapping actions → one or more bindings.
3) A **resolver system** that reads raw `ButtonInput` resources and produces a compact `ActionState`.

This keeps your gameplay systems dependent only on **actions/intents**, not devices.[^bevy_buttoninput]

---

### Code example: Action + bindings + map

```rust
// src/plugins/input_mapping/mod.rs
use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Fire,
    Pause,
}

/// A single binding. Keep it simple early; extend later (gamepad, chorded keys, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Binding {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// Maps each action to 1..N bindings.
#[derive(Resource, Debug, Default)]
pub struct InputMap {
    pub bindings: HashMap<Action, Vec<Binding>>,
}

impl InputMap {
    pub fn default_keyboard_mouse() -> Self {
        use Action::*;
        use Binding::*;

        let mut map = InputMap::default();
        map.bindings.insert(MoveUp, vec![Key(KeyCode::KeyW)]);
        map.bindings.insert(MoveDown, vec![Key(KeyCode::KeyS)]);
        map.bindings.insert(MoveLeft, vec![Key(KeyCode::KeyA)]);
        map.bindings.insert(MoveRight, vec![Key(KeyCode::KeyD)]);
        map.bindings.insert(Fire, vec![Mouse(MouseButton::Left)]);
        map.bindings.insert(Pause, vec![Key(KeyCode::Escape)]);
        map
    }
}
```

---

### Code example: ActionState produced from raw input

Bevy’s `ButtonInput<T>` is the canonical resource for button-like input state, with `pressed()` and `just_pressed()` semantics.[^bevy_buttoninput]

```rust
// src/plugins/input_mapping/state.rs
use bevy::prelude::*;
use std::collections::HashSet;

use super::{Action, Binding, InputMap};

#[derive(Resource, Debug, Default)]
pub struct ActionState {
    pressed: HashSet<Action>,
    just_pressed: HashSet<Action>,
}

impl ActionState {
    pub fn pressed(&self, a: Action) -> bool { self.pressed.contains(&a) }
    pub fn just_pressed(&self, a: Action) -> bool { self.just_pressed.contains(&a) }

    fn clear_frame(&mut self) {
        self.just_pressed.clear();
    }
}

pub fn resolve_actions(
    map: Res<InputMap>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    mut state: ResMut<ActionState>,
) {
    // Clear one-frame state each tick.
    state.clear_frame();

    // If no raw input resources exist (common in headless tests), keep everything false.
    let Some(keys) = keys else { return; };

    for (action, binds) in map.bindings.iter() {
        let mut is_pressed = false;
        let mut is_just_pressed = false;

        for b in binds {
            match *b {
                Binding::Key(k) => {
                    is_pressed |= keys.pressed(k);
                    is_just_pressed |= keys.just_pressed(k);
                }
                Binding::Mouse(mb) => {
                    if let Some(mouse) = mouse.as_deref() {
                        is_pressed |= mouse.pressed(mb);
                        is_just_pressed |= mouse.just_pressed(mb);
                    }
                }
            }
        }

        if is_pressed { state.pressed.insert(*action); } else { state.pressed.remove(action); }
        if is_just_pressed { state.just_pressed.insert(*action); }
    }
}
```

> Using `Option<Res<...>>` makes this system valid even when input resources are absent (useful for `MinimalPlugins` headless tests).[^minimal_plugins]

---

### Code example: Wiring input mapping into your intent layer

Now your intent system depends on `ActionState`, not raw inputs.

```rust
// src/plugins/input_intent/from_actions.rs
use bevy::prelude::*;

use crate::plugins::input_mapping::{Action, ActionState};
use crate::plugins::input_intent::PlayerIntent;

pub fn intent_from_actions(actions: Res<ActionState>, mut intent: ResMut<PlayerIntent>) {
    let x = (actions.pressed(Action::MoveRight) as i32 - actions.pressed(Action::MoveLeft) as i32) as f32;
    let y = (actions.pressed(Action::MoveUp) as i32 - actions.pressed(Action::MoveDown) as i32) as f32;

    intent.move_axis = Vec2::new(x, y).normalize_or_zero();
    intent.fire = actions.just_pressed(Action::Fire);
}
```

---

## Part B — Testing Input Intent (and Input Mapping)

### Goal

Test gameplay behavior by setting **intent/actions directly**, without needing windows, cursor positions, or OS input events.

This aligns with Bevy’s testing ergonomics: you can run systems on a `World` directly using `World::run_system_once` for tests/diagnostics.[^bevy_run_system_once]

---

### Strategy 1: Test intent consumers (recommended)

Most gameplay systems should only depend on `PlayerIntent`.
In tests, insert the intent resource and run the system.

```rust
use bevy::prelude::*;
use crate::plugins::input_intent::PlayerIntent;
use crate::common::test_utils::run_system_once;

#[test]
fn movement_system_uses_intent_only() {
    let mut world = World::new();

    // Pretend input layer produced this.
    world.insert_resource(PlayerIntent {
        move_axis: Vec2::new(1.0, 0.0),
        aim_dir: Vec2::new(1.0, 0.0),
        fire: false,
    });

    // Spawn the entity your movement system expects.
    // world.spawn((Player, LinearVelocity::ZERO, ...));

    // Run the movement system once.
    // run_system_once(&mut world, player::apply_movement);

    // Assert on velocity/transform.
}
```

**Why this is ideal:** it avoids input plugin dependencies entirely and isolates gameplay logic.

---

### Strategy 2: Test mapping resolver with manual `ButtonInput` resources

If you want to test that bindings resolve correctly, insert `ButtonInput` resources and manipulate them.
`ButtonInput<T>` exposes `press()` / `release()` and the `just_pressed()` semantics you need for one-frame triggers.[^bevy_buttoninput]

```rust
use bevy::prelude::*;
use bevy::input::ButtonInput;

use crate::plugins::input_mapping::{Action, InputMap};
use crate::plugins::input_mapping::state::{ActionState, resolve_actions};
use crate::common::test_utils::run_system_once;

#[test]
fn pressing_w_sets_moveup_action() {
    let mut world = World::new();

    world.insert_resource(InputMap::default_keyboard_mouse());
    world.init_resource::<ActionState>();

    // Insert raw input resources (headless safe).
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::KeyW);
    world.insert_resource(keys);

    // Mouse is optional for this test.
    world.insert_resource(ButtonInput::<MouseButton>::default());

    // Run resolver.
    run_system_once(&mut world, resolve_actions);

    let state = world.resource::<ActionState>();
    assert!(state.pressed(Action::MoveUp));
}
```

---

### Optional: Ensure the mapping system only runs when inputs exist

If you choose `Res<ButtonInput<...>>` (required) instead of `Option<Res<...>>`, gate it with `resource_exists::<T>`.
This is a standard Bevy run-condition pattern.[^bevy_resource_exists][^bevy_run_conditions]

```rust
use bevy::prelude::*;

app.add_systems(Update, resolve_actions.run_if(resource_exists::<ButtonInput<KeyCode>>));
```

---

## Bonus: Expanding & Managing `GameState` (Menus, Loading, Pause, Game Over)

This section shows a practical way to grow your `GameState` beyond `InGame` while keeping the project modular, testable, and predictable.

Bevy states are app-wide finite state machines used to structure high-level flow (menus, loading, gameplay, pause, etc.) and provide transition schedules such as `OnEnter`, `OnExit`, and `OnTransition`.[^bevy_state]

---

### Why use a state machine?

Common benefits in games:

- **Clear lifecycle boundaries**: spawn menu UI on enter, clean it up on exit.
- **Selective system execution**: run gameplay only in `InGame`, run UI logic only in `MainMenu`, etc.
- **Testability**: you can assert what gets spawned/removed when entering or leaving a state.

Bevy provides transition schedules (`OnEnter`, `OnExit`, `OnTransition`) and run conditions like `in_state` for state-driven control flow.[^bevy_state][^bevy_quickstart_states]

---

## Part A — A common `GameState` layout for modern games

### Suggested enum (common in many games)

```rust
// src/common/state.rs
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, States, Default)]
pub enum GameState {
    /// Bootstrapping: create resources that must always exist.
    #[default]
    Boot,

    /// Load assets, build levels, initialize save/profile.
    Loading,

    /// Main menu UI.
    MainMenu,

    /// Core gameplay.
    InGame,

    /// Pause overlay while the game is suspended.
    Paused,

    /// Death screen, results screen, or run summary.
    GameOver,
}
```

> Tip: Some teams also add `Settings`, `Credits`, or `LevelSelect` states as separate screens.

---

## Part B — Installing states correctly (and why `StatesPlugin` matters)

To use `init_state::<GameState>()`, the `StateTransition` schedule must exist.
Bevy provides `StatesPlugin` specifically to register the `StateTransition` schedule.[^states_plugin]

**Typical patterns:**

- Full app: `DefaultPlugins` includes state support.
- Headless tests: add `StatesPlugin` (and `MinimalPlugins`) manually.

```rust
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use crate::common::state::GameState;

pub fn configure_headless(app: &mut App) {
    // In a headless app, add minimal runtime + states.
    app.add_plugins((MinimalPlugins, StatesPlugin));

    // Now this is safe.
    app.init_state::<GameState>();
}
```

`StatesPlugin` registers the `StateTransition` schedule required by `init_state`.[^states_plugin]

---

## Part C — State-scoped entities (clean spawn/despawn contracts)

Bevy’s `state_scoped` utilities provide marker components like `DespawnOnExit<S>` and `DespawnOnEnter<S>`.
These are designed to make entity lifetimes match state boundaries (e.g., menu UI exists only during `MainMenu`).[^state_scoped]

```rust
use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;

use crate::common::state::GameState;

#[derive(Component)]
struct MainMenuUi;

fn spawn_main_menu(mut commands: Commands) {
    commands.spawn((
        MainMenuUi,
        Node::default(),
        // This menu root and its hierarchy will be despawned when we leave MainMenu.
        DespawnOnExit(GameState::MainMenu),
    ));
}
```

This is especially powerful when combined with `OnEnter` / `OnExit` schedules.[^bevy_state][^state_scoped]

---

## Part D — Where state-specific logic should live (plugin structure)

A clean modular approach:

- `plugins/menu/` owns menu UI + menu interactions
- `plugins/loading/` owns asset loading logic
- `plugins/gameplay/` owns gameplay systems
- `plugins/pause/` owns pause overlay + pause toggling

Each plugin registers systems in the schedules that match its state:

```rust
// src/plugins/menu/mod.rs
use bevy::prelude::*;
use crate::common::state::GameState;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
        .add_systems(Update, menu_input.run_if(in_state(GameState::MainMenu)))
        .add_systems(OnExit(GameState::MainMenu), cleanup_menu_optional);
}

fn spawn_main_menu(mut commands: Commands) {
    // spawn UI root with DespawnOnExit(MainMenu) etc.
}

fn menu_input(mut next: ResMut<NextState<GameState>>, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::Enter) {
        next.set(GameState::Loading);
    }
}

fn cleanup_menu_optional() {
    // Often unnecessary if you use DespawnOnExit.
}
```

Bevy’s state module documents transition schedules and the `NextState` mechanism for changing states.[^bevy_state][^bevy_quickstart_states]

---

## Part E — Common transitions in menu-driven games

A typical modern flow:

1) `Boot` → `Loading` (initialize resources)
2) `Loading` → `MainMenu` (assets ready)
3) `MainMenu` → `InGame` (start run)
4) `InGame` ↔ `Paused` (toggle pause)
5) `InGame` → `GameOver` (player dead)
6) `GameOver` → `MainMenu` (return)

### Code example: pause toggle (InGame ↔ Paused)

```rust
use bevy::prelude::*;
use crate::common::state::GameState;

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    current: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        match current.get() {
            GameState::InGame => next.set(GameState::Paused),
            GameState::Paused => next.set(GameState::InGame),
            _ => {}
        }
    }
}
```

### Code example: freezing gameplay while paused

```rust
use bevy::prelude::*;
use crate::common::state::GameState;

pub fn plugin(app: &mut App) {
    // Only run gameplay systems while InGame.
    app.add_systems(Update, gameplay_system.run_if(in_state(GameState::InGame)));

    // Pause menu systems only while Paused.
    app.add_systems(Update, pause_menu_system.run_if(in_state(GameState::Paused)));
}

fn gameplay_system() {}
fn pause_menu_system() {}
```

The `in_state` run condition is a standard way to restrict systems to specific states.[^bevy_state][^bevy_quickstart_states]

---

## Part F — Testing state transitions (unit + integration patterns)

### Strategy 1: Plugin-local “state boundary” unit tests

Use a minimal `App`, install `StatesPlugin`, initialize state, then call `app.update()` to trigger `OnEnter`.

```rust
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use crate::common::state::GameState;

#[test]
fn entering_main_menu_spawns_ui() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin));

    app.init_state::<GameState>();

    // Register your menu plugin systems.
    crate::plugins::menu::plugin(&mut app);

    // Force transition to MainMenu.
    app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::MainMenu);

    // Tick once: processes StateTransition and runs OnEnter(MainMenu).
    app.update();

    // Assert UI exists (query for your marker component).
    // assert!(app.world().query::<&MainMenuUi>().iter(app.world()).next().is_some());
}
```

`StatesPlugin` is required for the `StateTransition` schedule that powers transitions.[^states_plugin]

### Strategy 2: Integration tests via shared harness

Your integration harness can provide:

- a headless app with `(MinimalPlugins, StatesPlugin)`
- your state + plugin wiring

Then integration tests can validate multi-state flows (Boot → Loading → InGame, etc.).

---

## Bonus: GameState Transitions + UI Lifecycle + Testing

This document provides:

- **Concrete transition code** (Boot → Loading → MainMenu → InGame, plus pause and game over)
- A **state synchronization timeline** (when transitions *actually* apply)
- A **UI lifecycle per state** pattern using `DespawnOnExit`
- Explicit **state test patterns**, including the common “two-tick transition” case

Bevy states are app-wide finite state machines. They provide transition schedules (`OnEnter`, `OnExit`, `OnTransition`) and run conditions like `in_state` for controlling system execution.[^bevy_state]

---

## Part A — A common `GameState` for modern games

A typical state machine for menu-driven games includes boot/loading/menu/gameplay/pause/gameover phases.[^bevy_state]

```rust
// src/common/state.rs
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, States, Default)]
pub enum GameState {
    /// Create always-on resources, register gameplay subsystems.
    #[default]
    Boot,

    /// Load assets / build level / warm up.
    Loading,

    /// Main menu UI.
    MainMenu,

    /// Active gameplay.
    InGame,

    /// Gameplay suspended; pause overlay/UI.
    Paused,

    /// Results screen / death screen.
    GameOver,
}
```

> Tip: It’s common to add `Settings`, `Credits`, `LevelSelect`, or `RunSummary` states later.

---

## Part B — Installing state support correctly

### Why `StatesPlugin` matters

`init_state::<GameState>()` requires the `StateTransition` schedule to exist.
Bevy’s `StatesPlugin` exists specifically to register that schedule.[^states_plugin]

- In the **full app**, `DefaultPlugins` typically provides state support indirectly.
- In **headless tests** or trimmed app configs, add `StatesPlugin` explicitly before calling `init_state`.[^states_plugin]

```rust
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use crate::common::state::GameState;

pub fn configure_headless(app: &mut App) {
    // Minimal runtime + state transitions.
    app.add_plugins((MinimalPlugins, StatesPlugin));

    // Safe now.
    app.init_state::<GameState>();
}
```

---

## Part C — Writing transitions (Boot → Loading → MainMenu → InGame)

Bevy transitions are driven with `NextState<GameState>`.
You set the next state and Bevy applies it during the `StateTransition` schedule.[^bevy_state]

### 1) Automatic boot flow

```rust
// src/plugins/flow/mod.rs
use bevy::prelude::*;
use crate::common::state::GameState;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::Boot), boot_to_loading)
        .add_systems(OnEnter(GameState::Loading), loading_to_menu);
}

fn boot_to_loading(mut next: ResMut<NextState<GameState>>) {
    next.set(GameState::Loading);
}

fn loading_to_menu(mut next: ResMut<NextState<GameState>>) {
    // In a real game, gate this on asset loading completion.
    next.set(GameState::MainMenu);
}
```

### 2) Menu action (MainMenu → InGame)

A common menu pattern is: a menu-specific system runs only while `MainMenu` and sets `NextState`.
Restricting systems with `in_state` keeps scheduling clear and avoids accidental cross-state logic.[^bevy_state]

```rust
// src/plugins/menu/mod.rs
use bevy::prelude::*;
use crate::common::state::GameState;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::MainMenu), spawn_menu)
        .add_systems(Update, menu_input.run_if(in_state(GameState::MainMenu)));
}

#[derive(Component)]
pub struct MainMenuUi;

fn spawn_menu(mut commands: Commands) {
    commands.spawn((MainMenuUi, Name::new("MainMenuRoot")));
}

fn menu_input(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<GameState>>) {
    if keys.just_pressed(KeyCode::Enter) {
        next.set(GameState::InGame);
    }
}
```

`ButtonInput<T>` is Bevy’s resource for button-like inputs and provides `just_pressed()` semantics.[^bevy_buttoninput]

### 3) Pause toggle (InGame ↔ Paused)

A common pattern is toggling pause with Esc.
This reads the current state and sets the next state accordingly.[^bevy_state]

```rust
use bevy::prelude::*;
use crate::common::state::GameState;

pub fn pause_plugin(app: &mut App) {
    app.add_systems(Update, toggle_pause);
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    current: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        match current.get() {
            GameState::InGame => next.set(GameState::Paused),
            GameState::Paused => next.set(GameState::InGame),
            _ => {}
        }
    }
}
```

---

## Part D — State synchronization timeline (when transitions *actually* apply)

Bevy state changes are **staged**:

1. A system (or test) writes the desired next value into `NextState<GameState>`.
2. Bevy applies that value during the **`StateTransition` schedule**.
3. During that transition pass, Bevy runs transition schedules in a predictable order:
   - `OnExit(old_state)`
   - `OnTransition(old_state → new_state)`
   - `OnEnter(new_state)`

This is why it can look like “state changes are delayed”: they are synchronized at the `StateTransition` step for determinism and ordering.[^bevy_state][^states_plugin]

### Quick mental model

- If you set `NextState` **before** calling `app.update()`, the transition is typically applied on that tick.
- If you set `NextState` **inside** an `Update` system, the transition may be applied on the **next** tick because the `StateTransition` schedule for the current tick already ran.[^bevy_state]

---

## Part E — Managing state-scoped entities (automatic cleanup)

For menus and overlays, you typically want UI entities to exist **only** during a specific state.
Bevy’s `state_scoped` module provides `DespawnOnExit<S>` / `DespawnOnEnter<S>` to tie entity lifetime to transitions.[^state_scoped]

```rust
use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;
use crate::common::state::GameState;

#[derive(Component)]
struct PauseUi;

fn spawn_pause_ui(mut commands: Commands) {
    commands.spawn((
        PauseUi,
        Name::new("PauseOverlay"),
        DespawnOnExit(GameState::Paused),
    ));
}
```

This pairs well with `OnEnter(GameState::Paused)` to spawn the overlay and let `DespawnOnExit` handle cleanup.[^bevy_state][^state_scoped]

---

## Part F — UI lifecycle per state (spawn, update, teardown)

Treat each screen/overlay as a **state-scoped UI tree**.

### UI lifecycle checklist (recommended)

- **Spawn:** `OnEnter(GameState::X)` spawns a UI root entity.
- **Scope lifetime:** attach `DespawnOnExit(GameState::X)` to the root.
- **Update:** run UI update systems only while in that state via `run_if(in_state(GameState::X))`.
- **Teardown:** rely on `DespawnOnExit` for cleanup instead of manual “delete UI” logic.

Bevy provides `DespawnOnExit` / `DespawnOnEnter` in `state_scoped` specifically for state-bound lifetime management.[^state_scoped][^bevy_state]

### Code example: Main menu UI (spawn + update + automatic teardown)

```rust
use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;

use crate::common::state::GameState;

#[derive(Component)]
struct MainMenuUi;

#[derive(Component)]
struct StartHintText;

pub fn menu_ui_plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
        .add_systems(Update, update_main_menu.run_if(in_state(GameState::MainMenu)));
}

fn spawn_main_menu(mut commands: Commands) {
    commands
        .spawn((
            MainMenuUi,
            Name::new("MainMenuRoot"),
            Node::default(),
            DespawnOnExit(GameState::MainMenu),
        ))
        .with_children(|p| {
            p.spawn((
                StartHintText,
                Text::new("Press Enter to Start"),
            ));
        });
}

fn update_main_menu(time: Res<Time>, mut q: Query<&mut Text, With<StartHintText>>) {
    // Example: per-state UI update (blink / animate)
    let t = time.elapsed_seconds();
    let _alpha = (t * 3.0).sin().abs().clamp(0.2, 1.0);

    // Placeholder: in a real HUD you would update TextColor or style components.
    for mut text in &mut q {
        let _ = &mut *text;
    }
}
```

---

## Part G — How to test state logic (unit + integration patterns)

### Key idea

State-driven logic is best tested with a minimal `App` that includes:

- `MinimalPlugins` (headless core runtime)
- `StatesPlugin` (registers `StateTransition` schedule)

Then:

1. `init_state::<GameState>()`
2. register the plugin under test
3. set `NextState<GameState>`
4. call `app.update()` and assert on the world

`StatesPlugin` is the prerequisite for transitions because it registers the `StateTransition` schedule.[^states_plugin]

---

### 1) Test `OnEnter` spawn (MainMenu spawns UI)

```rust
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use crate::common::state::GameState;

#[test]
fn entering_main_menu_spawns_ui() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin));

    app.init_state::<GameState>();

    // Register menu systems.
    crate::plugins::menu::plugin(&mut app);

    // Request transition.
    app.world_mut()
        .resource_mut::<NextState<GameState>>()
        .set(GameState::MainMenu);

    // Tick: StateTransition runs and then OnEnter(MainMenu) runs.
    app.update();

    // Assert: UI exists.
    let count = app.world().query::<&crate::plugins::menu::MainMenuUi>().iter(app.world()).count();
    assert_eq!(count, 1);
}
```

**Timing note:** Setting `NextState` before the tick makes it apply on that tick; setting it during `Update` may require another tick.[^bevy_state]

---

### 2) Test `OnExit` cleanup with `DespawnOnExit`

```rust
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::state::state_scoped::DespawnOnExit;
use crate::common::state::GameState;

#[derive(Component)]
struct MenuRoot;

fn spawn_scoped_menu(mut commands: Commands) {
    commands.spawn((MenuRoot, DespawnOnExit(GameState::MainMenu)));
}

#[test]
fn leaving_main_menu_despawns_scoped_entities() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin));
    app.init_state::<GameState>();

    app.add_systems(OnEnter(GameState::MainMenu), spawn_scoped_menu);

    // Enter MainMenu.
    app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::MainMenu);
    app.update();
    assert_eq!(app.world().query::<&MenuRoot>().iter(app.world()).count(), 1);

    // Leave MainMenu.
    app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::InGame);
    app.update();

    assert_eq!(app.world().query::<&MenuRoot>().iter(app.world()).count(), 0);
}
```

This test relies on `state_scoped` cleanup behavior defined by `DespawnOnExit`.[^state_scoped]

---

## Part H — The explicit “two-tick transition” test pattern

Because `NextState` is applied during `StateTransition`, the number of `app.update()` calls you need depends on *when* `NextState` is set.

### 1) Set `NextState` *before* ticking (1 tick)

```rust
app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::MainMenu);
app.update();
// Now the transition has been processed; OnEnter(MainMenu) has run.
```

### 2) Set `NextState` *during* Update (often 2 ticks)

If an `Update` system sets `NextState`, you may need an additional tick to observe the new state:

```rust
app.update(); // Update systems run, and may set NextState
app.update(); // StateTransition processes NextState and runs OnExit/OnEnter
```

This is common when validating input-driven transitions (e.g., “Press Enter → InGame”).[^bevy_state][^bevy_buttoninput]

---

### 3) Test state transitions driven by input (simulate keypress)

You can test transitions without adding full input plugins by inserting `ButtonInput<KeyCode>` manually and calling `press()`.
`ButtonInput<T>` provides `press()` and `just_pressed()` semantics.[^bevy_buttoninput]

```rust
use bevy::prelude::*;
use bevy::input::ButtonInput;
use bevy::state::app::StatesPlugin;

use crate::common::state::GameState;

#[test]
fn pressing_enter_in_menu_transitions_to_ingame() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin));
    app.init_state::<GameState>();

    crate::plugins::menu::plugin(&mut app);

    // Enter MainMenu.
    app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::MainMenu);
    app.update();

    // Insert + press Enter.
    let mut keys = ButtonInput::<KeyCode>::default();
    keys.press(KeyCode::Enter);
    app.world_mut().insert_resource(keys);

    // Update tick: menu_input runs and sets NextState.
    app.update();

    // Next tick: StateTransition applies NextState.
    app.update();

    assert_eq!(*app.world().resource::<State<GameState>>().get(), GameState::InGame);
}
```

---

## Part I — Common pitfalls (and fixes)

### Pitfall: Missing `StateTransition` schedule

If you see:
> “The `StateTransition` schedule is missing. Did you forget to add StatesPlugin …?”

Then add `StatesPlugin` before calling `init_state`.[^states_plugin]

### Pitfall: Headless tests accidentally run UI / input systems

In headless tests, avoid adding render/UI plugins, or gate systems with run conditions like `in_state(...)` or `resource_exists::<T>`.
`resource_exists::<T>` is a built-in run condition for guarding on resource availability.[^bevy_resource_exists][^bevy_run_conditions]

---

## References

[^bevy_state]: Bevy state module docs (transition schedules, `NextState`, `in_state`): <https://docs.rs/bevy/latest/bevy/state/index.html>
[^states_plugin]: Bevy `StatesPlugin` docs (registers `StateTransition` schedule): <https://docs.rs/bevy/latest/bevy/state/app/struct.StatesPlugin.html>
[^state_scoped]: Bevy `state_scoped` docs (`DespawnOnEnter` / `DespawnOnExit`): <https://docs.rs/bevy/latest/bevy/state/state_scoped/index.html>
[^bevy_buttoninput]: Bevy `ButtonInput<T>` docs (pressed/just_pressed, press/release): <https://docs.rs/bevy/latest/bevy/input/struct.ButtonInput.html>
[^bevy_resource_exists]: Bevy `resource_exists` docs: <https://docs.rs/bevy/latest/bevy/prelude/fn.resource_exists.html>
[^bevy_run_conditions]: Bevy run-conditions example (resource_exists usage): <https://github.com/bevyengine/bevy/blob/main/examples/ecs/run_conditions.rs>
[^bevy_quickstart_states]: Bevy Quickstart Book — States overview and usage patterns: <https://thebevyflock.github.io/bevy-quickstart-book/1-intro/states.html>
[^minimal_plugins]: Bevy `MinimalPlugins` docs (minimal headless set, excludes many subsystems): <https://docs.rs/bevy/latest/bevy/struct.MinimalPlugins.html>
[^bevy_input_plugin]: Bevy input module docs (InputPlugin exists as part of the input system): <https://docs.rs/bevy/latest/bevy/input/index.html>
[^bevy_run_system_once]: Bevy `RunSystemOnce` docs (`World::run_system_once` for tests/diagnostics): <https://docs.rs/bevy/latest/bevy/ecs/system/trait.RunSystemOnce.html>
[^bevy_ui]: Bevy UI crate docs (UI basics: Node/Text/etc.): <https://docs.rs/bevy/latest/bevy/ui/index.html>
[^bevy_ui_crate]: bevy_ui crate docs (widgets, layout, etc.): <https://docs.rs/bevy_ui/latest/bevy_ui/>
[^bevy_diagnostics_frame]: Bevy FrameTimeDiagnosticsPlugin docs (FPS/frame time): <https://docs.rs/bevy/latest/bevy/diagnostic/struct.FrameTimeDiagnosticsPlugin.html>
[^bevy_log_diag_example]: Bevy example: log diagnostics (FPS/entity count/system info): <https://bevy.org/examples/diagnostics/log-diagnostics/>
[^bevy_run_conditions_example]: Bevy example: run conditions (resource_exists, combinators): <https://github.com/bevyengine/bevy/blob/main/examples/ecs/run_conditions.rs>
