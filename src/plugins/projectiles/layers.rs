use avian2d::prelude::*;

/// Collision layers.
///
/// This enum is identical to the one you showed. In your real codebase, keep it in a shared module.
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
