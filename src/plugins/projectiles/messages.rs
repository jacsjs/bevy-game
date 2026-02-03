//! Buffered spawn requests.
//!
//! We use Bevy **Messages** here instead of direct pool access.
//! The key idea is separation of concerns:
//! - producers create *intent*
//! - consumer applies intent (pool pop + component writes)
//!
//! This is a producer → queue → consumer pipeline.

use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulletKind {
    Player,
    Enemy,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct SpawnBulletRequest {
    pub kind: BulletKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub damage: i32,
    pub owner: Option<Entity>,
}
