//! Integration test harness.
//!
//! Keep integration tests headless:
//! - `MinimalPlugins` provides core ECS runtime.
//! - we then call `bevy_game::game::configure_headless` to install gameplay plugins.
use bevy::asset::AssetPlugin;
use bevy::input::InputPlugin;
use bevy::prelude::*;
use bevy::scene::ScenePlugin;
use bevy::state::app::StatesPlugin;

pub fn app_headless() -> App {
    let mut app = App::new();

    // Core ECS + states
    // Add AssetPlugin + ScenePlugin so SceneSpawner exists.
    // Add InputPlugin so ButtonInput<MouseButton>/KeyCode resources exist for systems using them.
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        InputPlugin,
        AssetPlugin::default(),
        ScenePlugin,
    ));

    bevy_game::game::configure_headless(&mut app);
    app
}