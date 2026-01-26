# Event / Messaging Architecture Rules (Messages vs Events vs Resources vs Queries)

This chapter codifies **communication rules** in a modular Bevy game to prevent “spaghetti coupling”.
It is optimized for your current design choices:

- Avian collision events → Bevy **Messages** (bulk processing)
- gameplay logic kept deterministic and testable
- presentation (VFX/SFX/UI) reacts to signals rather than owning gameplay state

Bevy provides **two** primary communication styles:

- **Messages**: buffered, pull-based, processed at fixed points in schedules; good for batch processing high volumes.[^bevy_message]
- **Events / Observers (EntityEvent)**: triggered immediately for observers; good for entity-specific reactions and UI interactions.[^bevy_ecs_event][^bevy_observers_example]

Messages are stored in a `Messages<M>` resource and read using `MessageReader<M>`. `MessageReader` supports `len()`, `is_empty()`, and independent consumption per reader.[^bevy_message][^bevy_message_reader][^bevy_messages]

Resources are singleton-like global values stored in the `World` and accessed via `Res` / `ResMut`.[^bevy_resource_module]

---

## 1) The decision rule: which mechanism to use?

### 1.1 Use **Messages** when… (high-volume, one-to-many)

Use Messages for:

- **high-volume** signals (hundreds/thousands per second)
- where **batch processing** is natural
- where you want predictable “flush points” (end of frame / PostUpdate)

Examples:

- `CollisionStart` / `CollisionEnd` from Avian processed with `MessageReader`
- `HitEvent` / `DamageEvent` emitted for many bullets
- “spawn requests” in bulk (e.g., `SpawnBulletRequested`)

Why: Bevy messages are specifically designed for buffered pull-based handling, efficient batch processing, and predictable scheduling; they can be more efficient than observer-driven events for large volumes.[^bevy_message]

**Rule of thumb:** if you can process it with `for msg in reader.read()` and it might be large, prefer a Message.[^bevy_message][^bevy_message_reader]

---

### 1.2 Use **Events/Observers** when… (entity-specific, immediate)

Use Events/Observers for:

- entity-specific “callbacks” (e.g., UI button clicked)
- interactive gameplay triggers (pressure plates, entering zones)
- logic that is best attached to a specific entity

Bevy observers demonstrate this model: an `Event` triggers any matching observers, and an `EntityEvent` targets specific entities (in addition to global observers).[^^bevy_observers_example][^bevy_ecs_event]

**Rule of thumb:** if the logic wants to live *on an entity* (e.g., UI widget), prefer observer events.

---

### 1.3 Use **Resources** when… (single source of truth)

Use resources for:

- global state: score, difficulty, run seed, current wave
- configuration / tunables
- global registries (handles, caches)

Bevy resources are singleton-like data stored in the `World` and accessed through `Res`/`ResMut` system params.[^bevy_resource_module]

**Rule of thumb:** if there should only ever be one, it’s a resource.

---

### 1.4 Use **Queries** when… (local deterministic logic)

Queries are best when:

- the system logic depends on *current* ECS state (components)
- you want deterministic local computation
- you’re not broadcasting a “thing happened”, you’re computing “what is true now”

Examples:

- movement: `Query<&mut LinearVelocity, With<Player>>`
- AI: `Query<&Transform, With<Player>>` + local enemy data
- validation: query for leaked entities

**Rule of thumb:** queries compute “state now”, messages/events communicate “something happened”.

---

## 2) Naming conventions (prevent semantic drift)

Use a three-phase naming convention that mirrors intent → application → resolution:

- `XRequested`: “I want X to happen” (intent-level; often produced by input/AI)
- `XApplied`: “X has been applied to state” (e.g., damage applied)
- `XResolved`: “All consequences of X are finalized” (e.g., despawns, score updates, VFX triggers)

### Examples

- `ShootRequested` (input/AI) → `BulletSpawned` (applied) → `HitResolved`
- `DamageRequested` → `DamageApplied` → `DeathResolved`

Why this works:

- it makes schedules explicit: requests in `Update`, applied in `FixedUpdate`, resolved in `PostUpdate`
- it clarifies what systems are allowed to mutate state

---

## 3) Message lifecycle rules (avoid backlog and missing data)

### 3.1 Messages must be consumed every frame (or you risk drops)

`Messages<M>` is double-buffered and represents messages that occurred within the last two `Messages::update` calls. If messages are not handled by the end of the frame after they are updated, they can be dropped.[^bevy_messages]

**Rule:** every message type should have a consumer that runs every frame where that message can be produced.

### 3.2 Backlog is measurable and actionable

`MessageReader` supports `len()` and `is_empty()` to inspect backlog without consuming messages.[^bevy_message_reader]

Use backlog metrics to catch:

- consumer systems not running due to state/run conditions
- producer rate exceeding consumer capacity

---

## 4) Canonical patterns

### 4.1 High-volume processing: collisions → damage → death

**Pattern:**

- Avian emits `CollisionStart` messages
- a “router” system converts collisions to `DamageRequested`
- an “apply” system converts `DamageRequested` to `DamageApplied`
- a “resolve” system handles `DeathResolved` (despawn, score, VFX)

Messages are ideal here because collisions and hits can be large volume and benefit from batch processing.[^bevy_message][^bevy_message_reader]

```rust
use bevy::prelude::*;
use avian2d::prelude::*;

#[derive(Message, Debug, Clone, Copy)]
pub struct DamageRequested {
    pub victim: Entity,
    pub amount: i32,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct DamageApplied {
    pub victim: Entity,
    pub new_hp: i32,
}

#[derive(Component)]
pub struct Health { pub hp: i32 }

fn collision_to_damage(
    mut collisions: MessageReader<CollisionStart>,
    bullets: Query<(), With<Bullet>>,
    enemies: Query<(), With<Enemy>>,
    mut damage: MessageWriter<DamageRequested>,
) {
    for c in collisions.read() {
        let a = c.collider1;
        let b = c.collider2;

        let (bullet, victim) = if bullets.contains(a) && enemies.contains(b) {
            (a, b)
        } else if bullets.contains(b) && enemies.contains(a) {
            (b, a)
        } else {
            continue;
        };

        // Request damage; other systems decide what to do.
        damage.write(DamageRequested { victim, amount: 1 });

        // Bullet removal is an implementation choice: do it here or in a later resolve system.
        let _ = bullet;
    }
}

fn apply_damage(
    mut req: MessageReader<DamageRequested>,
    mut applied: MessageWriter<DamageApplied>,
    mut q: Query<&mut Health>,
) {
    for d in req.read() {
        if let Ok(mut hp) = q.get_mut(d.victim) {
            hp.hp -= d.amount;
            applied.write(DamageApplied { victim: d.victim, new_hp: hp.hp });
        }
    }
}
```

---

### 4.2 Entity-specific reactions: UI click → transition

If an action is tightly associated with a UI widget/entity, prefer observer-driven events.
Observers run when an event is triggered; Bevy’s example shows both `Event` and `EntityEvent` usage.[^bevy_observers_example][^bevy_ecs_event]

---

---

## 4.3) Concrete examples of each communication type

This section provides **minimal, copy-pasteable examples** of each communication mechanism discussed above.
The goal is to make it obvious what each style looks like in real code.

### A) Message (buffered, pull-based) — high volume

**Pros**

- Excellent for high-volume streams (e.g., collisions, hits) with efficient batch processing.
- Predictable timing: processed at fixed schedule points, reducing callback spaghetti.
- Multiple consumers can read the same message stream independently (per-reader cursors).

**Cons**

- Requires consumers to run regularly; backlog can grow and old messages may be dropped if readers fall behind.
- Harder to model entity-specific, immediate interactions (better served by observers).
- Ordering bugs can happen if producers/consumers aren’t thoughtfully scheduled.

Messages are buffered and processed at fixed points in the schedule. They are written with `MessageWriter<T>` and read with `MessageReader<T>`.[^bevy_message][^bevy_message_reader]

```rust
use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy)]
struct HitRequested {
    victim: Entity,
    damage: i32,
}

fn produce_hits(mut w: MessageWriter<HitRequested>, q: Query<Entity, With<Enemy>>) {
    // Example: request damage for all enemies (high-volume / batch-friendly)
    for e in &q {
        w.write(HitRequested { victim: e, damage: 1 });
    }
}

fn apply_hits(mut r: MessageReader<HitRequested>, mut hp: Query<&mut Health>) {
    for hit in r.read() {
        if let Ok(mut h) = hp.get_mut(hit.victim) {
            h.hp -= hit.damage;
        }
    }
}
```

**Tip:** `MessageReader::len()` lets you measure backlog without consuming messages, which is useful for metrics and regression tests.[^bevy_message_reader][^bevy_messages]

---

### B) Classic `Event` (broadcast queue) — moderate volume

**Pros**

- Great for broadcasting "something happened" to multiple systems (UI, audio, VFX, analytics).
- Natural fit for moderate-volume notifications (e.g., enemy died, wave started).
- Keeps gameplay decoupled: producers don’t need to know who listens.

**Cons**

- Not ideal for extremely high volume; can become noisy and expensive if spammy.
- If receivers are gated by run_if / states, you can miss events unless you design for it.
- Easy to overuse and create implicit ordering dependencies.

Use `EventWriter<T>` / `EventReader<T>` when you want to broadcast something that multiple systems should react to.
In Bevy, the `Event` system is part of the ECS event module and is commonly used with observers and event triggers.[^bevy_ecs_event][^bevy_observers_example]

```rust
use bevy::prelude::*;

#[derive(Event, Debug, Clone, Copy)]
struct EnemyDied {
    where_: Vec2,
}

fn mark_deaths(mut ew: EventWriter<EnemyDied>, q: Query<&Transform, With<DeadEnemy>>) {
    for t in &q {
        ew.send(EnemyDied { where_: t.translation.truncate() });
    }
}

fn spawn_vfx(mut er: EventReader<EnemyDied>, mut commands: Commands) {
    for e in er.read() {
        commands.spawn((Name::new("Explosion"), Transform::from_translation(e.where_.extend(0.0))));
    }
}
```

---

### C) Observer on an `Event` — immediate reactions

**Pros**

- Excellent for callback-style logic (UI interaction, triggers, entity lifecycle hooks).
- Runs immediately when triggered, which can simplify interaction code.
- Keeps code close to the entity/feature that cares about the event.

**Cons**

- Can become hard to reason about if many observers chain-trigger other events.
- Less suited for bulk processing; observers are typically about localized reactions.
- Debugging ordering can be tricky unless you keep observers simple and well-scoped.

Observers are systems that run when an event is triggered. The official observers example demonstrates custom `Event` and `EntityEvent` usage and how observers respond to those triggers.[^bevy_observers_example][^bevy_ecs_event]

```rust
use bevy::prelude::*;

#[derive(Event)]
struct OpenSettings;

fn setup(mut commands: Commands) {
    commands.add_observer(|_ev: On<OpenSettings>, mut next: ResMut<NextState<GameState>>| {
        next.set(GameState::Settings);
    });
}

fn ui_button_clicked(mut commands: Commands) {
    // Triggering an Event will run any observers watching it.
    commands.trigger(OpenSettings);
}
```

Use this when an action is strongly tied to “callback-style” behavior (UI, interaction, triggers).

---

### D) `EntityEvent` (targeted event) — per-entity callbacks

**Pros**

- Perfect for targeted behavior ("do X to this entity"), while still allowing global observers.
- Great for UI widgets and interactables (hover, click, highlight, select).
- Keeps per-entity behavior modular without queries scanning the whole world.

**Cons**

- Still callback-like; large chains can make control flow implicit.
- If you use it for high-volume gameplay (e.g., bullets), it can get too chatty.
- Requires careful design of the event payload and target semantics.

An `EntityEvent` targets a specific entity and can also be observed globally.
The Bevy event module describes `EntityEvent` as a specialized `Event` that is triggered for a specific entity target.[^bevy_ecs_event][^bevy_observers_example]

```rust
use bevy::prelude::*;

#[derive(EntityEvent)]
struct Highlight {
    entity: Entity,
}

fn setup(mut commands: Commands) {
    // Spawn an entity that will react to Highlight events
    let e = commands.spawn(Name::new("Selectable"))
        .observe(|ev: On<Highlight>, mut cmds: Commands| {
            // Runs for this specific entity
            cmds.entity(ev.entity).insert(Name::new("Selected"));
        })
        .id();

    // Trigger a targeted event for that entity
    commands.trigger(Highlight { entity: e });
}
```

---

### E) Resource (single source of truth) — global state

**Pros**

- Best for globally unique state: config, score, run seed, wave index, registries.
- Simple mental model: one value, read/write via Res / ResMut.
- Highly testable: insert a resource and run systems deterministically.

**Cons**

- Easy to create hidden coupling if too many systems mutate the same resource.
- Large "god resources" become dumping grounds and harm modularity.
- Requires explicit change tracking if you want event-like reactions.

Resources are singleton-like values stored in the `World` and accessed via `Res` / `ResMut`.
This is the canonical Bevy pattern for globally unique state (settings, score, handles, etc.).[^bevy_resource_module]

```rust
use bevy::prelude::*;

#[derive(Resource, Default)]
struct Score(pub u32);

fn add_points(mut score: ResMut<Score>) {
    score.0 += 10;
}

fn show_score(score: Res<Score>) {
    info!(score = score.0, "score updated");
}
```

---

### F) Query (local deterministic logic) — compute “state now”

**Pros**

- Most direct way to compute current truth from ECS state (components).
- Deterministic and explicit: input is components, output is component/resource changes.
- Often the easiest to unit test (spawn entities, run system, assert).

**Cons**

- Can be expensive if used as a substitute for events (polling everything every frame).
- Cross-cutting concerns can lead to wide queries and implicit dependencies.
- Not a communication primitive: it answers "what is", not "what happened".

Queries are used to compute the current truth of the ECS world (components on entities) and are ideal for deterministic local logic.
Use queries when you don’t need to broadcast “something happened”.

```rust
use bevy::prelude::*;

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Velocity(pub Vec2);

fn integrate_player(mut q: Query<(&mut Transform, &Velocity), With<Player>>, time: Res<Time>) {
    for (mut t, v) in &mut q {
        t.translation.x += v.0.x * time.delta_seconds();
        t.translation.y += v.0.y * time.delta_seconds();
    }
}
```

---

### Summary: quick selection heuristic

- **Message**: high-volume, batch processing, predictable schedule points.[^bevy_message]
- **Event + EventReader**: moderate volume broadcast reactions.
- **Observer**: immediate callback behavior, UI/triggers.[^bevy_observers_example]
- **EntityEvent**: targeted per-entity callbacks.[^bevy_ecs_event]
- **Resource**: single source of truth global state.[^bevy_resource_module]
- **Query**: compute the current truth of the ECS world.

## 5) Tests (regression safety)

### 5.1 Test: “message processing does not despawn wrong entities”

This test ensures routing/processing logic only affects intended entities.

Key idea:

- spawn a bullet, an enemy, and a neutral entity
- inject a collision message that should only kill bullet+enemy
- run the processing system
- assert the neutral entity still exists

```rust
use bevy::prelude::*;
use avian2d::prelude::*;

#[derive(Component)] struct Bullet;
#[derive(Component)] struct Enemy;
#[derive(Component)] struct Neutral;

fn process_hits(mut starts: MessageReader<CollisionStart>, bullets: Query<(), With<Bullet>>, enemies: Query<(), With<Enemy>>, mut commands: Commands) {
    for e in starts.read() {
        let a = e.collider1;
        let b = e.collider2;
        let bullet_enemy = (bullets.contains(a) && enemies.contains(b)) || (bullets.contains(b) && enemies.contains(a));
        if bullet_enemy {
            commands.entity(a).despawn();
            commands.entity(b).despawn();
        }
    }
}

#[test]
fn message_processing_does_not_despawn_wrong_entities() {
    let mut app = App::new();

    // Register message type + message update system.
    app.add_message::<CollisionStart>();

    app.add_systems(Update, process_hits);

    let bullet = app.world_mut().spawn(Bullet).id();
    let enemy = app.world_mut().spawn(Enemy).id();
    let neutral = app.world_mut().spawn(Neutral).id();

    // Write a collision message.
    app.world_mut().write_message(CollisionStart {
        collider1: bullet,
        collider2: enemy,
        body1: None,
        body2: None,
    });

    // Tick once: messages update + processing.
    app.update();

    assert!(app.world().get_entity(neutral).is_some(), "neutral entity was despawned");
    assert!(app.world().get_entity(bullet).is_none(), "bullet should be despawned");
    assert!(app.world().get_entity(enemy).is_none(), "enemy should be despawned");
}
```

> Notes:
>
> - This relies on Bevy’s Messages being readable with `MessageReader` after `add_message::<T>()` installs the update system.
> - If your production code defers despawn to PostUpdate, align the test schedule accordingly.

---

### 5.2 Test: “no message backlog” (readers consume expected counts)

This test catches cases where:

- a consumer system is gated off (wrong state/run_if)
- message volume increased and consumers fell behind

Because `MessageReader::len()` reports how many messages are available to that reader without consuming, it’s perfect for backlog assertions.[^bevy_message_reader]

```rust
use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy)]
struct Ping;

fn produce(mut w: MessageWriter<Ping>) {
    for _ in 0..100 {
        w.write(Ping);
    }
}

fn consume(mut r: MessageReader<Ping>) {
    // Consume all pending messages for this reader.
    for _ in r.read() {}
}

fn assert_no_backlog(mut r: MessageReader<Ping>) {
    assert_eq!(r.len(), 0, "message backlog detected: {}", r.len());
}

#[test]
fn no_message_backlog_when_consumers_run_each_frame() {
    let mut app = App::new();
    app.add_message::<Ping>();

    // Producer then consumer every frame.
    app.add_systems(Update, (produce, consume, assert_no_backlog).chain());

    // Run a few frames.
    for _ in 0..10 {
        app.update();
    }
}
```

Why this is valid:

- `MessageReader` is designed to track per-system progress and offers `len()`/`is_empty()`/`clear()` to reason about pending messages.[^bevy_message_reader]
- `Messages<M>` uses a two-update buffer model; if consumers don’t read within that window, messages can be dropped (silent loss), so backlog indicates risk.[^bevy_messages]

---

## References

[^bevy_message]: Bevy `Message` trait docs (buffered pull-based messages, predictable schedule points, efficient batch processing): <https://docs.rs/bevy/latest/bevy/prelude/trait.Message.html>
[^bevy_message_reader]: Bevy `MessageReader` docs (`read`, `len`, `is_empty`, `clear`, independent cursors): <https://docs.rs/bevy/latest/bevy/prelude/struct.MessageReader.html>
[^bevy_messages]: Bevy `Messages<M>` docs (double-buffer semantics, update cadence, dropping behavior): <https://docs.rs/bevy/latest/bevy/ecs/message/struct.Messages.html>
[^bevy_ecs_event]: Bevy ECS event module docs (`Event`, `EntityEvent`, triggers/observers): <https://docs.rs/bevy/latest/bevy/ecs/event/index.html>
[^bevy_observers_example]: Bevy official example: Observers (custom events and entity events, entity-scoped handling): <https://bevy.org/examples/ecs-entity-component-system/observers/>
[^bevy_resource_module]: Bevy ECS resource module docs (resources are singleton-like data stored in the World): <https://docs.rs/bevy/latest/bevy/ecs/resource/index.html>
