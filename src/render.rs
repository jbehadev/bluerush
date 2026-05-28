use bevy::prelude::*;
use bevy_asset::RenderAssetUsages;
use bevy_mesh::{Indices, PrimitiveTopology};

use crate::grid::{ActiveLayer, GameState, GridDirty, PANEL_WIDTH};
use crate::simulation::{Cell, MAX_WATER_KG};
use crate::simulation3d::Grid3D;
use crate::textures::TextureAssets;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            setup_render
                .after(crate::textures::load_textures)
                .after(crate::levels::setup_level),
        )
        .add_systems(Update, (render_grid_3d, draw_hover_cursor));
    }
}

// ---------------------------------------------------------------------------
// Palette sizes
// ---------------------------------------------------------------------------

pub const WATER_PALETTE_SIZE: usize = 32;
pub const HEATMAP_PALETTE_SIZE: usize = 64;
pub const OBJECT_PALETTE_SIZE: usize = 64;

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Pre-created `StandardMaterial` handles for all cell types.
#[derive(Resource)]
pub struct MaterialPalette {
    pub air:      Handle<StandardMaterial>,
    pub wall:     Handle<StandardMaterial>,
    pub spring:   Handle<StandardMaterial>,
    pub drain:    Handle<StandardMaterial>,
    pub building: Handle<StandardMaterial>,
    pub rock:     Handle<StandardMaterial>,
    pub sand:     Handle<StandardMaterial>,
    pub water:    Vec<Handle<StandardMaterial>>,
    pub objects:  Vec<Handle<StandardMaterial>>,
    pub heatmap:  Vec<Handle<StandardMaterial>>,
    pub heatmap_zero: Handle<StandardMaterial>,
}

/// Handles to the single face-culled mesh entity for each cell category.
#[derive(Resource)]
pub struct VoxelMeshes {
    pub wall:     Entity,
    pub rock:     Entity,
    pub sand:     Entity,
    pub spring:   Entity,
    pub drain:    Entity,
    pub building: Entity,
    pub water:    Entity,
    pub object:   Entity,
}

// ---------------------------------------------------------------------------
// Face-culled mesh builder
// ---------------------------------------------------------------------------

/// Returns true if a face adjacent to `cell` should expose the face of its
/// neighbour — i.e., the adjacent cell is visually transparent.
fn is_transparent(cell: &Cell) -> bool {
    matches!(cell, Cell::Air | Cell::Drain | Cell::Sand)
}

/// Builds a face-culled mesh for all voxels matching `cell_test`.
/// Only emits a face when the neighbour in that direction is transparent or
/// out of bounds (world edge).
fn build_voxel_mesh(grid: &Grid3D, cell_test: &impl Fn(&Cell) -> bool) -> Mesh {
    // Each entry: (neighbour_offset, face_normal, 4 corner verts in unit-cube space)
    const FACE_DATA: [([i32; 3], [f32; 3], [[f32; 3]; 4]); 6] = [
        ([0,1,0],  [0.,1.,0.],  [[-0.5,0.5,-0.5],[0.5,0.5,-0.5],[0.5,0.5,0.5],[-0.5,0.5,0.5]]),
        ([0,-1,0], [0.,-1.,0.], [[-0.5,-0.5,0.5],[0.5,-0.5,0.5],[0.5,-0.5,-0.5],[-0.5,-0.5,-0.5]]),
        ([1,0,0],  [1.,0.,0.],  [[0.5,-0.5,-0.5],[0.5,-0.5,0.5],[0.5,0.5,0.5],[0.5,0.5,-0.5]]),
        ([-1,0,0], [-1.,0.,0.], [[-0.5,-0.5,0.5],[-0.5,-0.5,-0.5],[-0.5,0.5,-0.5],[-0.5,0.5,0.5]]),
        ([0,0,1],  [0.,0.,1.],  [[0.5,-0.5,0.5],[-0.5,-0.5,0.5],[-0.5,0.5,0.5],[0.5,0.5,0.5]]),
        ([0,0,-1], [0.,0.,-1.], [[-0.5,-0.5,-0.5],[0.5,-0.5,-0.5],[0.5,0.5,-0.5],[-0.5,0.5,-0.5]]),
    ];

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals:   Vec<[f32; 3]> = Vec::new();
    let mut uvs:       Vec<[f32; 2]> = Vec::new();
    let mut indices:   Vec<u32>      = Vec::new();

    for y in 0..grid.height {
        for z in 0..grid.depth {
            for x in 0..grid.width {
                if !cell_test(grid.get_cell(x, y, z)) { continue; }
                let (fx, fy, fz) = (x as f32, y as f32, z as f32);

                for ([no_x, no_y, no_z], normal, verts) in &FACE_DATA {
                    let (nx, ny, nz) = (x as i32 + no_x, y as i32 + no_y, z as i32 + no_z);
                    let exposed = if grid.in_bounds(nx, ny, nz) {
                        is_transparent(grid.get_cell(nx as usize, ny as usize, nz as usize))
                    } else {
                        true
                    };
                    if !exposed { continue; }

                    let base = positions.len() as u32;
                    for [vx, vy, vz] in verts {
                        positions.push([fx + vx, fy + vy, fz + vz]);
                        normals.push(*normal);
                        uvs.push([0.0, 0.0]);
                    }
                    indices.extend_from_slice(&[base, base+1, base+2, base, base+2, base+3]);
                }
            }
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL,   normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0,     uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

// ---------------------------------------------------------------------------
// Palette builder (unchanged from 2D version)
// ---------------------------------------------------------------------------

fn pressure_color(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.25 {
        let s = t / 0.25; (0.0, s, 1.0)
    } else if t < 0.5 {
        let s = (t - 0.25) / 0.25; (0.0, 1.0, 1.0 - s)
    } else if t < 0.75 {
        let s = (t - 0.5) / 0.25; (s, 1.0, 0.0)
    } else {
        let s = (t - 0.75) / 0.25; (1.0, 1.0 - s, 0.0)
    };
    Color::srgb(r, g, b)
}

fn build_palette(materials: &mut Assets<StandardMaterial>, froth: Handle<Image>) -> MaterialPalette {
    const FROTH_THRESHOLD: usize = WATER_PALETTE_SIZE / 4;

    let air      = materials.add(Color::srgb(0.34, 0.49, 0.27));
    let wall     = materials.add(Color::srgb(0.1, 0.1, 0.1));
    let spring   = materials.add(Color::srgb(0.0, 0.8, 0.7));
    let drain    = materials.add(Color::srgb(0.8, 0.4, 0.0));
    let building = materials.add(StandardMaterial {
        base_color: Color::srgb(0.76, 0.60, 0.42),
        cull_mode: None,
        ..default()
    });
    let rock = materials.add(StandardMaterial {
        base_color: Color::srgb(0.478, 0.416, 0.353),
        ..default()
    });
    let sand = materials.add(StandardMaterial {
        base_color: Color::srgb(0.831, 0.667, 0.416),
        ..default()
    });

    let water: Vec<_> = (0..WATER_PALETTE_SIZE)
        .map(|i| {
            let fill = i as f32 / (WATER_PALETTE_SIZE - 1) as f32;
            let base_color = Color::srgb(1.0 - fill * 0.4, 1.0 - fill * 0.4, 1.0);
            if i < FROTH_THRESHOLD {
                materials.add(StandardMaterial { base_color, base_color_texture: Some(froth.clone()), ..default() })
            } else {
                materials.add(base_color)
            }
        })
        .collect();

    let objects: Vec<_> = (0..OBJECT_PALETTE_SIZE)
        .map(|i| {
            let t = i as f32 / (OBJECT_PALETTE_SIZE - 1) as f32;
            let g = 0.80 - t * 0.70;
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

    MaterialPalette { air, wall, spring, drain, building, rock, sand, water, objects, heatmap, heatmap_zero }
}

// ---------------------------------------------------------------------------
// Startup system
// ---------------------------------------------------------------------------

fn setup_render(
    mut commands: Commands,
    grid: Res<Grid3D>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut config_store: ResMut<GizmoConfigStore>,
    texture_assets: Res<TextureAssets>,
) {
    commands.insert_resource(ClearColor(Color::srgb(0.357, 0.639, 0.851)));

    commands.spawn((
        DirectionalLight { shadows_enabled: true, illuminance: 12000.0, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.4, 0.0)),
    ));

    config_store.config_mut::<DefaultGizmoConfigGroup>().0.depth_bias = -1.0;
    config_store.config_mut::<DefaultGizmoConfigGroup>().0.line.width = 2.0;

    let palette = build_palette(&mut materials, texture_assets.froth_frame1.clone());

    let wall = commands.spawn((
        Mesh3d(meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Wall)))),
        MeshMaterial3d(palette.wall.clone()),
    )).id();
    let rock = commands.spawn((
        Mesh3d(meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Rock)))),
        MeshMaterial3d(palette.rock.clone()),
    )).id();
    let sand = commands.spawn((
        Mesh3d(meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Sand)))),
        MeshMaterial3d(palette.sand.clone()),
    )).id();
    let spring = commands.spawn((
        Mesh3d(meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Spring)))),
        MeshMaterial3d(palette.spring.clone()),
    )).id();
    let drain = commands.spawn((
        Mesh3d(meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Drain)))),
        MeshMaterial3d(palette.drain.clone()),
    )).id();
    let building = commands.spawn((
        Mesh3d(meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Building { .. })))),
        MeshMaterial3d(palette.building.clone()),
    )).id();
    let water = commands.spawn((
        Mesh3d(meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Water(_))))),
        MeshMaterial3d(palette.water[16].clone()),
    )).id();
    let object = commands.spawn((
        Mesh3d(meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Object(_))))),
        MeshMaterial3d(palette.objects[32].clone()),
    )).id();

    commands.insert_resource(palette);
    commands.insert_resource(VoxelMeshes { wall, rock, sand, spring, drain, building, water, object });
}

// ---------------------------------------------------------------------------
// Update systems
// ---------------------------------------------------------------------------

fn render_grid_3d(
    grid: Res<Grid3D>,
    mut dirty: MessageReader<GridDirty>,
    voxel_meshes: Res<VoxelMeshes>,
    mut mesh_q: Query<&mut Mesh3d>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if dirty.read().next().is_none() { return; }

    if let Ok(mut m) = mesh_q.get_mut(voxel_meshes.wall) {
        m.0 = meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Wall)));
    }
    if let Ok(mut m) = mesh_q.get_mut(voxel_meshes.rock) {
        m.0 = meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Rock)));
    }
    if let Ok(mut m) = mesh_q.get_mut(voxel_meshes.sand) {
        m.0 = meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Sand)));
    }
    if let Ok(mut m) = mesh_q.get_mut(voxel_meshes.spring) {
        m.0 = meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Spring)));
    }
    if let Ok(mut m) = mesh_q.get_mut(voxel_meshes.drain) {
        m.0 = meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Drain)));
    }
    if let Ok(mut m) = mesh_q.get_mut(voxel_meshes.building) {
        m.0 = meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Building { .. })));
    }
    if let Ok(mut m) = mesh_q.get_mut(voxel_meshes.water) {
        m.0 = meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Water(_))));
    }
    if let Ok(mut m) = mesh_q.get_mut(voxel_meshes.object) {
        m.0 = meshes.add(build_voxel_mesh(&grid, &|c: &Cell| matches!(c, Cell::Object(_))));
    }
}

fn draw_hover_cursor(
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform)>,
    grid: Res<Grid3D>,
    active_layer: Res<ActiveLayer>,
    state: Res<GameState>,
    mut gizmos: Gizmos,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, cam_t)) = camera_q.single() else { return };
    let Some(cursor_pos) = window.cursor_position() else { return };

    let Some((cx, _cy, cz)) =
        find_cursor_cell_3d(cursor_pos, camera, cam_t, &grid, active_layer.y)
    else { return };

    let brush = state.brush_radius as i32;
    let layer_y = active_layer.y as f32;

    for dz in -brush..=brush {
        for dx in -brush..=brush {
            let gx = cx as i32 + dx;
            let gz = cz as i32 + dz;
            if gx < 0 || gz < 0
                || gx as usize >= grid.width
                || gz as usize >= grid.depth
            { continue; }
            let y = layer_y + 1.02;
            let x0 = gx as f32;
            let x1 = gx as f32 + 1.0;
            let z0 = gz as f32;
            let z1 = gz as f32 + 1.0;
            let col = Color::srgb(1.0, 1.0, 0.0);
            gizmos.line(Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), col);
            gizmos.line(Vec3::new(x1, y, z0), Vec3::new(x1, y, z1), col);
            gizmos.line(Vec3::new(x1, y, z1), Vec3::new(x0, y, z1), col);
            gizmos.line(Vec3::new(x0, y, z1), Vec3::new(x0, y, z0), col);
        }
    }
}

// ---------------------------------------------------------------------------
// Cursor raycasting
// ---------------------------------------------------------------------------

/// Casts a ray from the cursor and returns the grid voxel `(x, y, z)` where
/// the ray intersects the horizontal plane at the active Y layer.
pub fn find_cursor_cell_3d(
    cursor_pos: Vec2,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    grid: &Grid3D,
    active_layer_y: usize,
) -> Option<(usize, usize, usize)> {
    if cursor_pos.x < PANEL_WIDTH { return None; }
    let ray = camera.viewport_to_world(camera_transform, cursor_pos).ok()?;
    let dir = *ray.direction;
    if dir.y.abs() < 1e-6 { return None; }

    // Intersect ray with horizontal plane at centre of the active layer voxel
    let plane_y = active_layer_y as f32 + 0.5;
    let t = (plane_y - ray.origin.y) / dir.y;
    if t < 0.0 { return None; }

    let hit = ray.origin + t * dir;
    let gx = hit.x.floor() as i32;
    let gz = hit.z.floor() as i32;

    if gx < 0 || gz < 0
        || gx as usize >= grid.width
        || gz as usize >= grid.depth
    {
        return None;
    }
    Some((gx as usize, active_layer_y, gz as usize))
}
