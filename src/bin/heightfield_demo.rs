//! Heightfield-water spike — evaluate a single smooth water SURFACE (vs particles).
//!
//! Run with:  cargo run --bin heightfield_demo
//!
//! The water is one continuous deforming mesh: a grid of vertices whose height
//! is driven by a wave-equation simulation. Because it's a connected surface
//! (not discrete particles) it reads as water from the first frame — smooth,
//! lit, translucent, with a visible floor beneath it. This demo is about the
//! LOOK; flooding/pooling (shallow-water) and object coupling come next.
//!
//! Controls:  hold LEFT MOUSE on the water to make ripples   •   R to calm it

use bevy::prelude::*;
use bevy_asset::RenderAssetUsages;
use bevy_mesh::{Indices, PrimitiveTopology};

// ---------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------

const W: usize = 110; // grid vertices across X
const D: usize = 110; // grid vertices across Z
const CELL: f32 = 6.0; // world units between vertices
const REST_Y: f32 = 0.0; // resting water level
const FLOOR_Y: f32 = -22.0; // floor sits below the water so you see through it

const WAVE_SPEED: f32 = 0.28; // propagation stiffness (keep < 0.5 for stability)
const DAMP: f32 = 0.990; // ripples fade over time
const SPLASH: f32 = 9.0; // impulse strength under the cursor
const SPLASH_RADIUS: i32 = 4;

fn span() -> f32 {
    (W - 1) as f32 * CELL
}
fn half() -> f32 {
    span() * 0.5
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Wave-equation heightfield: `height` is the surface offset from REST_Y,
/// `vel` its rate of change. A classic ripple model.
#[derive(Resource)]
struct Heightfield {
    height: Vec<f32>,
    vel: Vec<f32>,
}

/// Handle to the water mesh so we can rewrite its vertices each frame.
#[derive(Resource)]
struct WaterMesh(Handle<Mesh>);

fn idx(x: usize, z: usize) -> usize {
    z * W + x
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "BlueRush — Heightfield Water Demo (hold LMB to ripple, R to calm)".into(),
                resolution: (1000u32, 820u32).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.45, 0.62, 0.78)))
        .add_systems(Startup, setup)
        .add_systems(Update, (mouse_ripple, calm_on_key, step_water, update_mesh).chain())
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Camera looking obliquely down at the water.
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 340.0, 470.0).looking_at(Vec3::new(0.0, -10.0, 0.0), Vec3::Y),
    ));

    // Sun for specular sheen on the water surface.
    commands.spawn((
        DirectionalLight { illuminance: 11000.0, shadows_enabled: false, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.5, 0.0)),
    ));

    // Floor beneath the water — sandy, opaque, so the translucent water reads
    // as water with a bottom you can see through.
    let floor = meshes.add(Cuboid::new(span() + 40.0, 12.0, span() + 40.0));
    commands.spawn((
        Mesh3d(floor),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.80, 0.66, 0.44),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::from_xyz(0.0, FLOOR_Y - 6.0, 0.0),
    ));

    // Heightfield state — flat, but with a velocity impulse in the centre so
    // the surface ripples immediately on launch.
    let mut hf = Heightfield {
        height: vec![0.0; W * D],
        vel: vec![0.0; W * D],
    };
    for dz in -4i32..=4 {
        for dx in -4i32..=4 {
            let r = ((dx * dx + dz * dz) as f32).sqrt();
            if r <= 4.0 {
                let x = (W as i32 / 2 + dx) as usize;
                let z = (D as i32 / 2 + dz) as usize;
                hf.vel[idx(x, z)] -= SPLASH * (1.0 - r / 4.0);
            }
        }
    }

    // Static indices for the vertex grid.
    let mut indices: Vec<u32> = Vec::with_capacity((W - 1) * (D - 1) * 6);
    for z in 0..D - 1 {
        for x in 0..W - 1 {
            let v00 = idx(x, z) as u32;
            let v10 = idx(x + 1, z) as u32;
            let v01 = idx(x, z + 1) as u32;
            let v11 = idx(x + 1, z + 1) as u32;
            // Two triangles, wound CCW seen from above (+Y).
            indices.extend_from_slice(&[v00, v01, v11, v00, v11, v10]);
        }
    }

    // Build the initial (flat) surface mesh.
    let (positions, normals) = surface_attrs(&hf);
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; W * D]);
    mesh.insert_indices(Indices::U32(indices));
    let mesh_handle = meshes.add(mesh);

    // Translucent, shiny water material — transparency + low roughness give the
    // "watery" sheen and let the sandy floor show through.
    let water_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.10, 0.36, 0.62, 0.62),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.06,
        reflectance: 0.7,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Mesh3d(mesh_handle.clone()),
        MeshMaterial3d(water_mat),
        Transform::from_xyz(0.0, REST_Y, 0.0),
    ));

    commands.insert_resource(hf);
    commands.insert_resource(WaterMesh(mesh_handle));
}

/// Build vertex positions and slope-based normals from the current heights.
fn surface_attrs(hf: &Heightfield) -> (Vec<[f32; 3]>, Vec<[f32; 3]>) {
    let h = &hf.height;
    let mut positions = Vec::with_capacity(W * D);
    let mut normals = Vec::with_capacity(W * D);
    let off = half();
    for z in 0..D {
        for x in 0..W {
            let y = REST_Y + h[idx(x, z)];
            positions.push([x as f32 * CELL - off, y, z as f32 * CELL - off]);

            let hl = h[idx(x.saturating_sub(1), z)];
            let hr = h[idx((x + 1).min(W - 1), z)];
            let hu = h[idx(x, z.saturating_sub(1))];
            let hd = h[idx(x, (z + 1).min(D - 1))];
            let n = Vec3::new(hl - hr, 2.0 * CELL, hu - hd).normalize();
            normals.push([n.x, n.y, n.z]);
        }
    }
    (positions, normals)
}

/// Advance the wave equation one step: each vertex accelerates toward the
/// average of its neighbours, integrating into a rippling surface.
fn step_water(mut hf: ResMut<Heightfield>) {
    let h = hf.height.clone();
    for z in 0..D {
        for x in 0..W {
            let i = idx(x, z);
            let l = h[idx(x.saturating_sub(1), z)];
            let r = h[idx((x + 1).min(W - 1), z)];
            let u = h[idx(x, z.saturating_sub(1))];
            let dn = h[idx(x, (z + 1).min(D - 1))];
            let laplacian = (l + r + u + dn) * 0.25 - h[i];
            hf.vel[i] = (hf.vel[i] + laplacian * WAVE_SPEED) * DAMP;
        }
    }
    for i in 0..W * D {
        hf.height[i] += hf.vel[i];
    }
}

/// Rewrite the water mesh's vertices from the current heights.
fn update_mesh(hf: Res<Heightfield>, water: Res<WaterMesh>, mut meshes: ResMut<Assets<Mesh>>) {
    let Some(mesh) = meshes.get_mut(water.0.id()) else { return };
    let (positions, normals) = surface_attrs(&hf);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
}

/// While the left mouse is held, depress the surface under the cursor → ripples.
fn mouse_ripple(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut hf: ResMut<Heightfield>,
) {
    if !mouse.pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Ok((camera, cam_t)) = cameras.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_t, cursor) else { return };

    // Intersect the ray with the resting water plane (y = REST_Y).
    let dir = *ray.direction;
    if dir.y.abs() < 1e-5 {
        return;
    }
    let t = (REST_Y - ray.origin.y) / dir.y;
    if t < 0.0 {
        return;
    }
    let hit = ray.origin + dir * t;
    let off = half();
    let cx = ((hit.x + off) / CELL).round() as i32;
    let cz = ((hit.z + off) / CELL).round() as i32;

    for dz in -SPLASH_RADIUS..=SPLASH_RADIUS {
        for dx in -SPLASH_RADIUS..=SPLASH_RADIUS {
            let x = cx + dx;
            let z = cz + dz;
            if x < 0 || z < 0 || x >= W as i32 || z >= D as i32 {
                continue;
            }
            let r = ((dx * dx + dz * dz) as f32).sqrt();
            if r > SPLASH_RADIUS as f32 {
                continue;
            }
            let falloff = 1.0 - r / SPLASH_RADIUS as f32;
            hf.vel[idx(x as usize, z as usize)] -= SPLASH * falloff;
        }
    }
}

/// Press R to instantly calm the water back to flat.
fn calm_on_key(keys: Res<ButtonInput<KeyCode>>, mut hf: ResMut<Heightfield>) {
    if keys.just_pressed(KeyCode::KeyR) {
        hf.height.iter_mut().for_each(|h| *h = 0.0);
        hf.vel.iter_mut().for_each(|v| *v = 0.0);
    }
}
