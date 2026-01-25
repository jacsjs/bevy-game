
use bevy::prelude::*;
use avian2d::prelude::*;

#[test]
fn bullet_hit_despawns_entities() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins); // no input needed here [1](https://docs.rs/bevy/latest/bevy/struct.MinimalPlugins.html)

    // Add only the system under test (or the plugin if it only registers this system)
    app.add_systems(PostUpdate, bevy_game::plugins::projectiles::bullets::systems::process_bullet_hits);

    // Messages backing storage must exist for MessageReader<CollisionStart>
    app.world_mut().init_resource::<Messages<CollisionStart>>(); // Messages are Bevy's buffered queue backing MessageReader [2](https://docs.rs/bevy/latest/bevy/input/struct.ButtonInput.html)[7](https://bevy.org/assets/)

    // Spawn entities
    let bullet = app.world_mut().spawn(bevy_game::plugins::projectiles::bullets::Bullet { damage: 1 }).id();
    let enemy  = app.world_mut().spawn(bevy_game::plugins::enemies::Enemy).id();

    assert!(app.world().get_entity(bullet).is_ok());
    assert!(app.world().get_entity(enemy).is_ok());

    // Inject collision message
    app.world_mut().write_message(CollisionStart { collider1: bullet, collider2: enemy, body1: None, body2: None });

    // Run one tick so PostUpdate executes
    app.update();

    assert!(app.world().get_entity(bullet).is_err());
    assert!(app.world().get_entity(enemy).is_err());
}
