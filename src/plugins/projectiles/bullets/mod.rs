//! Bullets: spawn + lifetime + hit processing.

use bevy::prelude::*;

mod components;
pub mod systems;

pub use components::*;

pub fn plugin(app: &mut App) {
    app.add_systems(Update, systems::spawn_player_bullets)
        .add_systems(FixedUpdate, systems::bullet_lifetime)
        .add_systems(PostUpdate, systems::process_bullet_hits);
}

#[cfg(test)]
mod tests;
