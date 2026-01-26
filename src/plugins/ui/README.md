# UI Architecture (Screen UI vs HUD vs Debug Overlays)

This chapter organizes UI as a **first-class subsystem**:

- **Screen UI** (menus, settings, game over) lives under state-scoped UI trees.
- **HUD** lives with gameplay and is hidden or disabled while paused.
- **Debug overlays** (FPS, counts, perf toggles) are always available in dev builds and should not couple to gameplay.

Bevy’s UI is built by spawning entities with components like `Node` and `Text` and letting the UI system lay them out (Flexbox/CSS Grid).[^bevy_ui_crate]
The official UI text example demonstrates creating UI text and updating it in a system (e.g., FPS text), which is the pattern we’ll refine here (update only when needed).[^bevy_ui_text_example]

---

## 1) UI taxonomy (three layers)

### 1.1 Screen UI (state-owned)

- Exists only in a specific `GameState` (e.g., `MainMenu`, `Paused`, `GameOver`).
- Spawns on `OnEnter(state)`.
- Despawns automatically on `OnExit(state)` using state-scoped despawn markers.

Bevy states provide `OnEnter`/`OnExit` schedules for setup/teardown and run-conditions like `in_state` for steady-state updates.[^bevy_state_docs]
Bevy’s state-scoped utilities provide `DespawnOnExit<S>` / `DespawnOnEnter<S>` to bind entity lifetime to state transitions.[^bevy_state_scoped]

### 1.2 HUD (gameplay-owned)

- Exists during gameplay (`InGame`) and is typically hidden while paused.
- Driven by gameplay data via a **UI view model** resource.
- Updates at low frequency and/or only when data changes.

### 1.3 Debug overlays (dev-owned)

- Always available (or toggled by a dev flag).
- Uses diagnostics / counters and does not mutate gameplay state.
- Should be cheap and safe to disable.

---

## 2) State-scoped UI trees (screen UI)

### 2.1 The pattern

For each screen state:

- Create a **single root entity** for that UI tree.
- Tag it with `DespawnOnExit(GameState::ThatScreen)`.
- Keep screen logic in one plugin module to avoid cross-state coupling.

`DespawnOnExit` is explicitly defined as “removed when the world’s state no longer matches” the value.[^bevy_state_scoped]

### 2.2 Example: Main menu UI tree

```rust
use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;

use crate::common::state::GameState;

#[derive(Component)]
pub struct MainMenuUiRoot;

pub fn menu_ui_plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
        .add_systems(Update, menu_ui_input.run_if(in_state(GameState::MainMenu)));
}

fn spawn_main_menu(mut commands: Commands) {
    commands
        .spawn((
            MainMenuUiRoot,
            Name::new("UI:MainMenu"),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            DespawnOnExit(GameState::MainMenu),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Press Enter to Start"),
                TextFont { font_size: 48.0, ..default() },
            ));
        });
}

fn menu_ui_input(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<GameState>>) {
    if keys.just_pressed(KeyCode::Enter) {
        next.set(GameState::InGame);
    }
}
```

Bevy’s UI is created by spawning UI entities (e.g., `Node`, `Text`) and configured via components; the UI system handles layout and transforms.[^bevy_ui_crate]

---

---

## 2.3 Example UI tree structures (recommended hierarchy)

A clear UI hierarchy makes three things easier:

- **state teardown** (one root per screen)
- **targeted queries** (marker components on key nodes)
- **performance** (update only the leaf nodes that change)

Below are example UI trees for **Screen UI**, **HUD**, and **Debug overlays**.
The shapes are intentionally similar so your team can navigate UI code quickly.

### A) Screen UI: `MainMenu` (state-owned)

```text
UI:MainMenuRoot (MainMenuUiRoot, DespawnOnExit(MainMenu))
└─ FullscreenContainer (Node: 100% x 100%, center)
   ├─ TitleText (Text)
   ├─ Spacer (Node)
   ├─ ButtonsColumn (Node: flex column)
   │  ├─ StartButtonRoot (Node)
   │  │  └─ StartButtonText (Text)
   │  ├─ SettingsButtonRoot (Node)
   │  │  └─ SettingsButtonText (Text)
   │  └─ QuitButtonRoot (Node)
   │     └─ QuitButtonText (Text)
   └─ FooterText (Text: seed/version/build)
```

**Notes**

- The *only* entity tagged with `DespawnOnExit(MainMenu)` should be the **root**.
  Children will be despawned as part of the hierarchy.
- Put marker components on nodes you want to query/update (`TitleText`, `FooterText`, etc.).

### B) Screen UI: `Paused` overlay (state-owned)

```text
UI:PauseRoot (PauseUiRoot, DespawnOnExit(Paused))
└─ DimBackground (Node + BackgroundColor)
   └─ PauseCard (Node)
      ├─ PauseTitleText (Text)
      ├─ Spacer (Node)
      ├─ ResumeHintText (Text)
      └─ ControlsHintText (Text)
```

**Notes**

- The pause overlay can coexist with the HUD (see HUD rules below).
- If you want a blur/FX, keep it in render layers / post-processing, not in gameplay UI code.

### C) Gameplay HUD (gameplay-owned)

```text
UI:HudRoot (HudRoot, DespawnOnExit(InGame))
├─ TopLeftPanel (Node: absolute top-left)
│  ├─ HpText (HudHpText)
│  ├─ AmmoText (HudAmmoText)
│  └─ WaveText (HudWaveText)
└─ BottomLeftPanel (Node: absolute bottom-left)
   ├─ Ability1Text
   └─ Ability2Text
```

**Notes**

- Use the **HudViewModel** pattern: gameplay writes one resource; HUD reads it.
- Update only text leaves on change (`resource_changed::<HudViewModel>`).

### D) Debug overlay (dev-owned)

```text
UI:DebugRoot (DebugOverlayRoot)
└─ DebugPanel (Node: absolute top-right)
   ├─ FpsText
   ├─ EntityCountText
   ├─ BulletCountText
   ├─ CollisionBacklogText
   └─ StressConfigText
```

**Notes**

- Keep debug overlay independent: it reads diagnostics/resources but does not mutate gameplay state.
- If you have lots of debug metrics, update at a fixed cadence (e.g., 5–10 Hz) rather than every frame.

## 3) HUD architecture (gameplay UI)

### 3.1 HUD lifecycle rules

- Spawn HUD on `OnEnter(GameState::InGame)` and despawn on `OnExit(GameState::InGame)`.
- If you want to keep HUD entities alive across `Paused`, hide them (visibility) or gate update systems.

Using `in_state(...)` run conditions is the standard way to restrict systems to a given state.[^bevy_state_docs]

### 3.2 HUD View Model pattern (resource-driven)

The HUD should not pull gameplay components directly.
Instead:

1) Gameplay produces a **view model** resource.
2) HUD systems read the view model and update UI.

Resources are “singleton-like” data stored in the `World` and accessed via `Res`/`ResMut`.[^bevy_resource_module]

```rust
use bevy::prelude::*;

/// A stable, UI-friendly snapshot of gameplay state.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct HudViewModel {
    pub hp: i32,
    pub ammo: i32,
    pub wave: i32,
}
```

### 3.3 Gameplay writes the view model

```rust
use bevy::prelude::*;

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Health(i32);

#[derive(Component)]
struct Ammo(i32);

fn update_hud_view_model(
    player: Query<(&Health, &Ammo), With<Player>>,
    mut vm: ResMut<HudViewModel>,
) {
    if let Ok((hp, ammo)) = player.get_single() {
        // Update the view model; UI will react.
        vm.hp = hp.0;
        vm.ammo = ammo.0;
    }
}
```

### 3.4 HUD UI reads the view model (update only on change)

**Rule:** don’t rebuild text every frame.
Update when the view model changes.

The official Bevy UI text example demonstrates updating UI text in a system; we refine it by adding change-based gating.[^bevy_ui_text_example]

```rust
use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;

use crate::common::state::GameState;

#[derive(Component)]
struct HudRoot;

#[derive(Component)]
struct HudText;

pub fn hud_ui_plugin(app: &mut App) {
    app.init_resource::<HudViewModel>()
        .add_systems(OnEnter(GameState::InGame), spawn_hud)
        .add_systems(OnExit(GameState::InGame), despawn_hud_optional)
        // Only run HUD updates while in game AND when the view model changed.
        .add_systems(
            Update,
            update_hud_text
                .run_if(in_state(GameState::InGame))
                .run_if(resource_changed::<HudViewModel>),
        );
}

fn spawn_hud(mut commands: Commands) {
    commands
        .spawn((
            HudRoot,
            Name::new("UI:HUD"),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                left: Val::Px(8.0),
                ..default()
            },
            DespawnOnExit(GameState::InGame),
        ))
        .with_children(|p| {
            p.spawn((
                HudText,
                Text::new("HP: --  AMMO: --  WAVE: --"),
                TextFont { font_size: 24.0, ..default() },
            ));
        });
}

fn update_hud_text(vm: Res<HudViewModel>, mut q: Query<&mut Text, With<HudText>>) {
    let mut text = q.single_mut();
    *text = Text::new(format!("HP: {}  AMMO: {}  WAVE: {}", vm.hp, vm.ammo, vm.wave));
}

fn despawn_hud_optional() {
    // Usually unnecessary because of DespawnOnExit.
}
```

> Note: `resource_changed::<T>` is a run-condition pattern (often used with state conditions) to prevent running systems when nothing changed.

---

## 4) Paused state: hiding or freezing the HUD

Two common approaches:

### Option A: Gate HUD updates while paused

- Keep HUD spawned.
- Stop updating it while paused.

This is already handled by `run_if(in_state(GameState::InGame))` which uses the state run condition model.[^bevy_state_docs]

### Option B: Toggle visibility (HUD hidden on `Paused`)

```rust
use bevy::prelude::*;

fn hide_hud_on_pause(mut q: Query<&mut Visibility, With<HudRoot>>) {
    for mut v in &mut q {
        *v = Visibility::Hidden;
    }
}

fn show_hud_on_resume(mut q: Query<&mut Visibility, With<HudRoot>>) {
    for mut v in &mut q {
        *v = Visibility::Visible;
    }
}
```

Use `OnEnter(GameState::Paused)` / `OnExit(GameState::Paused)` to drive these.
Bevy states explicitly support `OnEnter` and `OnExit` schedules for this purpose.[^bevy_state_docs]

---

## 5) Debug overlays (dev-only UI)

You already have a debug HUD.
The architectural rules are:

- treat debug overlays like a separate plugin
- keep them behind a toggle
- keep them independent from gameplay state mutation

Bevy’s UI text example shows FPS display using diagnostics; this is an appropriate pattern for debug overlays.[^bevy_ui_text_example]

---

## 6) Update frequency rules (avoid UI perf traps)

### 6.1 Rule: update on change

- Prefer `resource_changed::<HudViewModel>` for HUD.
- Prefer change detection / infrequent timers for debug text.

### 6.2 Rule: avoid large string allocations in hot loops

- If text changes frequently, store numeric fields and format less often.
- If you must update every frame (FPS), keep the string small.

### 6.3 Rule: one root per UI subsystem

- One `MainMenuUiRoot`
- One `HudRoot`
- One `DebugOverlayRoot`

This makes teardown and tests straightforward.

---

## 7) Tests

### 7.1 Test: state transition spawns correct UI root and despawns old root

This is the core invariant of per-state UI trees.

```rust
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use crate::common::state::GameState;
use crate::plugins::ui::menu::MainMenuUiRoot;

#[test]
fn entering_main_menu_spawns_ui_and_leaving_despawns() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, StatesPlugin));
    app.init_state::<GameState>();

    // register menu ui plugin
    crate::plugins::ui::menu::menu_ui_plugin(&mut app);

    // enter main menu
    app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::MainMenu);
    app.update();

    let menu_count = app.world().query::<&MainMenuUiRoot>().iter(app.world()).count();
    assert_eq!(menu_count, 1);

    // leave main menu
    app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::InGame);
    app.update();

    let menu_count2 = app.world().query::<&MainMenuUiRoot>().iter(app.world()).count();
    assert_eq!(menu_count2, 0);
}
```

This relies on the state machine providing `OnEnter` / `OnExit` schedules and state-scoped entity lifetime management via `DespawnOnExit`.[^bevy_state_docs][^bevy_state_scoped]

---

### 7.2 Test: HUD counters update when resources change

This test verifies the view model pattern:

- set `HudViewModel`
- run `update_hud_text`
- assert the text changed

```rust
use bevy::prelude::*;

use crate::plugins::ui::hud::{HudText, HudViewModel, update_hud_text};

#[test]
fn hud_updates_when_view_model_changes() {
    let mut world = World::new();

    // Spawn a HUD Text entity
    world.spawn((HudText, Text::new("HP: --")));

    // Insert view model
    world.insert_resource(HudViewModel { hp: 10, ammo: 5, wave: 2 });

    // Run system once
    world.run_system_once(update_hud_text).unwrap();

    // Assert
    let text = world.query::<&Text>().iter(&world).next().unwrap();
    assert!(text.to_string().contains("HP: 10"));
}
```

> Note: Depending on your Bevy version and `Text` API, you may access the string via `Text` fields or use a helper.
> The principle remains: UI reads a view-model resource and updates UI components.

---

## References

[^bevy_ui_crate]: Bevy UI crate docs (UI system, Node/Text, layout models): <https://docs.rs/bevy_ui/latest/bevy_ui/>
[^bevy_ui_text_example]: Bevy official UI Text example (create UI text and update it in a system, includes FPS display): <https://bevy.org/examples/ui-user-interface/text/>
[^bevy_state_docs]: Bevy state module docs (OnEnter/OnExit schedules, in_state, state_scoped mention): <https://docs.rs/bevy/latest/bevy/state/index.html>
[^bevy_state_scoped]: Bevy state_scoped docs (`DespawnOnExit` / `DespawnOnEnter` lifetime tools): <https://docs.rs/bevy/latest/bevy/state/state_scoped/index.html>
[^bevy_resource_module]: Bevy ECS resource module docs (resources are singleton-like): <https://docs.rs/bevy/latest/bevy/ecs/resource/index.html>
