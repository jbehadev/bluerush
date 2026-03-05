use bevy::camera::ScalingMode;
use bevy::input::mouse::{AccumulatedMouseMotion, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::grid::{GridConfig, PANEL_WIDTH};

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_camera)
            .add_systems(Update, camera_controls);
    }
}

#[derive(Resource)]
pub struct CameraState {
    pub focus: Vec3,
    pub zoom: f32,
    pub base_offset: Vec3,
}

pub fn setup_camera(mut commands: Commands, config: Res<GridConfig>) {
    let width = config.cols;
    let height = config.rows;

    let vis_width = width as f32 + 4.0;
    let center_x = width as f32 / 2.0;
    let center_z = height as f32 / 2.0;
    let grid_extent = (width as f32 / 2.0).max(center_z);

    let focus = Vec3::new(center_x, 0.0, center_z);
    let cam_pos = Vec3::new(
        center_x + grid_extent * 0.5,
        grid_extent * 1.2,
        center_z + grid_extent * 0.5,
    );
    let base_offset = cam_pos - focus;

    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedHorizontal {
                viewport_width: vis_width * 1.5,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_translation(cam_pos).looking_at(focus, Vec3::Y),
    ));

    commands.insert_resource(CameraState {
        focus,
        zoom: 1.0,
        base_offset,
    });
}

fn camera_controls(
    mouse: Res<ButtonInput<MouseButton>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut scroll_events: MessageReader<MouseWheel>,
    accumulated_motion: Res<AccumulatedMouseMotion>,
    windows: Query<&Window>,
    mut camera_q: Query<(&mut Transform, &mut Projection)>,
    mut cam_state: ResMut<CameraState>,
    config: Res<GridConfig>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((mut cam_transform, mut projection)) = camera_q.single_mut() else {
        return;
    };
    let mut changed = false;

    let cursor_over_grid = window
        .cursor_position()
        .is_some_and(|pos| pos.x >= PANEL_WIDTH);

    // --- Zoom via scroll wheel ---
    for ev in scroll_events.read() {
        if cursor_over_grid {
            let scroll_amount = match ev.unit {
                MouseScrollUnit::Line => ev.y * 0.15,
                MouseScrollUnit::Pixel => ev.y * 0.002,
            };
            cam_state.zoom *= 1.0 - scroll_amount;
            cam_state.zoom = cam_state.zoom.clamp(0.2, 5.0);
            changed = true;
        }
    }

    // --- Pan via right mouse drag ---
    if mouse.pressed(MouseButton::Right)
        && cursor_over_grid
        && accumulated_motion.delta != Vec2::ZERO
    {
        let Projection::Orthographic(ref ortho) = *projection else {
            return;
        };
        let pixels_to_world = (ortho.area.max.x - ortho.area.min.x) / window.width();

        let right = cam_transform.right();
        let right_xz = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();
        let forward = cam_transform.forward();
        let forward_xz = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();

        let motion = accumulated_motion.delta;
        let pan = (-motion.x * right_xz + motion.y * forward_xz) * pixels_to_world;
        cam_state.focus += pan;
        changed = true;
    }

    // --- Reset on Home key ---
    if keyboard.just_pressed(KeyCode::Home) {
        let width = config.cols;
        let height = config.rows;
        let center_x = width as f32 / 2.0;
        let center_z = height as f32 / 2.0;
        cam_state.focus = Vec3::new(center_x, 0.0, center_z);
        cam_state.zoom = 1.0;
        changed = true;
    }

    // --- Apply camera state ---
    if changed {
        if let Projection::Orthographic(ref mut ortho) = *projection {
            ortho.scale = cam_state.zoom;
        }
        let new_pos = cam_state.focus + cam_state.base_offset;
        *cam_transform =
            Transform::from_translation(new_pos).looking_at(cam_state.focus, Vec3::Y);
    }
}

/// Converts a cursor window position to a grid cell, or None if it's in the toolbar/OOB.
/// Casts a ray from the 3D camera through the cursor onto the Y=0 ground plane.
pub(crate) fn cursor_to_grid(
    cursor_pos: Vec2,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) -> Option<(usize, usize)> {
    if cursor_pos.x < PANEL_WIDTH {
        return None;
    }
    let ray = camera
        .viewport_to_world(camera_transform, cursor_pos)
        .ok()?;
    let denom = ray.direction.y;
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = -ray.origin.y / denom;
    if t < 0.0 {
        return None;
    }
    let hit = ray.origin + t * *ray.direction;
    let gx = (hit.x + 0.5).floor();
    let gz = (hit.z + 0.5).floor();
    if gx < 0.0 || gz < 0.0 {
        return None;
    }
    Some((gx as usize, gz as usize))
}
