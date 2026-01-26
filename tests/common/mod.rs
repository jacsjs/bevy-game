//! Integration test harness.
//!
//! Keep integration tests headless:
//! - `MinimalPlugins` provides core ECS runtime.
//! - we then call `bevy_game::game::configure_headless` to install gameplay plugins.


use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::asset::AssetPlugin;
use bevy::scene::ScenePlugin;

pub fn app_headless() -> App {
    let mut app = App::new();

    // Core ECS + states (you already needed this)
    // Add AssetPlugin + ScenePlugin so SceneSpawner exists.
    app.add_plugins((MinimalPlugins, StatesPlugin, AssetPlugin::default(), ScenePlugin));

    bevy_game::game::configure_headless(&mut app);
    app
}

