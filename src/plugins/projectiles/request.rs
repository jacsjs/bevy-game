use bevy::prelude::*;
use bevy::ecs::message::MessageWriter;

use crate::common::tunables::Tunables;
use crate::plugins::camera::MainCamera;

use super::components::Player;
use super::messages::{BulletKind, SpawnBulletRequest};

/// Producer: read input + compute aim, then write a SpawnBulletRequest message.
///
/// This system intentionally does **not** access BulletPool.
pub fn request_player_bullets(
    buttons: Option<Res<ButtonInput<MouseButton>>>,
    windows: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    q_player: Query<(Entity, &Transform), With<Player>>,
    tunables: Res<Tunables>,
    mut writer: MessageWriter<SpawnBulletRequest>,
) {
    let Some(buttons) = buttons else { return; };
    if !buttons.just_pressed(MouseButton::Left) { return; }

    let (player_e, player_tf) = match q_player.single() {
        Ok(v) => v,
        Err(e) => { debug!("No single Player Transform: {e:?}"); return; }
    };
    let origin = player_tf.translation.truncate();

    let window = match windows.single() {
        Ok(w) => w,
        Err(e) => { debug!("No single Window: {e:?}"); return; }
    };

    let cursor = match window.cursor_position() {
        Some(c) => c,
        None => { debug!("Cursor position is None"); return; }
    };

    let (camera, camera_tf) = match q_camera.single() {
        Ok(v) => v,
        Err(e) => { debug!("No single MainCamera: {e:?}"); return; }
    };

    let world_cursor = match camera.viewport_to_world_2d(camera_tf, cursor) {
        Ok(p) => p,
        Err(e) => { debug!("viewport_to_world_2d failed: {e:?}"); return; }
    };

    let mut dir = world_cursor - origin;
    if dir.length_squared() < 1e-4 { dir = Vec2::Y; } else { dir = dir.normalize(); }

    let pos = origin + dir * 18.0;
    let vel = dir * tunables.bullet_speed;

    writer.write(SpawnBulletRequest {
        kind: BulletKind::Player,
        pos,
        vel,
        damage: 1,
        owner: Some(player_e),
    });
}
