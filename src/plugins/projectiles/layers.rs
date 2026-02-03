//! Collision layers used by Avian.
//!
//! Layers act like a tiny "schema" that lets you express collision intent.
//! Keeping these centralized reduces accidental mismatches.

use avian2d::prelude::*;

#[derive(PhysicsLayer, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    #[default]
    Default,
    World,
    Player,
    Enemy,
    PlayerBullet,
    EnemyBullet,
}
