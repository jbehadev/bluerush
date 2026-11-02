//! The water source, drawn as a rain cloud: a cluster of low-poly puffs
//! hovering over the source with rain streaking down onto the source patch.
//! On a level with `map_rain` (rain everywhere) the cloud instead spreads,
//! see-through, over the whole map and the rain falls all over it.
//!
//! Purely visual — the water itself is still added by `flood::run_source`.
//! This reads how hard it's raining (`Rainfall`) and shows it:
//!   * more raindrops the harder it rains (none when the wave is Off),
//!   * the cloud darkens from fair-weather white to storm grey.
//!
//! Raindrops are a fixed pool of entities. Each drop's position is worked out
//! from the time and its index (`drop_state`), not stored and stepped — so
//! there's no per-drop state to update, and a drop that hits the ground simply
//! reappears at the cloud somewhere new on its next fall.

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use std::f32::consts::TAU;

use crate::flood::{idx, rain_patch, smoothstep, Paused, Rainfall, Source, Terrain, Water, CELL, D, W};

const CLOUD_HEIGHT: f32 = 80.0; // how far above the source's ground the cloud floats
const CLOUD_CLEARANCE: f32 = 45.0; // …but always at least this far above the water under it
const CLOUD_RISE_RATE: f32 = 0.8; // how quickly (1/sec) the cloud eases up as the water rises
const CLOUD_SPREAD: f32 = 1.2; // cloud width relative to the rain patch
const DROPS: usize = 900; // raindrops used under a source's cloud (the rain covers a wide patch)
const MAP_DROPS: usize = 6000; // raindrops used when it rains on the whole map — the pool's size
const MAP_FALL: f32 = 320.0; // typical drop fall from the map-wide cloud (sets how fast drops cycle)
const MAP_CLOUD_LIFT: f32 = 250.0; // map-wide cloud height above the highest ground — above the usual camera
const MAP_CLOUD_FADE: f32 = 80.0; // the map-wide cloud fades out over this as the camera climbs past it
const MAP_CLOUD_COVER: f32 = 1.1; // map-wide cloud size relative to half the map's width
const MAP_CLOUD_FLAT: f32 = 0.15; // …and its thickness relative to its width
const MAP_CLOUD_CLEAR: f32 = 0.12; // map-wide cloud's opacity in fair weather…
const MAP_CLOUD_STORM: f32 = 0.35; // …and at the storm's height
const DROP_SPEED: f32 = 140.0; // world units / sec
const DROPS_AT_FULL: f32 = 0.45; // share of the pool falling at the level's full rate (surges use more)
const FAIR_WEATHER: Color = Color::srgb(0.95, 0.96, 0.98);
const STORM_GREY: Color = Color::srgb(0.42, 0.44, 0.50);

pub struct RainPlugin;

impl Plugin for RainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_cloud).add_systems(
            Update,
            // Source + Terrain come from flood's setup; wait for them.
            (place_cloud, animate_cloud, animate_rain)
                .chain()
                .run_if(resource_exists::<Source>)
                .run_if(resource_exists::<Terrain>)
                .run_if(resource_exists::<Water>),
        );
    }
}

/// Where the rain falls: under a source's cloud, or over the whole map.
#[derive(Clone, Copy, PartialEq, Debug)]
enum RainArea {
    Patch,
    Map,
}

impl RainArea {
    fn of(source: &Source) -> Self {
        if source.map_rain > 0.0 { RainArea::Map } else { RainArea::Patch }
    }

    /// How many drops of the pool this area uses.
    fn drops(self) -> usize {
        match self {
            RainArea::Patch => DROPS,
            RainArea::Map => MAP_DROPS,
        }
    }
}

/// Half the map's width in world units (the map is centred on the origin).
fn half_map() -> f32 {
    W as f32 * CELL * 0.5
}

/// The cloud's root entity; its puffs are children.
#[derive(Component)]
struct Cloud {
    material: Handle<StandardMaterial>,
    area: RainArea,
    /// Where the cloud sits on dry land (set when the level loads).
    home: Vec3,
    /// Height of the cloud's centre before the gentle bob. Climbs (smoothly)
    /// when the water piling up under the cloud would otherwise swallow it.
    base_y: f32,
}

/// One raindrop in the pool; `0` is its index, which seeds where it falls.
#[derive(Component)]
struct RainDrop(usize);

/// Low-poly puffs: (offset from the cloud centre, radius), as fractions of the
/// cloud's size. Flattened when spawned, so the cloud is wide and low.
const PUFFS: [(Vec3, f32); 7] = [
    (Vec3::new(0.0, 0.0, 0.0), 0.55),
    (Vec3::new(0.45, -0.05, 0.1), 0.42),
    (Vec3::new(-0.45, -0.05, -0.05), 0.45),
    (Vec3::new(0.15, 0.1, 0.4), 0.38),
    (Vec3::new(-0.2, 0.05, -0.4), 0.40),
    (Vec3::new(0.25, 0.25, -0.15), 0.35),
    (Vec3::new(-0.3, 0.2, 0.25), 0.32),
];

fn spawn_cloud(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut puff = Sphere::new(1.0).mesh().ico(1).expect("ico subdivision is small");
    puff.duplicate_vertices();
    puff.compute_flat_normals(); // faceted, like the trees
    let puff = meshes.add(puff);
    let material = materials.add(StandardMaterial {
        base_color: FAIR_WEATHER,
        perceptual_roughness: 1.0,
        ..default()
    });
    commands
        .spawn((Cloud { material: material.clone(), area: RainArea::Patch, home: Vec3::ZERO, base_y: 0.0 }, Transform::default(), Visibility::default()))
        .with_children(|cloud| {
            for (offset, radius) in PUFFS {
                cloud.spawn((
                    Mesh3d(puff.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::from_translation(offset).with_scale(Vec3::new(radius, radius * 0.6, radius)),
                ));
            }
        });

    // Thin, pale streaks. Translucent, and they don't cast shadows.
    let streak = meshes.add(Cuboid::new(0.25, 5.0, 0.25));
    let water = materials.add(StandardMaterial {
        base_color: Color::srgba(0.78, 0.85, 0.95, 0.45),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.3,
        ..default()
    });
    for i in 0..MAP_DROPS {
        commands.spawn((
            RainDrop(i),
            Mesh3d(streak.clone()),
            MeshMaterial3d(water.clone()),
            Transform::default(),
            Visibility::Hidden,
            NotShadowCaster,
        ));
    }
}

/// How wide the rain falls, in world units: the patch `run_source` pours on.
fn rain_radius(source: &Source) -> f32 {
    (rain_patch(source).0 as f32 + 0.5) * CELL
}

/// Where the cloud sits on dry land: over the source, `CLOUD_HEIGHT` above its ground.
fn cloud_center(source: &Source, terrain: &[f32]) -> Vec3 {
    let pos = crate::flood::cell_to_world(source.x as f32, source.z as f32);
    Vec3::new(pos.x, terrain[idx(source.x, source.z)] + CLOUD_HEIGHT, pos.y)
}

/// Move + size the cloud when the level changes: over the source, or — when
/// it rains on the whole map — over all of it, high enough that the usual
/// camera looks in under it, and see-through.
fn place_cloud(
    source: Res<Source>,
    terrain: Res<Terrain>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cloud: Query<(&mut Cloud, &mut Transform)>,
) {
    if !source.is_changed() && !terrain.is_changed() {
        return;
    }
    let area = RainArea::of(&source);
    for (mut cloud, mut t) in &mut cloud {
        let (home, scale, alpha_mode) = match area {
            RainArea::Patch => {
                (cloud_center(&source, &terrain.0), Vec3::splat(rain_radius(&source) * CLOUD_SPREAD), AlphaMode::Opaque)
            }
            RainArea::Map => {
                let top = terrain.0.iter().fold(0.0f32, |a, &b| a.max(b));
                let size = half_map() * MAP_CLOUD_COVER;
                (Vec3::new(0.0, top + MAP_CLOUD_LIFT, 0.0), Vec3::new(size, size * MAP_CLOUD_FLAT, size), AlphaMode::Blend)
            }
        };
        cloud.area = area;
        cloud.home = home;
        cloud.base_y = home.y;
        t.translation = home;
        t.scale = scale;
        if let Some(mut m) = materials.get_mut(&cloud.material) {
            m.alpha_mode = alpha_mode;
        }
    }
}

/// Slow turn + bob, keep clear of the water, and colour from fair-weather
/// white to storm grey with the rain.
fn animate_cloud(
    time: Res<Time>,
    rainfall: Res<Rainfall>,
    source: Res<Source>,
    terrain: Res<Terrain>,
    water: Res<Water>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera: Single<&GlobalTransform, With<Camera3d>>,
    mut cloud: Query<(&mut Cloud, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    let camera_y = camera.translation().y;
    let darkness = rainfall.0.clamp(0.0, 1.0);
    // Rain piles water up into a mound under the cloud (highest at the
    // source cell); stay `CLOUD_CLEARANCE` above it.
    let i = idx(source.x, source.z);
    let ease = 1.0 - (-time.delta_secs() * CLOUD_RISE_RATE).exp();
    for (mut cloud, mut transform) in &mut cloud {
        let (wanted_y, target) = match cloud.area {
            RainArea::Patch => (
                cloud.home.y.max(terrain.0[i] + water.depth[i] + CLOUD_CLEARANCE),
                FAIR_WEATHER.mix(&STORM_GREY, darkness),
            ),
            // Already above the highest ground; thickens as the storm builds,
            // and fades away if the camera climbs above it so the land shows.
            RainArea::Map => {
                let below = 1.0 - smoothstep(cloud.home.y - MAP_CLOUD_FADE, cloud.home.y, camera_y);
                let alpha = (MAP_CLOUD_CLEAR + (MAP_CLOUD_STORM - MAP_CLOUD_CLEAR) * darkness) * below;
                (cloud.home.y, FAIR_WEATHER.mix(&STORM_GREY, darkness).with_alpha(alpha))
            }
        };
        cloud.base_y += (wanted_y - cloud.base_y) * ease;
        transform.rotation = Quat::from_rotation_y(t * 0.03);
        transform.translation.y = cloud.base_y + (t * 0.4).sin() * 2.0;
        // Only touch the material when the colour actually moves: changing an
        // asset re-uploads it to the GPU.
        if let Some(mut m) = materials.get_mut(&cloud.material)
            && m.base_color.to_linear().to_vec4().distance(target.to_linear().to_vec4()) > 0.01
        {
            m.base_color = target;
        }
    }
}

/// Repeatable pseudo-random number in 0..1 for (drop, fall) — a different
/// spot under the cloud for every fall of every drop.
fn hash01(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9e37_79b9) ^ b.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

/// Where drop `i` is at time `t`, for a fall of `height`: its offset under the
/// cloud (x/z, inside a unit circle for a `Patch`, the square -1..1 for the
/// `Map`) and how far down it is (0 = at the cloud, 1 = at the ground). Each
/// drop starts its falls at a different moment, so they don't all land together.
fn drop_state(i: usize, t: f32, height: f32, area: RainArea) -> (Vec2, f32) {
    let falls = t * DROP_SPEED / height + hash01(i as u32, 0);
    let fall = falls.floor() as u32; // which fall this is — picks the spot
    let (a, b) = (hash01(i as u32, fall * 2 + 1), hash01(i as u32, fall * 2 + 2));
    let offset = match area {
        RainArea::Patch => Vec2::from_angle(a * TAU) * b.sqrt(), // sqrt → spread evenly over the disc
        RainArea::Map => Vec2::new(a, b) * 2.0 - 1.0,
    };
    (offset, falls.fract())
}

fn animate_rain(
    time: Res<Time>,
    paused: Res<Paused>,
    rainfall: Res<Rainfall>,
    source: Res<Source>,
    terrain: Res<Terrain>,
    water: Res<Water>,
    cloud: Single<&Cloud>,
    mut sim_time: Local<f32>,
    mut drops: Query<(&RainDrop, &mut Transform, &mut Visibility)>,
) {
    // Rain hangs in the air while paused, like the water.
    if !paused.0 {
        *sim_time += time.delta_secs();
    }
    let t = *sim_time;
    let start = cloud.base_y - 6.0; // from the cloud's underside
    // Centre, spread and fall height of the rain.
    let (centre, spread, fall) = match cloud.area {
        RainArea::Patch => (cloud.home, rain_radius(&source), CLOUD_HEIGHT),
        RainArea::Map => (Vec3::ZERO, half_map(), MAP_FALL),
    };
    // Drops hidden beyond this index: the pool fills as the rain gets heavier.
    let falling = (rainfall.0 * DROPS_AT_FULL * cloud.area.drops() as f32) as usize;
    for (drop, mut transform, mut vis) in &mut drops {
        if drop.0 >= falling {
            *vis = Visibility::Hidden;
            continue;
        }
        *vis = Visibility::Inherited;
        let (offset, down) = drop_state(drop.0, t, fall, cloud.area);
        let xz = Vec2::new(centre.x, centre.z) + offset * spread;
        // Fall to the surface under the drop — the water if there is any, else the ground.
        let (gx, gz) = crate::flood::cell_of(xz);
        let i = idx(gx.min(W - 1), gz.min(D - 1));
        let surface = terrain.0[i] + water.depth[i];
        transform.translation = Vec3::new(xz.x, start + (surface - start) * down, xz.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_fall_from_the_cloud_to_the_ground_and_stay_under_it() {
        for i in 0..50 {
            for step in 0..200 {
                let (offset, down) = drop_state(i, step as f32 * 0.037, CLOUD_HEIGHT, RainArea::Patch);
                assert!(offset.length() <= 1.0 + 1e-5, "drop strays outside the cloud");
                assert!((0.0..1.0).contains(&down));
                let (offset, _) = drop_state(i, step as f32 * 0.037, MAP_FALL, RainArea::Map);
                assert!(offset.abs().max_element() <= 1.0, "drop strays off the map");
            }
        }
    }

    #[test]
    fn a_drop_moves_down_as_time_passes() {
        let (_, a) = drop_state(7, 0.0, CLOUD_HEIGHT, RainArea::Patch);
        let (_, b) = drop_state(7, 0.01, CLOUD_HEIGHT, RainArea::Patch);
        // Unless it just landed and restarted at the top, it's further down.
        assert!(b > a || b < 0.05);
    }

    /// Map-wide rain reaches every corner of the map, not just the middle.
    #[test]
    fn map_rain_spreads_over_the_whole_map() {
        let mut quadrants = [0; 4];
        for i in 0..MAP_DROPS {
            let (o, _) = drop_state(i, 3.0, MAP_FALL, RainArea::Map);
            quadrants[(o.x > 0.0) as usize + 2 * (o.y > 0.0) as usize] += 1;
        }
        for q in quadrants {
            assert!(q > MAP_DROPS / 5, "rain should fall evenly over the map, got {quadrants:?}");
        }
    }

    #[test]
    fn map_rain_uses_the_map_wide_cloud() {
        let mut source = Source::default();
        assert_eq!(RainArea::of(&source), RainArea::Patch);
        source.map_rain = 0.6;
        assert_eq!(RainArea::of(&source), RainArea::Map);
        assert!(RainArea::Map.drops() > RainArea::Patch.drops());
    }
}
