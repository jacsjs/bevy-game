//! Camera plugin (render-only).
//!
//! Uses `Option<Single<...>>` so the system becomes a no-op when the player or camera is missing
//! (useful when reusing this plugin in tests or cut-down app configs).

use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;
use bevy_firefly::prelude::*;

use crate::common::state::GameState;
use crate::plugins::player::Player;

#[derive(Component)]
pub struct MainCamera;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::InGame), spawn_camera)
        .add_systems(PostUpdate, follow_player.before(TransformSystems::Propagate));
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("MainCamera"),
        Camera2d,
        MainCamera,
        FireflyConfig::default(),
        Transform::from_xyz(0.0, 0.0, 999.0),
        DespawnOnExit(GameState::InGame),
    ));
}

fn follow_player(
    q_player: Query<&Transform, (With<Player>, Without<MainCamera>)>,
    mut q_cam: Query<&mut Transform, (With<MainCamera>, Without<Player>)>,
) {
    let Ok(tf_player) = q_player.single() else { return; };
    let Ok(mut tf_cam) = q_cam.single_mut() else { return; };

    tf_cam.translation.x = tf_player.translation.x;
    tf_cam.translation.y = tf_player.translation.y;
}
