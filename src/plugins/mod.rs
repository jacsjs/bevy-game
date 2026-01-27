//! Feature plugins.

use bevy::prelude::*;

use crate::plugins::{projectiles::ProjectilesPlugin, ui::debug_hud};

pub mod core;
pub mod enemies;
pub mod physics;
pub mod player;
pub mod projectiles;
pub mod ui;
pub mod world;

// Render-only
pub mod camera;
pub mod lighting;

/// Register gameplay plugins that work in headless tests.
pub fn register_gameplay(app: &mut App) {
    core::plugin(app);
    physics::plugin(app);
    world::plugin(app);
    player::plugin(app);
    enemies::plugin(app);
    debug_hud::plugin(app);
    app.add_plugins(ProjectilesPlugin);
}

/// Register render-only plugins (requires DefaultPlugins / render infra).
pub fn register_render(app: &mut App) {
    lighting::plugin(app);
    camera::plugin(app);
}

/// Register all plugins (full app).
pub fn register_all(app: &mut App) {
    register_gameplay(app);
    register_render(app);
}
