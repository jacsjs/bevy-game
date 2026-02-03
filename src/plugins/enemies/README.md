# Architecture Exercises for Your Bevy Project

This README gives you **hands-on exercises** that build on what you already have:

- Enemies with `Health` + `Armour` + a short death state
- Global “game feel” effects (shake, flash, hitstop/slowmo)
- A clean, fast ECS style: **facts in components/resources**, **effects derived by systems**, **checks at boundaries**

You will find:
- Plain-language explanations
- “What to build” checklists
- Common pitfalls
- Small code snippets (illustrations, not full solutions)

---

## 1) A simple mental model (use this before adding any feature)

### 1.1 Facts vs effects
In an ECS game, it helps to separate:

- **Facts**: the actual game state.
  - Examples: health points, armour hits remaining, whether an enemy is dying.
- **Effects**: what you show or do *because* of the facts.
  - Examples: sprite flashing, camera shake, a short slow-motion.

**Rule of thumb:**
- **Gameplay systems** update facts.
- **Presentation systems** read facts and update visuals.

Why this matters:
- Your gameplay becomes easier to reason about.
- You can swap visuals (procedural -> sprite sheets) without rewriting gameplay.

### 1.2 Data change vs structural change
Two kinds of changes happen in ECS:

- **Data change**: modify fields inside components that already exist.
  - Example: `health.hp -= 1`, `flash_alpha = 0.5`.
  - Usually cheap.
- **Structural change**: spawn/despawn entities, add/remove components.
  - Usually more expensive, and can interact with deferred commands.

**Goal:** Keep frequent work (per-frame / per-hit) as **data changes**. Use structural changes sparingly and in one place.

### 1.3 One writer per decision
Try to make each important decision have one owner:

- One system decides “enemy enters dying”.
- One system decides “enemy is removed”.
- One system applies global FX (time scaling, shake, flash).

This avoids “systems fighting” each other.

---

## 2) Exercise A: Typestate-like `FxHandles` (Unready -> Ready)

### What you already have
You have a boundary system that finds/spawns:
- the camera entity
- the fullscreen flash overlay

…and stores those entity IDs somewhere so the hot path can do `get_mut(entity)`.

### The problem
Typically, the consumer does something like:

```rust
if handles.camera.is_none() || handles.overlay.is_none() {
    return;
}
```

That works, but it mixes “setup” concerns into your hot path.

### Goal
Create a clear “two state” pipeline:

- **Unready**: handles not established yet
- **Ready**: handles exist, so consumer can run with no missing-handle checks

### A simple approach
Use **two resources**:

- `FxUnready` (marker resource)
- `FxReady { camera: Entity, overlay: Entity }`

Then:
- `ensure_ready` runs only when `FxUnready` exists
- `apply_fx` runs only when `FxReady` exists

### Snippet (illustration)
```rust
#[derive(Resource)]
struct FxUnready;

#[derive(Resource)]
struct FxReady {
    camera: Entity,
    overlay: Entity,
}

fn ensure_ready(
    mut commands: Commands,
    q_cam: Query<Entity, With<Camera2d>>,
    q_overlay: Query<Entity, With<ScreenFlashOverlay>>,
) {
    // Find or spawn required entities.
    // If success: remove FxUnready, insert FxReady.
}

fn apply_fx(
    ready: Res<FxReady>,
    mut q_cam_tf: Query<&mut Transform>,
    mut q_overlay: Query<&mut Sprite, With<ScreenFlashOverlay>>,
) {
    // Straight-line: ready.camera and ready.overlay exist.
}
```

### Pitfalls
- Forgetting to reset state when leaving the game state.
- Storing stale entities (despawned) without noticing.

### Checklist
- [ ] `apply_fx` has no `Option` checks for handles.
- [ ] Only `ensure_ready` is allowed to create `FxReady`.
- [ ] When leaving `InGame`, `FxReady` is removed (or replaced by `FxUnready`).

---

## 3) Exercise B: Add one more global FX preset (without duplicating logic)

### What is a preset?
A preset is a **named bundle of knob values**:

- trauma amount (shake strength)
- flash intensity and decay
- hitstop duration
- slowmo minimum speed
- slowmo tail duration

### Goal
Add one new preset, such as:

- `trigger_armour_hit()` (small hit feel)
- `trigger_enemy_death()` (bigger, heavier feel)

…and keep the global consumer unchanged.

### Good structure
- All “what happened” logic stays in gameplay/presentation systems.
- All “how it feels” tuning stays inside `GlobalFx` methods.

### Snippet (illustration)
```rust
impl GlobalFx {
    fn trigger_armour_hit(&mut self) {
        self.trauma.add_clamped(0.15);
        self.flash = UnitF32::new_clamped(0.25);
        // no hitstop, no slowmo
    }

    fn trigger_enemy_death(&mut self) {
        self.trauma.add_clamped(0.70);
        self.flash = UnitF32::new_clamped(0.75);
        self.hitstop.set_max(0.06);
        self.slowmo_remaining.set_max(0.7);
        self.slowmo_min_speed = 0.35;
    }
}

// Call it from exactly one place:
if armour_took_hit && !armour_broke {
    fx.trigger_armour_hit();
}
```

### Pitfalls
- Applying camera shake in multiple systems.
- Setting time speed in multiple systems.

### Checklist
- [ ] Global time speed is changed in exactly one system.
- [ ] Camera shake is applied in exactly one system.
- [ ] Preset methods are short and named after “what happened”, not “how it works”.

---

## 4) Exercise C: Micro tests for your FX logic

### Why tests here?
You want fast feedback without running the whole game.
Test the **pure rules**:

- clamping stays in range
- timers never go negative
- hitstop overrides slowmo
- easing returns smoothly to normal speed

### Tip: test the math helpers
Small functions make great test targets:

- `UnitF32::add_clamped`
- `RealSeconds::tick_down`
- your easing function (e.g. smootherstep)
- a helper that computes the current virtual time speed

### Snippets (illustrations)

Clamping test:
```rust
#[test]
fn unit_clamps_to_one() {
    let mut x = UnitF32::new_clamped(0.9);
    x.add_clamped(0.5);
    assert!((0.0..=1.0).contains(&x.get()));
    assert!(x.get() <= 1.0);
}
```

Timer safety test:
```rust
#[test]
fn real_seconds_never_negative() {
    let mut t = RealSeconds::new(0.1);
    t.tick_down(10.0);
    assert!(t.get() >= 0.0);
}
```

Precedence test (hitstop wins):
```rust
fn compute_speed(hitstop: f32, slowmo_remaining: f32, slowmo_duration: f32, min_speed: f32) -> f32 {
    // You implement this. It should return 0 when hitstop > 0.
    1.0
}

#[test]
fn hitstop_overrides_slowmo() {
    let speed = compute_speed(0.05, 1.0, 1.0, 0.2);
    assert_eq!(speed, 0.0);
}
```

### Pitfalls
- Writing tests that depend on windowing, rendering, or physics.
- Testing exact numeric equality for floats (prefer ranges).

### Checklist
- [ ] Tests run with `cargo test` in headless mode.
- [ ] Tests don’t spawn a window.
- [ ] Tests focus on invariants and ordering rules.

---

## 5) Exercise D: Migrate to real sprite animations (state-driven)

Right now you do “procedural animation” by changing:
- `Sprite.color`
- `Transform.scale`
- overlay alpha

That is a great way to prototype.

Now you will migrate one piece (enemy death, or enemy idle) to a **sprite-sheet animation**.

### Goal
- Keep `EnemyLifeState` as the truth.
- Add an `Anim` component that stores animation playback state.
- Use two systems:
  1) **select animation** based on `EnemyLifeState`
  2) **tick animation** to advance frames

### What you will need
- A spritesheet image (even 4 frames is enough).
- A texture atlas setup (or equivalent) so you can select frames.

### Suggested component design
You need two kinds of data:

1) **Clip description** (static):
- start frame
- end frame
- fps
- loop mode

2) **Playback state** (per entity):
- which clip is active
- current frame
- timer accumulator

### Snippet: clip data (illustration)
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClipId { Idle, Death }

#[derive(Clone, Copy, Debug)]
struct Clip {
    start: usize,
    end: usize,
    fps: f32,
    looping: bool,
}

fn clip_data(id: ClipId) -> Clip {
    match id {
        ClipId::Idle => Clip { start: 0, end: 3, fps: 8.0, looping: true },
        ClipId::Death => Clip { start: 4, end: 11, fps: 12.0, looping: false },
    }
}
```

### Snippet: playback component (illustration)
```rust
#[derive(Component)]
struct Anim {
    clip: ClipId,
    frame: usize,
    timer: Timer,
}
```

### System 1: select animation
This system maps gameplay state to clip id:

- Alive -> Idle
- Dying -> Death

Important rule:
- If the clip is already correct, do nothing.
- If clip changed, reset frame + timer.

Snippet (illustration):
```rust
fn select_enemy_anim(mut q: Query<(&EnemyLifeState, &mut Anim)>) {
    for (life, mut anim) in &mut q {
        let desired = match life {
            EnemyLifeState::Alive => ClipId::Idle,
            EnemyLifeState::Dying { .. } => ClipId::Death,
            EnemyLifeState::Dead => ClipId::Death,
        };

        if anim.clip != desired {
            anim.clip = desired;
            let clip = clip_data(desired);
            anim.frame = clip.start;
            anim.timer = Timer::from_seconds(1.0 / clip.fps, TimerMode::Repeating);
        }
    }
}
```

### System 2: tick animation
This system advances frames and writes the atlas index.

Snippet (illustration):
```rust
fn tick_anim(time: Res<Time>, mut q: Query<(&mut Anim, &mut TextureAtlas)>) {
    for (mut anim, mut atlas) in &mut q {
        let clip = clip_data(anim.clip);
        anim.timer.tick(time.delta());

        while anim.timer.just_finished() {
            if anim.frame < clip.end {
                anim.frame += 1;
            } else if clip.looping {
                anim.frame = clip.start;
            }
            atlas.index = anim.frame;
        }
    }
}
```

### Pitfalls
- Restarting the death animation every frame because you keep “selecting” it.
- Mixing gameplay logic into the animation tick system.
- Driving animation with inconsistent time (choose Update or Fixed and stick with it).

### Checklist
- [ ] Enemy shows a sprite-sheet animation for at least one state.
- [ ] Clip selection does not restart every frame.
- [ ] Playback tick is separate from selection.

---

## 6) Common pitfalls cheat sheet

### Two queries conflict in one system
If you have two queries that both borrow the same component mutably in one system, you must:

- Prove they are disjoint using `Without<...>` filters, or
- Use a `ParamSet` and carefully access one query at a time.

Disjoint filter idea (illustration):
```rust
fn sys(
    mut cams: Query<&mut Transform, (With<Camera2d>, Without<ScreenFlashOverlay>)>,
    mut overlay: Query<&mut Transform, (With<ScreenFlashOverlay>, Without<Camera2d>)>,
) {}
```

### Structural changes during physics-heavy steps
Avoid despawning inside the fixed physics schedule.
Prefer:
- mark with a component
- despawn later in PostUpdate

---

## 7) Suggested order
If you want a smooth progression:

1. Add one new preset (quick win)
2. Add micro tests (solidifies confidence)
3. Typestate handles (makes hot paths cleaner)
4. Migrate one procedural animation to sprite-sheet animation

---

Good luck. Keep your changes small, measurable, and with one clear owner per decision.