use bevy::prelude::*;

use crate::config::AppConfig;
use crate::grid::{GridConfig, GridPlugin};
use crate::textures::TexturesPlugin;
mod camera;
mod config;
mod grid;
mod persistence;
mod render;
mod simulation;
mod textures;
mod ui;
mod undo;

fn main() {
    let config = AppConfig::load();

    App::new()
        .insert_resource(GridConfig {
            cols: config.grid_cols,
            rows: config.grid_rows,
            tile_size: config.tile_size,
            collision_destruction: config.collision_destruction,
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                resolution: (config.window_width as u32, config.window_height as u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(TexturesPlugin)
        .add_plugins(GridPlugin)
        .run();
}
