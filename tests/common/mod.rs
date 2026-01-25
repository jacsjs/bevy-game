//! Integration test harness.
//!
//! Keep integration tests headless:
//! - `MinimalPlugins` provides core ECS runtime.
//! - we then call `bevy_game::game::configure_headless` to install gameplay plugins.

use bevy::{prelude::*, state::app::StatesPlugin};

pub fn app_headless() -> App {
    let mut app = App::new();

    // FAILED: The `StateTransition` schedule is missing. Did you forget to add StatesPlugin or DefaultPlugins before calling init_state?
    app.add_plugins((MinimalPlugins, StatesPlugin));

    bevy_game::game::configure_headless(&mut app);
    app
}
