//! Lighting plugin (Firefly) (render-only).

use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;
use bevy_firefly::prelude::*;

use crate::common::state::GameState;
use crate::plugins::player::Player;

#[derive(Component)]
pub struct PlayerLight;

pub fn plugin(app: &mut App) {
    if !app.is_plugin_added::<FireflyPlugin>() {
        app.add_plugins(FireflyPlugin);
    }

    app.add_systems(OnEnter(GameState::InGame), setup)
        .add_systems(Update, follow_player_light);
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Name::new("PlayerLight"),
        PlayerLight,
        PointLight2d {
            color: Color::srgb(1.0, 0.9, 0.75),
            range: 450.0,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 10.0),
        DespawnOnExit(GameState::InGame),
    ));
}

fn follow_player_light(
    q_player: Query<&Transform, (With<Player>, Without<PlayerLight>)>,
    mut q_light: Query<&mut Transform, (With<PlayerLight>, Without<Player>)>,
) {
    let Ok(tf_player) = q_player.single() else {
        return;
    };
    let Ok(mut tf_light) = q_light.single_mut() else {
        return;
    };

    tf_light.translation.x = tf_player.translation.x;
    tf_light.translation.y = tf_player.translation.y;
}
