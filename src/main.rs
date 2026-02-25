use bevy::prelude::*;

use crate::grid::GridPlugin;
mod grid;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: (800, 600).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(GridPlugin)
        .run();
}
