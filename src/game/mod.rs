//! Game composition root.
//!
//! Provides two public configuration functions:
//! - `configure_full`: includes DefaultPlugins (window/render) + game plugins.
//! - `configure_headless`: minimal configuration for integration tests.

use bevy::prelude::*;
use bevy::window::WindowResolution;

use crate::common::state::GameState;
use crate::plugins;

// Only compile these imports on Windows.
// This avoids unused-import / missing-module issues on Linux.
#[cfg(target_os = "windows")]
use bevy::render::{
    settings::{Backends, PowerPreference, WgpuSettings},
    RenderPlugin,
};

pub fn run() {
    App::new().add_plugins(configure_full).run();
}

/// Full configuration for `cargo run`.
pub fn configure_full(app: &mut App) {
    // Start with your existing DefaultPlugins + WindowPlugin configuration.
    let default_plugins = DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Bevy Game".into(),
            resolution: WindowResolution::new(1280, 720),
            ..default()
        }),
        ..default()
    });

    // On Windows, override the renderer initialization:
    // - Force DX12 (since Vulkan was crashing for you)
    // - Prefer high-performance GPU (useful on hybrid iGPU + dGPU laptops)
    // - Optionally pin adapter name to "NVIDIA"
    //
    // Bevy supports configuring the renderer via RenderPlugin + WgpuSettings (including backends). [1](https://makspll.github.io/bevy_mod_scripting/core_bindings/types/text.html)[2](https://github.com/bevyengine/bevy/blob/main/examples/ui/text.rs)
    // Backends::DX12 is a valid backend selection. [4](https://bevy-cheatbook.github.io/builtins.html)[2](https://github.com/bevyengine/bevy/blob/main/examples/ui/text.rs)
    #[cfg(target_os = "windows")]
    let default_plugins = default_plugins.set(RenderPlugin {
        render_creation: WgpuSettings {
            backends: Some(Backends::DX12),
            power_preference: PowerPreference::HighPerformance,
            ..default()
        }
        .into(),
        ..default()
    });

    // Add the configured plugin group.
    // On Linux/macOS this is just your original DefaultPlugins+WindowPlugin setup.
    // On Windows it also includes the DX12 renderer override. [3](https://deepwiki.com/bevyengine/bevy/5.9-text-rendering)[2](https://github.com/bevyengine/bevy/blob/main/examples/ui/text.rs)
    app.add_plugins(default_plugins);

    // Keep your existing game wiring exactly the same:
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