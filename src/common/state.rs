//! Global state machine.

use bevy::prelude::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, States, Default)]
pub enum GameState {
    #[default]
    InGame,
}
