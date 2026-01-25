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

