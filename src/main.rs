use bevy::prelude::*;

use crate::grid::GridPlugin;
mod grid;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(GridPlugin)
        .run();
}
