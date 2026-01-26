use avian2d::prelude::*;
use bevy::prelude::*;

use crate::common::tunables::Tunables;

pub fn plugin(app: &mut App) {
    let ppm = app.world().resource::<Tunables>().pixels_per_meter;
    app.add_plugins(PhysicsPlugins::default().with_length_unit(ppm));
    app.insert_resource(Gravity(Vec2::ZERO));
}
