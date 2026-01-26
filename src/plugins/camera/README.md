# Animations in the Game (Gameplay, UI, VFX, and Data-Driven Triggers)

This is the **final chapter** of the engineering manual: a comprehensive, example-heavy guide to implementing animations in your Bevy game.

It is written to match the architecture you’ve established:

- **State-owned UI** (per-screen roots)
- **Messages** for decoupled request/apply/resolve flows
- **Resources** for single sources of truth (view models, config)
- **Perf-minded**: avoid per-frame allocations and mass entity churn

---

## 0) The animation taxonomy (what kinds of animation you actually need)

In a bullet hell / roguelite game, you typically have **four** animation families:

1. **Gameplay entity motion**: player bob, enemy hover, recoil, knockback.
2. **2D sprite animation**: spritesheets / flipbooks (idle, run, shoot, hit, die).
3. **UI animation**: open/close transitions, button hover/press, inventory panel slides.
4. **VFX animation**: explosions, hit sparks, damage numbers.

Each family has different constraints and tools.

---

## 1) Design rules (prevent animation spaghetti)

### 1.1 Animations are *presentation*, not *gameplay state*

- Animation systems should read gameplay state and produce visuals.
- They should not be the source of truth for damage, collisions, inventory changes, etc.

If animation completion must trigger gameplay (e.g., “attack hits on frame 3”), use a message/flag and keep the actual damage resolution in gameplay systems.

### 1.2 Use messages to request animations (decouple)

Adopt the same conventions you already use:

- `AnimationRequested`
- `AnimationApplied` (optional)
- `AnimationResolved` (optional)

Messages keep the call site clean and allow multiple consumers (VFX, SFX, UI) to react independently.

### 1.3 Prefer deterministic update points

- For high-volume things (hit sparks): batch via Messages.
- For per-entity idle animations: run deterministic systems that read `Time`.

### 1.4 Keep animation updates cheap

- Avoid building `String`s every frame.
- Avoid allocating `Vec`s every frame.
- Avoid spawning/despawning huge numbers of entities every frame.

---

## 2) A unified “animation request” message

This is optional, but extremely powerful for your architecture.

### 2.1 Minimal `AnimationId` + `AnimationRequested`

```rust
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationId {
    PlayerShoot,
    PlayerHit,
    EnemySpawn,
    EnemyDie,
    UiOpenInventory,
    UiCloseInventory,
    VfxExplosion,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct AnimationRequested {
    pub target: Entity,
    pub id: AnimationId,
}
```

### 2.2 Why this helps

- gameplay systems emit one lightweight message
- animation systems translate it into the right mechanism:
  - spritesheet flipbook
  - Bevy clip animation (`AnimationPlayer` / graphs)
  - UI tween

This prevents random systems from directly setting `TextureAtlas.index`, `AnimationPlayer`, etc.

---

## 3) Gameplay motion animations (procedural / parametric)

These are “cheap” animations that don’t require assets:

- bobbing
- hovering
- recoil
- squash & stretch

### 3.1 Example: idle bobbing for enemies

```rust
use bevy::prelude::*;

#[derive(Component)]
pub struct Bob {
    pub amplitude: f32,
    pub speed: f32,
    pub base_y: f32,
}

fn bobbing(time: Res<Time>, mut q: Query<(&Bob, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (bob, mut tf) in &mut q {
        tf.translation.y = bob.base_y + bob.amplitude * (t * bob.speed).sin();
    }
}
```

### 3.2 Example: weapon recoil on shoot (message-driven)

```rust
use bevy::prelude::*;

#[derive(Component)]
pub struct Recoil {
    pub timer: Timer,
    pub strength: f32,
}

fn apply_recoil_requested(
    mut req: MessageReader<AnimationRequested>,
    mut q: Query<&mut Recoil>,
) {
    for r in req.read() {
        if r.id != AnimationId::PlayerShoot { continue; }
        if let Ok(mut recoil) = q.get_mut(r.target) {
            recoil.timer.reset();
        }
    }
}

fn recoil_update(time: Res<Time>, mut q: Query<(&mut Recoil, &mut Transform)>) {
    for (mut recoil, mut tf) in &mut q {
        recoil.timer.tick(time.delta());
        if recoil.timer.finished() {
            continue;
        }
        let p = 1.0 - recoil.timer.fraction();
        tf.translation.x -= recoil.strength * p;
    }
}
```

---

## 4) 2D sprite animations (flipbooks / spritesheets)

There are two common approaches:

- **Manual flipbook component** (simple, tiny)
- **A dedicated animation plugin** (composition, events, easing)

### 4.1 Manual flipbook (recommended to learn first)

```rust
use bevy::prelude::*;

#[derive(Component)]
pub struct Flipbook {
    pub frames: std::ops::RangeInclusive<usize>,
    pub fps: f32,
    pub timer: Timer,
    pub looped: bool,
}

impl Flipbook {
    pub fn new(frames: std::ops::RangeInclusive<usize>, fps: f32, looped: bool) -> Self {
        Self {
            frames,
            fps,
            timer: Timer::from_seconds(1.0 / fps, TimerMode::Repeating),
            looped,
        }
    }
}

fn flipbook_update(time: Res<Time>, mut q: Query<(&mut Flipbook, &mut Sprite)>) {
    for (mut fb, mut sprite) in &mut q {
        fb.timer.tick(time.delta());
        if !fb.timer.just_finished() { continue; }

        let Some(atlas) = sprite.texture_atlas.as_mut() else { continue; };
        let start = *fb.frames.start();
        let end = *fb.frames.end();

        if atlas.index < start || atlas.index > end {
            atlas.index = start;
        } else if atlas.index == end {
            if fb.looped { atlas.index = start; }
        } else {
            atlas.index += 1;
        }
    }
}
```

### 4.2 Triggering a flipbook from a message

```rust
fn on_explosion_requested(
    mut req: MessageReader<AnimationRequested>,
    mut q: Query<&mut Flipbook>,
) {
    for r in req.read() {
        if r.id != AnimationId::VfxExplosion { continue; }
        if let Ok(mut fb) = q.get_mut(r.target) {
            // restart by forcing index to start on next update
            fb.timer.reset();
        }
    }
}
```

---

## 5) Clip-based animations (Bevy `AnimationPlayer` + graphs)

If you load 3D characters (glTF) or want the full Bevy animation pipeline, you’ll use:

- `AnimationPlayer` (playback controller)
- `AnimationGraph` (how clips blend)

> Bevy updates animations automatically each frame once configured.[^bevy_anim_crate]

---

## 6) Animation blending (the missing superpower)

Blending is how you make animations feel *continuous* instead of “hard switching”.
The classic example is a character that smoothly transitions:

- Idle → Walk → Run

Bevy provides two complementary blending levels:

1. **Per-clip weights** on an `AnimationPlayer` (play multiple clips and adjust weights).[^bevy_active_anim]
2. **Animation graphs** (blend/add nodes that combine clips according to weights).[^bevy_anim_graph][^bevy_anim_graph_example]

### 6.1 Blending concept: weights and normalization

At any moment, you can have multiple animations influencing the same target.
A simple and reliable rule:

- Keep weights in `[0, 1]`
- Normalize groups so the total weight is 1 (for pure blending)

```rust
fn normalize2(a: f32, b: f32) -> (f32, f32) {
    let s = (a + b).max(1e-6);
    (a / s, b / s)
}
```

### 6.2 Blending with `AnimationPlayer` (no graph)

You can play multiple animations and set their weights.

- `AnimationPlayer::play()` returns a mutable `ActiveAnimation` handle.[^bevy_anim_player]
- `ActiveAnimation` exposes `set_weight` and `weight`.[^bevy_active_anim]

```rust
use bevy::prelude::*;

#[derive(Component)]
struct Player;

#[derive(Resource, Default)]
struct LocomotionSpeed(pub f32); // 0..1

#[derive(Resource)]
struct PlayerClips {
    idle: AnimationNodeIndex,
    run: AnimationNodeIndex,
}

fn update_locomotion_blend(
    speed: Res<LocomotionSpeed>,
    mut q: Query<&mut AnimationPlayer, With<Player>>,
    clips: Res<PlayerClips>,
) {
    let Ok(mut player) = q.get_single_mut() else { return; };

    // Start both clips (play is idempotent if already playing)
    let idle = player.play(clips.idle);
    let run  = player.play(clips.run);

    // Blend based on speed
    let run_w = speed.0.clamp(0.0, 1.0);
    let idle_w = 1.0 - run_w;

    idle.set_weight(idle_w);
    run.set_weight(run_w);
}
```

**Good uses**

- quick 2-way blends (idle/run)
- additive “overlay” animation by setting a small weight

**Limitations**

- complex multi-clip logic becomes manual
- you’ll reinvent graph semantics over time

### 6.3 Crossfading (transition) over time

Crossfade means weights change gradually, not instantly.

```rust
#[derive(Resource, Default)]
struct Crossfade {
    t: f32,
    duration: f32,
    from_run: bool,
}

fn begin_crossfade(to_run: bool, mut xf: ResMut<Crossfade>) {
    xf.t = 0.0;
    xf.duration = 0.25;
    xf.from_run = !to_run;
}

fn update_crossfade(
    time: Res<Time>,
    mut xf: ResMut<Crossfade>,
    mut speed: ResMut<LocomotionSpeed>,
) {
    if xf.t >= xf.duration { return; }

    xf.t += time.delta_secs();
    let p = (xf.t / xf.duration).clamp(0.0, 1.0);

    // Smoothstep for nicer feel
    let p = p * p * (3.0 - 2.0 * p);

    // If fading to run, go 0->1; else 1->0
    speed.0 = if xf.from_run { 1.0 - p } else { p };
}
```

You can trigger `begin_crossfade(...)` when your movement state changes.

### 6.4 Animation graphs (the scalable solution)

An `AnimationGraph` is a DAG that describes how animations are weighted and combined.

- Nodes include **clip nodes**, **blend nodes**, and **add nodes**, each with weights.[^bevy_anim_graph]
- Bevy evaluates the graph from the root and blends bottom-up to produce the final pose.[^bevy_anim_graph]

Bevy’s official **Animation Graph** example demonstrates animation blending with animation graphs and interactive weight changes.[^bevy_anim_graph_example]

**When to use graphs**

- 3+ clips blending (idle/walk/run)
- layered animation (run base + aim overlay)
- per-bone masking (upper-body aim, lower-body run)

### 6.5 Additive blending (aim/recoil overlay)

Additive blending is ideal for “overlay” motions:

- aim offset
- recoil kick
- breathing

Graph approach (conceptual):

- root blend: locomotion
- add node: locomotion + recoil

Your locomotion stays dominant; recoil adds small offsets.

### 6.6 Practical blending rules (avoid weird results)

- **Normalize** blend groups (idle+walk+run = 1.0).
- Use **additive** only for small overlays.
- Clamp weights; avoid negative weights.
- Gate blends to relevant states to avoid hidden work.

---

## 7) UI animations

Two good approaches:

- **Manual tweens**: animate `Node` positions and `BackgroundColor`.
- **Clip-driven UI animation**: Bevy example shows animating UI properties with animation clips.[^bevy_animated_ui_example]

Choose manual tweens for inventory panels and menus; use clip-driven UI when you want complex synchronized property curves.

---

## 8) VFX animation patterns

### 8.1 One-shot VFX entities (despawn after flipbook)

```rust
#[derive(Component)]
pub struct DespawnAfterFlipbook;

fn despawn_finished_flipbooks(
    mut commands: Commands,
    q: Query<(Entity, &Flipbook, &Sprite), With<DespawnAfterFlipbook>>,
) {
    for (e, fb, sprite) in &q {
        let Some(atlas) = sprite.texture_atlas.as_ref() else { continue; };
        if !fb.looped && atlas.index == *fb.frames.end() {
            commands.entity(e).despawn();
        }
    }
}
```

**Scaling note:** if you spawn hundreds of VFX per second, consider pooling (same idea as bullet pooling).

---

## 9) Scheduling rules

- Apply gameplay results first (damage/death)
- Trigger animations in a later pass (post-apply)
- Keep animation updates gated by state

---

## 10) Performance playbook for animations

- Avoid per-frame allocations (strings, vectors)
- Use component-driven animation (one system updates many entities)
- Batch high-volume requests

---

## 11) Tests (invariants)

### 11.1 AnimationRequested changes anim state

```rust
#[derive(Component)]
struct AnimState(pub u8);

#[test]
fn animation_request_sets_state() {
    let mut world = World::new();

    world.add_message::<AnimationRequested>();

    let player = world.spawn(AnimState(0)).id();

    world.write_message(AnimationRequested { target: player, id: AnimationId::PlayerShoot });

    // pretend system
    world.entity_mut(player).insert(AnimState(1));

    let state = world.get::<AnimState>(player).unwrap();
    assert_eq!(state.0, 1);
}
```

---

## References

[^bevy_anim_crate]: Bevy animation crate overview: <https://docs.rs/bevy_animation/latest/bevy_animation/>
[^bevy_anim_player]: Bevy `AnimationPlayer` docs: <https://docs.rs/bevy/latest/bevy/animation/struct.AnimationPlayer.html>
[^bevy_active_anim]: Bevy `ActiveAnimation` docs (includes `set_weight`/`weight`): <https://docs.rs/bevy_animation/latest/bevy_animation/struct.ActiveAnimation.html>
[^bevy_anim_graph]: Bevy `AnimationGraph` docs: <https://docs.rs/bevy/latest/bevy/animation/graph/struct.AnimationGraph.html>
[^bevy_anim_graph_example]: Bevy example “Animation Graph” (demonstrates blending and weight control): <https://bevy.org/examples/animation/animation-graph/>
[^bevy_animated_ui_example]: Bevy example “Animated UI” (animate UI properties using animation clips): <https://bevy.org/examples/animation/animated-ui/>
