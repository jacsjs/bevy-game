//! Camera plugin (invariant-based edition).
//!
//! # Goal
//! Avoid per-frame singleton scans and encode ECS aliasing constraints explicitly.
//!
//! The key subtlety: **B0001**.
//! A system cannot have `Query<&Transform>` and `Query<&mut Transform>` at the same time
//! unless Bevy can prove those queries are disjoint.
//!
//! We encode disjointness using `Without<...>` filters.
//!
//! ```text
//! OnEnter(InGame): spawn MainCamera -> write MainCameraEntity resource
//! PostUpdate:      follow_player uses stored handles + disjoint queries
//! ```

use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;
use bevy_firefly::prelude::*;

use crate::common::state::GameState;
use crate::plugins::projectiles::components::{MainCameraEntity, Player, PlayerEntity};

#[derive(Component)]
pub struct MainCamera {
    pub responsiveness: f32,
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::InGame), spawn_camera)
        .add_systems(
            PostUpdate,
            follow_player
                .before(TransformSystems::Propagate)
                .run_if(in_state(GameState::InGame)),
        );
}

fn spawn_camera(mut commands: Commands) {
    let e = commands
        .spawn((
            Name::new("MainCamera"),
            Camera2d,
            MainCamera { responsiveness: 5.0 },
            FireflyConfig::default(),
            Transform::from_xyz(0.0, 0.0, 999.0),
            DespawnOnExit(GameState::InGame),
        ))
        .id();

    commands.insert_resource(MainCameraEntity(Some(e)));
}

fn follow_player(
    time: Res<Time>,
    player_e: Res<PlayerEntity>,
    cam_e: Res<MainCameraEntity>,
    // Disjointness proof: Player entities are not MainCamera entities.
    q_player: Query<&Transform, (With<Player>, Without<MainCamera>)>,
    // Disjointness proof: MainCamera entities are not Player entities.
    mut q_cam: Query<(&mut Transform, &MainCamera), Without<Player>>,
) {
    let player = player_e.0.expect("PlayerEntity not set");
    let cam = cam_e.0.expect("MainCameraEntity not set");

    let tf_player = q_player.get(player).expect("PlayerEntity invalid");
    let (mut tf_cam, main_cam) = q_cam.get_mut(cam).expect("MainCameraEntity invalid");

    let dt = time.delta_secs();
    let alpha = 1.0 - (-main_cam.responsiveness * dt).exp();

    tf_cam.translation.x = tf_cam.translation.x + (tf_player.translation.x - tf_cam.translation.x) * alpha;
    tf_cam.translation.y = tf_cam.translation.y + (tf_player.translation.y - tf_cam.translation.y) * alpha;
}
