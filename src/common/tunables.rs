//! Tunable gameplay constants.

use bevy::prelude::*;

#[derive(Resource, Debug, Clone)]
pub struct Tunables {
    pub pixels_per_meter: f32,
    pub player_speed: f32,
    pub bullet_speed: f32,
}

impl Default for Tunables {
    fn default() -> Self {
        Self { pixels_per_meter: 20.0, player_speed: 420.0, bullet_speed: 900.0 }
    }
}
