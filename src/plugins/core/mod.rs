//! Core plugin: shared resources and global settings.

use bevy::prelude::*;
use crate::common::tunables::Tunables;

pub fn plugin(app: &mut App) {
    app.insert_resource(Tunables::default());
    app.insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.07)));
}

#[cfg(test)]
mod tests;

