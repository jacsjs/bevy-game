pub mod layers;
pub mod components;
pub mod pool;
pub mod collision;

// v3 message-based spawn pipeline
pub mod messages;
pub mod request;
pub mod allocator;
pub mod commit;

use bevy::prelude::*;
use bevy::ecs::message::Messages;
use avian2d::collision::narrow_phase::CollisionEventSystems;

use crate::common::state::GameState;

pub struct ProjectilesPlugin;

/// Keep message buffers updated (double-buffered semantics).
///
/// This mirrors the engine-level message_update_system behavior.
fn update_spawn_messages(mut msgs: ResMut<Messages<messages::SpawnBulletRequest>>) {
    msgs.update();
}

impl Plugin for ProjectilesPlugin {
    fn build(&self, app: &mut App) {
        // Pool + pre-spawn
        app.insert_resource(pool::BulletPool::new(512))
            .add_systems(Startup, pool::init_bullet_pool);

        // Register message storage for spawn requests.
        app.init_resource::<Messages<messages::SpawnBulletRequest>>();
        // Update message buffers once per frame.
        app.add_systems(PostUpdate, update_spawn_messages);

        // Spawn pipeline in Update: request -> allocate
        app.add_systems(
            Update,
            (
                request::request_player_bullets,
                allocator::allocate_bullets_from_pool.after(request::request_player_bullets),
            )
                .run_if(in_state(GameState::InGame)),
        );

        // Collision pipeline in FixedPostUpdate.
        app.add_systems(
            FixedPostUpdate,
            collision::process_player_bullet_collisions
                .after(CollisionEventSystems)
                .run_if(in_state(GameState::InGame)),
        )
        .add_systems(
            FixedPostUpdate,
            commit::return_to_pool_commit
                .after(collision::process_player_bullet_collisions)
                .run_if(in_state(GameState::InGame)),
        );
    }
}
