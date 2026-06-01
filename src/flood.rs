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

// ---------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------

const W: usize = 100;
const D: usize = 100;
const CELL: f32 = 6.0;
const SLOPE_HEIGHT: f32 = 45.0; // stream bed drops this much from back (z=0) to front
const MEANDER_AMPL: f32 = 26.0; // how far (cells) the stream snakes left/right
const MEANDER_FREQ: f32 = 2.0 * std::f32::consts::PI * 1.5 / D as f32; // ~1.5 S-curves down the length
const CHANNEL_HW: f32 = 7.0; // half-width (cells) of the low stream bed
const BANK_K: f32 = 0.04; // how steeply the banks rise beyond the channel
const BANK_MAX: f32 = 55.0; // cap on bank height
const REF_HEIGHT: f32 = 30.0; // a mid height, used to aim mouse rays at the channel

const SOURCE_RATE: f32 = 110.0; // water depth/sec added at the source (spread over a patch)
const SOURCE_R: i32 = 3; // source patch radius (wider = gentler, no spike)
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
fn terrain_height(x: usize, z: usize) -> f32 {
    let slope = (1.0 - z as f32 / (D - 1) as f32) * SLOPE_HEIGHT;
    let dist = (x as f32 - channel_center(z)).abs();
    let over = (dist - CHANNEL_HW).max(0.0);
    let bank = (over * over * BANK_K).min(BANK_MAX);
    slope + bank
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Resource)]
struct Terrain(Vec<f32>);

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

fn spawn_object(
    commands: &mut Commands,
    cube: Handle<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    pos: Vec2,
    weight: f32,
) {
    let (gx, gz) = cell_of(pos);
    let y = terrain_height(gx, gz);
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
pub struct FloodPlugin;

impl Plugin for FloodPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.45, 0.62, 0.78)))
            .insert_resource(SelectedTool::Object(500.0))
            .insert_resource(Wave { pattern: WavePattern::Flood, rng_level: 1.0, since_roll: 0.0 })
            .insert_resource(Paused(false))
            .add_systems(Startup, (setup, setup_ui))
            // UI / camera / input handling (order-independent).
            .add_systems(
                Update,
                (
                    handle_weight_buttons,
                    handle_pour_button,
                    handle_erase_button,
                    handle_wave_buttons,
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

    // Terrain heights + a static terrain mesh (the bowl).
    let terrain: Vec<f32> = (0..W * D).map(|i| terrain_height(i % W, i / W)).collect();
    let terrain_mesh = build_terrain_mesh(&terrain);
    commands.spawn((
        Mesh3d(meshes.add(terrain_mesh)),
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

    // Object spawning assets + a starter trio (light / medium / heavy) so the
    // weight difference is visible as the bowl floods.
    // Objects sitting IN the stream bed: a heavy block mid-stream (grounds and
    // dams the flow), a light block upstream (washes down the meander), and a
    // medium block further downstream.
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0)); // unit cube, scaled per object by weight
    spawn_object(&mut commands, cube.clone(), &mut materials,
        cell_to_world(channel_center(D / 2), (D / 2) as f32), 4000.0);
    spawn_object(&mut commands, cube.clone(), &mut materials,
        cell_to_world(channel_center(D / 3), (D / 3) as f32), 150.0);
    spawn_object(&mut commands, cube.clone(), &mut materials,
        cell_to_world(channel_center(2 * D / 3), (2 * D / 3) as f32), 800.0);

    commands.insert_resource(Terrain(terrain));
    commands.insert_resource(water);
    commands.insert_resource(WaterMesh(water_handle));
    commands.insert_resource(ObjectAssets { cube });
    commands.insert_resource(Obstacle(vec![0.0; W * D]));
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
    mut water: ResMut<Water>,
    assets: Res<ObjectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    objects: Query<(Entity, &FloatObject)>,
    mut commands: Commands,
) {
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    if cursor.x < PANEL_WIDTH {
        return; // a click on the UI panel, not the world
    }
    let Ok((camera, cam_t)) = cameras.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_t, cursor) else { return };
    let dir = *ray.direction;
    if dir.y.abs() < 1e-5 {
        return;
    }
    let t = (REF_HEIGHT * 0.5 - ray.origin.y) / dir.y;
    if t < 0.0 {
        return;
    }
    let hit = ray.origin + dir * t;

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
                spawn_object(&mut commands, assets.cube.clone(), &mut materials, Vec2::new(hit.x, hit.z), w);
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

fn setup_ui(mut commands: Commands) {
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
            // Pause / Run toggle at the top.
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
    mut gizmos: Gizmos,
) {
    let Ok(window) = windows.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    if cursor.x < PANEL_WIDTH {
        return;
    }
    let Ok((camera, cam_t)) = cameras.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_t, cursor) else { return };
    let dir = *ray.direction;
    if dir.y.abs() < 1e-5 {
        return;
    }
    let t = (REF_HEIGHT * 0.5 - ray.origin.y) / dir.y;
    if t < 0.0 {
        return;
    }
    let hit = ray.origin + dir * t;
    let off = half();
    if hit.x.abs() > off || hit.z.abs() > off {
        return; // off the terrain
    }
    let (gx, gz) = cell_of(Vec2::new(hit.x, hit.z));
    let ground = terrain_height(gx, gz);

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

/// Add water at the central source every frame, plus under the mouse when held.
/// Each injection also kicks the ripple field so the inflow looks alive.
/// Feed the source at the top of the stream bed every frame, modulated by the
/// selected wave pattern (steady / pulsing / gusty).
fn run_source(time: Res<Time>, paused: Res<Paused>, mut wave: ResMut<Wave>, mut water: ResMut<Water>) {
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
    let sx = channel_center(4).round().clamp(0.0, (W - 1) as f32) as usize;
    add_water(&mut water, sx, 4, SOURCE_R, SOURCE_RATE * mult * DT, -0.8);
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
    let max_h = SLOPE_HEIGHT + BANK_MAX;
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
