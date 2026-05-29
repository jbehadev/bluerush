use bevy::input::mouse::{AccumulatedMouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI};

use crate::grid::{GridConfig, PANEL_WIDTH};

/// Sets up the isometric 3D camera and handles scroll-to-zoom and right-drag pan/orbit.
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera)
            .add_systems(Update, camera_controls);
    }
}

/// Persistent camera orbit state. Updated by `camera_controls` and applied to the
/// `Camera3d` `Transform` each frame when the state changes.
#[derive(Resource)]
pub struct CameraState {
    pub focus: Vec3,
    pub zoom: f32,
    /// Azimuth angle around the Y axis (radians)
    pub yaw: f32,
    /// Elevation angle above the XZ plane (radians, clamped to avoid flipping)
    pub pitch: f32,
    /// Distance from focus to camera
    pub distance: f32,
    // Saved defaults for Home reset
    default_yaw: f32,
    default_pitch: f32,
}

impl CameraState {
    /// Compute the camera position from spherical orbit coordinates.
    fn cam_pos(&self) -> Vec3 {
        self.focus
            + Vec3::new(
                self.distance * self.pitch.cos() * self.yaw.cos(),
                self.distance * self.pitch.sin(),
                self.distance * self.pitch.cos() * self.yaw.sin(),
            )
    }
}

/// Spawn the `Camera3d` with an orthographic projection centred on the grid,
/// and insert a `CameraState` resource with default orbit angles.
pub fn setup_camera(mut commands: Commands, config: Res<GridConfig>) {
    let width = config.cols;
    let depth = config.depth;

    let center_x = width as f32 / 2.0;
    let center_z = depth as f32 / 2.0;
    let grid_extent = (width as f32).max(depth as f32) / 2.0;

    // Focus at mid-height so we look into the bowl, not along its rim
    let focus = Vec3::new(center_x, 5.0, center_z);

    let yaw: f32 = PI / 4.0;
    let pitch: f32 = 1.05; // ~60° — steep enough to see the floor
    let distance = grid_extent * 3.5; // back up to see the full valley

    let cam_state = CameraState {
        focus,
        zoom: 1.0,
        yaw,
        pitch,
        distance,
        default_yaw: yaw,
        default_pitch: pitch,
    };
    let cam_pos = cam_state.cam_pos();

    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_4,
            ..default()
        }),
        Transform::from_translation(cam_pos).looking_at(focus, Vec3::Y),
    ));

    commands.insert_resource(cam_state);
}

fn camera_controls(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut scroll_events: MessageReader<MouseWheel>,
    accumulated_motion: Res<AccumulatedMouseMotion>,
    windows: Query<&Window>,
    mut camera_q: Query<&mut Transform, With<Camera3d>>,
    mut cam_state: ResMut<CameraState>,
    config: Res<GridConfig>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok(mut cam_transform) = camera_q.single_mut() else { return };
    let mut changed = false;

    let cursor_over_grid = window
        .cursor_position()
        .is_some_and(|pos| pos.x >= PANEL_WIDTH);

    let ctrl = keyboard.pressed(KeyCode::ControlLeft)
        || keyboard.pressed(KeyCode::ControlRight)
        || keyboard.pressed(KeyCode::SuperLeft)
        || keyboard.pressed(KeyCode::SuperRight);

    // --- Zoom via scroll wheel (changes orbit distance) ---
    for ev in scroll_events.read() {
        if cursor_over_grid {
            let scroll_amount = match ev.unit {
                MouseScrollUnit::Line => ev.y * 0.15,
                MouseScrollUnit::Pixel => ev.y * 0.002,
            };
            cam_state.distance *= 1.0 - scroll_amount;
            cam_state.distance = cam_state.distance.clamp(5.0, 300.0);
            changed = true;
        }
    }

    if accumulated_motion.delta != Vec2::ZERO && cursor_over_grid {
        let motion = accumulated_motion.delta;

        if mouse.pressed(MouseButton::Right) && ctrl {
            // --- Orbit / rotate via Ctrl+right drag ---
            let sensitivity = 0.005;
            cam_state.yaw -= motion.x * sensitivity;
            cam_state.pitch += motion.y * sensitivity;
            cam_state.pitch = cam_state.pitch.clamp(0.08, FRAC_PI_2 - 0.08);
            changed = true;
        } else if mouse.pressed(MouseButton::Right) {
            // --- Pan via right drag ---
            let pan_speed = cam_state.distance * 0.001;
            let right = cam_transform.right();
            let right_xz = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();
            let forward = cam_transform.forward();
            let forward_xz = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();

            let pan = (-motion.x * right_xz + motion.y * forward_xz) * pan_speed;
            cam_state.focus += pan;
            changed = true;
        }
    }

    // --- Reset on Home key ---
    if keyboard.just_pressed(KeyCode::Home) {
        let width = config.cols;
        let depth = config.depth;
        cam_state.focus = Vec3::new(width as f32 / 2.0, 5.0, depth as f32 / 2.0);
        cam_state.yaw   = cam_state.default_yaw;
        cam_state.pitch = cam_state.default_pitch;
        cam_state.zoom  = 1.0;
        changed = true;
    }

    // --- Apply camera state ---
    if changed {
        let new_pos = cam_state.cam_pos();
        *cam_transform = Transform::from_translation(new_pos).looking_at(cam_state.focus, Vec3::Y);
    }
}


