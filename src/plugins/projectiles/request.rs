//! Spawn producer: aim computation + request emission.
//!
//! # 3NF intuition (single source of truth)
//! `Aim` is a normalized fact: "cursor position in world coordinates".
//! We compute it once and store it, instead of recomputing camera/window conversions
//! everywhere we need it.
//!
//! # Runtime checks we keep
//! - The cursor may be outside the window → Aim becomes None.
//!
//! # Runtime checks we remove
//! - Re-discovering camera/player each click (architecture checks).
//!   We store `PlayerEntity` and `MainCameraEntity` once at spawn time.

use bevy::prelude::*;
use bevy::ecs::message::MessageWriter;

use crate::common::tunables::Tunables;

use super::components::{Aim, MainCameraEntity, PlayerEntity};
use super::messages::{BulletKind, SpawnBulletRequest};

pub fn update_aim_from_cursor(
    windows: Query<&Window>,
    cam_e: Res<MainCameraEntity>,
    q_camera: Query<(&Camera, &GlobalTransform)>,
    mut aim: ResMut<Aim>,
) {
    let window = windows.single().expect("Expected exactly one Window");

    let cam = cam_e.0.expect("MainCameraEntity not set (camera spawn invariant violated)");
    let (camera, camera_tf) = q_camera.get(cam).expect("MainCameraEntity invalid");

    let Some(cursor) = window.cursor_position() else {
        aim.world_cursor = None;
        return;
    };

    aim.world_cursor = camera.viewport_to_world_2d(camera_tf, cursor).ok();
}

pub fn request_player_bullets(
    buttons: Option<Res<ButtonInput<MouseButton>>>,
    tunables: Res<Tunables>,
    player_e: Res<PlayerEntity>,
    q_tf: Query<&Transform>,
    aim: Res<Aim>,
    mut writer: MessageWriter<SpawnBulletRequest>,
) {
    let Some(buttons) = buttons else { return; };
    if !buttons.just_pressed(MouseButton::Left) { return; }

    let player = player_e.0.expect("Clicked but PlayerEntity not set");
    let player_tf = q_tf.get(player).expect("PlayerEntity invalid");
    let origin = player_tf.translation.truncate();

    let world_cursor = aim.world_cursor.expect("Clicked but Aim.world_cursor is None");

    let mut dir = world_cursor - origin;
    if dir.length_squared() < 1e-4 {
        dir = Vec2::Y;
    } else {
        dir = dir.normalize();
    }

    let pos = origin + dir * 18.0;
    let vel = dir * tunables.bullet_speed;

    writer.write(SpawnBulletRequest {
        kind: BulletKind::Player,
        pos,
        vel,
        damage: 1,
        owner: Some(player),
    });
}
