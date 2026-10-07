//! Flood spike — terrain + shallow-water flooding on the heightfield surface.
//!
//! Run with:  cargo run --bin flood_demo
//!
//! Builds on the approved heightfield look and adds the core gameplay sim:
//!   * a bowl-shaped TERRAIN the water sits in,
//!   * a per-column WATER DEPTH evolved by a mass-conserving "water finds its
//!     level" flow (each column sends water to lower-surface neighbours),
//!   * a SOURCE in the middle that fills the bowl over time,
//!   * a small RIPPLE layer on top (purely visual) so the pooled surface
//!     shimmers and catches the light like real water.
//!
//! Controls:  hold LEFT MOUSE to pour water at the cursor   •   R to drain

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy_asset::RenderAssetUsages;
use bevy_mesh::{Indices, PrimitiveTopology};
use std::collections::HashMap;

use crate::level::{self, LoadedLevel, ObjectConfig, SourceConfig};

// ---------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------

// Grid resolution + cell size. The world span (≈ W·CELL) is kept ~constant when
// changing these, so the camera framing stays put; only the detail changes.
// NOTE: the channel-shape constants below (MEANDER_AMPL, CHANNEL_HW, BANK_K,
// SOURCE_R) are in *cell* units, so they're scaled to CELL to keep the river the
// same physical size — halving CELL means ~doubling those cell counts.
const W: usize = 150;
const D: usize = 150;
const CELL: f32 = 4.0;
const SLOPE_HEIGHT: f32 = 45.0; // stream bed drops this much from back (z=0) to front
const MEANDER_AMPL: f32 = 32.0; // how far (cells) the stream snakes left/right
const MEANDER_FREQ: f32 = 2.0 * std::f32::consts::PI * 2.5 / D as f32; // ~2.5 S-curves down the length
const CHANNEL_HW: f32 = 10.5; // half-width (cells) of the low stream bed
const BANK_K: f32 = 0.018; // how steeply the banks rise beyond the channel (∝ 1/CELL²)
const BANK_MAX: f32 = 55.0; // cap on bank height
const RIM_WIDTH: f32 = 4.0; // cells of raised containing wall along the left/right/back edges
const RIM_HEIGHT: f32 = 60.0; // how tall that wall rises (world units) so water can't spill off the map

const SOURCE_RATE: f32 = 110.0; // water depth/sec added at the source (spread over a patch)
const SOURCE_R: i32 = 5; // source patch radius in cells (wider = gentler, no spike)
const POUR_RATE: f32 = 220.0; // water depth/sec added under the mouse

// Weighted objects the flood pushes and floats. Size scales with weight, so a
// heavier block is bigger and taller (and dams the water higher).
const OBJ_FOOTPRINT_MIN: f32 = 9.0; // XZ size of the lightest block
const OBJ_FOOTPRINT_MAX: f32 = 18.0; // XZ size of the heaviest block
const OBJ_HEIGHT_MIN: f32 = 6.0; // height of the lightest block
const OBJ_HEIGHT_MAX: f32 = 28.0; // height of the heaviest block
const BUOYANCY: f32 = 400.0; // water depth × this = weight it can float (generous; contrast comes from mobility)
const DRAFT: f32 = 2.5; // how deep a floating object sits below the surface
const VERT_EASE: f32 = 0.15; // how fast an object eases toward its target height
const FLOW_TO_SPEED: f32 = 80.0; // converts the local current into a drift speed
const FLOW_EASE: f32 = 0.12; // how fast an object's velocity matches the current
const REF_WEIGHT: f32 = 150.0; // a "light" object; mobility = REF_WEIGHT / weight (capped at 1)

// UI / controls.
const PANEL_WIDTH: f32 = 120.0; // left toolbar width (world clicks under it are ignored)
const WEIGHTS: [f32; 5] = [200.0, 500.0, 1000.0, 2000.0, 5000.0]; // selectable object weights
const SINE_FREQ: f32 = 1.2; // rad/sec for the Sine wave pattern
const RANDOM_INTERVAL: f32 = 0.6; // seconds between re-rolls for the Random wave pattern
const FLOW_RATE: f32 = 0.5; // fraction of the surface gap equalised per iteration
const FLOW_ITERS: usize = 8; // flow iterations per frame (faster spreading = no spike)
const DT: f32 = 1.0 / 60.0;
const WET: f32 = 0.15; // depth below which a column is treated as dry

// Visual-only ripple layer.
const RIPPLE_SPEED: f32 = 0.25;
const RIPPLE_DAMP: f32 = 0.96;
const MAX_RIPPLE: f32 = 4.0;
const RIPPLE_FADE: f32 = 5.0; // ripples fade out in water shallower than this
const DEPTH_COLOR_MAX: f32 = 25.0; // depth at which water reaches its darkest/most opaque

fn span() -> f32 {
    (W - 1) as f32 * CELL
}
fn half() -> f32 {
    span() * 0.5
}
fn idx(x: usize, z: usize) -> usize {
    z * W + x
}

/// The X (cell) the stream bed runs through at depth `z` — a sine meander.
fn channel_center(z: usize) -> f32 {
    (W - 1) as f32 * 0.5 + MEANDER_AMPL * (z as f32 * MEANDER_FREQ).sin()
}

/// Convert a (cell-space) position to a world position.
fn cell_to_world(cx: f32, cz: f32) -> Vec2 {
    Vec2::new(cx * CELL - half(), cz * CELL - half())
}

/// Stream-bed terrain: a low winding channel (following `channel_center`) that
/// slopes downhill from the back (z=0, high) to the front (z=D-1, low), with
/// banks rising on either side. Water snakes down the channel as a current.
/// This is the BUILT-IN fallback terrain — normally the terrain comes from a
/// level's heightmap PNG (see `level.rs`); this also seeds the first level
/// template written to disk.
fn terrain_height(x: usize, z: usize) -> f32 {
    let slope = (1.0 - z as f32 / (D - 1) as f32) * SLOPE_HEIGHT;
    let dist = (x as f32 - channel_center(z)).abs();
    let over = (dist - CHANNEL_HW).max(0.0);
    let bank = (over * over * BANK_K).min(BANK_MAX);
    slope + bank + edge_rim(x, z)
}

/// Containing rim along the left/right/back edges so water visibly can't
/// escape the map. The front edge (z = D-1) stays open — it's the drain.
/// Shared by the built-in valley and the level-generator tests.
fn edge_rim(x: usize, z: usize) -> f32 {
    let edge = (x.min(W - 1 - x) as f32).min(z as f32);
    let rim_t = (1.0 - edge / RIM_WIDTH).clamp(0.0, 1.0);
    rim_t * rim_t * RIM_HEIGHT
}

/// The built-in valley as a level: procedural heights plus the default source
/// (top of the stream bed) and starter object trio (light / medium / heavy in
/// the channel). Used when the configured level can't be loaded, and baked out
/// as the first editable level template.
fn builtin_level() -> LoadedLevel {
    let heights = (0..W * D).map(|i| terrain_height(i % W, i / W)).collect();
    let fx = |cx: f32| cx / (W - 1) as f32;
    let fz = |cz: usize| cz as f32 / (D - 1) as f32;
    LoadedLevel {
        name: "Valley".into(),
        heights,
        source: SourceConfig { x: fx(channel_center(4)), z: fz(4), radius: SOURCE_R, rate: SOURCE_RATE },
        objects: vec![
            ObjectConfig { x: fx(channel_center(D / 2)), z: fz(D / 2), weight: 4000.0 },
            ObjectConfig { x: fx(channel_center(D / 3)), z: fz(D / 3), weight: 150.0 },
            ObjectConfig { x: fx(channel_center(2 * D / 3)), z: fz(2 * D / 3), weight: 800.0 },
        ],
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct Terrain(Vec<f32>);

/// Path of the level yaml to load (from `config.yaml`), set by `FloodPlugin`.
#[derive(Resource)]
struct LevelPath(String);

/// All levels found next to the configured one, for the in-game selector.
#[derive(Resource)]
struct LevelLibrary {
    entries: Vec<LevelEntry>,
    /// Display name of the level currently playing (shown on the dropdown).
    current_name: String,
}

struct LevelEntry {
    name: String,
    path: String,
}

/// Index into `LevelLibrary.entries` to switch to next frame (set by the
/// dropdown, consumed by `switch_level`).
#[derive(Resource, Default)]
struct PendingLevel(Option<usize>);

/// Handle of the terrain mesh, kept so a level switch can rebuild it in place.
#[derive(Resource)]
struct TerrainMesh(Handle<Mesh>);

/// Scan a directory for level yamls, labelled by their `name:` field (file
/// stem if the yaml doesn't parse). Sorted by name for a stable dropdown.
fn scan_levels(dir: &std::path::Path) -> Vec<LevelEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else { return entries };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let name = std::fs::read_to_string(&path)
            .ok()
            .and_then(|y| serde_yaml::from_str::<level::LevelConfig>(&y).ok())
            .map(|c| c.name)
            .unwrap_or_else(|| {
                path.file_stem().unwrap_or_default().to_string_lossy().into_owned()
            });
        entries.push(LevelEntry { name, path: path.to_string_lossy().into_owned() });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Where (and how fast) water enters the map, resolved from the level to grid
/// cells. `run_source` feeds this patch every frame.
#[derive(Resource)]
struct Source {
    x: usize,
    z: usize,
    radius: i32,
    rate: f32,
}

#[derive(Resource)]
struct Water {
    depth: Vec<f32>,
    ripple: Vec<f32>,
    rvel: Vec<f32>,
    /// Net water movement at each cell this frame (the local current), used to
    /// carry floating objects.
    flow: Vec<Vec2>,
}

#[derive(Resource)]
struct WaterMesh(Handle<Mesh>);

/// A weighted object that rests on the terrain and floats / gets carried once
/// the water is deep enough to lift its weight.
#[derive(Component)]
struct FloatObject {
    pos: Vec2, // world (x, z)
    vel: Vec2, // horizontal (x, z) velocity
    weight: f32,
    y: f32, // current height of the object's underside
}

/// Shared cube mesh for spawning objects at runtime (right-click).
#[derive(Resource)]
struct ObjectAssets {
    cube: Handle<Mesh>,
}

/// Per-cell extra floor height contributed by grounded objects. Raises the
/// effective terrain in the flow so water dams behind and diverts around them.
#[derive(Resource)]
struct Obstacle(Vec<f32>);

/// The active tool: drop an object of the chosen weight, or pour water.
#[derive(Resource, Clone, Copy, PartialEq)]
enum SelectedTool {
    Object(f32),
    Pour,
    Erase,
}

/// How the water source feeds the stream.
#[derive(Clone, Copy, PartialEq)]
enum WavePattern {
    Flood,  // steady
    Sine,   // smoothly pulsing
    Random, // gusty
}

#[derive(Resource)]
struct Wave {
    pattern: WavePattern,
    rng_level: f32,  // current Random multiplier
    since_roll: f32, // seconds since the last Random re-roll
}

/// Orbit camera state: a spherical position around a focus point on the ground.
#[derive(Resource)]
struct OrbitCamera {
    focus: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

#[derive(Component)]
struct WeightButton(f32);
#[derive(Component)]
struct PourButton;
#[derive(Component)]
struct EraseButton;
#[derive(Component)]
struct WaveButton(WavePattern);
/// The collapsed dropdown button showing the current level's name.
#[derive(Component)]
struct LevelDropdownButton;
#[derive(Component)]
struct LevelDropdownLabel;
/// The (initially hidden) container holding one button per level.
#[derive(Component)]
struct LevelOptions;
/// A level choice; the index points into `LevelLibrary.entries`.
#[derive(Component)]
struct LevelOptionButton(usize);

/// Whether the simulation is paused. Input, camera, and rendering keep running;
/// only the water + object simulation freezes.
#[derive(Resource, Default)]
struct Paused(bool);

#[derive(Component)]
struct PauseButton;
#[derive(Component)]
struct PauseLabel;

/// Object colour by weight: light = pale wood, heavy = dark stone.
fn weight_color(weight: f32) -> Color {
    let t = (weight / 4000.0).clamp(0.0, 1.0).sqrt();
    let c = 0.82 - t * 0.58;
    Color::srgb(c, c * 0.9, c * 0.75)
}

/// Normalised 0..1 size factor for a weight (200kg → 0, 5000kg → 1, sqrt-spaced
/// so the lighter weights still differ visibly).
fn weight_t(weight: f32) -> f32 {
    ((weight - 200.0) / 4800.0).clamp(0.0, 1.0).sqrt()
}

/// A block's XZ footprint (world units), scaled by weight.
fn obj_footprint(weight: f32) -> f32 {
    OBJ_FOOTPRINT_MIN + weight_t(weight) * (OBJ_FOOTPRINT_MAX - OBJ_FOOTPRINT_MIN)
}

/// A block's height (world units), scaled by weight.
fn obj_height(weight: f32) -> f32 {
    OBJ_HEIGHT_MIN + weight_t(weight) * (OBJ_HEIGHT_MAX - OBJ_HEIGHT_MIN)
}

/// Clamp a world position to a grid cell.
fn cell_of(pos: Vec2) -> (usize, usize) {
    let off = half();
    let gx = (((pos.x + off) / CELL).round() as i32).clamp(0, W as i32 - 1) as usize;
    let gz = (((pos.y + off) / CELL).round() as i32).clamp(0, D as i32 - 1) as usize;
    (gx, gz)
}

/// Height of the visible surface (terrain, plus any water on it) at world (x, z),
/// or `None` if the point is off the grid.
fn surface_height(terrain: &[f32], water: &Water, pos: Vec2) -> Option<f32> {
    let off = half();
    if pos.x.abs() > off || pos.y.abs() > off {
        return None;
    }
    let (gx, gz) = cell_of(pos);
    let i = idx(gx, gz);
    Some(terrain[i] + water.depth[i])
}

/// Cast the mouse ray onto the visible terrain/water surface. Marches along the
/// ray from the top of the heightfield in sub-cell steps until it dips below the
/// surface, then bisects to refine — so the hit is where the cursor *looks*.
fn cursor_hit(
    window: &Window,
    camera: &Camera,
    cam_t: &GlobalTransform,
    terrain: &[f32],
    water: &Water,
) -> Option<Vec3> {
    let cursor = window.cursor_position()?;
    if cursor.x < PANEL_WIDTH {
        return None; // over the UI panel, not the world
    }
    let ray = camera.viewport_to_world(cam_t, cursor).ok()?;
    let dir = *ray.direction;
    if dir.y >= -1e-5 {
        return None; // looking up / level: never meets the ground
    }
    // Start where the ray drops to the highest point of the surface, end where
    // it falls below the lowest — the hit must lie between.
    let (lo, hi) = terrain
        .iter()
        .zip(&water.depth)
        .map(|(t, d)| t + d)
        .fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(h), hi.max(h)));
    let t_start = ((hi - ray.origin.y) / dir.y).max(0.0);
    let t_end = (lo - ray.origin.y) / dir.y;
    let step = CELL * 0.5;

    let above = |t: f32| {
        let p = ray.origin + dir * t;
        surface_height(terrain, water, Vec2::new(p.x, p.z)).map(|h| p.y > h)
    };
    let mut prev = t_start;
    let mut t = t_start;
    while t <= t_end + step {
        if above(t) == Some(false) {
            // Crossed the surface between `prev` and `t`: bisect to pin it down.
            let (mut a, mut b) = (prev, t);
            for _ in 0..8 {
                let m = (a + b) * 0.5;
                if above(m) == Some(false) { b = m } else { a = m }
            }
            return Some(ray.origin + dir * b);
        }
        prev = t;
        t += step;
    }
    None
}

fn spawn_object(
    commands: &mut Commands,
    cube: Handle<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    terrain: &[f32],
    pos: Vec2,
    weight: f32,
) {
    let (gx, gz) = cell_of(pos);
    let y = terrain[idx(gx, gz)];
    let h = obj_height(weight);
    let fp = obj_footprint(weight);
    commands.spawn((
        FloatObject { pos, vel: Vec2::ZERO, weight, y },
        Mesh3d(cube),
        MeshMaterial3d(materials.add(weight_color(weight))),
        Transform {
            translation: Vec3::new(pos.x, y + h * 0.5, pos.y),
            scale: Vec3::new(fp, h, fp),
            ..default()
        },
    ));
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// The heightfield flood game: a meandering stream bed that floods, carries
/// weighted objects on its current, and lets grounded objects dam the flow.
/// Add this to the app; the window/config live in `main.rs`.
pub struct FloodPlugin {
    /// Path of the level yaml to load (from `config.yaml`).
    pub level: String,
}

impl Plugin for FloodPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LevelPath(self.level.clone()))
            .insert_resource(ClearColor(Color::srgb(0.45, 0.62, 0.78)))
            .insert_resource(SelectedTool::Object(500.0))
            .insert_resource(Wave { pattern: WavePattern::Flood, rng_level: 1.0, since_roll: 0.0 })
            .insert_resource(Paused(false))
            .insert_resource(PendingLevel::default())
            // setup builds the LevelLibrary that setup_ui's dropdown lists.
            .add_systems(Startup, (setup, setup_ui).chain())
            // UI / camera / input handling (order-independent).
            .add_systems(
                Update,
                (
                    handle_weight_buttons,
                    handle_pour_button,
                    handle_erase_button,
                    handle_wave_buttons,
                    handle_level_dropdown,
                    handle_level_option,
                    update_tool_highlight,
                    update_wave_highlight,
                    toggle_pause,
                    handle_pause_button,
                    update_pause_button,
                    camera_controls,
                    draw_placement_cursor,
                ),
            )
            // Input + simulation, in a fixed order each frame.
            .add_systems(
                Update,
                (
                    switch_level,
                    drain_on_key,
                    run_source,
                    handle_click,
                    build_obstacles,
                    step_flow,
                    step_ripples,
                    object_physics,
                    object_collision,
                    sync_objects,
                    update_water_mesh,
                )
                    .chain(),
            );
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    level_path: Res<LevelPath>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 360.0, 470.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(OrbitCamera { focus: Vec3::ZERO, yaw: 0.0, pitch: 0.65, distance: 592.0 });
    commands.spawn((
        DirectionalLight { illuminance: 11000.0, shadows_enabled: false, ..default() },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.5, 0.0)),
    ));

    // Terrain comes from the configured level (heightmap PNG + yaml). If the
    // level file doesn't exist yet, bake the built-in valley out as an editable
    // template — edit the PNG in any image editor to reshape the terrain.
    let loaded = match level::load(&level_path.0, W, D) {
        Ok(l) => {
            println!("Loaded level '{}' from {}", l.name, level_path.0);
            l
        }
        Err(e) => {
            let l = builtin_level();
            if std::path::Path::new(&level_path.0).exists() {
                eprintln!("Failed to load level {}: {e} — using built-in valley", level_path.0);
            } else {
                match level::write_template(&level_path.0, &l, W, D) {
                    Ok(()) => println!(
                        "Wrote level template to {} — edit the PNG next to it to reshape the terrain",
                        level_path.0
                    ),
                    Err(e) => eprintln!("Failed to write level template {}: {e}", level_path.0),
                }
            }
            l
        }
    };
    let terrain = loaded.heights;
    let terrain_mesh = meshes.add(build_terrain_mesh(&terrain));
    commands.insert_resource(TerrainMesh(terrain_mesh.clone()));
    commands.spawn((
        Mesh3d(terrain_mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            // White base so the per-vertex height gradient (brown → green) shows.
            base_color: Color::WHITE,
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::IDENTITY,
    ));

    // Water surface mesh — starts empty, rebuilt each frame from the depth field.
    let water = Water {
        depth: vec![0.0; W * D],
        ripple: vec![0.0; W * D],
        rvel: vec![0.0; W * D],
        flow: vec![Vec2::ZERO; W * D],
    };
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32; 3]; W * D]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 1.0, 0.0]; W * D]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; W * D]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.2f32, 0.4, 0.7, 0.5]; W * D]);
    mesh.insert_indices(Indices::U32(Vec::new()));
    let water_handle = meshes.add(mesh);
    commands.spawn((
        Mesh3d(water_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            // White base so the per-vertex depth gradient (shallow → deep) drives the colour.
            base_color: Color::WHITE,
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.04,
            reflectance: 0.65,
            cull_mode: None,
            ..default()
        })),
        Transform::IDENTITY,
    ));

    // Object spawning assets + the level's starting objects (the built-in
    // valley places a light / medium / heavy trio in the stream bed so the
    // weight difference is visible as it floods).
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0)); // unit cube, scaled per object by weight
    for obj in &loaded.objects {
        let cx = obj.x * (W - 1) as f32;
        let cz = obj.z * (D - 1) as f32;
        spawn_object(&mut commands, cube.clone(), &mut materials, &terrain,
            cell_to_world(cx, cz), obj.weight);
    }

    // The water source, resolved from map fractions to grid cells.
    let source = Source {
        x: (loaded.source.x * (W - 1) as f32).round().clamp(0.0, (W - 1) as f32) as usize,
        z: (loaded.source.z * (D - 1) as f32).round().clamp(0.0, (D - 1) as f32) as usize,
        radius: loaded.source.radius,
        rate: loaded.source.rate,
    };

    // Catalogue every level sitting next to the configured one (after the
    // template write above, so a fresh template lists itself too).
    let level_dir = std::path::Path::new(&level_path.0)
        .parent()
        .unwrap_or(std::path::Path::new("levels"));
    commands.insert_resource(LevelLibrary {
        entries: scan_levels(level_dir),
        current_name: loaded.name.clone(),
    });

    commands.insert_resource(Terrain(terrain));
    commands.insert_resource(water);
    commands.insert_resource(WaterMesh(water_handle));
    commands.insert_resource(ObjectAssets { cube });
    commands.insert_resource(Obstacle(vec![0.0; W * D]));
    commands.insert_resource(source);
}

/// Mark cells under grounded (can't-float) objects as raised floor, so the flow
/// dams behind them and diverts around. Floating objects don't obstruct.
fn build_obstacles(water: Res<Water>, objs: Query<&FloatObject>, mut obstacle: ResMut<Obstacle>) {
    for o in obstacle.0.iter_mut() {
        *o = 0.0;
    }
    for obj in &objs {
        let (gx, gz) = cell_of(obj.pos);
        let depth = water.depth[idx(gx, gz)];
        let grounded = obj.weight > depth * BUOYANCY; // too heavy to float here → it dams
        if !grounded {
            continue;
        }
        // Dam height tied to weight (taller blocks dam higher). Only raise cells
        // whose centre actually lies under the block footprint — no extra margin —
        // so the dammed (dry) area matches the cube and doesn't poke up through the
        // backed-up water as an oversized sandy shelf.
        let dam = obj_height(obj.weight);
        let hw = obj_footprint(obj.weight) * 0.5; // block half-width (world units)
        let r = (hw / CELL).ceil() as i32;
        let off = half();
        for dz in -r..=r {
            for dx in -r..=r {
                let x = gx as i32 + dx;
                let z = gz as i32 + dz;
                if x < 0 || z < 0 || x >= W as i32 || z >= D as i32 {
                    continue;
                }
                let cxw = x as f32 * CELL - off;
                let czw = z as f32 * CELL - off;
                if (cxw - obj.pos.x).abs() <= hw && (czw - obj.pos.y).abs() <= hw {
                    let c = idx(x as usize, z as usize);
                    obstacle.0[c] = obstacle.0[c].max(dam);
                }
            }
        }
    }
}

/// Separate overlapping objects (mass-weighted): the lighter one gets shoved
/// more, so a heavy block holds its ground and others pile against it.
fn object_collision(paused: Res<Paused>, mut q: Query<(Entity, &mut FloatObject)>) {
    if paused.0 {
        return;
    }
    let items: Vec<(Entity, Vec2, f32)> = q.iter().map(|(e, o)| (e, o.pos, o.weight)).collect();
    let n = items.len();
    let mut corr: HashMap<Entity, Vec2> = HashMap::new();

    for a in 0..n {
        for b in (a + 1)..n {
            let d = items[a].1 - items[b].1;
            let dist = d.length();
            let (wa, wb) = (items[a].2, items[b].2);
            let min_dist = (obj_footprint(wa) + obj_footprint(wb)) * 0.5;
            if dist < min_dist && dist > 1e-4 {
                let overlap = min_dist - dist;
                let dir = d / dist;
                let total = wa + wb;
                *corr.entry(items[a].0).or_default() += dir * (overlap * wb / total);
                *corr.entry(items[b].0).or_default() -= dir * (overlap * wa / total);
            }
        }
    }

    let bound = half() - CELL;
    for (e, mut obj) in &mut q {
        if let Some(c) = corr.get(&e) {
            obj.pos += *c;
            obj.pos.x = obj.pos.x.clamp(-bound, bound);
            obj.pos.y = obj.pos.y.clamp(-bound, bound);
        }
    }
}

/// Apply the selected tool at the cursor with the left mouse button: pour water
/// (while held) or drop one object of the chosen weight (on press). Clicks over
/// the left toolbar are ignored.
fn handle_click(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    tool: Res<SelectedTool>,
    terrain: Res<Terrain>,
    mut water: ResMut<Water>,
    assets: Res<ObjectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    objects: Query<(Entity, &FloatObject)>,
    mut commands: Commands,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, cam_t)) = cameras.single() else { return };
    let Some(hit) = cursor_hit(window, camera, cam_t, &terrain.0, &water) else { return };

    match *tool {
        SelectedTool::Pour => {
            if mouse.pressed(MouseButton::Left) {
                let off = half();
                let gx = ((hit.x + off) / CELL).round() as i32;
                let gz = ((hit.z + off) / CELL).round() as i32;
                if gx >= 0 && gz >= 0 && gx < W as i32 && gz < D as i32 {
                    add_water(&mut water, gx as usize, gz as usize, 3, POUR_RATE * DT, -2.0);
                }
            }
        }
        SelectedTool::Object(w) => {
            if mouse.just_pressed(MouseButton::Left) {
                spawn_object(&mut commands, assets.cube.clone(), &mut materials, &terrain.0,
                    Vec2::new(hit.x, hit.z), w);
            }
        }
        SelectedTool::Erase => {
            if mouse.just_pressed(MouseButton::Left) {
                // Delete the object nearest the cursor (within ~its footprint).
                let target = Vec2::new(hit.x, hit.z);
                let mut best: Option<(Entity, f32)> = None;
                for (e, obj) in &objects {
                    let dist = obj.pos.distance(target);
                    if dist < obj_footprint(obj.weight) * 0.7
                        && best.map_or(true, |(_, bd)| dist < bd)
                    {
                        best = Some((e, dist));
                    }
                }
                if let Some((e, _)) = best {
                    commands.entity(e).despawn();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// UI panel: object weights + wave patterns
// ---------------------------------------------------------------------------

const BTN_OFF: Color = Color::srgb(0.22, 0.24, 0.30);
const BTN_ON: Color = Color::srgb(0.85, 0.72, 0.20);
const POUR_OFF: Color = Color::srgb(0.15, 0.35, 0.55);
const POUR_ON: Color = Color::srgb(0.25, 0.65, 0.95);

fn setup_ui(mut commands: Commands, library: Res<LevelLibrary>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Px(PANEL_WIDTH),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.10, 0.11, 0.14)),
        ))
        .with_children(|panel| {
            // Level dropdown: the button shows the current level; clicking it
            // expands the list of levels found in the levels directory.
            panel.spawn((
                Text::new("LEVEL"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.65, 0.66, 0.72)),
            ));
            panel
                .spawn((
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(26.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.28, 0.30, 0.38)),
                    LevelDropdownButton,
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(library.current_name.clone()),
                        TextFont { font_size: 11.0, ..default() },
                        TextColor(Color::WHITE),
                        LevelDropdownLabel,
                    ));
                });
            panel
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        display: Display::None, // closed until the button is clicked
                        ..default()
                    },
                    LevelOptions,
                ))
                .with_children(|opts| {
                    for (i, entry) in library.entries.iter().enumerate() {
                        opts.spawn((
                            Button,
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Px(22.0),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.16, 0.18, 0.24)),
                            LevelOptionButton(i),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                Text::new(entry.name.clone()),
                                TextFont { font_size: 10.0, ..default() },
                                TextColor(Color::srgb(0.85, 0.86, 0.90)),
                            ));
                        });
                    }
                });

            // Pause / Run toggle.
            panel
                .spawn((
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(30.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        margin: UiRect::bottom(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.20, 0.50, 0.30)),
                    PauseButton,
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("Running"),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(Color::WHITE),
                        PauseLabel,
                    ));
                });

            panel.spawn((
                Text::new("OBJECTS"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.65, 0.66, 0.72)),
            ));
            for w in WEIGHTS {
                panel
                    .spawn((
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(26.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(BTN_OFF),
                        WeightButton(w),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(format!("{w:.0} kg")),
                            TextFont { font_size: 11.0, ..default() },
                            TextColor(Color::WHITE),
                        ));
                    });
            }
            panel
                .spawn((
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(26.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        margin: UiRect::top(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(POUR_OFF),
                    PourButton,
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("Pour Water"),
                        TextFont { font_size: 11.0, ..default() },
                        TextColor(Color::WHITE),
                    ));
                });
            panel
                .spawn((
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(26.0),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        margin: UiRect::top(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.45, 0.20, 0.18)),
                    EraseButton,
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("Erase"),
                        TextFont { font_size: 11.0, ..default() },
                        TextColor(Color::WHITE),
                    ));
                });

            panel.spawn((
                Text::new("WAVE"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.65, 0.66, 0.72)),
                Node { margin: UiRect::top(Val::Px(8.0)), ..default() },
            ));
            for (pat, name) in [
                (WavePattern::Flood, "Flood"),
                (WavePattern::Sine, "Sine"),
                (WavePattern::Random, "Random"),
            ] {
                panel
                    .spawn((
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(26.0),
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            ..default()
                        },
                        BackgroundColor(BTN_OFF),
                        WaveButton(pat),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(name),
                            TextFont { font_size: 11.0, ..default() },
                            TextColor(Color::WHITE),
                        ));
                    });
            }

            panel.spawn((
                Text::new("[Space] pause\n[R] drain"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.65, 0.66, 0.72)),
                Node { margin: UiRect::top(Val::Px(8.0)), ..default() },
            ));
        });
}

/// Space toggles pause.
fn toggle_pause(keys: Res<ButtonInput<KeyCode>>, mut paused: ResMut<Paused>) {
    if keys.just_pressed(KeyCode::Space) {
        paused.0 = !paused.0;
    }
}

fn handle_pause_button(
    q: Query<&Interaction, (Changed<Interaction>, With<PauseButton>)>,
    mut paused: ResMut<Paused>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            paused.0 = !paused.0;
        }
    }
}

fn update_pause_button(
    paused: Res<Paused>,
    mut btn: Query<&mut BackgroundColor, With<PauseButton>>,
    mut label: Query<&mut Text, With<PauseLabel>>,
) {
    if !paused.is_changed() {
        return;
    }
    let (color, text) = if paused.0 {
        (Color::srgb(0.60, 0.25, 0.20), "Paused")
    } else {
        (Color::srgb(0.20, 0.50, 0.30), "Running")
    };
    for mut bg in &mut btn {
        *bg = BackgroundColor(color);
    }
    for mut t in &mut label {
        *t = Text::new(text);
    }
}

fn handle_weight_buttons(
    q: Query<(&Interaction, &WeightButton), Changed<Interaction>>,
    mut tool: ResMut<SelectedTool>,
) {
    for (interaction, w) in &q {
        if *interaction == Interaction::Pressed {
            *tool = SelectedTool::Object(w.0);
        }
    }
}

fn handle_pour_button(
    q: Query<&Interaction, (Changed<Interaction>, With<PourButton>)>,
    mut tool: ResMut<SelectedTool>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            *tool = SelectedTool::Pour;
        }
    }
}

fn handle_erase_button(
    q: Query<&Interaction, (Changed<Interaction>, With<EraseButton>)>,
    mut tool: ResMut<SelectedTool>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            *tool = SelectedTool::Erase;
        }
    }
}

fn handle_wave_buttons(
    q: Query<(&Interaction, &WaveButton), Changed<Interaction>>,
    mut wave: ResMut<Wave>,
) {
    for (interaction, b) in &q {
        if *interaction == Interaction::Pressed {
            wave.pattern = b.0;
        }
    }
}

/// Clicking the dropdown button opens/closes the level list.
fn handle_level_dropdown(
    q: Query<&Interaction, (Changed<Interaction>, With<LevelDropdownButton>)>,
    mut options: Query<&mut Node, With<LevelOptions>>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            for mut node in &mut options {
                node.display =
                    if node.display == Display::None { Display::Flex } else { Display::None };
            }
        }
    }
}

/// Clicking a level in the list queues the switch and closes the dropdown.
fn handle_level_option(
    q: Query<(&Interaction, &LevelOptionButton), Changed<Interaction>>,
    mut pending: ResMut<PendingLevel>,
    mut options: Query<&mut Node, With<LevelOptions>>,
) {
    for (interaction, choice) in &q {
        if *interaction == Interaction::Pressed {
            pending.0 = Some(choice.0);
            for mut node in &mut options {
                node.display = Display::None;
            }
        }
    }
}

/// Apply a queued level switch: rebuild the terrain mesh in place, reset the
/// water, replace the objects with the level's starting set, and move the
/// source. On a load error the current level stays and the error is printed.
fn switch_level(
    mut pending: ResMut<PendingLevel>,
    library: Res<LevelLibrary>,
    mut terrain: ResMut<Terrain>,
    mut water: ResMut<Water>,
    mut source: ResMut<Source>,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_mesh: Res<TerrainMesh>,
    assets: Res<ObjectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    objects: Query<Entity, With<FloatObject>>,
    mut label: Query<&mut Text, With<LevelDropdownLabel>>,
    mut commands: Commands,
) {
    let Some(i) = pending.0.take() else { return };
    let entry = &library.entries[i];
    let loaded = match level::load(&entry.path, W, D) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to load level {}: {e} — keeping the current level", entry.path);
            return;
        }
    };

    if let Err(e) = meshes.insert(terrain_mesh.0.id(), build_terrain_mesh(&loaded.heights)) {
        eprintln!("Failed to swap terrain mesh: {e} — keeping the current level");
        return;
    }
    terrain.0 = loaded.heights;

    water.depth.iter_mut().for_each(|v| *v = 0.0);
    water.ripple.iter_mut().for_each(|v| *v = 0.0);
    water.rvel.iter_mut().for_each(|v| *v = 0.0);
    water.flow.iter_mut().for_each(|v| *v = Vec2::ZERO);

    for entity in &objects {
        commands.entity(entity).despawn();
    }
    for obj in &loaded.objects {
        let cx = obj.x * (W - 1) as f32;
        let cz = obj.z * (D - 1) as f32;
        spawn_object(&mut commands, assets.cube.clone(), &mut materials, &terrain.0,
            cell_to_world(cx, cz), obj.weight);
    }

    *source = Source {
        x: (loaded.source.x * (W - 1) as f32).round().clamp(0.0, (W - 1) as f32) as usize,
        z: (loaded.source.z * (D - 1) as f32).round().clamp(0.0, (D - 1) as f32) as usize,
        radius: loaded.source.radius,
        rate: loaded.source.rate,
    };

    for mut text in &mut label {
        *text = Text::new(loaded.name.clone());
    }
    println!("Loaded level '{}' from {}", loaded.name, entry.path);
}

fn update_tool_highlight(
    tool: Res<SelectedTool>,
    mut weights: Query<(&WeightButton, &mut BackgroundColor)>,
    mut pour: Query<&mut BackgroundColor, (With<PourButton>, Without<WeightButton>)>,
    mut erase: Query<
        &mut BackgroundColor,
        (With<EraseButton>, Without<WeightButton>, Without<PourButton>),
    >,
) {
    if !tool.is_changed() {
        return;
    }
    for (w, mut bg) in &mut weights {
        *bg = BackgroundColor(if *tool == SelectedTool::Object(w.0) { BTN_ON } else { BTN_OFF });
    }
    for mut bg in &mut pour {
        *bg = BackgroundColor(if *tool == SelectedTool::Pour { POUR_ON } else { POUR_OFF });
    }
    for mut bg in &mut erase {
        let on = Color::srgb(0.90, 0.35, 0.30);
        let off = Color::srgb(0.45, 0.20, 0.18);
        *bg = BackgroundColor(if *tool == SelectedTool::Erase { on } else { off });
    }
}

fn update_wave_highlight(wave: Res<Wave>, mut q: Query<(&WaveButton, &mut BackgroundColor)>) {
    for (b, mut bg) in &mut q {
        *bg = BackgroundColor(if wave.pattern == b.0 { POUR_ON } else { BTN_OFF });
    }
}

/// Buoyancy + flow-push for every object. An object floats when the water is
/// deep enough to support its weight; while floating it's carried along the
/// water-surface gradient, with heavier objects resisting the current more.
fn object_physics(
    terrain: Res<Terrain>,
    water: Res<Water>,
    paused: Res<Paused>,
    mut q: Query<&mut FloatObject>,
) {
    if paused.0 {
        return;
    }
    let t = &terrain.0;
    let d = &water.depth;
    let bound = half() - CELL;

    for mut obj in &mut q {
        let (gx, gz) = cell_of(obj.pos);
        let i = idx(gx, gz);
        let depth = d[i];
        let surface = t[i] + depth;

        // Vertical: float at the surface if the water can support the weight,
        // otherwise rest on the terrain.
        let floating = depth > WET && obj.weight <= depth * BUOYANCY;
        let target_y = if floating { surface - DRAFT } else { t[i] };
        obj.y += (target_y - obj.y) * VERT_EASE;

        // Horizontal: a floating object drifts toward the local current's speed,
        // scaled by mobility — light objects match the flow, heavy ones lag.
        if floating {
            let mobility = (REF_WEIGHT / obj.weight).min(1.0);
            let target_vel = water.flow[i] * FLOW_TO_SPEED * mobility;
            let new_vel = obj.vel.lerp(target_vel, FLOW_EASE);
            obj.vel = new_vel;
        } else {
            obj.vel *= 0.85; // ground friction
        }

        let step = obj.vel * DT;
        obj.pos += step;
        obj.pos.x = obj.pos.x.clamp(-bound, bound);
        obj.pos.y = obj.pos.y.clamp(-bound, bound);
    }
}

/// Orbit / pan / zoom the camera. Right-drag orbits, middle-drag pans across the
/// ground, the scroll wheel zooms. Left-drag is reserved for placing/pouring.
fn camera_controls(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut orbit: ResMut<OrbitCamera>,
    mut cam: Query<&mut Transform, With<Camera3d>>,
) {
    let d = motion.delta;
    if buttons.pressed(MouseButton::Right) {
        orbit.yaw -= d.x * 0.005;
        orbit.pitch = (orbit.pitch - d.y * 0.005).clamp(0.15, 1.5);
    }
    if buttons.pressed(MouseButton::Middle) {
        let pan = orbit.distance * 0.0015;
        let right = Vec3::new(orbit.yaw.cos(), 0.0, -orbit.yaw.sin());
        let fwd = Vec3::new(-orbit.yaw.sin(), 0.0, -orbit.yaw.cos());
        orbit.focus += right * (-d.x * pan) + fwd * (-d.y * pan);
    }
    if scroll.delta.y != 0.0 {
        // Gentle zoom; clamp the per-event step so trackpads don't lurch.
        let z = (scroll.delta.y * 0.04).clamp(-0.25, 0.25);
        orbit.distance = (orbit.distance * (1.0 - z)).clamp(120.0, 1400.0);
    }

    let (cp, sp) = (orbit.pitch.cos(), orbit.pitch.sin());
    let (cy, sy) = (orbit.yaw.cos(), orbit.yaw.sin());
    let offset = Vec3::new(orbit.distance * cp * sy, orbit.distance * sp, orbit.distance * cp * cy);
    if let Ok(mut t) = cam.single_mut() {
        *t = Transform::from_translation(orbit.focus + offset).looking_at(orbit.focus, Vec3::Y);
    }
}

/// Draw a wireframe preview at the cursor showing where (and how big) the next
/// placement lands: a box sized to the selected weight, or a flat square for Pour.
fn draw_placement_cursor(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    tool: Res<SelectedTool>,
    terrain: Res<Terrain>,
    water: Res<Water>,
    mut gizmos: Gizmos,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, cam_t)) = cameras.single() else { return };
    let Some(hit) = cursor_hit(window, camera, cam_t, &terrain.0, &water) else { return };
    let (gx, gz) = cell_of(Vec2::new(hit.x, hit.z));
    let ground = terrain.0[idx(gx, gz)];

    let mut edge = |a: Vec3, b: Vec3, c: Color| {
        gizmos.line(a, b, c);
    };

    match *tool {
        SelectedTool::Object(w) => {
            let hx = obj_footprint(w) * 0.5;
            let hy = obj_height(w) * 0.5;
            let cy = ground + hy;
            let col = Color::srgb(1.0, 0.95, 0.30);
            let corner = |sx: f32, sy: f32, sz: f32| {
                Vec3::new(hit.x + sx * hx, cy + sy * hy, hit.z + sz * hx)
            };
            let (b00, b10, b11, b01) = (
                corner(-1., -1., -1.),
                corner(1., -1., -1.),
                corner(1., -1., 1.),
                corner(-1., -1., 1.),
            );
            let (t00, t10, t11, t01) = (
                corner(-1., 1., -1.),
                corner(1., 1., -1.),
                corner(1., 1., 1.),
                corner(-1., 1., 1.),
            );
            edge(b00, b10, col);
            edge(b10, b11, col);
            edge(b11, b01, col);
            edge(b01, b00, col);
            edge(t00, t10, col);
            edge(t10, t11, col);
            edge(t11, t01, col);
            edge(t01, t00, col);
            edge(b00, t00, col);
            edge(b10, t10, col);
            edge(b11, t11, col);
            edge(b01, t01, col);
        }
        SelectedTool::Pour => {
            let col = Color::srgb(0.30, 0.80, 1.0);
            let y = ground + 0.5;
            let s = 12.0;
            let p = |dx: f32, dz: f32| Vec3::new(hit.x + dx, y, hit.z + dz);
            edge(p(-s, -s), p(s, -s), col);
            edge(p(s, -s), p(s, s), col);
            edge(p(s, s), p(-s, s), col);
            edge(p(-s, s), p(-s, -s), col);
        }
        SelectedTool::Erase => {
            let col = Color::srgb(0.95, 0.30, 0.25);
            let y = ground + 0.5;
            let s = 12.0;
            let p = |dx: f32, dz: f32| Vec3::new(hit.x + dx, y, hit.z + dz);
            edge(p(-s, -s), p(s, s), col);
            edge(p(s, -s), p(-s, s), col); // an X to read as "delete"
            edge(p(-s, -s), p(s, -s), col);
            edge(p(s, -s), p(s, s), col);
            edge(p(s, s), p(-s, s), col);
            edge(p(-s, s), p(-s, -s), col);
        }
    }
}

/// Copy each object's logical position onto its rendered cube.
fn sync_objects(mut q: Query<(&FloatObject, &mut Transform)>) {
    for (obj, mut tf) in &mut q {
        let h = obj_height(obj.weight);
        let fp = obj_footprint(obj.weight);
        tf.translation = Vec3::new(obj.pos.x, obj.y + h * 0.5, obj.pos.y);
        tf.scale = Vec3::new(fp, h, fp);
    }
}

/// Feed the level's source patch every frame, modulated by the selected wave
/// pattern (steady / pulsing / gusty). Each injection also kicks the ripple
/// field so the inflow looks alive.
fn run_source(
    time: Res<Time>,
    paused: Res<Paused>,
    source: Res<Source>,
    mut wave: ResMut<Wave>,
    mut water: ResMut<Water>,
) {
    if paused.0 {
        return;
    }
    let mult = match wave.pattern {
        WavePattern::Flood => 1.0,
        WavePattern::Sine => (0.5 + 0.5 * (time.elapsed_secs() * SINE_FREQ).sin()).max(0.0),
        WavePattern::Random => {
            wave.since_roll += DT;
            if wave.since_roll > RANDOM_INTERVAL {
                wave.since_roll = 0.0;
                wave.rng_level = 0.15 + rand::random::<f32>() * 1.5;
            }
            wave.rng_level
        }
    };
    add_water(&mut water, source.x, source.z, source.radius, source.rate * mult * DT, -0.8);
}

/// Press R to drain all the water (the source then refills it from empty).
fn drain_on_key(keys: Res<ButtonInput<KeyCode>>, mut water: ResMut<Water>) {
    if keys.just_pressed(KeyCode::KeyR) {
        water.depth.iter_mut().for_each(|d| *d = 0.0);
        water.ripple.iter_mut().for_each(|r| *r = 0.0);
        water.rvel.iter_mut().for_each(|v| *v = 0.0);
    }
}

/// Add `amount` water depth over a patch and kick the ripple velocity there.
fn add_water(water: &mut Water, cx: usize, cz: usize, r: i32, amount: f32, ripple_kick: f32) {
    for dz in -r..=r {
        for dx in -r..=r {
            let x = cx as i32 + dx;
            let z = cz as i32 + dz;
            if x < 0 || z < 0 || x >= W as i32 || z >= D as i32 {
                continue;
            }
            let i = idx(x as usize, z as usize);
            water.depth[i] += amount;
            water.rvel[i] += ripple_kick;
        }
    }
}

/// Shallow "water finds its level" flow. Each column distributes water to its
/// lower-surface neighbours, capped so it never sends more than it holds — so
/// water flows downhill, pools in the bowl, and settles to a flat surface.
/// Mass-conserving via a delta buffer (all transfers applied at once).
fn step_flow(terrain: Res<Terrain>, obstacle: Res<Obstacle>, paused: Res<Paused>, mut water: ResMut<Water>) {
    if paused.0 {
        return;
    }
    const NB: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    let t = &terrain.0;
    let obs = &obstacle.0;
    // Effective floor = terrain raised by any grounded-object obstacle.
    let floor = |i: usize| t[i] + obs[i];

    // Accumulate the net water movement (current) per cell across all iterations.
    let mut flow = vec![Vec2::ZERO; W * D];

    for _ in 0..FLOW_ITERS {
        let d = water.depth.clone();
        let mut delta = vec![0.0f32; W * D];

        for z in 0..D {
            for x in 0..W {
                let i = idx(x, z);
                let avail = d[i];
                if avail <= 0.0 {
                    continue;
                }
                let floor_i = floor(i);
                let si = floor_i + d[i];

                let mut lower: [(usize, f32, Vec2); 4] = [(0, 0.0, Vec2::ZERO); 4];
                let mut count = 0;
                let mut total_gap = 0.0;
                for (dx, dz) in NB {
                    let nx = x as i32 + dx;
                    let nz = z as i32 + dz;
                    if nx < 0 || nz < 0 || nx >= W as i32 || nz >= D as i32 {
                        continue;
                    }
                    let j = idx(nx as usize, nz as usize);
                    let fj = floor(j);
                    let sj = fj + d[j];
                    // Weir flux: water can only move over the higher of the two
                    // floors (the "sill"). The drivable head is the surface above
                    // that sill, so a tall obstacle dams the flow until water backs
                    // up to its crest, then spills only the thin overtopping layer.
                    // (The old `si - sj` gap dumped the whole column over a block in
                    // one step — draining the crest to a dry dip and surging below.)
                    let sill = floor_i.max(fj);
                    let head_i = (si - sill).max(0.0);
                    let head_j = (sj - sill).max(0.0);
                    if head_i > head_j {
                        let gap = head_i - head_j;
                        lower[count] = (j, gap, Vec2::new(dx as f32, dz as f32));
                        total_gap += gap;
                        count += 1;
                    }
                }
                if count == 0 {
                    continue;
                }

                // Move ~half each gap; scale down so total outflow ≤ avail.
                let desired = total_gap * 0.5 * FLOW_RATE;
                let scale = if desired > avail { avail / desired } else { 1.0 };
                for &(j, gap, dir) in lower.iter().take(count) {
                    let out = gap * 0.5 * FLOW_RATE * scale;
                    delta[i] -= out;
                    delta[j] += out;
                    flow[i] += dir * out; // water leaving cell i in this direction
                }
            }
        }

        for i in 0..W * D {
            water.depth[i] = (water.depth[i] + delta[i]).max(0.0);
        }
    }

    // Outflow: water reaching the low front edge runs off, so the channel keeps
    // a sustained downhill current instead of pooling to a standstill.
    for x in 0..W {
        water.depth[idx(x, D - 1)] = 0.0;
    }

    water.flow = flow;
}

/// Advance the visual-only ripple field (a damped wave equation) on wet cells.
/// Dry cells are reset so ripples never linger on bare terrain.
fn step_ripples(paused: Res<Paused>, mut water: ResMut<Water>) {
    if paused.0 {
        return;
    }
    let depth = water.depth.clone();
    let r = water.ripple.clone();
    for z in 0..D {
        for x in 0..W {
            let i = idx(x, z);
            if depth[i] < WET {
                water.ripple[i] = 0.0;
                water.rvel[i] = 0.0;
                continue;
            }
            let l = r[idx(x.saturating_sub(1), z)];
            let ri = r[idx((x + 1).min(W - 1), z)];
            let u = r[idx(x, z.saturating_sub(1))];
            let dn = r[idx(x, (z + 1).min(D - 1))];
            let lap = (l + ri + u + dn) * 0.25 - r[i];
            water.rvel[i] = (water.rvel[i] + lap * RIPPLE_SPEED) * RIPPLE_DAMP;
        }
    }
    for i in 0..W * D {
        water.ripple[i] = (water.ripple[i] + water.rvel[i]).clamp(-MAX_RIPPLE, MAX_RIPPLE);
    }
}

/// Rebuild the water surface mesh from depth (+ visual ripple). Vertex height =
/// terrain + depth + ripple; only quads with water are emitted (clean shore).
fn update_water_mesh(
    terrain: Res<Terrain>,
    obstacle: Res<Obstacle>,
    water: Res<Water>,
    handle: Res<WaterMesh>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(mesh) = meshes.get_mut(handle.0.id()) else { return };
    let t = &terrain.0;
    let obs = &obstacle.0;
    let d = &water.depth;
    let off = half();

    // Surface elevation per vertex; ripple fades out in shallow water. Water rests
    // on the obstacle top (terrain + obstacle), matching the flow sim, so water that
    // crests a block is drawn on top of it as a continuous sheet rather than dropping
    // back to bare ground. Dry obstacle cells stay below WET and aren't emitted, so
    // there's no water "tent" over an un-flooded block.
    let surf = |i: usize| {
        let fade = (d[i] / RIPPLE_FADE).clamp(0.0, 1.0);
        t[i] + obs[i] + d[i] + water.ripple[i] * fade
    };

    let mut positions = vec![[0.0f32; 3]; W * D];
    let mut normals = vec![[0.0f32, 1.0, 0.0]; W * D];
    let mut colors = vec![[0.0f32; 4]; W * D];
    // Depth shading: shallow water is light and clear, deep water dark and more opaque.
    let shallow = Color::srgba(0.42, 0.64, 0.86, 0.42).to_linear();
    let deep = Color::srgba(0.02, 0.16, 0.40, 0.85).to_linear();
    for z in 0..D {
        for x in 0..W {
            let i = idx(x, z);
            positions[i] = [x as f32 * CELL - off, surf(i), z as f32 * CELL - off];
            let hl = surf(idx(x.saturating_sub(1), z));
            let hr = surf(idx((x + 1).min(W - 1), z));
            let hu = surf(idx(x, z.saturating_sub(1)));
            let hd = surf(idx(x, (z + 1).min(D - 1)));
            let n = Vec3::new(hl - hr, 2.0 * CELL, hu - hd).normalize();
            normals[i] = [n.x, n.y, n.z];
            let dt = (d[i] / DEPTH_COLOR_MAX).clamp(0.0, 1.0);
            colors[i] = [
                shallow.red + (deep.red - shallow.red) * dt,
                shallow.green + (deep.green - shallow.green) * dt,
                shallow.blue + (deep.blue - shallow.blue) * dt,
                shallow.alpha + (deep.alpha - shallow.alpha) * dt,
            ];
        }
    }

    let mut indices: Vec<u32> = Vec::new();
    for z in 0..D - 1 {
        for x in 0..W - 1 {
            let v00 = idx(x, z);
            let v10 = idx(x + 1, z);
            let v01 = idx(x, z + 1);
            let v11 = idx(x + 1, z + 1);
            let wet = d[v00].max(d[v10]).max(d[v01]).max(d[v11]) > WET;
            if !wet {
                continue;
            }
            indices.extend_from_slice(&[
                v00 as u32, v01 as u32, v11 as u32, v00 as u32, v11 as u32, v10 as u32,
            ]);
        }
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}

/// Build the static terrain mesh (full grid, slope-shaded normals).
fn build_terrain_mesh(t: &[f32]) -> Mesh {
    let off = half();
    let mut positions = vec![[0.0f32; 3]; W * D];
    let mut normals = vec![[0.0f32, 1.0, 0.0]; W * D];
    let mut uvs = vec![[0.0f32; 2]; W * D];
    // Height gradient: light brown in the low streambed → green on the high
    // banks, so elevation reads clearly.
    let mut colors = vec![[1.0f32; 4]; W * D];
    let sand = Color::srgb(0.80, 0.66, 0.44).to_linear();
    let green = Color::srgb(0.38, 0.52, 0.26).to_linear();
    // Span the gradient over the actual height range, so any level heightmap
    // (not just the built-in valley) shades low → high correctly.
    let max_h = t.iter().fold(0.0f32, |a, &b| a.max(b)).max(1.0);
    for z in 0..D {
        for x in 0..W {
            let i = idx(x, z);
            positions[i] = [x as f32 * CELL - off, t[i], z as f32 * CELL - off];
            let hl = t[idx(x.saturating_sub(1), z)];
            let hr = t[idx((x + 1).min(W - 1), z)];
            let hu = t[idx(x, z.saturating_sub(1))];
            let hd = t[idx(x, (z + 1).min(D - 1))];
            let n = Vec3::new(hl - hr, 2.0 * CELL, hu - hd).normalize();
            normals[i] = [n.x, n.y, n.z];
            uvs[i] = [0.0, 0.0];
            let g = (t[i] / max_h).clamp(0.0, 1.0);
            colors[i] = [
                sand.red + (green.red - sand.red) * g,
                sand.green + (green.green - sand.green) * g,
                sand.blue + (green.blue - sand.blue) * g,
                1.0,
            ];
        }
    }
    let mut indices: Vec<u32> = Vec::with_capacity((W - 1) * (D - 1) * 6);
    for z in 0..D - 1 {
        for x in 0..W - 1 {
            let v00 = idx(x, z) as u32;
            let v10 = idx(x + 1, z) as u32;
            let v01 = idx(x, z + 1) as u32;
            let v11 = idx(x + 1, z + 1) as u32;
            indices.extend_from_slice(&[v00, v01, v11, v00, v11, v10]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    /// Regenerate the shipped valley level files from the procedural terrain:
    ///   cargo test generate_valley_level -- --ignored
    /// Ignored by default so a plain `cargo test` never rewrites level assets.
    #[test]
    #[ignore]
    fn generate_valley_level() {
        let level = builtin_level();
        level::write_template("levels/valley.yaml", &level, W, D).unwrap();
    }

    /// Bake a height function out as an editable level (yaml + heightmap PNG).
    fn bake(
        path: &str,
        name: &str,
        h: &dyn Fn(usize, usize) -> f32,
        source: SourceConfig,
        objects: Vec<ObjectConfig>,
    ) {
        let heights = (0..W * D).map(|i| h(i % W, i / W)).collect();
        let level = LoadedLevel { name: name.into(), heights, source, objects };
        level::write_template(path, &level, W, D).unwrap();
    }

    /// Map a cell x / z to the 0..1 map fraction used by level files.
    fn fx(cx: f32) -> f32 {
        cx / (W - 1) as f32
    }
    fn fz(cz: f32) -> f32 {
        cz / (D - 1) as f32
    }

    /// Regenerate the extra shipped levels (winding river, river delta,
    /// highland lake):
    ///   cargo test generate_extra_levels -- --ignored
    /// Like the valley, these are seeds for hand-editing — the PNGs on disk
    /// are the source of truth at runtime.
    #[test]
    #[ignore]
    fn generate_extra_levels() {
        let mid = (W - 1) as f32 * 0.5;
        let dmax = (D - 1) as f32;

        // --- Winding River: tighter, deeper S-curves than the valley. The
        // current whips around four full bends; light blocks race them, the
        // heavy block grounds at a bend and forces the water over its banks.
        let center = |z: f32| mid + 42.0 * (z * 2.0 * PI * 4.0 / D as f32).sin();
        let winding = |x: usize, z: usize| {
            let slope = (1.0 - z as f32 / dmax) * 55.0;
            let over = ((x as f32 - center(z as f32)).abs() - 6.5).max(0.0);
            slope + (over * over * 0.05).min(60.0) + edge_rim(x, z)
        };
        bake(
            "levels/winding-river.yaml",
            "Winding River",
            &winding,
            SourceConfig { x: fx(center(4.0)), z: fz(4.0), radius: 4, rate: 120.0 },
            vec![
                ObjectConfig { x: fx(center(0.20 * dmax)), z: 0.20, weight: 150.0 },
                ObjectConfig { x: fx(center(0.45 * dmax)), z: 0.45, weight: 800.0 },
                ObjectConfig { x: fx(center(0.65 * dmax)), z: 0.65, weight: 3000.0 },
            ],
        );

        // --- River Delta: one stem meanders down a steep upper valley, then
        // splits into three distributaries fanning across a flat marshy mouth.
        // Banks shrink across the fan, so backed-up water spills between the
        // branches. The heavy block sits right on the split — nudge the flow.
        let split = 0.40; // map fraction where the stem divides
        let main_c = |z: f32| mid + 10.0 * (z * 2.0 * PI / D as f32).sin();
        let stem_end = main_c(split * dmax);
        let targets = [0.16 * (W - 1) as f32, mid, 0.84 * (W - 1) as f32];
        // 0 → just split, 1 → river mouth, smoothstepped so branches peel away gently.
        let fan = move |zf: f32| {
            let t = ((zf - split) / (1.0 - split)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let branch_x = move |k: usize, zf: f32| stem_end + (targets[k] - stem_end) * fan(zf);
        let delta = |x: usize, z: usize| {
            let zf = z as f32 / dmax;
            let slope = 60.0 * (1.0 - zf).powf(1.6); // steep valley, flat fan
            let dist = if zf < split {
                (x as f32 - main_c(z as f32)).abs()
            } else {
                (0..3).map(|k| (x as f32 - branch_x(k, zf)).abs()).fold(f32::MAX, f32::min)
            };
            let s = fan(zf);
            let over = (dist - (9.0 - 3.5 * s)).max(0.0); // branches narrower than the stem
            let bank_max = 55.0 - 37.0 * s; // banks fade out across the fan
            slope + (over * over * 0.045).min(bank_max) + edge_rim(x, z)
        };
        bake(
            "levels/river-delta.yaml",
            "River Delta",
            &delta,
            SourceConfig { x: fx(main_c(4.0)), z: fz(4.0), radius: 5, rate: 140.0 },
            vec![
                ObjectConfig { x: fx(stem_end), z: split, weight: 2500.0 },
                ObjectConfig { x: fx(branch_x(0, 0.75)), z: 0.75, weight: 150.0 },
                ObjectConfig { x: fx(branch_x(2, 0.75)), z: 0.75, weight: 300.0 },
            ],
        );

        // --- Highland Lake: the source fills a deep basin behind a ridge.
        // The only way out is a narrow spill notch — the lake rises to the
        // sill, then pours down a guided runout channel. A heavy block starts
        // as a plug in the notch; floats ride the lake up and out.
        let ridge_z = 0.67 * dmax;
        let lake = |x: usize, z: usize| {
            let (xf, zf) = (x as f32, z as f32);
            let mut h = (1.0 - zf / dmax) * 35.0 + 20.0;
            // Basin: a paraboloid dip, floor well below the spill sill.
            let r2 = (xf - mid).powi(2) + (zf - 0.37 * dmax).powi(2);
            h -= 26.0 * (1.0 - r2 / (38.0f32 * 38.0)).max(0.0);
            // Ridge wall with a notch at the centre — the spillway sill.
            let ridge = 60.0 * (-((zf - ridge_z) / 7.0).powi(2)).exp();
            let notch = 1.0 - 0.92 * (-((xf - mid) / 7.0).powi(2)).exp();
            h += ridge * notch;
            // Runout banks guiding the spill from the notch to the drain.
            if zf > ridge_z {
                let over = ((xf - mid).abs() - 7.0).max(0.0);
                let guide = ((zf - ridge_z) / 30.0).clamp(0.0, 1.0);
                h += (over * over * 0.03).min(25.0) * guide;
            }
            h + edge_rim(x, z)
        };
        bake(
            "levels/highland-lake.yaml",
            "Highland Lake",
            &lake,
            SourceConfig { x: 0.5, z: fz(4.0), radius: 5, rate: 120.0 },
            vec![
                ObjectConfig { x: 0.45, z: 0.32, weight: 150.0 },
                ObjectConfig { x: 0.56, z: 0.40, weight: 300.0 },
                ObjectConfig { x: 0.5, z: fz(ridge_z), weight: 2000.0 },
            ],
        );
    }

    /// Regenerate the cliff levels (waterfall, cascades):
    ///   cargo test generate_cliff_levels -- --ignored
    /// A cliff is just a hard edge in the heightmap — one cell high, the next
    /// one far lower. The weir flux in `step_flow` spills the water over the
    /// lip, and the water mesh stretches from the lip down to the pool below,
    /// which draws the falling sheet for free.
    #[test]
    #[ignore]
    fn generate_cliff_levels() {
        let mid = (W - 1) as f32 * 0.5;
        let dmax = (D - 1) as f32;
        // Smooth 0→1 ramp, used to blend channels and pools into the ground.
        let smooth = |t: f32| {
            let t = t.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };

        // --- Waterfall: a river crosses a high plateau and pours off a ~60 wu
        // cliff into a plunge pool, then winds down a lower valley to the drain.
        let cliff_z = 0.42 * dmax; // last row of the plateau (the lip)
        let pool_z = cliff_z + 11.0; // centre of the plunge pool
        let upper_c = move |z: f32| mid + 18.0 * (PI * z / cliff_z).sin();
        let lower_c = move |z: f32| mid + 22.0 * (1.5 * PI * (z - cliff_z) / (dmax - cliff_z)).sin();
        let waterfall = |x: usize, z: usize| {
            let (xf, zf) = (x as f32, z as f32);
            let h = if zf <= cliff_z {
                // Plateau: gentle tilt toward the lip, river cut into high banks.
                let base = 95.0 + 12.0 * (1.0 - zf / cliff_z);
                let over = ((xf - upper_c(zf)).abs() - 7.0).max(0.0);
                base + (over * over * 0.05).min(30.0)
            } else {
                // Lower valley: starts ~60 below the lip and slopes to the drain.
                let base = 35.0 * (1.0 - (zf - cliff_z) / (dmax - cliff_z));
                let over = ((xf - lower_c(zf)).abs() - 9.0).max(0.0);
                let bank = (over * over * 0.04).min(35.0);
                // Plunge pool: a bowl at the cliff foot; banks fade out inside it.
                let r = ((xf - mid).powi(2) + (zf - pool_z).powi(2)).sqrt();
                let pool = 1.0 - smooth(r / 16.0);
                base + bank * (1.0 - pool) - 18.0 * pool
            };
            h.max(0.0) + edge_rim(x, z)
        };
        bake(
            "levels/waterfall.yaml",
            "Waterfall",
            &waterfall,
            SourceConfig { x: fx(upper_c(4.0)), z: fz(4.0), radius: 5, rate: 120.0 },
            vec![
                ObjectConfig { x: fx(upper_c(0.20 * dmax)), z: 0.20, weight: 150.0 },
                ObjectConfig { x: fx(upper_c(0.34 * dmax)), z: 0.34, weight: 300.0 },
                ObjectConfig { x: fx(lower_c(0.80 * dmax)), z: 0.80, weight: 2500.0 },
            ],
        );

        // --- Cascades: four terraces stepping down ~25 wu each. Every lip has
        // a notch on alternating sides, so the stream zig-zags across the map,
        // dropping into a small pool at the foot of each fall.
        let edges = [0.0, 0.28 * dmax, 0.50 * dmax, 0.72 * dmax, dmax]; // terrace boundaries (rows)
        let bases = [100.0, 72.0, 44.0, 16.0]; // height at each terrace's lip
        let xs = [0.5, 0.33, 0.67, 0.33, 0.5].map(|f| f * (W - 1) as f32); // stream x at each boundary
        let cascades = |x: usize, z: usize| {
            let (xf, zf) = (x as f32, z as f32);
            let k = (0..4).rev().find(|&k| zf >= edges[k]).unwrap_or(0);
            let t = (zf - edges[k]) / (edges[k + 1] - edges[k]); // 0 at the foot, 1 at the lip
            let base = bases[k] + 5.0 * (1.0 - t); // slight tilt toward the lip
            // Channel bends from where the last fall landed over to this lip's notch.
            let c = xs[k] + (xs[k + 1] - xs[k]) * smooth(t);
            let over = ((xf - c).abs() - 6.0).max(0.0);
            let bank = (over * over * 0.05).min(22.0);
            // Small plunge pool just below each fall (not on the top terrace).
            let pool = if k > 0 {
                let r = ((xf - xs[k]).powi(2) + (zf - edges[k] - 5.0).powi(2)).sqrt();
                1.0 - smooth(r / 9.0)
            } else {
                0.0
            };
            (base + bank * (1.0 - pool) - 8.0 * pool).max(0.0) + edge_rim(x, z)
        };
        bake(
            "levels/cascades.yaml",
            "Cascades",
            &cascades,
            SourceConfig { x: fx(xs[0]), z: fz(4.0), radius: 5, rate: 120.0 },
            vec![
                ObjectConfig { x: fx(xs[1]), z: 0.24, weight: 150.0 },
                ObjectConfig { x: fx(xs[2]), z: 0.47, weight: 300.0 },
                ObjectConfig { x: fx(xs[3]), z: 0.69, weight: 2000.0 },
            ],
        );
    }

    /// The built-in valley round-trips through the level template format: the
    /// heights baked to PNG come back (within 16-bit quantisation) on load.
    #[test]
    fn test_builtin_level_round_trip() {
        let dir = std::env::temp_dir().join("bluerush_flood_test");
        let path = dir.join("valley.yaml");
        let level = builtin_level();
        level::write_template(path.to_str().unwrap(), &level, W, D).unwrap();
        let loaded = level::load(path.to_str().unwrap(), W, D).unwrap();
        assert_eq!(loaded.objects.len(), level.objects.len());
        let max_err = level
            .heights
            .iter()
            .zip(&loaded.heights)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(max_err < 0.01, "max height error after round trip: {max_err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
