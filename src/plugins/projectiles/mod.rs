pub mod layers;
pub mod components;
pub mod pool;
pub mod collision;

use bevy::prelude::*;
use avian2d::collision::narrow_phase::CollisionEventSystems;

use crate::{common::tunables::Tunables, plugins::{camera::MainCamera, projectiles::{components::Player, pool::{BulletPool, acquire_bullet}}}};

pub struct ProjectilesPlugin;

impl Plugin for ProjectilesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(pool::BulletPool::new(512))
            .add_systems(Startup, pool::init_bullet_pool)

            // Fire bullets from input
            .add_systems(Update, spawn_player_bullets_pooled)

            // Collision processing must run after Avian emits CollisionStart/CollisionEnd.
            .add_systems(
                FixedPostUpdate,
                collision::process_player_bullet_collisions.after(CollisionEventSystems),
            )
            // Pool commit must run after collision decisions have been made.
            .add_systems(
                FixedPostUpdate,
                pool::return_to_pool_commit.after(collision::process_player_bullet_collisions),
            );
    }
}

pub fn spawn_player_bullets_pooled(
    mut commands: Commands,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    q_player: Query<&Transform, With<Player>>,
    tunables: Res<Tunables>,
    mut pool: ResMut<BulletPool>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }

    let player_tf = match q_player.single() {
        Ok(v) => v,
        Err(e) => {
            println!("No single Player Transform: {e:?}");
            return;
        }
    };
    let origin = player_tf.translation.truncate();

    let window = match windows.single() {
        Ok(w) => w,
        Err(e) => {
            println!("No single Window: {e:?}");
            return;
        }
    };

    let cursor = match window.cursor_position() {
        Some(c) => c,
        None => {
            println!("Cursor position is None (cursor outside window or not available yet)");
            return;
        }
    };

    let (camera, camera_tf) = match q_camera.single() {
        Ok(v) => v,
        Err(e) => {
            println!("No single MainCamera: {e:?}");
            return;
        }
    };

    let world_cursor = match camera.viewport_to_world_2d(camera_tf, cursor) {
        Ok(p) => p,
        Err(e) => {
            println!("viewport_to_world_2d failed: {e:?}");
            return;
        }
    };

    let mut dir = world_cursor - origin;
    if dir.length_squared() < 1e-4 { dir = Vec2::Y; } else { dir = dir.normalize(); }

    let pos = origin + dir * 18.0;
    let vel = dir * tunables.bullet_speed;
    let _ = acquire_bullet(&mut commands, &mut pool, pos, vel, 1);
}


#[cfg(test)]
mod tests;
