//! Projectiles feature.

use bevy::prelude::*;

pub mod bullets;

pub fn plugin(app: &mut App) {
    bullets::plugin(app);
}
