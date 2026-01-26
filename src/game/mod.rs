//! Game composition root.
//!
//! Provides two public configuration functions:
//! - `configure_full`: includes DefaultPlugins (window/render) + game plugins.
//! - `configure_headless`: minimal configuration for integration tests.

use bevy::prelude::*;
use bevy::window::WindowResolution;

use crate::common::state::GameState;
use crate::plugins;

pub fn run() {
    App::new().add_plugins(configure_full).run();
}

/// Full configuration for `cargo run`.
pub fn configure_full(app: &mut App) {
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Bevy Game".into(),
            resolution: WindowResolution::new(1280, 720),
            ..default()
        }),
        ..default()
    }));

    configure_game(app);
    plugins::register_render(app);
}

/// Headless configuration for integration tests.
///
/// Notes:
/// - Do NOT add DefaultPlugins.
/// - Do NOT add render-only plugins (Firefly/camera).
pub fn configure_headless(app: &mut App) {
    configure_game(app);
}

/// Configuration shared by both full and headless apps.
fn configure_game(app: &mut App) {
    app.init_state::<GameState>();
    plugins::register_gameplay(app);
}
