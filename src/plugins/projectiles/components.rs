use bevy::prelude::*;

#[derive(Component)]
pub struct Player;

#[derive(Component)]
pub struct Enemy;

/// Marker for entities that are part of the bullet pool.
#[derive(Component)]
pub struct PooledBullet;

/// Bullet lifecycle state (always present; avoids structural churn).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletState {
    Inactive,
    Active,
    PendingReturn,
}

impl Default for BulletState {
    fn default() -> Self {
        Self::Inactive
    }
}

/// Bullet gameplay state.
#[derive(Component, Debug, Clone)]
pub struct Bullet {
    pub damage: i32,
    pub wall_bounces_left: u8,
}

impl Bullet {
    pub const DEFAULT_WALL_BOUNCES: u8 = 3;

    pub fn activate(damage: i32) -> Self {
        Self {
            damage,
            wall_bounces_left: Self::DEFAULT_WALL_BOUNCES,
        }
    }
}

#[derive(Component, Debug, Clone)]
pub struct Armour {
    pub hits_remaining: u16,
    pub max_hits: u16,
}

impl Armour {
    #[inline]
    pub fn is_up(&self) -> bool {
        self.hits_remaining > 0
    }

    #[inline]
    pub fn wear_one(&mut self) {
        self.hits_remaining = self.hits_remaining.saturating_sub(1);
    }
}

#[derive(Component, Debug, Clone)]
pub struct Health {
    pub hp: i32,
}