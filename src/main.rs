use bevy::prelude::*;

use crate::grid::GridPlugin;
use crate::textures::TexturesPlugin;
mod grid;
mod textures;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: (800, 600).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(TexturesPlugin)
        .add_plugins(GridPlugin)
        .run();
}
