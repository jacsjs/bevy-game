use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy)]
pub struct Bullet {
    pub damage: u32,
}

#[derive(Component, Deref, DerefMut)]
pub struct Lifetime(pub Timer);
