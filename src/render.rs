use bevy::prelude::*;
use bevy_asset::RenderAssetUsages;
use bevy_mesh::{Indices, PrimitiveTopology};

use crate::grid::{GameState, PANEL_WIDTH, SelectedTool, ViewMode};
use crate::simulation::{Cell, Grid, MAX_WATER_KG, build_depth_pressure, build_flow_distance};
use crate::textures::TextureAssets;

/// Spawns tile mesh entities and updates their transform/material each frame to
/// reflect the current grid state.
pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            setup_render
                .after(crate::textures::load_textures)
                .after(crate::levels::setup_level),
        )
        .add_systems(Update, (render_grid, draw_hover_cursor, draw_flow_arrows));
    }
}

/// Number of pre-baked `StandardMaterial` handles for the water fill gradient.
pub const WATER_PALETTE_SIZE: usize = 32;
/// Number of pre-baked `StandardMaterial` handles for the pressure heatmap.
pub const HEATMAP_PALETTE_SIZE: usize = 64;
/// Number of pre-baked `StandardMaterial` handles for the object weight gradient.
pub const OBJECT_PALETTE_SIZE: usize = 64;
/// World-space height of a fully-filled cell cube. Lower cells are scaled down proportionally.
pub const CUBE_HEIGHT: f32 = 5.0;

/// Marker component attached to every tile mesh entity, identifying its grid position.
#[derive(Component)]
pub struct Tile {
    /// Column index (x axis).
    pub x: usize,
    /// Row index (y axis, 0 = top/inlet row).
    pub y: usize,
}

/// Pre-created `StandardMaterial` handles for all cell types.
/// Shared across all tile entities to enable draw-call batching.
#[derive(Resource)]
pub struct MaterialPalette {
    pub air: Handle<StandardMaterial>,
    pub wall: Handle<StandardMaterial>,
    pub spring: Handle<StandardMaterial>,
    pub drain: Handle<StandardMaterial>,
    pub building: Handle<StandardMaterial>,
    pub rock: Handle<StandardMaterial>,
    pub sand: Handle<StandardMaterial>,
    pub water: Vec<Handle<StandardMaterial>>,
    pub objects: Vec<Handle<StandardMaterial>>,
    pub heatmap: Vec<Handle<StandardMaterial>>,
    pub heatmap_zero: Handle<StandardMaterial>,
}

/// Mesh handles for cell types that need non-cuboid shapes.
#[derive(Resource)]
pub struct MeshHandles {
    pub cube: Handle<Mesh>,
    pub house: Handle<Mesh>,
}

/// Water palette entries below this index (fill < ~25%) use the froth texture.
const FROTH_THRESHOLD: usize = WATER_PALETTE_SIZE / 4;

fn house_push_quad(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    verts: [[f32; 3]; 4],
    normal: [f32; 3],
) {
    let base = positions.len() as u32;
    for v in &verts {
        positions.push(*v);
        normals.push(normal);
        uvs.push([0.0, 0.0]);
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn house_push_tri(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    verts: [[f32; 3]; 3],
    normal: [f32; 3],
) {
    let base = positions.len() as u32;
    for v in &verts {
        positions.push(*v);
        normals.push(normal);
        uvs.push([0.0, 0.0]);
    }
    indices.extend_from_slice(&[base, base + 1, base + 2]);
}

/// Builds a house-shaped mesh in unit space (fits inside a 1×1×1 cube centred at origin).
///
/// Body occupies y ∈ [-0.5, 0.1] (60% of height); gable roof occupies y ∈ [0.1, 0.5]
/// with the ridge peak at x=0.
fn build_house_mesh() -> Mesh {
    let bt: f32 = 0.1; // body top / eave level
    let rp: f32 = 0.5; // roof ridge peak

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Body faces
    house_push_quad(&mut positions, &mut normals, &mut uvs, &mut indices,
        [[-0.5, -0.5, -0.5], [0.5, -0.5, -0.5], [0.5, -0.5, 0.5], [-0.5, -0.5, 0.5]],
        [0.0, -1.0, 0.0]);
    house_push_quad(&mut positions, &mut normals, &mut uvs, &mut indices,
        [[-0.5, -0.5, -0.5], [-0.5, bt, -0.5], [0.5, bt, -0.5], [0.5, -0.5, -0.5]],
        [0.0, 0.0, -1.0]);
    house_push_quad(&mut positions, &mut normals, &mut uvs, &mut indices,
        [[0.5, -0.5, 0.5], [0.5, bt, 0.5], [-0.5, bt, 0.5], [-0.5, -0.5, 0.5]],
        [0.0, 0.0, 1.0]);
    house_push_quad(&mut positions, &mut normals, &mut uvs, &mut indices,
        [[-0.5, -0.5, 0.5], [-0.5, bt, 0.5], [-0.5, bt, -0.5], [-0.5, -0.5, -0.5]],
        [-1.0, 0.0, 0.0]);
    house_push_quad(&mut positions, &mut normals, &mut uvs, &mut indices,
        [[0.5, -0.5, -0.5], [0.5, bt, -0.5], [0.5, bt, 0.5], [0.5, -0.5, 0.5]],
        [1.0, 0.0, 0.0]);

    // Roof faces
    let dh = rp - bt;
    let lm = (0.25f32 + dh * dh).sqrt();
    let ln = [-dh / lm, 0.5 / lm, 0.0];
    let rn = [dh / lm, 0.5 / lm, 0.0];

    house_push_quad(&mut positions, &mut normals, &mut uvs, &mut indices,
        [[-0.5, bt, -0.5], [0.0, rp, -0.5], [0.0, rp, 0.5], [-0.5, bt, 0.5]], ln);
    house_push_quad(&mut positions, &mut normals, &mut uvs, &mut indices,
        [[0.5, bt, 0.5], [0.0, rp, 0.5], [0.0, rp, -0.5], [0.5, bt, -0.5]], rn);
    house_push_tri(&mut positions, &mut normals, &mut uvs, &mut indices,
        [[-0.5, bt, -0.5], [0.5, bt, -0.5], [0.0, rp, -0.5]], [0.0, 0.0, -1.0]);
    house_push_tri(&mut positions, &mut normals, &mut uvs, &mut indices,
        [[0.5, bt, 0.5], [-0.5, bt, 0.5], [0.0, rp, 0.5]], [0.0, 0.0, 1.0]);

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

fn build_palette(materials: &mut Assets<StandardMaterial>, froth: Handle<Image>) -> MaterialPalette {
    let air = materials.add(Color::srgb(0.34, 0.49, 0.27));
    let wall = materials.add(Color::srgb(0.1, 0.1, 0.1));
    let spring = materials.add(Color::srgb(0.0, 0.8, 0.7));
    let drain = materials.add(Color::srgb(0.8, 0.4, 0.0));
    // Warm tan/brown — distinct from water blue, object grey, wall dark, spring teal, drain orange
    let building = materials.add(StandardMaterial {
        base_color: Color::srgb(0.76, 0.60, 0.42),
        cull_mode: None, // render both sides so the gable roof looks solid
        ..default()
    });
    let rock = materials.add(StandardMaterial {
        base_color: Color::srgb(0.478, 0.416, 0.353), // #7a6a5a stone grey-brown
        ..default()
    });
    let sand = materials.add(StandardMaterial {
        base_color: Color::srgb(0.831, 0.667, 0.416), // #d4aa6a warm tan
        ..default()
    });

    let water: Vec<_> = (0..WATER_PALETTE_SIZE)
        .map(|i| {
            let fill = i as f32 / (WATER_PALETTE_SIZE - 1) as f32;
            let base_color = Color::srgb(1.0 - fill * 0.4, 1.0 - fill * 0.4, 1.0);
            if i < FROTH_THRESHOLD {
                materials.add(StandardMaterial {
                    base_color,
                    base_color_texture: Some(froth.clone()),
                    ..default()
                })
            } else {
                materials.add(base_color)
            }
        })
        .collect();

    let objects: Vec<_> = (0..OBJECT_PALETTE_SIZE)
        .map(|i| {
            let t = i as f32 / (OBJECT_PALETTE_SIZE - 1) as f32;
            let g = 0.80 - t * 0.70; // lerp: 0.80 (lightest, ~0 kg) → 0.10 (darkest, 5000 kg)
            materials.add(Color::srgb(g, g, g))
        })
        .collect();

    let heatmap: Vec<_> = (0..HEATMAP_PALETTE_SIZE)
        .map(|i| {
            let t = (i as f32 + 1.0) / HEATMAP_PALETTE_SIZE as f32;
            materials.add(pressure_color(t))
        })
        .collect();
    let heatmap_zero = materials.add(Color::WHITE);

    MaterialPalette {
        air,
        wall,
        spring,
        drain,
        building,
        rock,
        sand,
        water,
        objects,
        heatmap,
        heatmap_zero,
    }
}

/// Maps `t` in \[0, 1\] through a five-stop rainbow: blue → cyan → green → yellow → red.
fn pressure_color(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.25 {
        let s = t / 0.25;
        (0.0, s, 1.0)
    } else if t < 0.5 {
        let s = (t - 0.25) / 0.25;
        (0.0, 1.0, 1.0 - s)
    } else if t < 0.75 {
        let s = (t - 0.5) / 0.25;
        (s, 1.0, 0.0)
    } else {
        let s = (t - 0.75) / 0.25;
        (1.0, 1.0 - s, 0.0)
    };
    Color::srgb(r, g, b)
}

fn setup_render(
    mut commands: Commands,
    grid: Res<crate::simulation::Grid>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut config_store: ResMut<GizmoConfigStore>,
    texture_assets: Res<TextureAssets>,
) {
    commands.insert_resource(ClearColor(Color::srgb(0.357, 0.639, 0.851))); // #5ba3d9 sky blue

    let width = grid.width;
    let height = grid.height;

    // Directional light (sun) for shadows and depth
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 12000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.4, 0.0)),
    ));

    // Configure gizmos to always render on top of geometry
    config_store
        .config_mut::<DefaultGizmoConfigGroup>()
        .0
        .depth_bias = -1.0;
    config_store
        .config_mut::<DefaultGizmoConfigGroup>()
        .0
        .line
        .width = 2.0;

    // Build shared material palette
    let palette = build_palette(&mut materials, texture_assets.froth_frame1.clone());

    // Shared mesh handles
    let cube_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let house_mesh = meshes.add(build_house_mesh());

    for row in 0..height {
        for col in 0..width {
            commands.spawn((
                Mesh3d(cube_mesh.clone()),
                MeshMaterial3d(palette.air.clone()),
                Transform::from_xyz(col as f32, 0.05, row as f32)
                    .with_scale(Vec3::new(1.0, 0.1, 1.0)),
                Tile { x: col, y: row },
            ));
        }
    }

    commands.insert_resource(palette);
    commands.insert_resource(MeshHandles {
        cube: cube_mesh,
        house: house_mesh,
    });
}

fn render_grid(
    grid: Res<Grid>,
    mut tile_query: Query<(
        &Tile,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Mesh3d,
    )>,
    palette: Res<MaterialPalette>,
    mesh_handles: Res<MeshHandles>,
    view_mode: Res<ViewMode>,
    state: Res<GameState>,
) {
    // Skip rendering when neither the grid nor display mode has changed
    if !grid.is_changed() && !state.is_changed() && !view_mode.is_changed() {
        return;
    }

    if *view_mode == ViewMode::Pressure {
        render_heat_grid_3d(&grid, &mut tile_query, &palette, &mesh_handles);
        return;
    }
    for (tile, mut transform, mut mat, mut mesh3d) in &mut tile_query {
        let cell = &grid.cells[tile.y * grid.width + tile.x];
        let (h, handle) = match cell {
            Cell::Air => (0.1, &palette.air),
            Cell::Water(kg) => {
                let fill = kg / MAX_WATER_KG;
                let idx = (fill * (WATER_PALETTE_SIZE - 1) as f32).round() as usize;
                (
                    0.1 + fill * 0.9,
                    &palette.water[idx.min(WATER_PALETTE_SIZE - 1)],
                )
            }
            Cell::Wall => (1.0, &palette.wall),
            Cell::Spring => (1.0, &palette.spring),
            Cell::Drain => (0.3, &palette.drain),
            Cell::Object(w) => {
                let t = (w / 5000.0).clamp(0.0, 1.0);
                let idx = (t * (OBJECT_PALETTE_SIZE - 1) as f32).round() as usize;
                (0.8, &palette.objects[idx])
            }
            Cell::Building { .. } => (1.0, &palette.building),
            Cell::Rock => (1.0, &palette.rock),
            Cell::Sand => (0.2, &palette.sand),
        };
        let scaled = h * CUBE_HEIGHT;
        transform.scale.y = scaled;
        transform.translation.y = scaled / 2.0;
        mat.0 = handle.clone();
        mesh3d.0 = if matches!(cell, Cell::Building { .. }) {
            mesh_handles.house.clone()
        } else {
            mesh_handles.cube.clone()
        };
    }
}

fn render_heat_grid_3d(
    grid: &Grid,
    tile_query: &mut Query<(
        &Tile,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
        &mut Mesh3d,
    )>,
    palette: &MaterialPalette,
    mesh_handles: &MeshHandles,
) {
    let depth = build_depth_pressure(grid);
    let max = depth.iter().cloned().fold(1.0f32, f32::max);

    for (tile, mut transform, mut mat, mut mesh3d) in tile_query.iter_mut() {
        let idx = tile.y * grid.width + tile.x;
        let val = depth[idx];
        let handle = if val > 0.0 {
            let t = val / max;
            let i = (t * (HEATMAP_PALETTE_SIZE - 1) as f32).round() as usize;
            &palette.heatmap[i.min(HEATMAP_PALETTE_SIZE - 1)]
        } else {
            &palette.heatmap_zero
        };

        let h = match &grid.cells[idx] {
            Cell::Air => 0.1,
            Cell::Water(kg) => 0.1 + (kg / MAX_WATER_KG) * 0.9,
            Cell::Wall => 1.0,
            Cell::Spring => 1.0,
            Cell::Drain => 0.3,
            Cell::Object(_) => 0.8,
            Cell::Building { .. } => 1.0,
            Cell::Rock => 1.0,
            Cell::Sand => 0.2,
        };
        let scaled = h * CUBE_HEIGHT;
        transform.scale.y = scaled;
        transform.translation.y = scaled / 2.0;
        mat.0 = handle.clone();
        mesh3d.0 = if matches!(grid.cells[idx], Cell::Building { .. }) {
            mesh_handles.house.clone()
        } else {
            mesh_handles.cube.clone()
        };
    }
}

/// Returns the world-space Y of the rendered top surface of a cell.
fn cell_surface_y(cell: &Cell) -> f32 {
    let h = match cell {
        Cell::Air => 0.1,
        Cell::Water(kg) => 0.1 + (kg / MAX_WATER_KG) * 0.9,
        Cell::Wall => 1.0,
        Cell::Spring => 1.0,
        Cell::Drain => 0.3,
        Cell::Object(_) => 0.8,
        Cell::Building { .. } => 1.0,
        Cell::Rock => 1.0,
        Cell::Sand => 0.2,
    };
    h * CUBE_HEIGHT
}

/// Slab method ray–AABB intersection. Returns the entry t (>= 0) or None.
fn ray_hits_aabb(origin: Vec3, dir: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let inv = Vec3::new(1.0 / dir.x, 1.0 / dir.y, 1.0 / dir.z);
    let t1 = (min - origin) * inv;
    let t2 = (max - origin) * inv;
    let t_enter = t1.min(t2).max_element();
    let t_exit = t1.max(t2).min_element();
    if t_exit >= t_enter && t_exit >= 0.0 {
        Some(t_enter.max(0.0))
    } else {
        None
    }
}

/// Casts a ray from the cursor and returns the grid cell whose AABB is hit
/// first (closest to the camera). Falls back to the Y=0 ground plane if no
/// AABB is hit.
pub fn find_cursor_cell(
    cursor_pos: Vec2,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    grid: &Grid,
) -> Option<(usize, usize)> {
    if cursor_pos.x < PANEL_WIDTH {
        return None;
    }
    let ray = camera
        .viewport_to_world(camera_transform, cursor_pos)
        .ok()?;
    let dir = *ray.direction;
    if dir.y.abs() < 1e-6 {
        return None;
    }

    // 1. Project ray to the Y=0 ground plane for candidate center.
    let t_ground = -ray.origin.y / dir.y;
    if t_ground < 0.0 {
        return None;
    }
    let ground = ray.origin + t_ground * dir;
    let gx0 = (ground.x + 0.5).floor() as i32;
    let gz0 = (ground.z + 0.5).floor() as i32;

    // 2. Compute per-axis search windows based on actual ray direction signs.
    //    The ground projection undershoots in the direction opposite to the ray's XZ travel.
    let shift_x = ((CUBE_HEIGHT / dir.y.abs()) * dir.x.abs()).ceil() as i32 + 1;
    let shift_z = ((CUBE_HEIGHT / dir.y.abs()) * dir.z.abs()).ceil() as i32 + 1;
    let (dx_lo, dx_hi) = if dir.x < 0.0 { (-1, shift_x) } else { (-shift_x, 1) };
    let (dz_lo, dz_hi) = if dir.z < 0.0 { (-1, shift_z) } else { (-shift_z, 1) };

    // 3. Ray-AABB test for each candidate cell; keep the closest hit.
    let mut best_t = f32::MAX;
    let mut best: Option<(usize, usize)> = None;
    for dz in dz_lo..=dz_hi {
        for dx in dx_lo..=dx_hi {
            let gx = gx0 + dx;
            let gz = gz0 + dz;
            if gx < 0 || gz < 0 {
                continue;
            }
            let gx = gx as usize;
            let gz = gz as usize;
            if gx >= grid.width || gz >= grid.height {
                continue;
            }

            let cell = grid.get_cell(gx, gz);
            // Skip Air cells — their short AABBs can occlude a tall neighbor's
            // side face. Air cells are handled by the ground-plane fallback.
            if matches!(cell, Cell::Air) {
                continue;
            }
            let surface_y = cell_surface_y(cell);
            let aabb_min = Vec3::new(gx as f32 - 0.5, 0.0, gz as f32 - 0.5);
            let aabb_max = Vec3::new(gx as f32 + 0.5, surface_y, gz as f32 + 0.5);

            if let Some(t) = ray_hits_aabb(ray.origin, dir, aabb_min, aabb_max) {
                if t < best_t {
                    best_t = t;
                    best = Some((gx, gz));
                }
            }
        }
    }

    // 4. Fallback to ground projection if no AABB was hit.
    best.or_else(|| {
        if gx0 >= 0
            && gz0 >= 0
            && (gx0 as usize) < grid.width
            && (gz0 as usize) < grid.height
        {
            Some((gx0 as usize, gz0 as usize))
        } else {
            None
        }
    })
}

/// For a hypothetical block of `weight` at (x, y), returns the predicted
/// movement direction (dx, dy) and the net pushing pressure, or None if
/// the block would not move.
fn compute_arrow_info(
    grid: &Grid,
    depth: &[f32],
    flow_dist: &[u32],
    x: usize,
    y: usize,
    weight: f32,
) -> Option<(isize, isize, f32)> {
    let width = grid.width;
    let height = grid.height;
    let idx = y * width + x;

    // Horizontal pressure from adjacent water cells
    let p_left = if x > 0 {
        match &grid.cells[y * width + (x - 1)] {
            Cell::Water(_) => depth[y * width + (x - 1)],
            _ => 0.0,
        }
    } else {
        0.0
    };
    let p_right = if x < width - 1 {
        match &grid.cells[y * width + (x + 1)] {
            Cell::Water(_) => depth[y * width + (x + 1)],
            _ => 0.0,
        }
    } else {
        0.0
    };
    let x_force = p_left - p_right;

    // Raw upward pressure at this cell (before subtracting object weight)
    let raw_pressure = match &grid.cells[idx] {
        Cell::Water(_) | Cell::Spring => depth[idx],
        // depth[] for existing objects had weight subtracted — recover it
        Cell::Object(w) => depth[idx] + w,
        Cell::Air | Cell::Drain => {
            // No water column here; use max of adjacent water pressures as a proxy
            let p_below = if y > 0 {
                match &grid.cells[(y - 1) * width + x] {
                    Cell::Water(_) => depth[(y - 1) * width + x],
                    _ => 0.0,
                }
            } else {
                0.0
            };
            p_left.max(p_right).max(p_below)
        }
        Cell::Wall | Cell::Building { .. } | Cell::Rock => return None,
        Cell::Sand => {
            let p_below = if y > 0 {
                match &grid.cells[(y - 1) * width + x] {
                    Cell::Water(_) => depth[(y - 1) * width + x],
                    _ => 0.0,
                }
            } else {
                0.0
            };
            p_left.max(p_right).max(p_below)
        }
    };

    let net_y = (raw_pressure - weight).max(0.0);
    let net_x = (x_force.abs() - weight).max(0.0);
    let avg_pressure = (p_left + p_right) * 0.5;
    let x_stable = x_force.abs() < avg_pressure * 0.1;

    // Downstream direction from flow BFS
    let obj_fd = flow_dist[idx];
    let mut downstream_dx = 0.0f32;
    let mut downstream_dy = 0.0f32;
    if obj_fd != u32::MAX {
        for (ddx, ddy) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
            let nx = x as isize + ddx;
            let ny = y as isize + ddy;
            if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize {
                continue;
            }
            let nidx = ny as usize * width + nx as usize;
            let nd = flow_dist[nidx];
            if nd != u32::MAX && nd > obj_fd {
                downstream_dx += ddx as f32;
                downstream_dy += ddy as f32;
            }
        }
    }
    let has_flow = downstream_dx != 0.0 || downstream_dy != 0.0;

    let threshold = 0.1;
    let (dx, dy) = if net_y >= net_x && net_y > threshold {
        if has_flow {
            if downstream_dx.abs() >= downstream_dy.abs() {
                (downstream_dx.signum() as isize, 0isize)
            } else {
                (0isize, downstream_dy.signum() as isize)
            }
        } else {
            (0isize, 1isize)
        }
    } else if net_x > threshold && !x_stable {
        (x_force.signum() as isize, 0isize)
    } else if has_flow && (net_y > threshold || net_x > threshold) {
        if downstream_dx.abs() >= downstream_dy.abs() {
            (downstream_dx.signum() as isize, 0isize)
        } else {
            (0isize, downstream_dy.signum() as isize)
        }
    } else {
        (0isize, 0isize)
    };

    if dx == 0 && dy == 0 {
        return None;
    }

    Some((dx, dy, net_x.max(net_y)))
}

fn draw_flow_arrows(
    grid: Res<Grid>,
    view_mode: Res<ViewMode>,
    selected: Res<SelectedTool>,
    mut gizmos: Gizmos,
) {
    if *view_mode != ViewMode::FlowArrows {
        return;
    }

    let flow_dist = build_flow_distance(&grid);
    let depth = build_depth_pressure(&grid);
    let max_pressure = depth.iter().cloned().fold(1.0f32, f32::max).max(1.0);

    let weight = match *selected {
        SelectedTool::Block(w) => w,
        SelectedTool::Building { weight, .. } => weight,
        SelectedTool::Eraser | SelectedTool::Spring | SelectedTool::Drain => 200.0,
    };

    let arrow_y = CUBE_HEIGHT + 0.5;

    for y in 0..grid.height {
        for x in 0..grid.width {
            let Some((dx, dy, net)) = compute_arrow_info(&grid, &depth, &flow_dist, x, y, weight)
            else {
                continue;
            };

            // Brighter orange = stronger force relative to max pressure
            let t = (net / max_pressure).clamp(0.0, 1.0);
            let color = Color::srgba(1.0, 0.3 + t * 0.7, 0.0, 0.4 + t * 0.6);

            let start = Vec3::new(x as f32, arrow_y, y as f32);
            let end = Vec3::new(
                x as f32 + dx as f32 * 0.45,
                arrow_y,
                y as f32 + dy as f32 * 0.45,
            );
            gizmos.arrow(start, end, color);
        }
    }
}

fn draw_hover_cursor(
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    grid: Res<Grid>,
    state: Res<GameState>,
    mut gizmos: Gizmos,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = camera_q.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Some((cx, cy)) = find_cursor_cell(cursor_pos, camera, camera_transform, &grid) else {
        return;
    };

    let r = state.brush_radius as usize;
    // White outline stands out against blue water and orange/brown blocks
    let color = Color::WHITE;

    for dy in 0..=(r * 2) {
        for dx in 0..=(r * 2) {
            let bx = (cx + dx).saturating_sub(r);
            let by = (cy + dy).saturating_sub(r);
            if bx < grid.width && by < grid.height {
                // Use the actual cell's rendered height so the box always fits the cell.
                // For non-Air cells, raise the wireframe base to the air-tile
                // surface (0.1 * CUBE_HEIGHT = 0.5) so it matches the visible
                // extent — adjacent air tiles occlude the building base in the
                // depth buffer, but gizmos render on top.
                let cell = grid.get_cell(bx, by);
                let top = cell_surface_y(cell);
                let base = if matches!(cell, Cell::Air) { 0.0 } else { 0.1 * CUBE_HEIGHT };
                let height = top - base;
                let center = Vec3::new(bx as f32, base + height / 2.0, by as f32);
                gizmos.cube(
                    Transform::from_translation(center).with_scale(Vec3::new(1.0, height, 1.0)),
                    color,
                );
            }
        }
    }
}
