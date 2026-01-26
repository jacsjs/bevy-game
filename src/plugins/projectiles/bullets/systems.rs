use bevy::prelude::*;
use bevy::state::state_scoped::DespawnOnExit;
use avian2d::prelude::*;

use crate::common::{layers::Layer, state::GameState, tunables::Tunables};
use crate::plugins::camera::MainCamera;
use crate::plugins::player::Player;
use crate::plugins::enemies::Enemy;

use super::{Bullet, Lifetime};

/// Spawn a bullet on mouse click.
///
/// Implementation notes:
/// - We use `Option<Single<...>>` for camera/window/player so this system becomes a no-op
///   in headless test apps where those entities don't exist.
/// - `CollisionEventsEnabled` is attached only to bullets to opt-in collision events.
pub fn spawn_player_bullets(
    mut commands: Commands,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    q_camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    q_player: Query<&Transform, With<Player>>,
    tunables: Res<Tunables>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(player_tf) = q_player.single() else { return; };
    let Ok((camera, camera_tf)) = q_camera.single() else { return; };
    let Ok(window) = windows.single() else { return; };

    let Some(cursor) = window.cursor_position() else { return; };
    let Ok(world_cursor) = camera.viewport_to_world_2d(camera_tf, cursor) else { return; };

    let origin = player_tf.translation.truncate();
    let dir = world_cursor - origin;
    let dir = if dir.length_squared() > 0.001 { dir.normalize() } else { Vec2::Y };

    let layers = CollisionLayers::new(Layer::PlayerBullet, [Layer::World, Layer::Enemy]);

    // Bouncy bullets (dynamic): easiest baseline.
    let restitution = Restitution::new(0.95).with_combine_rule(CoefficientCombine::Max);
    let friction = Friction::ZERO;

    commands.spawn((
        Name::new("Bullet"),
        Bullet { damage: 1 },
        Lifetime(Timer::from_seconds(3.0, TimerMode::Once)),
        Sprite { color: Color::srgb(1.0, 0.85, 0.3), custom_size: Some(Vec2::splat(8.0)), ..default() },
        Transform::from_translation((origin + dir * 18.0).extend(2.0)),
        RigidBody::Dynamic,
        Collider::circle(4.0),
        layers,
        restitution,
        friction,
        LinearVelocity(dir * tunables.bullet_speed),

        // Opt-in collision events: Avian only emits CollisionStart/End if one collider has this marker.
        CollisionEventsEnabled,

        DespawnOnExit(GameState::InGame),
    ));
}

pub fn bullet_lifetime(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    mut q: Query<(Entity, &mut Lifetime)>,
) {
    for (e, mut lt) in &mut q {
        lt.tick(time.delta());
        if lt.is_finished() {
            commands.entity(e).despawn();
        }
    }
}

/// Bulk collision processing for bullet hits.
///
/// Avian docs: reading collision events as Messages via MessageReader is efficient for many collisions (bullet hits).
/// CollisionStart is only written if one of the colliders has CollisionEventsEnabled.
pub fn process_bullet_hits(
    mut commands: Commands,
    mut started: MessageReader<CollisionStart>,
    q_bullets: Query<(), With<Bullet>>,
    q_enemies: Query<(), With<Enemy>>,
) {
    for ev in started.read() {
        let a = ev.collider1;
        let b = ev.collider2;

        let (bullet, other) = if q_bullets.contains(a) {
            (a, b)
        } else if q_bullets.contains(b) {
            (b, a)
        } else {
            continue;
        };

        if q_enemies.contains(other) {
            commands.entity(bullet).despawn();
            commands.entity(other).despawn();
        }
    }
}
