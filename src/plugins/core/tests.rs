use bevy::prelude::*;
use crate::plugins::core;
use crate::common::tunables::Tunables;

#[test]
fn inserts_resources() {
    let mut app = App::new();
    core::plugin(&mut app);
    assert!(app.world().get_resource::<Tunables>().is_some());
    assert!(app.world().get_resource::<ClearColor>().is_some());
}
