use bevy::prelude::*;

use crate::grid::{GameState, GridConfig, PANEL_WIDTH, SelectedTool};
use crate::simulation::{Cell, Grid, MAX_WATER_KG, build_depth_pressure};

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_render)
            .add_systems(Update, (render_grid, draw_hover_cursor));
    }
}

pub const WATER_PALETTE_SIZE: usize = 32;
pub const HEATMAP_PALETTE_SIZE: usize = 64;
pub const OBJECT_PALETTE_SIZE: usize = 64;
/// Height multiplier so cubes are visible relative to grid width
pub const CUBE_HEIGHT: f32 = 5.0;

#[derive(Component)]
pub struct Tile {
    pub x: usize,
    pub y: usize,
}

#[derive(Resource)]
pub struct MaterialPalette {
    pub air: Handle<StandardMaterial>,
    pub wall: Handle<StandardMaterial>,
    pub spring: Handle<StandardMaterial>,
    pub drain: Handle<StandardMaterial>,
    pub water: Vec<Handle<StandardMaterial>>,
    pub objects: Vec<Handle<StandardMaterial>>,
    pub heatmap: Vec<Handle<StandardMaterial>>,
    pub heatmap_zero: Handle<StandardMaterial>,
}

fn build_palette(materials: &mut Assets<StandardMaterial>) -> MaterialPalette {
    let air = materials.add(Color::srgb(0.34, 0.49, 0.27));
    let wall = materials.add(Color::srgb(0.1, 0.1, 0.1));
    let spring = materials.add(Color::srgb(0.0, 0.8, 0.7));
    let drain = materials.add(Color::srgb(0.8, 0.4, 0.0));

    let water: Vec<_> = (0..WATER_PALETTE_SIZE)
        .map(|i| {
            let fill = i as f32 / (WATER_PALETTE_SIZE - 1) as f32;
            materials.add(Color::srgb(1.0 - fill, 1.0 - fill, 1.0))
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
        water,
        objects,
        heatmap,
        heatmap_zero,
    }
}

/// Maps t in [0,1] through a five-stop rainbow: blue -> cyan -> green -> yellow -> red.
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
    config: Res<GridConfig>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut config_store: ResMut<GizmoConfigStore>,
) {
    let width = config.cols;
    let height = config.rows;

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
    let palette = build_palette(&mut materials);

    // Shared cube mesh for all tiles
    let cube_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

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
}

fn render_grid(
    grid: Res<Grid>,
    mut tile_query: Query<(&Tile, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
    palette: Res<MaterialPalette>,
    state: Res<GameState>,
) {
    if state.show_pressure {
        render_heat_grid_3d(&grid, &mut tile_query, &palette);
        return;
    }
    for (tile, mut transform, mut mat) in &mut tile_query {
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
        };
        let scaled = h * CUBE_HEIGHT;
        transform.scale.y = scaled;
        transform.translation.y = scaled / 2.0;
        mat.0 = handle.clone();
    }
}

fn render_heat_grid_3d(
    grid: &Grid,
    tile_query: &mut Query<(&Tile, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
    palette: &MaterialPalette,
) {
    let depth = build_depth_pressure(grid);
    let max = depth.iter().cloned().fold(1.0f32, f32::max);

    for (tile, mut transform, mut mat) in tile_query.iter_mut() {
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
        };
        let scaled = h * CUBE_HEIGHT;
        transform.scale.y = scaled;
        transform.translation.y = scaled / 2.0;
        mat.0 = handle.clone();
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
    };
    h * CUBE_HEIGHT
}

/// Casts a ray from the cursor and returns the grid cell of the first solid
/// surface hit, scanning from the top of the scene downward (camera-near first).
/// Falls back to the Y=0 ground plane if no solid surface is found.
pub fn find_cursor_cell(
    cursor_pos: Vec2,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    grid: &Grid,
) -> Option<(usize, usize)> {
    if cursor_pos.x < PANEL_WIDTH {
        return None;
    }
    let ray = camera.viewport_to_world(camera_transform, cursor_pos).ok()?;
    let denom = ray.direction.y;
    if denom.abs() < 1e-6 {
        return None;
    }

    // Scan from max height (5.0) down to 0 in small steps.
    // Each step moves ~0.1 world units in Y, ~0.05 world units in XZ.
    // Scanning top-down ensures the frontmost (camera-nearest) surface is found first.
    let steps = 50u32;
    for step in 0..=steps {
        let h = CUBE_HEIGHT * (1.0 - step as f32 / steps as f32);
        let t = (h - ray.origin.y) / denom;
        if t < 0.0 {
            continue;
        }
        let hit = ray.origin + t * *ray.direction;
        let gx_f = (hit.x + 0.5).floor();
        let gz_f = (hit.z + 0.5).floor();
        if gx_f < 0.0 || gz_f < 0.0 {
            continue;
        }
        let gx = gx_f as usize;
        let gz = gz_f as usize;
        if gx >= grid.width || gz >= grid.height {
            continue;
        }
        let cell = grid.get_cell(gx, gz);
        if matches!(cell, Cell::Air) {
            continue;
        }
        // Hit if the ray is at or just below this cell's top surface
        if cell_surface_y(cell) >= h - (CUBE_HEIGHT / steps as f32) {
            return Some((gx, gz));
        }
    }

    // No solid surface — fall back to the Y=0 ground plane
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

fn draw_hover_cursor(
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    grid: Res<Grid>,
    state: Res<GameState>,
    tool: Res<SelectedTool>,
    mut gizmos: Gizmos,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = camera_q.single() else {
        return;
    };
    // Height fraction matches what render_grid uses for each cell type
    let h = match *tool {
        SelectedTool::Block(_) => 0.8,
        SelectedTool::Spring => 1.0,
        SelectedTool::Drain => 0.3,
        SelectedTool::Eraser => 0.15,
    };
    let scaled = h * CUBE_HEIGHT;

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
                let center = Vec3::new(bx as f32, scaled / 2.0, by as f32);
                gizmos.cube(
                    Transform::from_translation(center)
                        .with_scale(Vec3::new(1.0, scaled, 1.0)),
                    color,
                );
            }
        }
    }
}
