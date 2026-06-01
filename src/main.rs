use std::time::Duration;

use bevy::prelude::*;
use bevy::winit::{UpdateMode, WinitSettings};

use crate::config::AppConfig;

mod config;
mod flood;

fn main() {
    let config = AppConfig::load();

    App::new()
        .insert_resource(WinitSettings {
            focused_mode: UpdateMode::reactive(Duration::from_secs_f64(1.0 / 60.0)),
            unfocused_mode: UpdateMode::reactive(Duration::from_secs(1)),
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "BlueRush".into(),
                resolution: (config.window_width as u32, config.window_height as u32).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(flood::FloodPlugin)
        .run();
}
