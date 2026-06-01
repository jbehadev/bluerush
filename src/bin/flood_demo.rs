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
const SLOPE_HEIGHT: f32 = 45.0; // channel floor drops this much from back (z=0) to front
const WALL_HEIGHT: f32 = 45.0; // side walls keep the flow inside the channel
const REF_HEIGHT: f32 = 30.0; // a mid height, used to aim mouse rays at the channel

const SOURCE_RATE: f32 = 110.0; // water depth/sec added at the source (spread over a patch)
const SOURCE_R: i32 = 3; // source patch radius (wider = gentler, no spike)
const POUR_RATE: f32 = 220.0; // water depth/sec added under the mouse

// Weighted objects the flood pushes and floats.
const OBJ_SIZE: f32 = 9.0;
const BUOYANCY: f32 = 400.0; // water depth × this = weight it can float (generous; contrast comes from mobility)
const DRAFT: f32 = 2.5; // how deep a floating object sits below the surface
const VERT_EASE: f32 = 0.15; // how fast an object eases toward its target height
const FLOW_TO_SPEED: f32 = 80.0; // converts the local current into a drift speed
const FLOW_EASE: f32 = 0.12; // how fast an object's velocity matches the current
const REF_WEIGHT: f32 = 150.0; // a "light" object; mobility = REF_WEIGHT / weight (capped at 1)
const COLLIDE_DIST: f32 = OBJ_SIZE; // objects closer than this push apart
const OBSTACLE_HEIGHT: f32 = 9.0; // how high a grounded object dams the water column
const OBSTACLE_R: i32 = 1; // grounded-object footprint radius (cells) for damming
const FLOW_RATE: f32 = 0.5; // fraction of the surface gap equalised per iteration
const FLOW_ITERS: usize = 8; // flow iterations per frame (faster spreading = no spike)
const DT: f32 = 1.0 / 60.0;
const WET: f32 = 0.15; // depth below which a column is treated as dry

// Visual-only ripple layer.
const RIPPLE_SPEED: f32 = 0.25;
const RIPPLE_DAMP: f32 = 0.96;
const MAX_RIPPLE: f32 = 4.0;
const RIPPLE_FADE: f32 = 5.0; // ripples fade out in water shallower than this

fn span() -> f32 {
    (W - 1) as f32 * CELL
}
fn half() -> f32 {
    span() * 0.5
}
fn idx(x: usize, z: usize) -> usize {
    z * W + x
}

/// Channel terrain: a floor sloping downhill from the back (z=0, high) to the
/// front (z=D-1, low), with raised U-shaped side walls so the flow stays in the
/// channel. Water poured at the top runs down it as a sustained current.
fn terrain_height(x: usize, z: usize) -> f32 {
    let slope = (1.0 - z as f32 / (D - 1) as f32) * SLOPE_HEIGHT;
    let cx = (W - 1) as f32 * 0.5;
    let wall = ((x as f32 - cx).abs() / cx).powi(2) * WALL_HEIGHT;
    slope + wall
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

/// Object colour by weight: light = pale wood, heavy = dark stone.
fn weight_color(weight: f32) -> Color {
    let t = (weight / 4000.0).clamp(0.0, 1.0).sqrt();
    let c = 0.82 - t * 0.58;
    Color::srgb(c, c * 0.9, c * 0.75)
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
    commands.spawn((
        FloatObject { pos, vel: Vec2::ZERO, weight, y },
        Mesh3d(cube),
        MeshMaterial3d(materials.add(weight_color(weight))),
        Transform::from_xyz(pos.x, y + OBJ_SIZE * 0.5, pos.y),
    ));
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "BlueRush — Flood Demo (hold LMB to pour, R to drain)".into(),
                resolution: (1000u32, 820u32).into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.45, 0.62, 0.78)))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                drain_on_key,
                source_and_pour,
                right_click_drop,
                build_obstacles,
                step_flow,
                step_ripples,
                object_physics,
                object_collision,
                sync_objects,
                update_water_mesh,
            )
                .chain(),
        )
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 360.0, 470.0).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
    ));
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
            base_color: Color::srgb(0.78, 0.64, 0.43),
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
    mesh.insert_indices(Indices::U32(Vec::new()));
    let water_handle = meshes.add(mesh);
    commands.spawn((
        Mesh3d(water_handle.clone()),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.05, 0.42, 0.70, 0.55),
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
    // Heavy block in the channel centre (it grounds and dams the flow); a light
    // and a medium block to the sides get washed around it.
    let cube = meshes.add(Cuboid::new(OBJ_SIZE, OBJ_SIZE, OBJ_SIZE));
    spawn_object(&mut commands, cube.clone(), &mut materials, Vec2::new(0.0, 0.0), 4000.0);
    spawn_object(&mut commands, cube.clone(), &mut materials, Vec2::new(-50.0, -60.0), 150.0);
    spawn_object(&mut commands, cube.clone(), &mut materials, Vec2::new(50.0, -60.0), 800.0);

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
        for dz in -OBSTACLE_R..=OBSTACLE_R {
            for dx in -OBSTACLE_R..=OBSTACLE_R {
                let x = gx as i32 + dx;
                let z = gz as i32 + dz;
                if x < 0 || z < 0 || x >= W as i32 || z >= D as i32 {
                    continue;
                }
                let c = idx(x as usize, z as usize);
                obstacle.0[c] = obstacle.0[c].max(OBSTACLE_HEIGHT);
            }
        }
    }
}

/// Separate overlapping objects (mass-weighted): the lighter one gets shoved
/// more, so a heavy block holds its ground and others pile against it.
fn object_collision(mut q: Query<(Entity, &mut FloatObject)>) {
    let items: Vec<(Entity, Vec2, f32)> = q.iter().map(|(e, o)| (e, o.pos, o.weight)).collect();
    let n = items.len();
    let mut corr: HashMap<Entity, Vec2> = HashMap::new();

    for a in 0..n {
        for b in (a + 1)..n {
            let d = items[a].1 - items[b].1;
            let dist = d.length();
            if dist < COLLIDE_DIST && dist > 1e-4 {
                let overlap = COLLIDE_DIST - dist;
                let dir = d / dist;
                let (wa, wb) = (items[a].2, items[b].2);
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

/// Right-click drops a light object at the cursor (fun to watch it drift).
fn right_click_drop(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    assets: Res<ObjectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    if !mouse.just_pressed(MouseButton::Right) {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Ok((camera, cam_t)) = cameras.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
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
    spawn_object(&mut commands, assets.cube.clone(), &mut materials, Vec2::new(hit.x, hit.z), 200.0);
}

/// Buoyancy + flow-push for every object. An object floats when the water is
/// deep enough to support its weight; while floating it's carried along the
/// water-surface gradient, with heavier objects resisting the current more.
fn object_physics(terrain: Res<Terrain>, water: Res<Water>, mut q: Query<&mut FloatObject>) {
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

/// Copy each object's logical position onto its rendered cube.
fn sync_objects(mut q: Query<(&FloatObject, &mut Transform)>) {
    for (obj, mut tf) in &mut q {
        tf.translation = Vec3::new(obj.pos.x, obj.y + OBJ_SIZE * 0.5, obj.pos.y);
    }
}

/// Add water at the central source every frame, plus under the mouse when held.
/// Each injection also kicks the ripple field so the inflow looks alive.
fn source_and_pour(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut water: ResMut<Water>,
) {
    // Steady source at the top of the channel (back edge).
    add_water(&mut water, W / 2, 4, SOURCE_R, SOURCE_RATE * DT, -0.8);

    // Pour under the cursor while the left button is held.
    if !mouse.pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Ok((camera, cam_t)) = cameras.single() else { return };
    let Some(cursor) = window.cursor_position() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_t, cursor) else { return };
    let dir = *ray.direction;
    if dir.y.abs() < 1e-5 {
        return;
    }
    let plane_y = REF_HEIGHT * 0.5; // aim at the channel so pours land inside
    let t = (plane_y - ray.origin.y) / dir.y;
    if t < 0.0 {
        return;
    }
    let hit = ray.origin + dir * t;
    let off = half();
    let gx = ((hit.x + off) / CELL).round() as i32;
    let gz = ((hit.z + off) / CELL).round() as i32;
    if gx >= 0 && gz >= 0 && gx < W as i32 && gz < D as i32 {
        add_water(&mut water, gx as usize, gz as usize, 3, POUR_RATE * DT, -2.0);
    }
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
fn step_flow(terrain: Res<Terrain>, obstacle: Res<Obstacle>, mut water: ResMut<Water>) {
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
                let si = floor(i) + d[i];

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
                    let sj = floor(j) + d[j];
                    if si > sj {
                        let gap = si - sj;
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
fn step_ripples(mut water: ResMut<Water>) {
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
    water: Res<Water>,
    handle: Res<WaterMesh>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(mesh) = meshes.get_mut(handle.0.id()) else { return };
    let t = &terrain.0;
    let d = &water.depth;
    let off = half();

    // Surface elevation per vertex; ripple fades out in shallow water.
    let surf = |i: usize| {
        let fade = (d[i] / RIPPLE_FADE).clamp(0.0, 1.0);
        t[i] + d[i] + water.ripple[i] * fade
    };

    let mut positions = vec![[0.0f32; 3]; W * D];
    let mut normals = vec![[0.0f32, 1.0, 0.0]; W * D];
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
    mesh.insert_indices(Indices::U32(indices));
}

/// Build the static terrain mesh (full grid, slope-shaded normals).
fn build_terrain_mesh(t: &[f32]) -> Mesh {
    let off = half();
    let mut positions = vec![[0.0f32; 3]; W * D];
    let mut normals = vec![[0.0f32, 1.0, 0.0]; W * D];
    let mut uvs = vec![[0.0f32; 2]; W * D];
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
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
