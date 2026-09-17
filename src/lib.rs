use std::time::Duration;

use bevy::prelude::*;
use bevy::winit::{UpdateMode, WinitSettings};

use crate::config::AppConfig;
use crate::grid::{GridConfig, GridPlugin};
use crate::textures::TexturesPlugin;

mod camera;
mod config;
mod grid;
mod levels;
mod persistence;
mod render;
mod simulation;
mod textures;
mod ui;
mod undo;

/// Builds and runs the game.
///
/// `#[bevy_main]` additionally emits the `android_main` entry point that the
/// Android activity calls into, so this one function serves both the desktop
/// binary (via `src/main.rs`) and the Android library.
#[bevy_main]
pub fn main() {
    let config = AppConfig::load();

    App::new()
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::reactive(Duration::from_secs_f64(1.0 / 60.0)),
            unfocused_mode: UpdateMode::reactive(Duration::from_secs(1)),
        })
        .insert_resource(GridConfig {
            cols: config.grid_cols,
            rows: config.grid_rows,
            tile_size: config.tile_size,
            collision_destruction: config.collision_destruction,
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(primary_window(&config)),
            ..default()
        }))
        .add_plugins(TexturesPlugin)
        .add_plugins(GridPlugin)
        .add_plugins(crate::levels::LevelsPlugin {
            level_path: config.level.clone(),
        })
        .run();
}

/// Desktop opens a fixed-size window from the config file.
#[cfg(not(target_os = "android"))]
fn primary_window(config: &AppConfig) -> Window {
    Window {
        resolution: (config.window_width as u32, config.window_height as u32).into(),
        ..default()
    }
}

/// Android ignores any requested resolution (the activity is always fullscreen),
/// so the window is left to adopt the device's own size and orientation.
#[cfg(target_os = "android")]
fn primary_window(_config: &AppConfig) -> Window {
    Window {
        resizable: false,
        recognize_rotation_gesture: true,
        ..default()
    }
}
