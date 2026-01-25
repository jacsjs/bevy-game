//! Collision layers.

use avian2d::prelude::*;

#[derive(PhysicsLayer, Default, Clone, Copy, Debug)]
pub enum Layer {
    #[default]
    Default,
    World,
    Player,
    Enemy,
    PlayerBullet,
    EnemyBullet,
}

