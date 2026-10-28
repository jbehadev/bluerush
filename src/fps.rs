//! Frames-per-second readout in the top-right corner.
//!
//! Bevy's `FrameTimeDiagnosticsPlugin` measures frame times and stores them in
//! the `DiagnosticsStore` resource; we just read the smoothed FPS from it and
//! write it into a UI `Text` a few times a second (every frame would flicker).

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

const REFRESH_SECS: f32 = 0.25; // how often the number is updated

pub struct FpsPlugin;

impl Plugin for FpsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, spawn_fps_text)
            .add_systems(Update, update_fps_text);
    }
}

/// Marks the FPS text so the update system can find it.
#[derive(Component)]
struct FpsText;

fn spawn_fps_text(mut commands: Commands) {
    commands.spawn((
        FpsText,
        Text::new("FPS --"),
        TextFont { font_size: FontSize::Px(16.0), ..default() },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(10.0),
            ..default()
        },
    ));
}

fn update_fps_text(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    mut since: Local<f32>,
    mut text: Query<&mut Text, With<FpsText>>,
) {
    *since += time.delta_secs();
    if *since < REFRESH_SECS {
        return;
    }
    *since = 0.0;
    let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
    else {
        return;
    };
    for mut t in &mut text {
        *t = Text::new(format!("FPS {fps:.0}"));
    }
}
