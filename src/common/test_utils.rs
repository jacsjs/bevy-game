//! Test helpers.
//!
//! Bevy provides `World::run_system_once` (via the `RunSystemOnce` trait) for quickly
//! executing a system in tests/diagnostics without building a full schedule. [1](https://docs.rs/bevy/latest/bevy/ecs/system/trait.RunSystemOnce.html)
//!
//! Systems that use `Commands` enqueue structural changes; applying them is normally handled by
//! `ApplyDeferred` / schedule boundaries. We call `world.flush()` after running so queued commands
//! are applied before assertions. [2](https://docs.rs/bevy/latest/bevy/prelude/struct.Commands.html)[3](https://deepwiki.com/bevyengine/bevy/2.5-commands-and-deferred-operations)

use bevy::ecs::system::{IntoSystem, RunSystemOnce};
use bevy::prelude::*;

/// Run a system once on the given world, then flush deferred commands.
/// Returns the system output.
pub fn run_system_once<T, Out, Marker>(world: &mut World, system: T) -> Out
where
    T: IntoSystem<(), Out, Marker>,
{
    let out = world.run_system_once(system).expect("system run failed");
    world.flush();
    out
}
