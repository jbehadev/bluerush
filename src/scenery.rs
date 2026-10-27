//! Scenery — sets of decoration that dress the land when a level loads:
//! TREES (groves on the grassland), a CITY (a small grid town on flat, low
//! ground) and RUINS (broken walls and columns scattered on the hills).
//!
//! Which sets a level starts with comes from its yaml (`scenery: [trees, city]`,
//! see `docs/LEVELS.md`); the SCENERY buttons in the panel toggle them live.
//!
//! How it fits together:
//!   * `plan_scenery` is pure: terrain heights + the active sets in, a list of
//!     buildings / ruin pieces / trees out. Same terrain → same layout every run
//!     (randomness comes from `hash01` of grid positions, not an RNG). It's where
//!     all the placement rules live, and what the unit tests check.
//!   * `rebuild_scenery` uses Bevy change detection: whenever the `Terrain` (a
//!     level switch) or `ActiveScenery` (a button) changes, it despawns every
//!     `Scenery` entity and spawns the new plan.
//!   * Sets are placed in a fixed order — city, ruins, trees — and share an
//!     `Occupancy` grid, so later sets never grow through earlier ones.
//!   * Nothing goes where the water reaches in its first `DRY_SECS` — found by
//!     running the real flow sim at load (`flood::predict_wet`).
//!
//! For now scenery is decoration only: water flows through it.

use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::Button;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, SQRT_2, TAU};

use crate::flood::{
    cell_to_world, ground_at, height_fraction, idx, predict_wet, smoothstep, terrain_normal, Source, Terrain,
    BTN_OFF, CELL, D, GROUND_SAND_LINE, POUR_ON, W,
};

// ---------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------

// Scenery keeps out of where the water gets to in the first DRY_SECS of a
// flood (predicted by running the sim at load), plus a margin.
const DRY_SECS: f32 = 8.0;
const DRY_MARGIN: usize = 1; // extra cells kept clear around the predicted wet area

// Trees.
const TREE_SPACING: usize = 3; // cells between candidate tree spots (each jittered within its slot)
const TREE_GROVE_SIZE: f32 = 14.0; // cells per grove-noise blob: trees cluster at about this scale
const TREE_DENSITY: f32 = 0.45; // share of the grassland that's wooded (0 = none, 1 = everywhere)
const TREE_MIN_FLATNESS: f32 = 0.85; // terrain normal.y needed — no trees on steep banks
const TREE_ABOVE_SAND: f32 = 0.05; // trees start this far (height fraction) above the sand line
const CONIFER_LINE: f32 = 0.6; // height fraction where pines take over from broadleaf trees

// City.
const LOT: usize = 3; // cells per building lot (12 world units)
const STREET_EVERY: i32 = 3; // every 3rd row/column of lots is a street (incl. through the centre)
const TOWN_RADIUS: f32 = 18.0; // cells from the town square to the furthest lots
const TOWN_FLATNESS: f32 = 0.9; // terrain normal.y a lot needs to be built on
const TOWN_MAX_HEIGHT: f32 = 0.7; // towns stay below this height fraction (no hill-top cities)
const TOWN_MIN_LOTS: usize = 6; // fewer buildable lots than this → no town on this level

// Ruins.
const RUIN_SITES: usize = 4; // at most this many ruins per level
const RUIN_SITE_GAP: f32 = 30.0; // minimum cells between two ruin sites
const RUIN_FLATNESS: f32 = 0.88; // terrain normal.y a ruin site needs
const RUIN_SITE_REACH: usize = 6; // cells from a site's centre its walls/columns/rubble can reach

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

/// One set of scenery. Written lowercase in level yamls: `trees`, `city`, `ruins`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SceneryKind {
    Trees,
    City,
    Ruins,
}

impl SceneryKind {
    pub const ALL: [SceneryKind; 3] = [SceneryKind::Trees, SceneryKind::City, SceneryKind::Ruins];

    fn label(self) -> &'static str {
        match self {
            SceneryKind::Trees => "Trees",
            SceneryKind::City => "City",
            SceneryKind::Ruins => "Ruins",
        }
    }
}

/// The scenery sets currently shown. Set from the level yaml on load, toggled
/// by the panel buttons; any change triggers `rebuild_scenery`.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct ActiveScenery(pub Vec<SceneryKind>);

impl ActiveScenery {
    fn has(&self, kind: SceneryKind) -> bool {
        self.0.contains(&kind)
    }

    fn toggle(&mut self, kind: SceneryKind) {
        if self.has(kind) {
            self.0.retain(|&k| k != kind);
        } else {
            self.0.push(kind);
        }
    }
}

/// Marks the root entity of a piece of scenery (its parts are children), so a
/// rebuild can clear them all.
#[derive(Component)]
struct Scenery;

/// A panel button that toggles one scenery set.
#[derive(Component)]
struct SceneryButton(SceneryKind);

#[derive(Debug, Clone, Copy, PartialEq)]
enum TreeKind {
    Broadleaf,
    Conifer,
}

#[derive(Debug, Clone, PartialEq)]
struct TreeSpot {
    pos: Vec2,    // world x/z
    ground: f32,  // terrain height under it
    scale: f32,   // overall size, around 1
    yaw: f32,     // turn about the vertical, so trees don't all face the same way
    kind: TreeKind,
    shade: usize, // which of the canopy greens
}

#[derive(Debug, Clone, PartialEq)]
struct Building {
    pos: Vec2,     // world x/z of the footprint centre
    base: f32,     // lowest ground under the footprint (walls start here)
    size: Vec3,    // width, wall height (above the highest ground under it), depth
    roof: f32,     // roof height
    wall: usize,   // wall colour index
    roof_tint: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PieceShape {
    Block,
    Column,
}

/// One broken wall segment, column or rubble block, in world space.
#[derive(Debug, Clone, PartialEq)]
struct RuinPiece {
    center: Vec3,
    size: Vec3, // block: x/y/z extent; column: diameter, length, diameter
    rotation: Quat,
    shape: PieceShape,
    mossy: bool,
}

/// Everything one rebuild will spawn.
#[derive(Debug, Default)]
struct SceneryPlan {
    buildings: Vec<Building>,
    ruins: Vec<RuinPiece>,
    trees: Vec<TreeSpot>,
}

/// Which grid cells are already taken by a building or ruin.
struct Occupancy(Vec<bool>);

impl Occupancy {
    fn is_free(&self, x: usize, z: usize) -> bool {
        !self.0[idx(x, z)]
    }

    /// Is every cell within `r` of (x, z) free?
    fn area_free(&self, x: usize, z: usize, r: usize) -> bool {
        let (x0, z0) = (x.saturating_sub(r), z.saturating_sub(r));
        let (x1, z1) = ((x + r).min(W - 1), (z + r).min(D - 1));
        (z0..=z1).all(|zz| (x0..=x1).all(|xx| self.is_free(xx, zz)))
    }

    /// Mark every cell within `r` of (x, z) as taken.
    fn take(&mut self, x: usize, z: usize, r: usize) {
        let (x0, z0) = (x.saturating_sub(r), z.saturating_sub(r));
        let (x1, z1) = ((x + r).min(W - 1), (z + r).min(D - 1));
        for zz in z0..=z1 {
            for xx in x0..=x1 {
                self.0[idx(xx, zz)] = true;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Placement (pure — no Bevy entities)
// ---------------------------------------------------------------------------

/// Repeatable pseudo-random number in 0..1 for a grid point. Same inputs →
/// same output, so scenery lands in the same places every run. `salt` picks an
/// independent stream (jitter, size, kind, …) from the same point.
fn hash01(x: u32, z: u32, salt: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8da6_b343) ^ z.wrapping_mul(0xd816_3841) ^ salt.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

/// Smooth noise in 0..1: random values on an integer lattice, blended between.
/// Nearby points get similar values, which is what makes trees clump into groves.
fn value_noise(x: f32, z: f32, salt: u32) -> f32 {
    let (x0, z0) = (x.floor(), z.floor());
    let (fx, fz) = (smoothstep(0.0, 1.0, x - x0), smoothstep(0.0, 1.0, z - z0));
    let (ix, iz) = (x0 as u32, z0 as u32);
    let a = hash01(ix, iz, salt);
    let b = hash01(ix + 1, iz, salt);
    let c = hash01(ix, iz + 1, salt);
    let d = hash01(ix + 1, iz + 1, salt);
    let top = a + (b - a) * fx;
    let bottom = c + (d - c) * fx;
    top + (bottom - top) * fz
}

fn height_range(t: &[f32]) -> (f32, f32) {
    let min_h = t.iter().copied().fold(f32::INFINITY, f32::min);
    let max_h = t.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    (min_h, max_h)
}

/// Lay out every active set on this terrain, in a fixed order (city, ruins,
/// trees) so the bigger, rarer things get first pick of the land. Cells in
/// `wet` (and a margin around them) start out taken.
fn plan_scenery(t: &[f32], wet: &[bool], active: &ActiveScenery) -> SceneryPlan {
    let mut taken = Occupancy(vec![false; W * D]);
    for z in 0..D {
        for x in 0..W {
            if wet[idx(x, z)] {
                taken.take(x, z, DRY_MARGIN);
            }
        }
    }
    let mut plan = SceneryPlan::default();
    if active.has(SceneryKind::City) {
        plan.buildings = plan_city(t, &mut taken);
    }
    if active.has(SceneryKind::Ruins) {
        plan.ruins = plan_ruins(t, &mut taken);
    }
    if active.has(SceneryKind::Trees) {
        plan.trees = plan_trees(t, &taken);
    }
    plan
}

/// Is cell (x, z) gentle land above the sand line by at least `margin`?
fn is_dry_land(t: &[f32], range: (f32, f32), x: usize, z: usize, margin: f32, flatness: f32) -> bool {
    height_fraction(t[idx(x, z)], range.0, range.1) >= GROUND_SAND_LINE + margin
        && terrain_normal(t, x, z).y >= flatness
}

/// Candidate tree spots on a jittered grid; a spot keeps its tree if it's free
/// grassland (above the sand line, gentle slope — where terrain.wesl paints
/// grass) inside a grove.
fn plan_trees(t: &[f32], taken: &Occupancy) -> Vec<TreeSpot> {
    let range = height_range(t);
    let step = TREE_SPACING as f32;
    let mut spots = Vec::new();
    for gz in (0..D).step_by(TREE_SPACING) {
        for gx in (0..W).step_by(TREE_SPACING) {
            let (ux, uz) = (gx as u32, gz as u32);
            let cx = gx as f32 + hash01(ux, uz, 1) * step;
            let cz = gz as f32 + hash01(ux, uz, 2) * step;
            // Keep off the outermost cells (the rim around the map).
            let (x, z) = (cx.round() as usize, cz.round() as usize);
            if x < 2 || z < 2 || x > W - 3 || z > D - 3 || !taken.is_free(x, z) {
                continue;
            }
            if !is_dry_land(t, range, x, z, TREE_ABOVE_SAND, TREE_MIN_FLATNESS) {
                continue;
            }
            if value_noise(cx / TREE_GROVE_SIZE, cz / TREE_GROVE_SIZE, 3) < 1.0 - TREE_DENSITY {
                continue;
            }
            let ground = ground_at(t, cx, cz);
            // Pines get more likely the higher up the spot is.
            let hf = height_fraction(ground, range.0, range.1);
            let pine_odds = smoothstep(CONIFER_LINE - 0.2, CONIFER_LINE + 0.2, hf);
            let kind = if hash01(ux, uz, 4) < pine_odds { TreeKind::Conifer } else { TreeKind::Broadleaf };
            spots.push(TreeSpot {
                pos: cell_to_world(cx, cz),
                ground,
                scale: 0.75 + 0.55 * hash01(ux, uz, 5),
                yaw: hash01(ux, uz, 6) * TAU,
                kind,
                shade: ((hash01(ux, uz, 7) * 3.0) as usize).min(2),
            });
        }
    }
    spots
}

/// Where the town square goes: the flattest, lowest dry land — a floodplain
/// town, which is what the water will eventually threaten. `None` if the
/// level has nowhere suitable.
fn town_center(t: &[f32], taken: &Occupancy) -> Option<(usize, usize)> {
    let range = height_range(t);
    let margin = (TOWN_RADIUS as usize).min(W / 3);
    let mut best: Option<(f32, usize, usize)> = None;
    for z in (margin..D - margin).step_by(3) {
        for x in (margin..W - margin).step_by(3) {
            let hf = height_fraction(t[idx(x, z)], range.0, range.1);
            if hf < GROUND_SAND_LINE + 0.03 || hf > TOWN_MAX_HEIGHT || !taken.area_free(x, z, LOT) {
                continue;
            }
            // How bumpy the land around it is (height spread over a 13×13
            // patch), and how much of it is already taken (water, mostly).
            let (mut lo, mut hi, mut blocked) = (f32::INFINITY, f32::NEG_INFINITY, 0);
            for dz in (-6i32..=6).step_by(2) {
                for dx in (-6i32..=6).step_by(2) {
                    let (px, pz) = ((x as i32 + dx) as usize, (z as i32 + dz) as usize);
                    let h = t[idx(px, pz)];
                    lo = lo.min(h);
                    hi = hi.max(h);
                    blocked += !taken.is_free(px, pz) as i32;
                }
            }
            // Flat and clear matter most; among those, prefer lower ground —
            // close to the river, so a big flood can reach it.
            let score = (hi - lo) + 2.0 * blocked as f32 + 30.0 * (hf - GROUND_SAND_LINE);
            if best.is_none_or(|(s, _, _)| score < s) {
                best = Some((score, x, z));
            }
        }
    }
    best.map(|(_, x, z)| (x, z))
}

/// A grid town around `town_center`: lots in rows and columns with every
/// third row/column left as a street, a ragged edge, and taller buildings
/// toward the middle.
fn plan_city(t: &[f32], taken: &mut Occupancy) -> Vec<Building> {
    let Some((cx, cz)) = town_center(t, taken) else { return Vec::new() };
    let range = height_range(t);
    let reach = (TOWN_RADIUS / LOT as f32).ceil() as i32;
    let half_lot = LOT / 2;
    let mut buildings = Vec::new();
    for lz in -reach..=reach {
        for lx in -reach..=reach {
            if lx.rem_euclid(STREET_EVERY) == 0 || lz.rem_euclid(STREET_EVERY) == 0 {
                continue; // street
            }
            let (x, z) = (cx as i32 + lx * LOT as i32, cz as i32 + lz * LOT as i32);
            if x < 3 || z < 3 || x > W as i32 - 4 || z > D as i32 - 4 {
                continue;
            }
            let (x, z) = (x as usize, z as usize);
            let (ux, uz) = (x as u32, z as u32);
            // Distance from the square, in cells; the edge is ragged, not a circle.
            let dist = Vec2::new(lx as f32, lz as f32).length() * LOT as f32;
            if dist > TOWN_RADIUS * (0.65 + 0.35 * hash01(ux, uz, 20)) {
                continue;
            }
            if !is_dry_land(t, range, x, z, 0.02, TOWN_FLATNESS) || !taken.area_free(x, z, half_lot) {
                continue;
            }
            let lot = LOT as f32 * CELL;
            let w = lot * (0.55 + 0.3 * hash01(ux, uz, 21));
            let d = lot * (0.55 + 0.3 * hash01(ux, uz, 22));
            // Sit on the lowest corner; walls reach `height` above the highest one.
            let (fx, fz) = (x as f32, z as f32);
            let (hw, hd) = (w / CELL / 2.0, d / CELL / 2.0);
            let corners = [(-hw, -hd), (hw, -hd), (-hw, hd), (hw, hd)].map(|(ox, oz)| ground_at(t, fx + ox, fz + oz));
            let lo = corners.iter().copied().fold(f32::INFINITY, f32::min);
            let hi = corners.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let centrality = (1.0 - dist / TOWN_RADIUS).clamp(0.0, 1.0);
            let height = 5.0 + 6.0 * hash01(ux, uz, 23) + 12.0 * centrality * centrality;
            buildings.push(Building {
                pos: cell_to_world(fx, fz),
                base: lo - 0.5,
                size: Vec3::new(w, height + (hi - lo) + 0.5, d),
                roof: 0.35 * w.min(d) + 1.0,
                wall: ((hash01(ux, uz, 24) * 4.0) as usize).min(3),
                roof_tint: ((hash01(ux, uz, 25) * 3.0) as usize).min(2),
            });
            taken.take(x, z, half_lot);
        }
    }
    if buildings.len() < TOWN_MIN_LOTS {
        return Vec::new(); // a few huts on a slope isn't a town — leave the land to the trees
    }
    buildings
}

/// A few ruin sites spread over the hills, each a broken rectangular
/// foundation with a colonnade and some rubble.
fn plan_ruins(t: &[f32], taken: &mut Occupancy) -> Vec<RuinPiece> {
    let range = height_range(t);
    // Candidate sites, ranked by a hash so the choice is repeatable but not
    // biased toward one corner of the map.
    let mut candidates: Vec<(f32, usize, usize)> = Vec::new();
    for z in (12..D - 12).step_by(5) {
        for x in (12..W - 12).step_by(5) {
            if is_dry_land(t, range, x, z, 0.04, RUIN_FLATNESS) && taken.area_free(x, z, RUIN_SITE_REACH) {
                candidates.push((hash01(x as u32, z as u32, 30), x, z));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut sites: Vec<(usize, usize)> = Vec::new();
    for &(_, x, z) in &candidates {
        if sites.len() == RUIN_SITES {
            break;
        }
        let far_enough = sites.iter().all(|&(sx, sz)| {
            Vec2::new(sx as f32 - x as f32, sz as f32 - z as f32).length() >= RUIN_SITE_GAP
        });
        if far_enough {
            sites.push((x, z));
        }
    }

    let mut pieces = Vec::new();
    for &(x, z) in &sites {
        ruin_site(t, x, z, &mut pieces);
        taken.take(x, z, RUIN_SITE_REACH);
    }
    pieces
}

/// The pieces of one ruin centred on cell (x, z).
fn ruin_site(t: &[f32], x: usize, z: usize, pieces: &mut Vec<RuinPiece>) {
    let (ux, uz) = (x as u32, z as u32);
    let r = |salt: u32| hash01(ux, uz, salt);
    let centre = Vec2::new(x as f32, z as f32); // in cells
    let yaw = r(31) * TAU;
    let turn = Quat::from_rotation_y(yaw);
    // Site-local (cells) → world, with the ground height under it.
    let to_world = |local: Vec2| -> (Vec2, f32) {
        let rotated = Vec2::from_angle(-yaw).rotate(local); // matches `turn` about +Y
        let cell = centre + rotated;
        (cell_to_world(cell.x, cell.y), ground_at(t, cell.x, cell.y))
    };
    let mut salt = 100u32;
    let mut next = || {
        salt += 1;
        r(salt)
    };

    // Foundation walls: the outline of a rectangle, one segment per cell,
    // with gaps and uneven crumbled heights.
    let (a, b) = (2.0 + (next() * 2.0).floor(), 1.5 + (next() * 1.5).floor()); // half-sizes, cells
    let thick = 1.4;
    let mut wall = |from: Vec2, to: Vec2, next: &mut dyn FnMut() -> f32| {
        let len = from.distance(to);
        let steps = len.round().max(1.0) as usize;
        let along_x = (to.x - from.x).abs() > (to.y - from.y).abs();
        for s in 0..steps {
            let keep = next();
            let height = 1.5 + 9.0 * next().powi(2); // mostly low, the odd tall stretch
            if keep > 0.7 {
                continue; // collapsed
            }
            let mid = from.lerp(to, (s as f32 + 0.5) / steps as f32);
            let (pos, ground) = to_world(mid);
            let seg = CELL * 1.02;
            let size = if along_x { Vec3::new(seg, height, thick) } else { Vec3::new(thick, height, seg) };
            pieces.push(RuinPiece {
                center: Vec3::new(pos.x, ground + height / 2.0 - 0.6, pos.y),
                size,
                rotation: turn,
                shape: PieceShape::Block,
                mossy: next() < 0.3,
            });
        }
    };
    let corners = [Vec2::new(-a, -b), Vec2::new(a, -b), Vec2::new(a, b), Vec2::new(-a, b)];
    for i in 0..4 {
        wall(corners[i], corners[(i + 1) % 4], &mut next);
    }

    // A colonnade along the front: standing columns, stumps and fallen drums.
    let columns = 3 + (next() * 3.0) as usize;
    for c in 0..columns {
        let along = -a + 2.0 * a * (c as f32 + 0.5) / columns as f32;
        let (pos, ground) = to_world(Vec2::new(along, -b - 1.5));
        let fate = next();
        let radius = 0.9;
        if fate < 0.5 {
            let len = 7.0 + 6.0 * next();
            pieces.push(RuinPiece {
                center: Vec3::new(pos.x, ground + len / 2.0 - 0.5, pos.y),
                size: Vec3::new(radius * 2.0, len, radius * 2.0),
                rotation: Quat::IDENTITY,
                shape: PieceShape::Column,
                mossy: false,
            });
        } else if fate < 0.75 {
            let len = 1.0 + 1.5 * next();
            pieces.push(RuinPiece {
                center: Vec3::new(pos.x, ground + len / 2.0 - 0.3, pos.y),
                size: Vec3::new(radius * 2.0, len, radius * 2.0),
                rotation: Quat::IDENTITY,
                shape: PieceShape::Column,
                mossy: true,
            });
        } else {
            // Toppled: lying on its side, pointing any which way.
            let len = 6.0 + 4.0 * next();
            let lie = Quat::from_rotation_y(next() * TAU) * Quat::from_rotation_z(FRAC_PI_2);
            pieces.push(RuinPiece {
                center: Vec3::new(pos.x, ground + radius - 0.2, pos.y),
                size: Vec3::new(radius * 2.0, len, radius * 2.0),
                rotation: lie,
                shape: PieceShape::Column,
                mossy: next() < 0.5,
            });
        }
    }

    // Rubble: a few tumbled blocks inside and around the walls.
    let rubble = 3 + (next() * 4.0) as usize;
    for _ in 0..rubble {
        let local = Vec2::new((next() * 2.0 - 1.0) * (a + 1.0), (next() * 2.0 - 1.0) * (b + 1.0));
        let (pos, ground) = to_world(local);
        let s = 1.0 + 1.5 * next();
        let tilt = Quat::from_euler(EulerRot::XYZ, next() * 0.6, next() * TAU, next() * 0.6);
        pieces.push(RuinPiece {
            center: Vec3::new(pos.x, ground + s * 0.3, pos.y),
            size: Vec3::splat(s),
            rotation: tilt,
            shape: PieceShape::Block,
            mossy: next() < 0.4,
        });
    }
}

// ---------------------------------------------------------------------------
// Bevy side
// ---------------------------------------------------------------------------

pub struct SceneryPlugin;

impl Plugin for SceneryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveScenery>()
            .add_systems(Startup, setup_scenery_assets)
            .add_systems(
                Update,
                (
                    handle_scenery_buttons,
                    update_scenery_highlight,
                    // `Terrain` + `Source` are inserted by flood's setup; wait for them.
                    rebuild_scenery.run_if(resource_exists::<Terrain>).run_if(resource_exists::<Source>),
                ),
            );
    }
}

/// Shared meshes and materials: every tree / building / ruin reuses these
/// handles, so Bevy can batch hundreds of them into a few draw calls.
#[derive(Resource)]
struct SceneryAssets {
    cube: Handle<Mesh>,
    column: Handle<Mesh>,
    pyramid: Handle<Mesh>, // unit-footprint roof, scaled per building
    trunk: Handle<Mesh>,
    crown: Handle<Mesh>,     // broadleaf canopy (faceted ball)
    cone_low: Handle<Mesh>,  // conifer: wide lower tier
    cone_high: Handle<Mesh>, // conifer: narrow upper tier
    bark: Handle<StandardMaterial>,
    leaves: [Handle<StandardMaterial>; 3],
    needles: [Handle<StandardMaterial>; 3],
    walls: [Handle<StandardMaterial>; 4],
    roofs: [Handle<StandardMaterial>; 3],
    stone: Handle<StandardMaterial>,
    moss: Handle<StandardMaterial>,
}

/// Flat-shaded (one normal per triangle) for a crisp low-poly look.
fn faceted(mut mesh: Mesh) -> Mesh {
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    mesh
}

fn setup_scenery_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut matte = |r: f32, g: f32, b: f32| {
        materials.add(StandardMaterial { base_color: Color::srgb(r, g, b), perceptual_roughness: 0.92, ..default() })
    };
    let assets = SceneryAssets {
        bark: matte(0.36, 0.25, 0.16),
        leaves: [matte(0.24, 0.42, 0.15), matte(0.31, 0.47, 0.17), matte(0.38, 0.45, 0.16)],
        needles: [matte(0.12, 0.29, 0.15), matte(0.15, 0.33, 0.17), matte(0.10, 0.25, 0.13)],
        // Mediterranean town: whitewash, cream, ochre, terracotta walls…
        walls: [matte(0.93, 0.91, 0.86), matte(0.88, 0.83, 0.71), matte(0.84, 0.69, 0.47), matte(0.76, 0.50, 0.37)],
        // …under clay-tile, brown and slate roofs.
        roofs: [matte(0.62, 0.26, 0.18), matte(0.46, 0.31, 0.22), matte(0.34, 0.35, 0.39)],
        stone: matte(0.63, 0.61, 0.56),
        moss: matte(0.47, 0.51, 0.38),
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        column: meshes.add(faceted(Cylinder::new(0.5, 1.0).mesh().resolution(8).build())),
        // A 4-sided cone is a pyramid with its corners on the axes; turn it 45°
        // and widen it so its base is a 1×1 square, matching a unit cube.
        pyramid: meshes.add(faceted(
            Cone { radius: 1.0, height: 1.0 }
                .mesh()
                .resolution(4)
                .build()
                .rotated_by(Quat::from_rotation_y(FRAC_PI_4))
                .scaled_by(Vec3::new(1.0 / SQRT_2, 1.0, 1.0 / SQRT_2)),
        )),
        trunk: meshes.add(Cylinder::new(0.8, 7.0)),
        crown: meshes.add(faceted(Sphere::new(5.5).mesh().ico(1).expect("ico subdivision is small"))),
        cone_low: meshes.add(faceted(Cone { radius: 5.0, height: 10.0 }.mesh().resolution(7).build())),
        cone_high: meshes.add(faceted(Cone { radius: 3.6, height: 8.0 }.mesh().resolution(7).build())),
    };
    commands.insert_resource(assets);
}

/// Respawn all scenery whenever the terrain (level switch) or the active sets
/// (panel buttons) change. Change detection makes this free on other frames.
fn rebuild_scenery(
    mut commands: Commands,
    terrain: Res<Terrain>,
    source: Res<Source>,
    active: Res<ActiveScenery>,
    assets: Res<SceneryAssets>,
    existing: Query<Entity, With<Scenery>>,
    // Remembered between runs (a `Local` belongs to this system alone): the
    // wet-area prediction only changes with the level, so a button toggle
    // reuses it instead of re-running the sim.
    mut wet: Local<Vec<bool>>,
) {
    let level_changed = terrain.is_changed() || source.is_changed();
    if !level_changed && !active.is_changed() {
        return;
    }
    if level_changed || wet.is_empty() {
        *wet = predict_wet(&terrain.0, &source, DRY_SECS);
    }
    // Despawning a root takes its children (trunk, roof, …) with it.
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let plan = plan_scenery(&terrain.0, &wet, &active);
    for b in &plan.buildings {
        spawn_building(&mut commands, &assets, b);
    }
    if !plan.ruins.is_empty() {
        spawn_ruins(&mut commands, &assets, &plan.ruins);
    }
    for tree in &plan.trees {
        spawn_tree(&mut commands, &assets, tree);
    }
}

/// A tree is a root entity (position, size, turn) with trunk and canopy as
/// children placed relative to it, so the parts move and scale together.
fn spawn_tree(commands: &mut Commands, assets: &SceneryAssets, spot: &TreeSpot) {
    // Sink the base a little so the trunk never hovers on a slope.
    let root = Transform::from_xyz(spot.pos.x, spot.ground - 0.5, spot.pos.y)
        .with_rotation(Quat::from_rotation_y(spot.yaw))
        .with_scale(Vec3::splat(spot.scale));
    let trunk = (Mesh3d(assets.trunk.clone()), MeshMaterial3d(assets.bark.clone()), Transform::from_xyz(0.0, 3.5, 0.0));
    match spot.kind {
        TreeKind::Broadleaf => {
            let green = assets.leaves[spot.shade].clone();
            commands.spawn((Scenery, root, Visibility::default(), children![
                trunk,
                (Mesh3d(assets.crown.clone()), MeshMaterial3d(green),
                    Transform::from_xyz(0.0, 10.5, 0.0).with_scale(Vec3::new(1.0, 0.85, 1.0))),
            ]));
        }
        TreeKind::Conifer => {
            let green = assets.needles[spot.shade].clone();
            commands.spawn((Scenery, root, Visibility::default(), children![
                trunk,
                (Mesh3d(assets.cone_low.clone()), MeshMaterial3d(green.clone()), Transform::from_xyz(0.0, 9.0, 0.0)),
                (Mesh3d(assets.cone_high.clone()), MeshMaterial3d(green), Transform::from_xyz(0.0, 14.0, 0.0)),
            ]));
        }
    }
}

/// A building: walls (a scaled cube) under a pyramid roof with a little overhang.
fn spawn_building(commands: &mut Commands, assets: &SceneryAssets, b: &Building) {
    let s = b.size;
    commands.spawn((
        Scenery,
        Transform::from_xyz(b.pos.x, b.base, b.pos.y),
        Visibility::default(),
        children![
            (Mesh3d(assets.cube.clone()), MeshMaterial3d(assets.walls[b.wall].clone()),
                Transform::from_xyz(0.0, s.y / 2.0, 0.0).with_scale(s)),
            (Mesh3d(assets.pyramid.clone()), MeshMaterial3d(assets.roofs[b.roof_tint].clone()),
                Transform::from_xyz(0.0, s.y + b.roof / 2.0, 0.0)
                    .with_scale(Vec3::new(s.x * 1.1, b.roof, s.z * 1.1))),
        ],
    ));
}

/// All ruin pieces hang off one root; each is already in world space.
fn spawn_ruins(commands: &mut Commands, assets: &SceneryAssets, pieces: &[RuinPiece]) {
    commands
        .spawn((Scenery, Transform::IDENTITY, Visibility::default()))
        .with_children(|root| {
            for p in pieces {
                let mesh = match p.shape {
                    PieceShape::Block => assets.cube.clone(),
                    PieceShape::Column => assets.column.clone(),
                };
                let material = if p.mossy { assets.moss.clone() } else { assets.stone.clone() };
                root.spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Transform { translation: p.center, rotation: p.rotation, scale: p.size },
                ));
            }
        });
}

// --- Panel ------------------------------------------------------------------

/// The SCENERY section of the side panel: one toggle button per set.
/// Called from flood's `setup_ui` while it builds the panel.
pub fn spawn_panel_section(panel: &mut ChildSpawnerCommands) {
    panel.spawn((
        Text::new("SCENERY"),
        TextFont { font_size: FontSize::Px(11.0), ..default() },
        TextColor(Color::srgb(0.65, 0.66, 0.72)),
        Node { margin: UiRect::top(Val::Px(8.0)), ..default() },
    ));
    for kind in SceneryKind::ALL {
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
                SceneryButton(kind),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new(kind.label()),
                    TextFont { font_size: FontSize::Px(11.0), ..default() },
                    TextColor(Color::WHITE),
                ));
            });
    }
}

fn handle_scenery_buttons(
    q: Query<&SceneryButton, Added<Pressed>>,
    mut active: ResMut<ActiveScenery>,
) {
    for b in &q {
        active.toggle(b.0);
    }
}

fn update_scenery_highlight(active: Res<ActiveScenery>, mut q: Query<(&SceneryButton, &mut BackgroundColor)>) {
    if !active.is_changed() {
        return;
    }
    for (b, mut bg) in &mut q {
        *bg = BackgroundColor(if active.has(b.0) { POUR_ON } else { BTN_OFF });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test terrain: a valley — sand along the middle, a grassy terrace on
    /// either side, hills at the edges.
    fn valley() -> Vec<f32> {
        (0..W * D)
            .map(|i| {
                let x = (i % W) as f32;
                let from_middle = (x - W as f32 / 2.0).abs();
                // Gentle banks 10–30 cells out up to a flat mid-height terrace
                // (where a town fits), then hills from 45 cells out.
                (from_middle - 10.0).clamp(0.0, 20.0) * 1.5 + (from_middle - 45.0).clamp(0.0, 10.0) * 3.0
            })
            .collect()
    }

    /// No water anywhere.
    fn dry() -> Vec<bool> {
        vec![false; W * D]
    }

    fn all() -> ActiveScenery {
        ActiveScenery(SceneryKind::ALL.to_vec())
    }

    #[test]
    fn toggle_adds_then_removes() {
        let mut a = ActiveScenery(vec![SceneryKind::Trees]);
        a.toggle(SceneryKind::City);
        assert!(a.has(SceneryKind::City));
        a.toggle(SceneryKind::City);
        assert_eq!(a, ActiveScenery(vec![SceneryKind::Trees]));
    }

    #[test]
    fn scenery_kinds_read_lowercase_from_yaml() {
        let kinds: Vec<SceneryKind> = serde_yaml::from_str("[trees, city, ruins]").unwrap();
        assert_eq!(kinds, SceneryKind::ALL.to_vec());
    }

    #[test]
    fn layout_is_the_same_every_run() {
        let t = valley();
        let (a, b) = (plan_scenery(&t, &dry(), &all()), plan_scenery(&t, &dry(), &all()));
        assert_eq!(a.buildings, b.buildings);
        assert_eq!(a.ruins, b.ruins);
        assert_eq!(a.trees, b.trees);
    }

    #[test]
    fn only_active_sets_are_planned() {
        let t = valley();
        let plan = plan_scenery(&t, &dry(), &ActiveScenery(vec![SceneryKind::Ruins]));
        assert!(plan.buildings.is_empty() && plan.trees.is_empty());
        assert!(!plan.ruins.is_empty());
    }

    #[test]
    fn flat_land_is_all_sand_so_nothing_is_placed() {
        let plan = plan_scenery(&vec![5.0; W * D], &dry(), &all());
        assert!(plan.buildings.is_empty() && plan.ruins.is_empty() && plan.trees.is_empty());
    }

    #[test]
    fn city_builds_a_town_on_flat_dry_land() {
        let t = valley();
        let plan = plan_scenery(&t, &dry(), &all());
        assert!(plan.buildings.len() >= TOWN_MIN_LOTS, "got {} buildings", plan.buildings.len());
        let range = height_range(&t);
        for b in &plan.buildings {
            let (x, z) = crate::flood::cell_of(b.pos);
            assert!(height_fraction(t[idx(x, z)], range.0, range.1) > GROUND_SAND_LINE, "building on sand at {:?}", b.pos);
        }
    }

    #[test]
    fn buildings_do_not_overlap() {
        let plan = plan_scenery(&valley(), &dry(), &all());
        for (i, a) in plan.buildings.iter().enumerate() {
            for b in &plan.buildings[i + 1..] {
                let gap = (a.pos - b.pos).abs();
                let apart = gap.x >= (a.size.x + b.size.x) / 2.0 || gap.y >= (a.size.z + b.size.z) / 2.0;
                assert!(apart, "buildings at {:?} and {:?} overlap", a.pos, b.pos);
            }
        }
    }

    #[test]
    fn trees_keep_out_of_town_and_ruins() {
        let t = valley();
        let everything = plan_scenery(&t, &dry(), &all());
        let trees_only = plan_scenery(&t, &dry(), &ActiveScenery(vec![SceneryKind::Trees]));
        assert!(everything.trees.len() < trees_only.trees.len(), "town/ruins should displace some trees");
        let near = |p: Vec2, q: Vec2| (p - q).length() < CELL * 1.5;
        for tree in &everything.trees {
            assert!(!everything.buildings.iter().any(|b| near(tree.pos, b.pos)), "tree inside a building at {:?}", tree.pos);
        }
    }

    #[test]
    fn nothing_is_placed_on_or_next_to_water() {
        let t = valley();
        // Flood the whole left half of the map.
        let wet: Vec<bool> = (0..W * D).map(|i| i % W < W / 2).collect();
        let plan = plan_scenery(&t, &wet, &all());
        let dry_side = |p: Vec2| crate::flood::cell_of(p).0 >= W / 2 + DRY_MARGIN;
        assert!(!plan.trees.is_empty() && !plan.buildings.is_empty(), "the dry half still gets scenery");
        assert!(plan.trees.iter().all(|s| dry_side(s.pos)));
        assert!(plan.buildings.iter().all(|b| dry_side(b.pos)));
        assert!(plan.ruins.iter().all(|r| dry_side(Vec2::new(r.center.x, r.center.z))));
    }

    #[test]
    fn ruin_pieces_stay_on_the_map() {
        let mut taken = Occupancy(vec![false; W * D]);
        let pieces = plan_ruins(&valley(), &mut taken);
        assert!(!pieces.is_empty());
        for p in &pieces {
            assert!(p.center.x.abs() < (W as f32) * CELL / 2.0 && p.center.z.abs() < (D as f32) * CELL / 2.0);
        }
    }
}
