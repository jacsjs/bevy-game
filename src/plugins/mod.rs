//! Feature plugins.

use bevy::prelude::*;

pub mod core;
pub mod physics;
pub mod world;
pub mod player;
pub mod enemies;
pub mod projectiles;

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
    projectiles::plugin(app);
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
