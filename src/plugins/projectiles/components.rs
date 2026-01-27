use bevy::prelude::*;

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct Enemy;

#[derive(Component)]
pub struct PooledBullet;

#[derive(Component)]
pub struct ReturnToPool;

/// Bullet gameplay state.
///
/// - `wall_bounces_left` implements: 3 bounces on world, then absorb.
#[derive(Component, Debug, Clone)]
pub struct Bullet {
    pub damage: i32,
    pub wall_bounces_left: u8,
}

impl Bullet {
    pub const DEFAULT_WALL_BOUNCES: u8 = 3;

    pub fn activate(damage: i32) -> Self {
        Self { damage, wall_bounces_left: Self::DEFAULT_WALL_BOUNCES }
    }
}

/// Arcade armour: fixed wear per hit.
#[derive(Component, Debug, Clone)]
pub struct Armour {
    pub hits_remaining: u16,
    pub max_hits: u16,
}

impl Armour {
    #[inline] pub fn is_up(&self) -> bool { self.hits_remaining > 0 }
    #[inline] pub fn wear_one(&mut self) { self.hits_remaining = self.hits_remaining.saturating_sub(1); }
}

#[derive(Component, Debug, Clone)]
pub struct Health {
    pub hp: i32,
}
