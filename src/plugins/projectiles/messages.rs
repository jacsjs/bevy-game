use bevy::prelude::*;

/// Team / source of a spawn request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulletKind {
    Player,
    Enemy,
}

/// Buffered spawn request.
///
/// We use Bevy **Messages** (pull-based, double-buffered) so that:
/// - producer systems can write requests with MessageWriter
/// - a consumer system can read and apply them at a well-defined schedule point
///
/// Messages are evaluated at fixed points in the schedule and are updated via `Messages<T>::update()`.
#[derive(Message, Clone, Copy, Debug)]
pub struct SpawnBulletRequest {
    pub kind: BulletKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub damage: i32,
    pub owner: Option<Entity>,
}
