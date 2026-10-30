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
//! Destruction: trees, buildings, ruin walls and standing columns are
//! `Breakable`. Buildings and ruin walls also dam the flow (a `Blocker`).
//!   * Each frame `wear_scenery` measures the water's push on every piece
//!     (`water_load`: drag from the current, plus water piled up on one side).
//!     While that beats the piece's strength it loses health (`wear`) and
//!     leans downstream.
//!   * At zero health it starts `Collapsing`: trees and columns topple with the
//!     current, buildings crumble into a low rubble mound and shed chunks.
//!   * Fallen trees and chunks become `Debris` and float off on the current.
//!   * R (drain) puts all the scenery back.

use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::Button;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, SQRT_2, TAU};

use crate::flood::{
    cell_of, cell_to_world, ground_at, height_fraction, idx, predict_wet, smoothstep, terrain_normal, Blocker,
    Paused, Source, Terrain, Water, BTN_OFF, CELL, D, DT, GROUND_SAND_LINE, POUR_ON, W, WET,
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

// Destruction. Strengths are in `water_load` units — roughly "water this much
// standing against it", with a fast current counting extra.
const LOAD_SPEED: f32 = 10.0; // current (world units/sec) at which drag pushes as hard as the water is deep
const BREAK_SECS: f32 = 2.0; // a load of twice the strength breaks a piece in this long
const TREE_STRENGTH: f32 = 6.0; // × the tree's scale
const BUILDING_STRENGTH: f32 = 1.5; // × the side of its footprint (≈ 7–10 world units)
const RUIN_WALL_STRENGTH: f32 = 20.0;
const COLUMN_STRENGTH: f32 = 10.0;
const LEAN: f32 = 0.15; // radians a piece leans downstream just before it gives way
const TOPPLE_SECS: f32 = 1.2; // how long a tree/column takes to fall
const TOPPLE_ANGLE: f32 = 1.45; // radians it falls through (a little short of flat: it lands on its side)
const CRUMBLE_SECS: f32 = 1.0; // how long a building takes to fall in
const CRUMBLE_SQUASH: f32 = 0.75; // share of a building's height lost when it falls in
const CRUMBLE_TILT: f32 = 0.2; // radians the rubble ends up tilted downstream
const CHUNKS: usize = 4; // pieces a falling building sheds into the water
const DEBRIS_MOBILITY: f32 = 0.6; // debris drifts at this share of the current
const DEBRIS_LIFE: f32 = 45.0; // seconds before debris sinks out of sight
const DEBRIS_FADE: f32 = 2.0; // seconds it takes to sink away

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
    breakable: bool, // standing walls and columns; rubble and fallen drums are already down
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
                breakable: true,
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
                breakable: true,
            });
        } else if fate < 0.75 {
            let len = 1.0 + 1.5 * next();
            pieces.push(RuinPiece {
                center: Vec3::new(pos.x, ground + len / 2.0 - 0.3, pos.y),
                size: Vec3::new(radius * 2.0, len, radius * 2.0),
                rotation: Quat::IDENTITY,
                shape: PieceShape::Column,
                mossy: true,
                breakable: false,
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
                breakable: false,
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
            breakable: false,
        });
    }
}

// ---------------------------------------------------------------------------
// Destruction (pure — no Bevy entities)
// ---------------------------------------------------------------------------

/// How hard the water pushes on a piece that feels it over `cells`, and which
/// way (a unit x/z vector; zero if there's no push). Two parts, added up:
///   * drag — a current pushes with depth × speed², like real drag;
///   * head — water standing higher on one side than the other (backed up
///     behind a dam) pushes from the high side, up to its full depth.
/// Water that's deep but still and level pushes evenly from every side, so
/// on its own it does nothing: trees survive a calm lake.
fn water_load(cells: &[usize], ground: &[f32], depth: &[f32], current: impl Fn(usize) -> Vec2) -> (f32, Vec2) {
    let (mut drag, mut flow) = (0.0f32, Vec2::ZERO);
    let (mut high, mut low) = ((f32::NEG_INFINITY, 0), (f32::INFINITY, 0)); // (surface, cell)
    let mut deepest = 0.0f32;
    for &c in cells {
        let (d, v) = (depth[c], current(c));
        drag = drag.max(d * (v.length() / LOAD_SPEED).powi(2));
        flow += v * d; // weighted by depth, so the deep water has the say
        let surface = ground[c] + d;
        if d > WET && surface > high.0 {
            high = (surface, c);
        }
        if surface < low.0 {
            low = (surface, c);
        }
        deepest = deepest.max(d);
    }
    let head = (high.0 - low.0).clamp(0.0, deepest);
    let push = if head > drag {
        let at = |c: usize| cell_to_world((c % W) as f32, (c / W) as f32);
        (at(low.1) - at(high.1)).normalize_or_zero()
    } else {
        flow.normalize_or_zero()
    };
    (drag + head, push)
}

/// Health (1 = sound, 0 = broken) after `dt` seconds under `load`. Nothing
/// happens at or below `strength`; above it the piece wears faster the more
/// it's overloaded — twice its strength breaks it in BREAK_SECS.
fn wear(health: f32, load: f32, strength: f32, dt: f32) -> f32 {
    let overload = (load / strength - 1.0).max(0.0);
    (health - overload * dt / BREAK_SECS).max(0.0)
}

/// `rest` tipped over by `angle` radians toward `toward` (world x/z), turning
/// about the ground point `pivot` — the way a tree falls from its base.
fn tipped(rest: &Transform, pivot: Vec3, toward: Vec2, angle: f32) -> Transform {
    // The horizontal axis square to `toward`, picked so a positive angle swings
    // the top toward it (right-hand rule: axis × up = toward).
    let axis = Vec3::new(toward.y, 0.0, -toward.x).normalize_or(Vec3::X);
    let turn = Quat::from_axis_angle(axis, angle);
    Transform {
        translation: pivot + turn * (rest.translation - pivot),
        rotation: turn * rest.rotation,
        scale: rest.scale,
    }
}

/// Grid cells (as indices) whose centres lie within `radius` world units of
/// `center` (x/z) — where a piece feels the water.
fn cells_near(center: Vec2, radius: f32) -> Vec<usize> {
    let (cx, cz) = cell_of(center);
    let r = (radius / CELL).ceil() as usize;
    let mut cells = Vec::new();
    for z in cz.saturating_sub(r)..=(cz + r).min(D - 1) {
        for x in cx.saturating_sub(r)..=(cx + r).min(W - 1) {
            if cell_to_world(x as f32, z as f32).distance(center) <= radius {
                cells.push(idx(x, z));
            }
        }
    }
    cells
}

/// Grid cells whose centres lie under a box standing at `center`, with
/// horizontal half-size `half` (local x, z) turned by `rotation` about the
/// vertical. Each half-size counts as at least ¾ of a cell: then even a thin
/// wall turned at any angle covers a cell centre, and a line of them dams.
fn cells_under(center: Vec3, half: Vec2, rotation: Quat) -> Vec<usize> {
    let half = half.max(Vec2::splat(CELL * 0.75));
    let undo = rotation.inverse();
    cells_near(Vec2::new(center.x, center.z), half.length())
        .into_iter()
        .filter(|&i| {
            let cell = cell_to_world((i % W) as f32, (i / W) as f32);
            let local = undo * Vec3::new(cell.x - center.x, 0.0, cell.y - center.z);
            local.x.abs() <= half.x && local.z.abs() <= half.y
        })
        .collect()
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
                    (wear_scenery, collapse_scenery, drift_debris)
                        .chain()
                        .run_if(resource_exists::<Water>)
                        .run_if(|paused: Res<Paused>| !paused.0),
                ),
            );
    }
}

/// A piece of scenery the water can break. It stands until the water's load
/// beats its `strength`, wears down while it does, then starts `Collapsing`.
#[derive(Component)]
struct Breakable {
    strength: f32,
    health: f32,       // 1 = sound, 0 = gives way
    cells: Vec<usize>, // where it feels the water: its footprint and a ring around it
    rest: Transform,   // its undamaged pose
    pivot: Vec3,       // the ground point it leans and falls about
    push: Vec2,        // which way (unit, world x/z) the water last pushed it
    fate: Fate,
}

impl Breakable {
    fn new(strength: f32, cells: Vec<usize>, rest: Transform, pivot: Vec3, fate: Fate) -> Self {
        // Until the water says otherwise, things fall downstream (toward the
        // front edge, where the water drains).
        Breakable { strength, health: 1.0, cells, rest, pivot, push: Vec2::Y, fate }
    }
}

/// What happens to a piece once it gives way.
#[derive(Clone, Copy)]
enum Fate {
    /// Falls over with the current (trees, columns, ruin walls). Trees then
    /// float off; stone stays where it lands.
    Topple { floats: bool },
    /// Falls in on itself, leaving a low mound that still dams a little, and
    /// sheds a few chunks into the water (buildings).
    Crumble { size: Vec3, wall: usize, roof: usize },
}

/// A piece that has given way, mid-fall. `t` counts up from 0 (seconds).
#[derive(Component)]
struct Collapsing {
    t: f32,
    rest: Transform,
    pivot: Vec3,
    push: Vec2,
    fate: Fate,
}

/// Wreckage carried on the current (fallen trees, chunks of building). Rests
/// on the ground where it's dry, and sinks out of sight after DEBRIS_LIFE.
#[derive(Component)]
struct Debris {
    vel: Vec2,   // world x/z
    age: f32,    // seconds
    spin: f32,   // radians/sec it turns while afloat
    lift: f32,   // how far its origin sits above the water (or ground) surface
    scale: Vec3, // full size, shrunk as it sinks away
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
    keys: Res<ButtonInput<KeyCode>>,
    // Remembered between runs (a `Local` belongs to this system alone): the
    // wet-area prediction only changes with the level, so a button toggle
    // reuses it instead of re-running the sim.
    mut wet: Local<Vec<bool>>,
) {
    let level_changed = terrain.is_changed() || source.is_changed();
    // R drains the water (flood.rs); here it also rebuilds whatever was wrecked.
    let reset = keys.just_pressed(KeyCode::KeyR);
    if !level_changed && !active.is_changed() && !reset {
        return;
    }
    if level_changed || wet.is_empty() {
        *wet = predict_wet(&terrain.0, &source, DRY_SECS);
    }
    // Despawning a root takes its children (trunk, roof, …) with it. `try_`:
    // debris that drifted off the map this frame may already be on its way out.
    for entity in &existing {
        commands.entity(entity).try_despawn();
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
    // The water pushes on the trunk, so it only needs to feel the cells close by.
    let breakable = Breakable::new(
        TREE_STRENGTH * spot.scale,
        cells_near(spot.pos, CELL),
        root,
        root.translation,
        Fate::Topple { floats: true },
    );
    match spot.kind {
        TreeKind::Broadleaf => {
            let green = assets.leaves[spot.shade].clone();
            commands.spawn((Scenery, breakable, root, Visibility::default(), children![
                trunk,
                (Mesh3d(assets.crown.clone()), MeshMaterial3d(green),
                    Transform::from_xyz(0.0, 10.5, 0.0).with_scale(Vec3::new(1.0, 0.85, 1.0))),
            ]));
        }
        TreeKind::Conifer => {
            let green = assets.needles[spot.shade].clone();
            commands.spawn((Scenery, breakable, root, Visibility::default(), children![
                trunk,
                (Mesh3d(assets.cone_low.clone()), MeshMaterial3d(green.clone()), Transform::from_xyz(0.0, 9.0, 0.0)),
                (Mesh3d(assets.cone_high.clone()), MeshMaterial3d(green), Transform::from_xyz(0.0, 14.0, 0.0)),
            ]));
        }
    }
}

/// A building: walls (a scaled cube) under a pyramid roof with a little overhang.
/// It dams the water up to its eaves until it falls in.
fn spawn_building(commands: &mut Commands, assets: &SceneryAssets, b: &Building) {
    let s = b.size;
    let root = Transform::from_xyz(b.pos.x, b.base, b.pos.y);
    let half = Vec2::new(s.x, s.z) / 2.0;
    commands.spawn((
        Scenery,
        Breakable::new(
            BUILDING_STRENGTH * (s.x * s.z).sqrt(),
            cells_near(b.pos, half.length() + CELL),
            root,
            root.translation,
            Fate::Crumble { size: s, wall: b.wall, roof: b.roof_tint },
        ),
        Blocker { cells: cells_under(root.translation, half, Quat::IDENTITY), top: b.base + s.y },
        root,
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

/// Each ruin piece is its own entity (so each can fall on its own), already
/// in world space. Standing walls and columns can break; walls also dam.
fn spawn_ruins(commands: &mut Commands, assets: &SceneryAssets, pieces: &[RuinPiece]) {
    for p in pieces {
        let (mesh, strength) = match p.shape {
            PieceShape::Block => (assets.cube.clone(), RUIN_WALL_STRENGTH),
            PieceShape::Column => (assets.column.clone(), COLUMN_STRENGTH),
        };
        let material = if p.mossy { assets.moss.clone() } else { assets.stone.clone() };
        let rest = Transform { translation: p.center, rotation: p.rotation, scale: p.size };
        let mut piece = commands.spawn((Scenery, Mesh3d(mesh), MeshMaterial3d(material), rest));
        if !p.breakable {
            continue;
        }
        let at = Vec2::new(p.center.x, p.center.z);
        let half = Vec2::new(p.size.x, p.size.z) / 2.0;
        let pivot = p.center - Vec3::Y * p.size.y / 2.0; // middle of its base
        piece.insert(Breakable::new(
            strength,
            cells_near(at, half.length() + CELL),
            rest,
            pivot,
            Fate::Topple { floats: false },
        ));
        if p.shape == PieceShape::Block {
            piece.insert(Blocker { cells: cells_under(p.center, half, p.rotation), top: p.center.y + p.size.y / 2.0 });
        }
    }
}

// --- Destruction ------------------------------------------------------------

/// Let the water push on every standing piece. Overloaded pieces lose health
/// and lean downstream; at zero they give way and start `Collapsing`.
fn wear_scenery(
    mut commands: Commands,
    terrain: Res<Terrain>,
    water: Res<Water>,
    assets: Res<SceneryAssets>,
    mut q: Query<(Entity, &mut Breakable, &mut Transform)>,
) {
    for (entity, mut b, mut tf) in &mut q {
        let (load, push) = water_load(&b.cells, &terrain.0, &water.depth, |c| water.current(c));
        if load <= b.strength {
            continue; // holding — and Bevy's change detection never sees a write
        }
        if push != Vec2::ZERO {
            b.push = push;
        }
        b.health = wear(b.health, load, b.strength, DT);
        if b.health > 0.0 {
            *tf = tipped(&b.rest, b.pivot, b.push, (1.0 - b.health) * LEAN);
            continue;
        }

        // It gives way. (Removing `Breakable` stops this system seeing it again.)
        // The `try_` commands quietly skip a piece that's gone by the time they
        // run — a level switch or R in this same frame despawns all scenery.
        let mut piece = commands.entity(entity);
        piece
            .try_remove::<Breakable>()
            .try_insert(Collapsing { t: 0.0, rest: b.rest, pivot: b.pivot, push: b.push, fate: b.fate });
        match b.fate {
            Fate::Topple { .. } => {
                piece.try_remove::<Blocker>(); // a fallen wall no longer holds water back
            }
            Fate::Crumble { size, wall, roof } => {
                // What's left still dams a little: a mound the height it squashes to.
                piece.try_insert(Blocker {
                    cells: cells_under(b.rest.translation, Vec2::new(size.x, size.z) / 2.0, Quat::IDENTITY),
                    top: b.rest.translation.y + size.y * (1.0 - CRUMBLE_SQUASH),
                });
                shed_chunks(&mut commands, &assets, b.rest.translation, size, wall, roof, b.push);
            }
        }
    }
}

/// A falling building throws a few chunks of wall and roof into the water.
fn shed_chunks(
    commands: &mut Commands,
    assets: &SceneryAssets,
    base: Vec3,
    size: Vec3,
    wall: usize,
    roof: usize,
    push: Vec2,
) {
    let (ux, uz) = (base.x.to_bits(), base.z.to_bits()); // repeatable "randomness" per building
    for k in 0..CHUNKS as u32 {
        let r = |salt: u32| hash01(ux, uz, 40 + k * 8 + salt);
        let offset = Vec3::new((r(0) - 0.5) * size.x, size.y * (0.3 + 0.6 * r(1)), (r(2) - 0.5) * size.z);
        let scale = Vec3::new(1.5 + 2.5 * r(3), 1.0 + 1.5 * r(4), 1.5 + 2.5 * r(5));
        let material = if k == 0 { assets.roofs[roof].clone() } else { assets.walls[wall].clone() };
        commands.spawn((
            Scenery,
            Debris { vel: push * (4.0 + 6.0 * r(6)), age: 0.0, spin: (r(7) - 0.5) * 2.0, lift: 0.0, scale },
            Mesh3d(assets.cube.clone()),
            MeshMaterial3d(material),
            Transform::from_translation(base + offset)
                .with_rotation(Quat::from_euler(EulerRot::XYZ, r(0) * 0.8, r(1) * TAU, r(2) * 0.8))
                .with_scale(scale),
        ));
    }
}

/// Play out each fall. Toppling pieces swing down faster and faster (like a
/// real fall); buildings slump. Fallen trees then float off as `Debris`.
fn collapse_scenery(mut commands: Commands, mut q: Query<(Entity, &mut Collapsing, &mut Transform)>) {
    for (entity, mut c, mut tf) in &mut q {
        c.t += DT;
        let done = match c.fate {
            Fate::Topple { .. } => {
                let k = (c.t / TOPPLE_SECS).min(1.0);
                // Picks up from wherever it was leaning when it gave way.
                *tf = tipped(&c.rest, c.pivot, c.push, LEAN + (TOPPLE_ANGLE - LEAN) * k * k);
                k >= 1.0
            }
            Fate::Crumble { .. } => {
                let k = smoothstep(0.0, CRUMBLE_SECS, c.t);
                *tf = tipped(&c.rest, c.pivot, c.push, LEAN + (CRUMBLE_TILT - LEAN) * k);
                tf.scale.y = 1.0 - CRUMBLE_SQUASH * k;
                c.t >= CRUMBLE_SECS
            }
        };
        if !done {
            continue;
        }
        let mut piece = commands.entity(entity);
        piece.try_remove::<Collapsing>();
        if let Fate::Topple { floats: true } = c.fate {
            let spin = if c.push.x >= 0.0 { 0.4 } else { -0.4 };
            piece.try_insert(Debris { vel: Vec2::ZERO, age: 0.0, spin, lift: 0.3, scale: tf.scale });
        }
    }
}

/// Float debris on the current; on dry land it slides to a stop. After
/// DEBRIS_LIFE it sinks away, and it's gone as soon as it reaches the map edge.
fn drift_debris(
    mut commands: Commands,
    terrain: Res<Terrain>,
    water: Res<Water>,
    mut q: Query<(Entity, &mut Debris, &mut Transform)>,
) {
    for (entity, mut d, mut tf) in &mut q {
        d.age += DT;
        let pos = Vec2::new(tf.translation.x, tf.translation.z);
        let (x, z) = cell_of(pos);
        let fade = 1.0 - (d.age - DEBRIS_LIFE).max(0.0) / DEBRIS_FADE;
        if fade <= 0.0 || x == 0 || z == 0 || x == W - 1 || z == D - 1 {
            commands.entity(entity).try_despawn();
            continue;
        }
        let i = idx(x, z);
        let depth = water.depth[i];
        let afloat = depth > WET;
        if afloat {
            d.vel = d.vel.lerp(water.current(i) * DEBRIS_MOBILITY, 0.1);
            tf.rotate_y(d.spin * DT);
        } else {
            d.vel *= 0.85; // dragging on the ground
        }
        let target_y = terrain.0[i] + depth + d.lift;
        tf.translation.y += (target_y - tf.translation.y) * 0.1;
        tf.translation.x += d.vel.x * DT;
        tf.translation.z += d.vel.y * DT;
        tf.scale = d.scale * fade;
    }
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

    /// Town in Peril only works if the town really is in the low pocket the
    /// gully runs into: the shipped level's city must build on that terrace
    /// (centred left/right, 70% of the way to the drain).
    #[test]
    fn town_in_peril_builds_its_town_in_the_pocket() {
        let level = crate::level::load("levels/town-in-peril.yaml", W, D).unwrap();
        let plan = plan_scenery(&level.heights, &dry(), &ActiveScenery(vec![SceneryKind::City]));
        assert!(plan.buildings.len() >= TOWN_MIN_LOTS, "got {} buildings", plan.buildings.len());
        let square = crate::flood::cell_to_world((W - 1) as f32 * 0.5, 0.70 * (D - 1) as f32);
        for b in &plan.buildings {
            assert!(b.pos.distance(square) < 40.0 * CELL, "building at {:?} is outside the pocket", b.pos);
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

    /// The water's push on cells (10, 10) and (11, 10) of flat ground, with
    /// these depths and the same `current` everywhere.
    fn load_on_pair(left: f32, right: f32, current: Vec2) -> (f32, Vec2) {
        let ground = vec![0.0; W * D];
        let mut depth = vec![0.0; W * D];
        depth[idx(10, 10)] = left;
        depth[idx(11, 10)] = right;
        water_load(&[idx(10, 10), idx(11, 10)], &ground, &depth, |_| current)
    }

    #[test]
    fn still_level_water_does_not_push() {
        assert_eq!(load_on_pair(0.0, 0.0, Vec2::ZERO), (0.0, Vec2::ZERO)); // dry
        assert_eq!(load_on_pair(8.0, 8.0, Vec2::ZERO), (0.0, Vec2::ZERO)); // a calm lake
    }

    #[test]
    fn a_current_drags_downstream_growing_with_speed_squared() {
        let (load, push) = load_on_pair(3.0, 3.0, Vec2::new(0.0, LOAD_SPEED));
        assert_eq!(load, 3.0); // at LOAD_SPEED drag = depth
        assert_eq!(push, Vec2::Y);
        assert_eq!(load_on_pair(3.0, 3.0, Vec2::new(0.0, 2.0 * LOAD_SPEED)).0, 12.0);
    }

    #[test]
    fn water_backed_up_on_one_side_pushes_toward_the_low_side() {
        // 6 deep on the left, dry on the right: a dam holding back 6 of water.
        let (load, push) = load_on_pair(6.0, 0.0, Vec2::ZERO);
        assert_eq!(load, 6.0);
        assert_eq!(push, Vec2::X);
    }

    #[test]
    fn wear_only_bites_above_strength() {
        assert_eq!(wear(1.0, 5.0, 5.0, 1.0), 1.0, "a load at its strength does nothing");
        assert_eq!(wear(1.0, 2.0, 5.0, 1.0), 1.0, "nor does a lighter one");
        // Twice the strength breaks it in BREAK_SECS…
        let mut health = 1.0;
        let steps = (BREAK_SECS / DT).round() as usize;
        for _ in 0..steps {
            health = wear(health, 10.0, 5.0, DT);
        }
        assert!(health < 1e-3, "health left: {health}");
        // …and it never goes below zero.
        assert_eq!(wear(0.1, 100.0, 5.0, 1.0), 0.0);
    }

    #[test]
    fn tipping_swings_the_top_toward_the_push() {
        let rest = Transform::from_xyz(0.0, 5.0, 0.0); // centre 5 up from its base
        for toward in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::new(1.0, 1.0).normalize()] {
            let flat = tipped(&rest, Vec3::ZERO, toward, FRAC_PI_2);
            let expected = Vec3::new(toward.x, 0.0, toward.y) * 5.0;
            assert!(flat.translation.distance(expected) < 1e-4, "toward {toward}: {}", flat.translation);
            // Its own up axis now points along the push too.
            assert!((flat.rotation * Vec3::Y).distance(expected / 5.0) < 1e-4);
        }
        assert_eq!(tipped(&rest, Vec3::ZERO, Vec2::X, 0.0), rest, "no angle, no change");
    }

    #[test]
    fn a_building_blocks_the_cells_under_it() {
        let centre = cell_to_world(50.0, 60.0);
        // 10×6 world units = 2.5×1.5 cells: covers cell centres within ±1 cell in x, ±0 in z.
        let cells = cells_under(Vec3::new(centre.x, 0.0, centre.y), Vec2::new(5.0, 3.0), Quat::IDENTITY);
        let mut expected = vec![idx(49, 60), idx(50, 60), idx(51, 60)];
        let mut got = cells.clone();
        expected.sort();
        got.sort();
        assert_eq!(got, expected);
        // Turned 90° it runs along z instead.
        let turned = cells_under(Vec3::new(centre.x, 0.0, centre.y), Vec2::new(5.0, 3.0), Quat::from_rotation_y(FRAC_PI_2));
        assert!(turned.contains(&idx(50, 59)) && turned.contains(&idx(50, 61)) && !turned.contains(&idx(49, 60)));
    }

    #[test]
    fn a_thin_wall_still_blocks_a_line_of_cells() {
        let centre = cell_to_world(40.0, 40.0);
        // 1.4 units thick — thinner than a cell — but it must still dam.
        let cells = cells_under(Vec3::new(centre.x, 0.0, centre.y), Vec2::new(CELL * 0.51, 0.7), Quat::IDENTITY);
        assert!(cells.contains(&idx(40, 40)));
    }

    #[test]
    fn spawned_buildings_and_walls_get_blockers() {
        // Every building in a real plan covers at least one cell (so it dams).
        let plan = plan_scenery(&valley(), &dry(), &all());
        for b in &plan.buildings {
            let at = Vec3::new(b.pos.x, b.base, b.pos.y);
            assert!(!cells_under(at, Vec2::new(b.size.x, b.size.z) / 2.0, Quat::IDENTITY).is_empty());
        }
        for p in plan.ruins.iter().filter(|p| p.breakable && p.shape == PieceShape::Block) {
            assert!(!cells_under(p.center, Vec2::new(p.size.x, p.size.z) / 2.0, p.rotation).is_empty());
        }
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
