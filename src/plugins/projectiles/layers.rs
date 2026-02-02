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
