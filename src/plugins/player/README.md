# Inventory System & Interactable Items (Pickups, Equipment, Chests, Doors)

This chapter defines a **comprehensive, scalable design** for:

- interactable objects (pickups, chests, doors, shrines, NPC triggers)
- inventory (stacks, unique items, currency)
- equipment (slots, modifiers)
- derived stats recomputation
- UI prompt integration
- tests that prevent regression

It is intentionally **example-heavy** and aligned with the architecture you’ve already built:

- **Messages** for high-volume / batchable flows
- **Resources** for single sources of truth (inventory, run state)
- **Queries** for deterministic “state now” logic (focus selection)
- **Observers / Picking** optionally for click interactions

Bevy messages are buffered and processed at fixed points; they are written with `MessageWriter` and read using `MessageReader`.[^bevy_message][^bevy_message_writer]
Resources are singleton-like values stored in the `World` and accessed via `Res`/`ResMut`.[^bevy_resource]
Bevy observers demonstrate entity-specific event reactions, and `bevy_picking` provides pointer events that bubble up the entity hierarchy and are often handled elegantly with observers.[^bevy_observers_example][^bevy_picking]

---

## 0) Goals and non-goals

### Goals

- A clean interaction pipeline: **detect → request → apply → resolve**.
- Inventory and equipment implemented as **data**, not as “everything is an entity”.
- Interactions are deterministic and testable.
- Compatible with roguelite runs (seeded maps, upgrades, resets).

### Non-goals

- A full editor / content authoring UI.
- A networked authoritative inventory (separate chapter if needed).

---

## 1) Mental model (the most important part)

### 1.1 Interactions are not “behavior”: they are **messages**

Gameplay code should not directly:

- despawn pickup entities
- mutate inventory
- spawn UI popups
- play audio

Instead, gameplay emits **requests** and specialized systems apply and resolve.

### 1.2 Inventory is a **resource**

Inventory is globally unique per player/run (usually one), which maps directly to Bevy’s `Resource` model.[^bevy_resource]

### 1.3 Equipment produces modifiers → recompute stats

Treat equipment as a set of modifiers that contribute to a derived `PlayerStats` resource/component.

### 1.4 Two interaction “front ends”

- **Proximity (“Press E”)**: ideal for bullet hell / roguelite.
- **Pointer picking (“click”)**: optional; use `bevy_picking` pointer events with observers.[^bevy_picking]

---

## 2) Interactable subsystem

### 2.1 Core components

```rust
use bevy::prelude::*;

/// Marker for things that can be interacted with.
#[derive(Component, Debug, Clone, Copy)]
pub struct Interactable {
    pub kind: InteractableKind,
    pub radius: f32,
    pub prompt: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractableKind {
    PickupItem(ItemId),
    PickupCurrency(CurrencyKind, u32),
    OpenChest(ChestId),
    Door(DoorId),
    Shrine(ShrineId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChestId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DoorId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShrineId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CurrencyKind {
    Gold,
    XP,
}
```

### 2.2 Interaction pipeline messages

Use the naming convention you already adopted (`Requested → Applied → Resolved`).

```rust
use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy)]
pub struct InteractRequested {
    pub actor: Entity,
    pub target: Entity,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct InteractApplied {
    pub actor: Entity,
    pub target: Entity,
    pub outcome: InteractOutcome,
}

#[derive(Debug, Clone, Copy)]
pub enum InteractOutcome {
    PickedUpItem(ItemId, u32),
    PickedUpCurrency(CurrencyKind, u32),
    OpenedChest(ChestId),
    EnteredDoor(DoorId),
    ActivatedShrine(ShrineId),
}

#[derive(Message, Debug, Clone, Copy)]
pub struct InteractResolved {
    pub actor: Entity,
    pub target: Entity,
    pub outcome: InteractOutcome,
}
```

Bevy messages are designed for buffered, pull-based handling and batch processing, written by `MessageWriter` and read using `MessageReader`.[^bevy_message]

---

## 3) Proximity interaction (Press E)

### 3.1 Focus selection resource

```rust
use bevy::prelude::*;

#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct FocusedInteractable(pub Option<Entity>);
```

### 3.2 Selection system: choose best interactable in range

This is deterministic “state now” logic: it uses queries, no events.

```rust
use bevy::prelude::*;

#[derive(Component)]
pub struct Player;

fn update_focus(
    player_q: Query<&Transform, With<Player>>,
    interactables: Query<(Entity, &Transform, &Interactable)>,
    mut focused: ResMut<FocusedInteractable>,
) {
    let Ok(player_tf) = player_q.get_single() else {
        focused.0 = None;
        return;
    };

    let player_pos = player_tf.translation.truncate();

    let mut best: Option<(Entity, f32)> = None;

    for (e, tf, i) in &interactables {
        let d = tf.translation.truncate().distance(player_pos);
        if d <= i.radius {
            match best {
                None => best = Some((e, d)),
                Some((_, best_d)) if d < best_d => best = Some((e, d)),
                _ => {}
            }
        }
    }

    focused.0 = best.map(|(e, _)| e);
}
```

### 3.3 Input → `InteractRequested`

```rust
use bevy::prelude::*;

fn request_interact(
    keys: Res<ButtonInput<KeyCode>>,
    focused: Res<FocusedInteractable>,
    player: Query<Entity, With<Player>>,
    mut out: MessageWriter<InteractRequested>,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }

    let Ok(actor) = player.get_single() else { return; };
    let Some(target) = focused.0 else { return; };

    out.write(InteractRequested { actor, target });
}
```

> Why this works well for bullet hell: the player usually interacts with the nearest pickup; selection is predictable and cheap.

---

## 4) Click interaction (optional): `bevy_picking` observers

If you want clicking/hovering on world objects, `bevy_picking` provides pointer events (`Pointer<Click>`, `Pointer<Over>`, etc.) that bubble up the hierarchy and can be handled with observers attached directly to entities.[^bevy_picking]

### 4.1 Example: clicking a pickup triggers `InteractRequested`

```rust
use bevy::prelude::*;
use bevy::picking::pointer::Pointer;

fn make_pickup_clickable(mut commands: Commands, pickup: Entity, player: Entity) {
    commands.entity(pickup).observe(move |click: On<Pointer<bevy::picking::events::Click>>,
                                         mut w: MessageWriter<InteractRequested>| {
        // click.entity is the clicked entity
        let target = click.entity;
        w.write(InteractRequested { actor: player, target });
    });
}
```

> Recommendation: keep pointer interactions for UI-heavy or mouse-centric games. For bullet hell, proximity is often enough.

---

## 5) Inventory model (data, not entities)

### 5.1 Item identity: `ItemId` and `ItemDef`

Use a stable `ItemId` to refer to item definitions.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemId {
    Medkit,
    DamageUp,
    FireRateUp,
    Shotgun,
    Boots,
}

#[derive(Debug, Clone)]
pub struct ItemDef {
    pub id: ItemId,
    pub name: &'static str,
    pub max_stack: u32,
    pub kind: ItemKind,
}

#[derive(Debug, Clone)]
pub enum ItemKind {
    Consumable(ConsumableEffect),
    Equipment(EquipSlot, Vec<StatModifier>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EquipSlot {
    Weapon,
    Armor,
    Trinket,
}
```

### 5.2 Item registry (resource)

Start hardcoded; later move to data-driven assets.

```rust
use bevy::prelude::*;

#[derive(Resource)]
pub struct ItemRegistry {
    pub defs: std::collections::HashMap<ItemId, ItemDef>,
}

impl Default for ItemRegistry {
    fn default() -> Self {
        use ItemId::*;
        use ItemKind::*;
        use EquipSlot::*;

        let mut defs = std::collections::HashMap::new();

        defs.insert(Medkit, ItemDef {
            id: Medkit,
            name: "Medkit",
            max_stack: 5,
            kind: Consumable(ConsumableEffect::Heal(2)),
        });

        defs.insert(DamageUp, ItemDef {
            id: DamageUp,
            name: "Damage Up",
            max_stack: 99,
            kind: Equipment(Trinket, vec![StatModifier::AddDamage(1)]),
        });

        defs.insert(Shotgun, ItemDef {
            id: Shotgun,
            name: "Shotgun",
            max_stack: 1,
            kind: Equipment(Weapon, vec![StatModifier::AddProjectiles(2)]),
        });

        Self { defs }
    }
}
```

### 5.3 Inventory resource

Two common inventory types:

- **Stack-based** (most roguelites): `Vec<ItemStack>`
- **Instance-based** (unique rolls): `Vec<ItemInstance>`

Start with stacks.

```rust
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemStack {
    pub id: ItemId,
    pub count: u32,
}

#[derive(Resource, Default, Debug)]
pub struct Inventory {
    pub items: Vec<ItemStack>,
    pub gold: u32,
}
```

Resources are singletons in the World and accessed through `Res` and `ResMut`.[^bevy_resource]

---

## 6) Equipment model + derived stats

### 6.1 Equipment resource

```rust
use bevy::prelude::*;

#[derive(Resource, Default, Debug)]
pub struct Equipment {
    pub weapon: Option<ItemId>,
    pub armor: Option<ItemId>,
    pub trinkets: Vec<ItemId>,
}
```

### 6.2 Stat modifiers

```rust
#[derive(Debug, Clone)]
pub enum StatModifier {
    AddDamage(i32),
    MulFireRate(f32),
    AddProjectiles(i32),
    AddMoveSpeed(f32),
    AddMaxHp(i32),
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct PlayerBaseStats {
    pub damage: i32,
    pub fire_rate: f32,
    pub projectiles: i32,
    pub move_speed: f32,
    pub max_hp: i32,
}

impl Default for PlayerBaseStats {
    fn default() -> Self {
        Self { damage: 1, fire_rate: 6.0, projectiles: 1, move_speed: 260.0, max_hp: 5 }
    }
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct PlayerStats {
    pub damage: i32,
    pub fire_rate: f32,
    pub projectiles: i32,
    pub move_speed: f32,
    pub max_hp: i32,
}

impl Default for PlayerStats {
    fn default() -> Self {
        Self { damage: 1, fire_rate: 6.0, projectiles: 1, move_speed: 260.0, max_hp: 5 }
    }
}
```

### 6.3 Recompute stats when equipment changes

```rust
use bevy::prelude::*;

fn recompute_player_stats(
    base: Res<PlayerBaseStats>,
    equip: Res<Equipment>,
    items: Res<ItemRegistry>,
    mut out: ResMut<PlayerStats>,
) {
    // Run only when something changed.
    if !(base.is_changed() || equip.is_changed()) {
        return;
    }

    let mut stats = PlayerStats {
        damage: base.damage,
        fire_rate: base.fire_rate,
        projectiles: base.projectiles,
        move_speed: base.move_speed,
        max_hp: base.max_hp,
    };

    let mut mods: Vec<StatModifier> = Vec::new();

    let mut push_mods = |id: ItemId| {
        if let Some(def) = items.defs.get(&id) {
            if let ItemKind::Equipment(_, m) = &def.kind {
                mods.extend(m.iter().cloned());
            }
        }
    };

    if let Some(w) = equip.weapon { push_mods(w); }
    if let Some(a) = equip.armor { push_mods(a); }
    for t in &equip.trinkets { push_mods(*t); }

    for m in mods {
        match m {
            StatModifier::AddDamage(x) => stats.damage += x,
            StatModifier::MulFireRate(x) => stats.fire_rate *= x,
            StatModifier::AddProjectiles(x) => stats.projectiles += x,
            StatModifier::AddMoveSpeed(x) => stats.move_speed += x,
            StatModifier::AddMaxHp(x) => stats.max_hp += x,
        }
    }

    *out = stats;
}
```

> This is the core of the “equipment affects gameplay” loop. Everything else (shooting, movement) reads `PlayerStats`.

---

---

## 6.4 Examples of equipment modifiers (many patterns)

This section gives you **practical modifier recipes** you can copy into your game.
It covers:

- simple additive modifiers (+damage)
- multiplicative modifiers (*fire rate)
- multi-stat modifiers (weapon bundles)
- conditional modifiers ("low HP" berserk)
- proc modifiers (chance to chain lightning)
- stackable modifiers (trinkets that scale)
- set bonuses

> Design rule: keep `PlayerStats` recomputation deterministic.
>
> - **Static modifiers** live on equipment items.
> - **Conditional/proc modifiers** should be expressed as *temporary buffs* generated by systems.
>   This keeps your stat compute step simple and testable.

---

### A) Additive modifiers (flat adds)

Use these for “immediately feelable” changes.

```rust
// Flat increases are simple and predictable.
StatModifier::AddDamage(1)
StatModifier::AddProjectiles(2)
StatModifier::AddMaxHp(3)
```

**Example item defs**

```rust
use ItemId::*;
use ItemKind::*;
use EquipSlot::*;

defs.insert(DamageUp, ItemDef {
    id: DamageUp,
    name: "Damage Up",
    max_stack: 99,
    kind: Equipment(Trinket, vec![StatModifier::AddDamage(1)]),
});

defs.insert(Boots, ItemDef {
    id: Boots,
    name: "Boots",
    max_stack: 1,
    kind: Equipment(Armor, vec![StatModifier::AddMoveSpeed(35.0)]),
});
```

---

### B) Multiplicative modifiers (scaling)

Use multiplicative modifiers for scaling that stays relevant later.

```rust
StatModifier::MulFireRate(1.15) // +15%
StatModifier::MulFireRate(0.85) // -15% (slow)
```

**Recommended convention**

- Use `MulX(f32)` as a multiplier (1.0 = no change)
- Apply all additive mods first, then multiplicative mods

This avoids surprises like “+1 projectile then *0.5 projectiles”.

---

### C) Multi-stat weapon bundles

Weapons often want multiple effects at once.

```rust
defs.insert(Shotgun, ItemDef {
    id: Shotgun,
    name: "Shotgun",
    max_stack: 1,
    kind: Equipment(Weapon, vec![
        StatModifier::AddProjectiles(4),  // spread pellets
        StatModifier::MulFireRate(0.85),  // slower firing
        StatModifier::AddDamage(1),       // compensate
    ]),
});
```

**Design note:** this approach makes “weapons as equipment” easy without needing separate weapon entities.

---

### D) Conditional modifiers (low HP berserk)

Conditional modifiers should not live directly in the stat compute loop.
Instead, generate **temporary buffs** based on conditions (HP thresholds, room state, etc.).

#### D.1 Add a Buff resource

```rust
use bevy::prelude::*;

#[derive(Resource, Default, Debug)]
pub struct ActiveBuffs {
    pub mods: Vec<StatModifier>,
}
```

#### D.2 Generate buffs from condition (e.g., low HP)

```rust
use bevy::prelude::*;

fn generate_low_hp_berserk_buff(
    mut buffs: ResMut<ActiveBuffs>,
    hp: Query<&Health, With<Player>>,
) {
    let Ok(hp) = hp.get_single() else { return; };

    // Clear and rebuild per frame or per change.
    buffs.mods.clear();

    if hp.hp <= 2 {
        buffs.mods.push(StatModifier::MulFireRate(1.25));
        buffs.mods.push(StatModifier::AddMoveSpeed(50.0));
    }
}
```

#### D.3 Include buffs in `recompute_player_stats`

```rust
fn recompute_player_stats(
    base: Res<PlayerBaseStats>,
    equip: Res<Equipment>,
    items: Res<ItemRegistry>,
    buffs: Res<ActiveBuffs>,
    mut out: ResMut<PlayerStats>,
) {
    if !(base.is_changed() || equip.is_changed() || buffs.is_changed()) {
        return;
    }

    let mut stats = PlayerStats { /* ...copy base... */ ..PlayerStats::default() };

    let mut mods: Vec<StatModifier> = Vec::new();

    // 1) equipment mods
    // (same as earlier: push_mods)

    // 2) buff mods
    mods.extend(buffs.mods.iter().cloned());

    // Apply in stable order
    for m in mods {
        match m {
            StatModifier::AddDamage(x) => stats.damage += x,
            StatModifier::AddProjectiles(x) => stats.projectiles += x,
            StatModifier::AddMoveSpeed(x) => stats.move_speed += x,
            StatModifier::AddMaxHp(x) => stats.max_hp += x,
            StatModifier::MulFireRate(x) => stats.fire_rate *= x,
        }
    }

    *out = stats;
}
```

This keeps “conditions” out of item definitions, and keeps the stat recompute predictable.

---

### E) Proc modifiers (chance-based effects on hit)

Proc effects are best handled by the **messaging architecture**:

- your hit pipeline already exists (collisions → damage messages)
- when a hit is resolved, emit a `ProcRequested` message
- a proc system reads equipment + RNG seed and emits extra effects

#### E.1 Define a proc modifier

Extend `StatModifier` (or create a separate `ProcModifier`) for on-hit procs.
Here’s a minimal pattern using a separate enum so `PlayerStats` stays numeric:

```rust
#[derive(Debug, Clone)]
pub enum ProcModifier {
    ChainLightning { chance: f32, bounces: u8 },
    LifeSteal { chance: f32, heal: i32 },
}

#[derive(Resource, Default, Debug)]
pub struct ActiveProcs {
    pub procs: Vec<ProcModifier>,
}
```

#### E.2 Example item that grants a proc

```rust
// Pseudo-item id
// ItemId::StormRing

defs.insert(ItemId::StormRing, ItemDef {
    id: ItemId::StormRing,
    name: "Storm Ring",
    max_stack: 1,
    kind: ItemKind::Equipment(EquipSlot::Trinket, vec![
        // keep numeric mods here if any
        StatModifier::AddDamage(0),
    ]),
});

// And separately in a proc registry, or attach a ProcModifier in a parallel table.
```

#### E.3 Proc request message

```rust
#[derive(Message, Debug, Clone, Copy)]
pub struct ProcRequested {
    pub attacker: Entity,
    pub victim: Entity,
}
```

Emit `ProcRequested` from your hit resolution system.
Then a proc system decides whether to trigger chain lightning, lifesteal, etc.

> Tip: For determinism in roguelites, use your run seed + message id + attacker entity id to derive RNG.

---

### F) Stackable modifiers (trinkets that scale)

Stackables are easiest when inventory stacks map to modifiers.
Example: each “DamageUp” stack adds +1 damage.

```rust
fn damage_from_stacks(inv: Res<Inventory>, mut buffs: ResMut<ActiveBuffs>) {
    // Add a derived buff based on stacks.
    let stacks = inv.items.iter().find(|s| s.id == ItemId::DamageUp).map(|s| s.count).unwrap_or(0);
    if stacks > 0 {
        buffs.mods.push(StatModifier::AddDamage(stacks as i32));
    }
}
```

This keeps item definition simple and makes balancing easy.

---

### G) Set bonuses

Set bonuses should be computed from equipment combinations.

```rust
fn apply_set_bonuses(equip: Res<Equipment>, mut buffs: ResMut<ActiveBuffs>) {
    // Example set: Boots + Armor = +10% move speed
    let has_boots = equip.armor == Some(ItemId::Boots);
    let has_shotgun = equip.weapon == Some(ItemId::Shotgun);

    if has_boots && has_shotgun {
        buffs.mods.push(StatModifier::AddMoveSpeed(25.0));
    }
}
```

Use the same “buff generation” mechanism so your main stat recompute stays clean.

---

### H) Balancing tips (practical)

- Prefer **additive** early-game and **multiplicative** late-game.
- Clamp extremes (e.g., `fire_rate` min/max) to keep the game readable.
- Bundle downsides with upsides (shotgun: more bullets but slower).

---

## 6.5 Balancing modifiers (practical playbook)

This section turns “modifiers” from a grab-bag of numbers into a **balanceable system**.
The goal is not perfect balance on day 1: it’s a framework that lets you **iterate safely** as content grows.

### 6.5.1 Start with 3 measurable targets

Pick targets you can measure while playtesting and in stress mode:

1. **Time to kill (TTK)** for common enemies at wave N
2. **Time to die (TTD)** for an average player mistake rate
3. **Screen readability** (maximum bullets on screen without becoming noise)

If you already have the stress scene + perf metrics, you can treat “readability” as a hard constraint: a pattern is invalid if players can’t parse it.

---

### 6.5.2 Decide which stats are additive vs multiplicative

A robust rule of thumb:

- **Additive** for early feel and linear growth
  - `+damage`, `+max_hp`, `+projectiles` (small integers)
- **Multiplicative** for scaling that stays relevant late
  - `*fire_rate`, `*move_speed` (small percentages)

Avoid mixing too many multipliers on the same axis unless you also add **caps** or **diminishing returns**.

---

### 6.5.3 Use “budget per rarity” (content scaling)

Give each item a **power budget** based on its rarity/tier.
Then “spend” that budget on modifiers.

Example budgets:

- Common: 1 budget point
- Uncommon: 2
- Rare: 3
- Epic: 4

Example costs:

- `AddDamage(1)` costs 1
- `MulFireRate(1.10)` costs 1
- `AddProjectiles(1)` costs 2 (because projectile count has multiplicative synergy)
- `AddMaxHp(2)` costs 1

This turns balancing into a bookkeeping exercise instead of guesswork.

> Tip: Track budgets in the item registry for debugging:
>
> - Show total budget in the inventory screen
> - Warn if an item exceeds its tier budget

---

### 6.5.4 Watch for multiplicative synergies (the usual culprits)

The most common “break the game” combos are multiplicative stacks across:

- `damage × fire_rate` (DPS explosion)
- `projectiles × damage × fire_rate` (triple synergy)
- `move_speed × invulnerability/dash` (avoidance becomes trivial)

**Rule:** if a stat multiplies another stat’s effect, treat it as higher budget cost or apply a cap.

---

### 6.5.5 Apply caps and floors (game feel + readability)

Caps are not a failure: they are a design tool.
They keep the game readable and prevent one lucky run from trivializing content.

Recommended caps (example):

- `fire_rate`: clamp to `[2.0, 20.0]` shots/s
- `projectiles`: clamp to `[1, 12]`
- `move_speed`: clamp to `[150, 500]`
- `damage`: clamp to `[1, 999]` (or leave uncapped if you have enemy scaling)

#### Example: clamp inside stat recompute

```rust
fn clamp_stats(mut s: PlayerStats) -> PlayerStats {
    s.fire_rate = s.fire_rate.clamp(2.0, 20.0);
    s.projectiles = s.projectiles.clamp(1, 12);
    s.move_speed = s.move_speed.clamp(150.0, 500.0);
    s.damage = s.damage.clamp(1, 999);
    s
}

// In recompute_player_stats:
*out = clamp_stats(stats);
```

---

### 6.5.6 Diminishing returns (when you want stacking but not runaway)

If you *want* many items that buff the same axis, use diminishing returns.

Two common patterns:

#### A) Hyperbolic diminishing returns

Great for “more is better but less and less.”

```rust
fn diminishing_returns(x: f32, k: f32) -> f32 {
    // x >= 0
    // k controls curve steepness
    x / (x + k)
}

// Example: convert additive fire-rate bonus into a multiplier
// bonus_sum = 0.0..infty, returns 0..1
let dr = diminishing_returns(bonus_sum, 3.0);
let mult = 1.0 + 0.8 * dr; // max +80%
```

#### B) Soft cap around a target

Great when you want a “sweet spot” (e.g., 12 shots/s) but allow small overshoot.

```rust
fn soft_cap(value: f32, cap: f32, softness: f32) -> f32 {
    // softness > 0. Higher means gentler approach to cap.
    cap - (cap - value) / (1.0 + ((cap - value).abs() / softness))
}

// Example usage
s.fire_rate = soft_cap(s.fire_rate, 16.0, 4.0).clamp(2.0, 20.0);
```

> Practical advice: start with hard clamps. Add diminishing returns only after you have enough content that stacking becomes a real problem.

---

### 6.5.7 Balance around “DPS per wave” and “HP per wave” scaling

For bullet hell / roguelite, the game usually scales by **waves**.
You have two main knobs:

- Enemy HP scaling
- Enemy bullet density / pattern difficulty scaling

Keep your modifiers readable by ensuring:

- One tier of upgrades ≈ one wave of enemy scaling
- A run with “average” upgrades can reach wave N

This avoids the common pitfall where upgrades are either meaningless or mandatory.

---

### 6.5.8 Testing balance invariants (cheap, effective)

You can write fast unit tests that catch **runaway stats**.

#### Example: “stats never exceed cap for a worst-case loadout”

```rust
#[test]
fn stats_caps_hold_for_worst_case_loadout() {
    let mut equip = Equipment::default();
    equip.weapon = Some(ItemId::Shotgun);
    equip.trinkets = vec![ItemId::DamageUp; 50];

    // Build a fake registry where DamageUp gives +1 damage each stack.
    // Then recompute and assert caps.

    // Assert that fire_rate/projectiles/move_speed remain within clamp.
}
```

#### Example: “DPS doesn’t explode superlinearly”

Define a simple DPS proxy:

```rust
fn dps_proxy(s: PlayerStats) -> f32 {
    s.damage as f32 * s.fire_rate * s.projectiles as f32
}

// In a test, ensure adding one more average upgrade doesn't increase DPS by > X%.
```

These tests don’t guarantee perfect balance, but they prevent disasters.

---

### 6.5.9 Common balancing pitfalls (and fixes)

- **Pitfall:** +projectiles is treated like +damage.
  - **Fix:** charge higher budget or apply a strict cap.
- **Pitfall:** fire rate upgrades compound without limit.
  - **Fix:** clamp and/or diminishing returns.
- **Pitfall:** movement speed makes patterns trivial.
  - **Fix:** cap speed, or scale pattern difficulty with speed.
- **Pitfall:** upgrades feel samey.
  - **Fix:** add tradeoffs (shotgun: more bullets but slower).

---

### 6.5.10 A “starter balance sheet” you can copy

Use these as a baseline and adjust during playtesting:

- Common item: +1 damage OR +10% fire rate OR +1 HP
- Uncommon: +2 damage OR +20% fire rate OR +2 HP
- Rare: +1 projectile OR +30% fire rate OR +3 HP
- Epic: +2 projectiles (cap!) OR strong conditional buff

Then enforce:

- `projectiles <= 12`
- `fire_rate <= 20`
- `move_speed <= 500`

This keeps the game readable and prevents runaway builds.

## 7) Applying interactions: pickups, chests, doors

### 7.1 Router: `InteractRequested` → specialized actions

```rust
use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy)]
pub struct PickupRequested { pub actor: Entity, pub item: ItemId, pub count: u32, pub target: Entity }

#[derive(Message, Debug, Clone, Copy)]
pub struct CurrencyPickupRequested { pub actor: Entity, pub kind: CurrencyKind, pub amount: u32, pub target: Entity }

#[derive(Message, Debug, Clone, Copy)]
pub struct ChestOpenRequested { pub actor: Entity, pub chest: ChestId, pub target: Entity }

fn route_interactions(
    mut req: MessageReader<InteractRequested>,
    interactables: Query<&Interactable>,
    mut pickup: MessageWriter<PickupRequested>,
    mut currency: MessageWriter<CurrencyPickupRequested>,
    mut chest: MessageWriter<ChestOpenRequested>,
) {
    for r in req.read() {
        let Ok(i) = interactables.get(r.target) else { continue; };
        match i.kind {
            InteractableKind::PickupItem(item) => pickup.write(PickupRequested { actor: r.actor, item, count: 1, target: r.target }),
            InteractableKind::PickupCurrency(kind, amount) => currency.write(CurrencyPickupRequested { actor: r.actor, kind, amount, target: r.target }),
            InteractableKind::OpenChest(chest_id) => chest.write(ChestOpenRequested { actor: r.actor, chest: chest_id, target: r.target }),
            _ => {}
        }
    }
}
```

### 7.2 Apply pickup to inventory

```rust
use bevy::prelude::*;

fn apply_item_pickups(
    mut req: MessageReader<PickupRequested>,
    registry: Res<ItemRegistry>,
    mut inv: ResMut<Inventory>,
    mut applied: MessageWriter<InteractApplied>,
    mut commands: Commands,
) {
    for r in req.read() {
        let Some(def) = registry.defs.get(&r.item) else { continue; };

        // Stack merge
        let mut remaining = r.count;
        for stack in &mut inv.items {
            if stack.id == r.item && stack.count < def.max_stack {
                let space = def.max_stack - stack.count;
                let add = remaining.min(space);
                stack.count += add;
                remaining -= add;
                if remaining == 0 { break; }
            }
        }
        if remaining > 0 {
            inv.items.push(ItemStack { id: r.item, count: remaining });
        }

        // Despawn the pickup entity.
        commands.entity(r.target).despawn();

        applied.write(InteractApplied {
            actor: r.actor,
            target: r.target,
            outcome: InteractOutcome::PickedUpItem(r.item, r.count),
        });
    }
}
```

### 7.3 Apply currency pickup

```rust
use bevy::prelude::*;

fn apply_currency_pickups(
    mut req: MessageReader<CurrencyPickupRequested>,
    mut inv: ResMut<Inventory>,
    mut applied: MessageWriter<InteractApplied>,
    mut commands: Commands,
) {
    for r in req.read() {
        match r.kind {
            CurrencyKind::Gold => inv.gold += r.amount,
            CurrencyKind::XP => { /* XP resource here */ }
        }

        commands.entity(r.target).despawn();

        applied.write(InteractApplied {
            actor: r.actor,
            target: r.target,
            outcome: InteractOutcome::PickedUpCurrency(r.kind, r.amount),
        });
    }
}
```

### 7.4 Resolve interactions (VFX/SFX/UI)

This stage is where you emit `SfxEvent`, spawn floating text, etc.
Keep it separate so gameplay stays clean.

```rust
use bevy::prelude::*;

fn resolve_interactions(
    mut applied: MessageReader<InteractApplied>,
    mut resolved: MessageWriter<InteractResolved>,
    mut sfx: MessageWriter<SfxEvent>,
) {
    for a in applied.read() {
        // Example SFX routing.
        match a.outcome {
            InteractOutcome::PickedUpItem(_, _) => sfx.write(SfxEvent::new(SfxId::UiClick)),
            InteractOutcome::PickedUpCurrency(_, _) => sfx.write(SfxEvent::new(SfxId::UiClick)),
            _ => {}
        }

        resolved.write(InteractResolved { actor: a.actor, target: a.target, outcome: a.outcome });
    }
}
```

---

## 8) Using items (consumables) and equipping gear

### 8.1 “Use item” messages

```rust
use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy)]
pub struct ItemUseRequested {
    pub actor: Entity,
    pub item: ItemId,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct ItemUseApplied {
    pub actor: Entity,
    pub item: ItemId,
}
```

### 8.2 Apply consumable effects

```rust
use bevy::prelude::*;

#[derive(Debug, Clone)]
pub enum ConsumableEffect {
    Heal(i32),
}

#[derive(Component)]
pub struct Health { pub hp: i32 }

fn apply_item_use(
    mut req: MessageReader<ItemUseRequested>,
    registry: Res<ItemRegistry>,
    mut inv: ResMut<Inventory>,
    mut hp: Query<&mut Health, With<Player>>,
    mut out: MessageWriter<ItemUseApplied>,
) {
    let Ok(mut health) = hp.get_single_mut() else { return; };

    for r in req.read() {
        let Some(def) = registry.defs.get(&r.item) else { continue; };

        // Ensure item exists in inventory
        let Some(stack_idx) = inv.items.iter().position(|s| s.id == r.item && s.count > 0) else { continue; };

        match &def.kind {
            ItemKind::Consumable(effect) => {
                match effect {
                    ConsumableEffect::Heal(amount) => {
                        health.hp += *amount;
                    }
                }
                // Consume one
                inv.items[stack_idx].count -= 1;
                if inv.items[stack_idx].count == 0 {
                    inv.items.swap_remove(stack_idx);
                }
                out.write(ItemUseApplied { actor: r.actor, item: r.item });
            }
            _ => {
                // Not consumable
            }
        }
    }
}
```

### 8.3 Equip / unequip

```rust
use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy)]
pub struct EquipRequested {
    pub actor: Entity,
    pub item: ItemId,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct UnequipRequested {
    pub actor: Entity,
    pub slot: EquipSlot,
}

fn apply_equip(
    mut req: MessageReader<EquipRequested>,
    registry: Res<ItemRegistry>,
    mut equip: ResMut<Equipment>,
) {
    for r in req.read() {
        let Some(def) = registry.defs.get(&r.item) else { continue; };
        let ItemKind::Equipment(slot, _) = &def.kind else { continue; };

        match slot {
            EquipSlot::Weapon => equip.weapon = Some(r.item),
            EquipSlot::Armor => equip.armor = Some(r.item),
            EquipSlot::Trinket => {
                if !equip.trinkets.contains(&r.item) {
                    equip.trinkets.push(r.item)
                }
            }
        }
    }
}

fn apply_unequip(
    mut req: MessageReader<UnequipRequested>,
    mut equip: ResMut<Equipment>,
) {
    for r in req.read() {
        match r.slot {
            EquipSlot::Weapon => equip.weapon = None,
            EquipSlot::Armor => equip.armor = None,
            EquipSlot::Trinket => {
                equip.trinkets.pop();
            }
        }
    }
}
```

Because `Equipment` is a resource, `recompute_player_stats` will detect `equip.is_changed()` and recompute derived stats.

---

## 9) UI integration (prompt + inventory screen)

### 9.1 Prompt UI from focused interactable

Use the same view-model idea as HUD:

- `FocusedInteractable` (resource) is updated by gameplay
- Prompt UI reads it and displays `Interactable.prompt`

```rust
use bevy::prelude::*;

#[derive(Component)]
struct InteractPromptText;

fn update_interact_prompt(
    focused: Res<FocusedInteractable>,
    interactables: Query<&Interactable>,
    mut q: Query<&mut Text, With<InteractPromptText>>,
) {
    if !focused.is_changed() {
        return;
    }

    let mut text = q.single_mut();

    if let Some(e) = focused.0 {
        if let Ok(i) = interactables.get(e) {
            *text = Text::new(format!("E: {}", i.prompt));
            return;
        }
    }

    *text = Text::new("");
}
```

### 9.2 Inventory UI is a state-owned screen

- Enter `GameState::Inventory`
- Spawn a screen UI root (`DespawnOnExit(Inventory)`) and list items
- On exit, it despawns cleanly

(See your UI chapter for state-scoped UI trees.)

---

---

## 9.3 UI Inventory design (screen UI + view model)

Inventory UI is where systems often become messy because it touches:

- data (inventory, equipment, item definitions)
- interactions (use, equip, drop)
- presentation (icons, tooltips)

To keep this clean, treat inventory as **Screen UI** owned by a state (e.g., `GameState::Inventory`).
This matches the UI Architecture chapter pattern: one root per screen, despawn on state exit.

### 9.3.1 Principles

1. **Inventory UI is state-owned**
   - Spawn on `OnEnter(GameState::Inventory)`
   - Despawn on `OnExit(GameState::Inventory)` via `DespawnOnExit`

2. **UI reads a view model, never raw gameplay queries**
   - Convert `Inventory + Equipment + ItemRegistry` into a UI-friendly snapshot.
   - UI systems update only when the view model changes.

3. **All UI actions emit messages**
   - UI does not mutate inventory/equipment directly.
   - UI emits `ItemUseRequested`, `EquipRequested`, `UnequipRequested`, `DropRequested`.

4. **Update frequency: don’t rebuild all text every frame**
   - Update on change (`resource_changed::<InventoryViewModel>`)
   - For frequent counters, use a timer (5–10 Hz)

---

### 9.3.2 Recommended UI tree structure

A pragmatic layout that scales:

```text
UI:InventoryRoot (InventoryUiRoot, DespawnOnExit(Inventory))
└─ FullscreenDim (Node + BackgroundColor)
   └─ InventoryWindow (Node: panel)
      ├─ HeaderRow (Node)
      │  ├─ TitleText ("Inventory")
      │  ├─ GoldText
      │  └─ CloseHintText ("Esc")
      ├─ ContentRow (Node: flex row)
      │  ├─ LeftColumn (Node: flex column)
      │  │  ├─ EquipmentPanel
      │  │  │  ├─ WeaponSlotButton + Icon
      │  │  │  ├─ ArmorSlotButton + Icon
      │  │  │  └─ TrinketSlots (list)
      │  │  └─ ActionsPanel
      │  │     ├─ UseButton (enabled if consumable selected)
      │  │     ├─ EquipButton (enabled if equippable selected)
      │  │     └─ DropButton
      │  ├─ InventoryGridPanel
      │  │  ├─ ItemSlot[0]
      │  │  ├─ ItemSlot[1]
      │  │  └─ ...
      │  └─ TooltipPanel
      │     ├─ ItemNameText
      │     ├─ RarityText
      │     └─ DescriptionText
      └─ FooterRow (Node)
         └─ DebugSeedOrRunInfoText (optional)
```

**Why this structure works**

- Inventory grid is a dedicated subtree: easy to regenerate if you choose.
- Equipment slots are dedicated nodes with their own markers.
- Tooltip reads selection state and the view model.

---

### 9.3.3 Inventory view model pattern

#### A) Define a view model

The view model should be UI-ready:

- pre-resolved item names
- counts already formatted or raw numbers
- selection state included

```rust
use bevy::prelude::*;

#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct InventoryViewModel {
    pub gold: u32,
    pub slots: Vec<InventorySlotVm>,
    pub equipment: EquipmentVm,
    pub selected: Option<SlotRef>,
    pub tooltip: Option<TooltipVm>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventorySlotVm {
    pub item: ItemId,
    pub name: String,
    pub count: u32,
    pub is_equippable: bool,
    pub is_consumable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquipmentVm {
    pub weapon: Option<ItemId>,
    pub armor: Option<ItemId>,
    pub trinkets: Vec<ItemId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotRef {
    InventoryIndex(usize),
    Weapon,
    Armor,
    Trinket(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TooltipVm {
    pub name: String,
    pub lines: Vec<String>,
}
```

#### B) Build the view model from resources

```rust
use bevy::prelude::*;

fn build_inventory_view_model(
    inv: Res<Inventory>,
    equip: Res<Equipment>,
    registry: Res<ItemRegistry>,
    mut vm: ResMut<InventoryViewModel>,
) {
    if !(inv.is_changed() || equip.is_changed()) {
        return;
    }

    let mut slots = Vec::with_capacity(inv.items.len());
    for s in &inv.items {
        let def = registry.defs.get(&s.id);
        let name = def.map(|d| d.name).unwrap_or("Unknown").to_string();
        let (is_consumable, is_equippable) = match def.map(|d| &d.kind) {
            Some(ItemKind::Consumable(_)) => (true, false),
            Some(ItemKind::Equipment(_, _)) => (false, true),
            _ => (false, false),
        };

        slots.push(InventorySlotVm {
            item: s.id,
            name,
            count: s.count,
            is_equippable,
            is_consumable,
        });
    }

    vm.gold = inv.gold;
    vm.slots = slots;
    vm.equipment = EquipmentVm {
        weapon: equip.weapon,
        armor: equip.armor,
        trinkets: equip.trinkets.clone(),
    };

    // Tooltip/selection can be computed here or in UI logic.
}
```

---

### 9.3.4 UI updates: patch, don’t rebuild (when possible)

There are two reasonable strategies:

#### Strategy 1: Patch existing UI nodes (recommended)

- Spawn a fixed set of UI slot entities at setup.
- Update only text/icon for the slots that changed.

This is best when:

- your grid size is fixed (e.g., 30 slots)
- you want stable entity IDs

#### Strategy 2: Rebuild the grid subtree (acceptable early)

- Despawn all slot children
- Recreate them from the view model

This is OK when:

- inventory sizes are small (<= 50)
- inventory UI is not open during intense gameplay

If you rebuild, **only rebuild when view model changes**, not every frame.

---

### 9.3.5 UI interactions → messages (examples)

The inventory UI should emit messages; gameplay systems apply them.

#### A) Selecting an item

Selection is UI state. Keep it in `InventoryViewModel.selected` or a separate `InventoryUiState` resource.

```rust
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct InventoryUiState {
    pub selected: Option<SlotRef>,
}
```

#### B) Press “Use” button

```rust
fn on_use_clicked(
    ui: Res<InventoryUiState>,
    vm: Res<InventoryViewModel>,
    mut out: MessageWriter<ItemUseRequested>,
    player: Query<Entity, With<Player>>,
) {
    let Ok(actor) = player.get_single() else { return; };
    let Some(SlotRef::InventoryIndex(i)) = ui.selected else { return; };
    let Some(slot) = vm.slots.get(i) else { return; };

    if slot.is_consumable {
        out.write(ItemUseRequested { actor, item: slot.item });
    }
}
```

#### C) Press “Equip” button

```rust
fn on_equip_clicked(
    ui: Res<InventoryUiState>,
    vm: Res<InventoryViewModel>,
    mut out: MessageWriter<EquipRequested>,
    player: Query<Entity, With<Player>>,
) {
    let Ok(actor) = player.get_single() else { return; };
    let Some(SlotRef::InventoryIndex(i)) = ui.selected else { return; };
    let Some(slot) = vm.slots.get(i) else { return; };

    if slot.is_equippable {
        out.write(EquipRequested { actor, item: slot.item });
    }
}
```

#### D) Unequip from equipment slot

```rust
fn on_unequip_weapon(
    mut out: MessageWriter<UnequipRequested>,
    player: Query<Entity, With<Player>>,
) {
    let Ok(actor) = player.get_single() else { return; };
    out.write(UnequipRequested { actor, slot: EquipSlot::Weapon });
}
```

---

### 9.3.6 Tooltip content examples

Tooltips should be generated from `ItemDef` + modifiers.

```rust
fn make_tooltip(def: &ItemDef) -> TooltipVm {
    let mut lines = vec![];

    match &def.kind {
        ItemKind::Consumable(_) => lines.push("Consumable".to_string()),
        ItemKind::Equipment(slot, mods) => {
            lines.push(format!("Slot: {:?}", slot));
            for m in mods {
                lines.push(format!("- {:?}", m));
            }
        }
    }

    TooltipVm { name: def.name.to_string(), lines }
}
```

For better UX, replace debug formatting with human-friendly strings (e.g., “+15% Fire Rate”).

---

### 9.3.7 Tests for inventory UI

#### Test A) View model updates when inventory changes

```rust
#[test]
fn inventory_view_model_updates_on_inventory_change() {
    let mut world = World::new();
    world.insert_resource(Inventory { items: vec![], gold: 0 });
    world.insert_resource(Equipment::default());
    world.insert_resource(ItemRegistry::default());
    world.insert_resource(InventoryViewModel::default());

    // Initial build
    world.run_system_once(build_inventory_view_model).unwrap();
    assert_eq!(world.resource::<InventoryViewModel>().gold, 0);

    // Mutate inventory
    world.resource_mut::<Inventory>().gold = 10;
    world.resource_mut::<Inventory>().items.push(ItemStack { id: ItemId::Medkit, count: 2 });

    world.run_system_once(build_inventory_view_model).unwrap();
    let vm = world.resource::<InventoryViewModel>();
    assert_eq!(vm.gold, 10);
    assert_eq!(vm.slots.len(), 1);
    assert_eq!(vm.slots[0].count, 2);
}
```

#### Test B) UI action emits correct message

A full UI click simulation is heavy. Instead, test the “action handler” systems by seeding UI state and checking the message queue.

```rust
#[test]
fn clicking_use_emits_item_use_requested() {
    let mut world = World::new();
    world.add_message::<ItemUseRequested>();

    world.spawn(Player);
    world.insert_resource(InventoryUiState { selected: Some(SlotRef::InventoryIndex(0)) });
    world.insert_resource(InventoryViewModel {
        gold: 0,
        slots: vec![InventorySlotVm {
            item: ItemId::Medkit,
            name: "Medkit".into(),
            count: 1,
            is_equippable: false,
            is_consumable: true,
        }],
        equipment: EquipmentVm { weapon: None, armor: None, trinkets: vec![] },
        selected: None,
        tooltip: None,
    });

    world.run_system_once(on_use_clicked).unwrap();

    // Assert one message exists by reading with a cursor.
    let mut reader = world.get_resource_mut::<bevy::ecs::message::Messages<ItemUseRequested>>().unwrap().get_cursor();
    let msgs = reader.read(world.resource::<bevy::ecs::message::Messages<ItemUseRequested>>()).collect::<Vec<_>>();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].item, ItemId::Medkit);
}
```

(If you prefer, you can test message counts using `MessageReader` inside a system, like in the earlier message backlog chapter.)

---

### 9.3.8 Recommended implementation order

1) Build `InventoryViewModel`
2) Spawn Inventory UI root + static layout (panels)
3) Populate the grid from view model
4) Add selection + tooltip
5) Add Use/Equip/Unequip actions that emit messages
6) Add drag-and-drop later (optional)

---

## 9.4 Drag-and-drop UI (inventory grid)

Drag-and-drop is optional, but it’s the most common “inventory UI” expectation.
If you do it without a plan it becomes brittle, because it mixes:

- pointer input
- hover state
- selection state
- swapping data
- equipment constraints

The clean approach is:

1) Use **picking pointer events** for drag state (UI nodes are pickable through Bevy’s picking backend). citeturn58search421turn58search425
2) Convert pointer drag/drop into **messages** like `MoveItemRequested` / `EquipRequested`. citeturn53search407turn53search411
3) Apply those messages in gameplay/inventory systems (data-only), then rebuild/patch the UI from the view model.

### 9.4.1 What events you’ll use

`bevy_picking` defines a full set of dragging/dropping events:

- `DragStart`, `Drag`, `DragEnd`
- `DragEnter`, `DragOver`, `DragLeave`
- `DragDrop`

and notes they can be received with **Observers or MessageReaders**, with bubbling up the entity hierarchy. citeturn58search429turn58search425

The `Drag` event provides `delta` and `distance` in **screen pixels**, which is perfect for moving a “ghost icon” UI node. citeturn58search430

---

### 9.4.2 Data model: drag state resource

Keep transient drag state out of `InventoryViewModel`.
Treat it as UI-only state.

```rust
use bevy::prelude::*;

#[derive(Resource, Default, Debug, Clone)]
pub struct DragState {
    pub dragging: Option<DragPayload>,
    pub ghost_entity: Option<Entity>,
    pub over: Option<DropTarget>,
}

#[derive(Debug, Clone)]
pub struct DragPayload {
    pub from: SlotRef,
    pub item: ItemId,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    Slot(SlotRef),
    Trash,
}
```

---

### 9.4.3 UI markers: source slots and drop targets

Attach markers to slot UI entities so observers know what a node represents.

```rust
use bevy::prelude::*;

/// Attached to each inventory slot UI element.
#[derive(Component, Debug, Clone, Copy)]
pub struct UiInventorySlot {
    pub index: usize,
}

/// Attached to weapon/armor slots.
#[derive(Component, Debug, Clone, Copy)]
pub enum UiEquipSlot {
    Weapon,
    Armor,
    Trinket(usize),
}

/// Attached to a trash bin drop zone.
#[derive(Component)]
pub struct UiTrashDropZone;
```

---

### 9.4.4 Messages: drag/drop translates to data mutations

Your UI should emit messages. These are processed by inventory systems, not UI.

```rust
use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy)]
pub struct MoveItemRequested {
    pub from: SlotRef,
    pub to: SlotRef,
    pub amount: u32,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct DropRequested {
    pub from: SlotRef,
    pub amount: u32,
}
```

---

### 9.4.5 The “ghost icon” pattern

During drag, create a UI node that follows the pointer.
This avoids fighting layout systems and keeps the dragged slot stable.

```rust
use bevy::prelude::*;

#[derive(Component)]
pub struct DragGhost;

fn spawn_drag_ghost(mut commands: Commands, item_name: &str) -> Entity {
    commands
        .spawn((
            DragGhost,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                // Ensure it's drawn above most UI.
                z_index: ZIndex::Global(10),
                ..default()
            },
            // Placeholder: show name; later use ImageNode icon.
            Text::new(item_name.to_string()),
            TextFont { font_size: 18.0, ..default() },
        ))
        .id()
}

fn move_drag_ghost(mut q: Query<&mut Node, With<DragGhost>>, mut drag: bevy::prelude::MessageReader<bevy::picking::pointer::Pointer<bevy::picking::events::Drag>>) {
    // Alternative: handle this using observers (recommended).
    for e in drag.read() {
        // e.event.delta is in pixels; adjust left/top accordingly.
        let mut node = q.single_mut();
        // This is pseudo: you'd track absolute position in DragState.
        // node.left = Val::Px(...);
        // node.top  = Val::Px(...);
        let _ = e;
        let _ = &mut *node;
    }
}
```

> In practice, it’s cleaner to update the ghost from `On<Pointer<Drag>>` observers attached to the drag source.

---

### 9.4.6 Observer wiring (recommended)

Observers keep drag logic close to the UI entities, and pointer events bubble to parents.
`bevy_picking` explicitly highlights observers as an expressive approach for pointer interactions. citeturn58search425turn58search429

Below is a conceptual wiring strategy:

- Each slot observes `DragStart` to begin dragging
- Slots observe `DragEnter` / `DragLeave` to show highlight
- Drop targets observe `DragDrop` to emit `MoveItemRequested` / `EquipRequested`
- Drag source observes `DragEnd` to cleanup ghost

#### A) Begin drag on `DragStart`

```rust
use bevy::prelude::*;
use bevy::picking::pointer::Pointer;
use bevy::picking::events::{DragStart, DragEnd, Drag, DragDrop, DragEnter, DragLeave};

fn slot_begin_drag(
    start: On<Pointer<DragStart>>,
    slot: Query<&UiInventorySlot>,
    vm: Res<InventoryViewModel>,
    registry: Res<ItemRegistry>,
    mut drag_state: ResMut<DragState>,
    mut commands: Commands,
) {
    let Ok(slot) = slot.get(start.entity) else { return; };
    let Some(slot_vm) = vm.slots.get(slot.index) else { return; };

    let def = registry.defs.get(&slot_vm.item);
    let name = def.map(|d| d.name).unwrap_or("Item");

    drag_state.dragging = Some(DragPayload {
        from: SlotRef::InventoryIndex(slot.index),
        item: slot_vm.item,
        count: slot_vm.count,
    });

    let ghost = commands
        .spawn((
            DragGhost,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                z_index: ZIndex::Global(10),
                ..default()
            },
            Text::new(name.to_string()),
            TextFont { font_size: 18.0, ..default() },
        ))
        .id();

    drag_state.ghost_entity = Some(ghost);

    // Optional: stop bubbling to avoid parent handlers.
    // start.propagate(false);
}
```

#### B) Move ghost during `Drag`

`Drag.delta` is in screen pixels. citeturn58search430

```rust
fn slot_drag_move(
    drag: On<Pointer<Drag>>,
    mut drag_state: ResMut<DragState>,
    mut nodes: Query<&mut Node>,
) {
    let Some(ghost) = drag_state.ghost_entity else { return; };
    let Ok(mut node) = nodes.get_mut(ghost) else { return; };

    // Move the ghost by the pixel delta.
    // If you want absolute positioning, store current px coords in DragState.
    // Here we assume node.left/top are Px and we increment them.
    let dx = drag.event.delta.x;
    let dy = drag.event.delta.y;

    // NOTE: y axis in screen pixels is top->bottom.
    // For UI Node absolute positioning, this is usually fine.
    let cur_left = match node.left { Val::Px(v) => v, _ => 0.0 };
    let cur_top  = match node.top  { Val::Px(v) => v, _ => 0.0 };
    node.left = Val::Px(cur_left + dx);
    node.top  = Val::Px(cur_top + dy);
}
```

#### C) Highlight drop target on enter/leave

```rust
fn slot_drag_enter(_enter: On<Pointer<DragEnter>>, mut bg: Query<&mut BackgroundColor>) {
    if let Ok(mut c) = bg.get_mut(_enter.entity) {
        c.0 = Color::srgb(0.2, 0.4, 0.2);
    }
}

fn slot_drag_leave(_leave: On<Pointer<DragLeave>>, mut bg: Query<&mut BackgroundColor>) {
    if let Ok(mut c) = bg.get_mut(_leave.entity) {
        c.0 = Color::srgb(0.1, 0.1, 0.1);
    }
}
```

#### D) Drop onto another inventory slot → `MoveItemRequested`

```rust
fn slot_drop(
    drop: On<Pointer<DragDrop>>,
    slot: Query<&UiInventorySlot>,
    mut drag_state: ResMut<DragState>,
    mut out: MessageWriter<MoveItemRequested>,
) {
    let Some(payload) = drag_state.dragging.take() else { return; };
    let Ok(to) = slot.get(drop.entity) else { return; };

    out.write(MoveItemRequested {
        from: payload.from,
        to: SlotRef::InventoryIndex(to.index),
        amount: 1,
    });
}
```

#### E) Drop onto equipment slot → `EquipRequested`

This is a nice example of UI-only logic staying thin: it merely emits the equip request.

```rust
fn equip_drop(
    drop: On<Pointer<DragDrop>>,
    equip_slot: Query<&UiEquipSlot>,
    drag_state: Res<DragState>,
    mut out: MessageWriter<EquipRequested>,
    player: Query<Entity, With<Player>>,
) {
    let Ok(actor) = player.get_single() else { return; };
    let Ok(_slot) = equip_slot.get(drop.entity) else { return; };

    let Some(payload) = drag_state.dragging.as_ref() else { return; };
    out.write(EquipRequested { actor, item: payload.item });
}
```

#### F) Cleanup on drag end

```rust
fn slot_drag_end(
    _end: On<Pointer<DragEnd>>,
    mut drag_state: ResMut<DragState>,
    mut commands: Commands,
) {
    if let Some(ghost) = drag_state.ghost_entity.take() {
        commands.entity(ghost).despawn();
    }
    drag_state.dragging = None;
    drag_state.over = None;
}
```

---

### 9.4.7 Applying `MoveItemRequested` in data code

This system runs outside UI.
It modifies `Inventory` (and later Equipment) deterministically.

```rust
use bevy::prelude::*;

fn apply_move_item(
    mut req: MessageReader<MoveItemRequested>,
    mut inv: ResMut<Inventory>,
) {
    for r in req.read() {
        let (SlotRef::InventoryIndex(from), SlotRef::InventoryIndex(to)) = (r.from, r.to) else {
            continue;
        };

        if from >= inv.items.len() || to >= inv.items.len() || from == to {
            continue;
        }

        // Simple swap for now.
        inv.items.swap(from, to);
    }
}
```

> Real inventory UX often needs “split stack”, “move one”, “merge if same item”, etc.
> Implement those as additional messages or extra fields on `MoveItemRequested`.

---

### 9.4.8 Practical UX features (optional)

- **Split stacks**: hold Shift while dragging → move half
- **Quick equip**: double click (Pointer<Click>)
- **Drop outside**: dropping onto “world” spawns a pickup entity

All of these still follow the same rule: UI emits messages; gameplay applies.

---

### 9.4.9 Testing drag-and-drop

Full pointer simulation is heavy.
Instead, test invariants at the message layer:

- `MoveItemRequested` swaps/merges correctly
- `EquipRequested` equips correctly and recompute stats triggers

Then add a single “smoke test” for UI wiring (optional) once everything works.

## 10) Common design choices (with examples)

### 10.1 “Items as entities” vs “items as data”

**Use items as data (recommended)**

- inventory is a resource
- equipment is a resource
- item pickups are world entities only while on ground

**Use items as entities** when:

- each item has complex per-instance behavior (durability, sockets, randomized rolls)
- items can exist and act independently in the world

Hybrid approach:

- keep inventory as data
- store per-instance rolls in a small `ItemInstance { id, seed, affixes }`

### 10.2 Chests as generators

A chest doesn’t “contain entities” — it produces `PickupSpawnRequested` messages.

```rust
#[derive(Message, Debug, Clone, Copy)]
pub struct PickupSpawnRequested {
    pub at: Vec2,
    pub item: ItemId,
    pub count: u32,
}
```

Then a pickup spawner system creates pickup entities at that position.

---

## 11) Tests (high value, low effort)

### 11.1 Test: interacting affects only the target

```rust
use bevy::prelude::*;

#[derive(Component)] struct Player;
#[derive(Component)] struct Neutral;

#[test]
fn interacting_does_not_despawn_neutral_entities() {
    let mut app = App::new();

    app.add_message::<InteractRequested>();
    app.add_message::<PickupRequested>();
    app.add_message::<InteractApplied>();

    app.init_resource::<Inventory>();
    app.init_resource::<ItemRegistry>();

    // Systems
    app.add_systems(Update, (route_interactions, apply_item_pickups));

    let player = app.world_mut().spawn(Player).id();
    let neutral = app.world_mut().spawn(Neutral).id();

    let pickup = app.world_mut().spawn((
        Interactable { kind: InteractableKind::PickupItem(ItemId::Medkit), radius: 1.0, prompt: "Pick up" },
        Transform::default(),
    )).id();

    app.world_mut().write_message(InteractRequested { actor: player, target: pickup });
    app.update();

    assert!(app.world().get_entity(neutral).is_some(), "neutral despawned unexpectedly");
}
```

### 11.2 Test: pickup adds to inventory

```rust
use bevy::prelude::*;

#[test]
fn pickup_adds_item_stack() {
    let mut app = App::new();

    app.add_message::<PickupRequested>();
    app.add_message::<InteractApplied>();

    app.init_resource::<Inventory>();
    app.init_resource::<ItemRegistry>();

    app.add_systems(Update, apply_item_pickups);

    let player = app.world_mut().spawn(Player).id();
    let pickup = app.world_mut().spawn_empty().id();

    app.world_mut().write_message(PickupRequested { actor: player, item: ItemId::Medkit, count: 2, target: pickup });
    app.update();

    let inv = app.world().resource::<Inventory>();
    assert!(inv.items.iter().any(|s| s.id == ItemId::Medkit && s.count == 2));
}
```

### 11.3 Test: equip changes derived stats

```rust
use bevy::prelude::*;

#[test]
fn equipping_item_recomputes_stats() {
    let mut app = App::new();

    app.init_resource::<ItemRegistry>();
    app.init_resource::<Equipment>();
    app.init_resource::<PlayerBaseStats>();
    app.init_resource::<PlayerStats>();

    app.add_message::<EquipRequested>();
    app.add_systems(Update, (apply_equip, recompute_player_stats));

    let player = app.world_mut().spawn(Player).id();
    let _ = player;

    // Equip Shotgun => +2 projectiles
    app.world_mut().write_message(EquipRequested { actor: Entity::PLACEHOLDER, item: ItemId::Shotgun });
    app.update();

    let stats = *app.world().resource::<PlayerStats>();
    assert!(stats.projectiles >= 3);
}
```

---

## 12) Performance notes

- Focus selection is O(N interactables). Keep N small or use a spatial index later.
- Use `MessageWriter::write_batch` if you emit many pickups at once (e.g., chest explosion of loot).[^bevy_message_writer]
- Keep UI prompt updates gated on change (`focused.is_changed()`), not every frame.

---

## 13) Recommended implementation order

1. Proximity focus selection + prompt UI
2. Pickup → Inventory
3. Equip → Derived stats recompute
4. Consumables
5. Chest as loot generator
6. Doors / shrines
7. Optional picking click interactions

---

## References

[^bevy_message]: Bevy `Message` trait docs (buffered pull-based messages, MessageWriter/MessageReader): <https://docs.rs/bevy/latest/bevy/prelude/trait.Message.html>
[^bevy_message_writer]: Bevy `MessageWriter` docs (write, write_batch, concurrency notes): <https://docs.rs/bevy_ecs/latest/bevy_ecs/message/struct.MessageWriter.html>
[^bevy_resource]: Bevy `Resource` trait docs (singleton world data accessed via Res/ResMut): <https://docs.rs/bevy_ecs/latest/bevy_ecs/resource/trait.Resource.html>
[^bevy_observers_example]: Bevy official example: Observers (events/EntityEvents and observation patterns): <https://bevy.org/examples/ecs-entity-component-system/observers/>
[^bevy_picking]: Bevy picking crate docs (pointer events, bubbling, observer-friendly interaction): <https://docs.rs/bevy/latest/bevy/picking/index.html>
