use bevy::prelude::*;

use crate::grid::{GridConfig, GridPlugin};
use crate::textures::TexturesPlugin;
mod grid;
mod textures;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let width: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(800.0);
    let height: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(600.0);

    App::new()
        .insert_resource(GridConfig {
            window_width: width,
            window_height: height,
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: (width as u32, height as u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(TexturesPlugin)
        .add_plugins(GridPlugin)
        .run();
}
