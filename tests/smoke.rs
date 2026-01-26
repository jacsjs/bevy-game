mod common;

use bevy::prelude::*;
use bevy_game::common::state::GameState;

#[test]
fn boots_and_ticks() {

    // Configure your headless game (states + gameplay plugins)
    let mut app = common::app_headless();
    
    for _ in 0..3 {
        app.update();
    }
}

#[test]
fn player_interpolation_pipeline_is_wired() {

    // Configure your headless game (states + gameplay plugins)
    let mut app = common::app_headless();

    // Enter InGame if needed (depends on your state setup)
    app.world_mut().resource_mut::<NextState<GameState>>().set(GameState::InGame);
    app.update();

    // Just tick a few frames; should not panic.
    for _ in 0..5 {
        app.update();
    }

    // Verify at least one Player has TranslationInterpolation
    let ok = app.world_mut().query::<(&bevy_game::plugins::player::Player, &avian2d::interpolation::TranslationInterpolation)>()
        .iter(app.world())
        .next()
        .is_some();

    assert!(ok, "Player should opt in to interpolation via TranslationInterpolation");
}

