//! Surround terrain — the land beyond the edge of the playable map.
//!
//! The sim grid is only ~600 units across. Past its edge the camera used to see
//! the atmosphere's "planet ground": a flat grey band between the map and the
//! horizon. This builds a big, purely decorative ring of land around the map:
//!
//!   * right at the edge it continues the map's own heights (no seam, no cliff),
//!   * it relaxes into gently rolling countryside a few hundred units out,
//!   * and far away it rises into a ring of mountains that hides the horizon
//!     from any camera angle the orbit controls allow.
//!
//! The atmosphere's aerial perspective hazes the far land blue automatically,
//! so the mountains read as distant without any extra fog.
//!
//! It's rendered with the same ground-cover material as the map, but it isn't
//! part of the simulation: water, objects and scenery never see it.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::flood::{height_fraction, idx, smoothstep, terrain_normal, CELL, D, W};

/// How far (world units) the surround reaches past the map edge.
const OUTER: f32 = 9000.0;
/// Rings of vertices from the map edge out to OUTER. They're spaced
/// quadratically — dense near the map, where detail shows, sparse far away.
const RINGS: usize = 48;
/// Within this distance of the edge, heights ease from the map's exact edge to
/// a smoothed version of it (so a rim wall carries on as a soft ridge).
const EDGE_BLEND: f32 = 80.0;
/// Distance range over which the land relaxes from the map's edge height to
/// flat countryside at the edge's average height.
const COUNTRY_BLEND: (f32, f32) = (150.0, 1800.0);
/// Rolling hills: size (world units per noise cell) and height.
const HILL_SIZE: f32 = 350.0;
const HILL_HEIGHT: f32 = 30.0;
/// Distant mountains: where they start/peak and how tall they get.
const MOUNTAIN_RANGE: (f32, f32) = (2500.0, 6500.0);
const MOUNTAIN_SIZE: f32 = 1400.0;
const MOUNTAIN_HEIGHT: (f32, f32) = (350.0, 1100.0); // lowest saddle .. tallest peak
/// Open land turns from the edge's ground cover to grass (at least this height
/// fraction, above the sand line) over this distance.
const GRASS_FRACTION: f32 = 0.45;
const GRASS_BLEND: f32 = 250.0;
/// Distance at which the ground-cover patches reach their largest size.
const FAR_PATCHES: f32 = 4000.0;

/// The map's edge cells, in order around the square (each listed once).
fn perimeter() -> Vec<(usize, usize)> {
    let mut p = Vec::with_capacity(2 * (W - 1) + 2 * (D - 1));
    p.extend((0..W - 1).map(|x| (x, 0))); // back edge, left → right
    p.extend((0..D - 1).map(|z| (W - 1, z))); // right edge, back → front
    p.extend((1..W).rev().map(|x| (x, D - 1))); // front edge, right → left
    p.extend((1..D).rev().map(|z| (0, z))); // left edge, front → back
    p
}

/// Distance (world units) of ring `k` from the map edge.
fn ring_distance(k: usize) -> f32 {
    OUTER * (k as f32 / RINGS as f32).powf(2.2)
}

/// Repeatable pseudo-random value in 0..1 for an integer lattice point.
fn lattice(x: i32, z: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b9) ^ (z as u32).wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

/// Smooth value noise in 0..1: random values on a lattice, blended between.
fn value_noise(p: Vec2) -> f32 {
    let (x0, z0) = (p.x.floor(), p.y.floor());
    let (fx, fz) = (p.x - x0, p.y - z0);
    let (ux, uz) = (fx * fx * (3.0 - 2.0 * fx), fz * fz * (3.0 - 2.0 * fz));
    let (ix, iz) = (x0 as i32, z0 as i32);
    let top = lattice(ix, iz) * (1.0 - ux) + lattice(ix + 1, iz) * ux;
    let bottom = lattice(ix, iz + 1) * (1.0 - ux) + lattice(ix + 1, iz + 1) * ux;
    top * (1.0 - uz) + bottom * uz
}

/// A few octaves of value noise ("fractal"), roughly 0..1.
fn fbm(p: Vec2) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut q = p;
    for _ in 0..4 {
        sum += amp * value_noise(q);
        q = q * 2.03 + Vec2::new(17.1, 9.4);
        amp *= 0.5;
    }
    sum / 0.9375 // 0.5 + 0.25 + 0.125 + 0.0625
}

/// Height of the surround at world point `p`, `r` units out from the map edge.
/// `edge` is the map's height where this spoke leaves it, `edge_smooth` the
/// same averaged along the edge, and `country` the level of the open land.
pub(crate) fn surround_height(p: Vec2, r: f32, edge: f32, edge_smooth: f32, country: f32) -> f32 {
    let near = edge + (edge_smooth - edge) * smoothstep(0.0, EDGE_BLEND, r);
    let base = near + (country - near) * smoothstep(COUNTRY_BLEND.0, COUNTRY_BLEND.1, r);
    let hills = (fbm(p / HILL_SIZE) - 0.5) * 2.0 * HILL_HEIGHT * smoothstep(0.0, 400.0, r);
    // Ridged noise (1 - |n|) gives sharp crests instead of round lumps.
    let ridge = 1.0 - (fbm(p / MOUNTAIN_SIZE + Vec2::new(3.7, 1.3)) - 0.5).abs() * 2.0;
    let peak = MOUNTAIN_HEIGHT.0 + (MOUNTAIN_HEIGHT.1 - MOUNTAIN_HEIGHT.0) * ridge * ridge;
    let mountains = peak * smoothstep(MOUNTAIN_RANGE.0, MOUNTAIN_RANGE.1, r);
    base + hills + mountains
}

/// Build the surround mesh for terrain `t`: rings of vertices growing outward
/// from the map's edge, each ring the edge square scaled up about the centre.
pub(crate) fn build_surround_mesh(t: &[f32]) -> Mesh {
    let half = (W - 1) as f32 * CELL * 0.5;
    let ring = perimeter();
    let n = ring.len();
    let edge: Vec<f32> = ring.iter().map(|&(x, z)| t[idx(x, z)]).collect();
    // Edge heights averaged over ±12 edge cells (wrapping round the corners).
    let edge_smooth: Vec<f32> = (0..n)
        .map(|i| (-12i32..=12).map(|o| edge[(i as i32 + o).rem_euclid(n as i32) as usize]).sum::<f32>() / 25.0)
        .collect();
    let country = edge.iter().sum::<f32>() / n as f32;
    let min_h = t.iter().copied().fold(f32::INFINITY, f32::min);
    let max_h = t.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    let mut positions = Vec::with_capacity(n * (RINGS + 1));
    let mut uvs = Vec::with_capacity(n * (RINGS + 1));
    for k in 0..=RINGS {
        let r = ring_distance(k);
        let scale = 1.0 + r / half;
        for (i, &(x, z)) in ring.iter().enumerate() {
            let p = Vec2::new(x as f32 * CELL - half, z as f32 * CELL - half) * scale;
            // Ring 0 *is* the map edge: use its exact height so there's no seam.
            let h = if k == 0 { edge[i] } else { surround_height(p, r, edge[i], edge_smooth[i], country) };
            positions.push([p.x, h, p.y]);
            // UV.x: same height → colour mapping as the map (sand low, grass
            // high) at the edge, turning to grass farther out so low land past
            // the drain isn't a desert. UV.y: how far out (0..1), which
            // terrain.wesl uses to enlarge its patches far away.
            let frac = height_fraction(h, min_h, max_h);
            let grassy = frac.max(GRASS_FRACTION);
            let away = smoothstep(0.0, GRASS_BLEND, r);
            uvs.push([frac + (grassy - frac) * away, (r / FAR_PATCHES).min(1.0)]);
        }
    }

    // Two triangles per quad between neighbouring spokes of neighbouring rings,
    // wound so they face up.
    let vi = |k: usize, i: usize| (k * n + i % n) as u32;
    let mut indices = Vec::with_capacity(RINGS * n * 6);
    for k in 0..RINGS {
        for i in 0..n {
            let (a, b, c, d) = (vi(k, i), vi(k, i + 1), vi(k + 1, i + 1), vi(k + 1, i));
            let pa = Vec3::from(positions[a as usize]);
            let up = (Vec3::from(positions[b as usize]) - pa).cross(Vec3::from(positions[c as usize]) - pa).y > 0.0;
            if up {
                indices.extend_from_slice(&[a, b, c, a, c, d]);
            } else {
                indices.extend_from_slice(&[a, c, b, a, d, c]);
            }
        }
    }

    // Smooth normals: sum each triangle's (area-weighted) normal into its corners.
    let mut normals = vec![Vec3::ZERO; positions.len()];
    for tri in indices.chunks_exact(3) {
        let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let pa = Vec3::from(positions[a]);
        let fnorm = (Vec3::from(positions[b]) - pa).cross(Vec3::from(positions[c]) - pa);
        for v in [a, b, c] {
            normals[v] += fnorm;
        }
    }
    // The edge ring takes the map's own normals, so the lighting matches across the seam.
    for (i, &(x, z)) in ring.iter().enumerate() {
        normals[i] = terrain_normal(t, x, z);
    }
    let normals: Vec<[f32; 3]> = normals.iter().map(|v| v.normalize_or(Vec3::Y).to_array()).collect();

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perimeter_visits_every_edge_cell_once() {
        let p = perimeter();
        assert_eq!(p.len(), 2 * (W - 1) + 2 * (D - 1));
        let mut seen = std::collections::HashSet::new();
        for &(x, z) in &p {
            assert!(x == 0 || z == 0 || x == W - 1 || z == D - 1, "({x}, {z}) is not on the edge");
            assert!(seen.insert((x, z)), "({x}, {z}) listed twice");
        }
    }

    #[test]
    fn rings_start_at_the_edge_and_grow_outward() {
        assert_eq!(ring_distance(0), 0.0);
        assert!((ring_distance(RINGS) - OUTER).abs() < 1e-3);
        for k in 0..RINGS {
            assert!(ring_distance(k + 1) > ring_distance(k));
        }
    }

    #[test]
    fn surround_starts_at_the_edge_height_and_rises_to_mountains() {
        let p = Vec2::new(1234.0, -567.0);
        assert_eq!(surround_height(p, 0.0, 12.0, 30.0, 20.0), 12.0);
        let far = surround_height(p, OUTER, 12.0, 30.0, 20.0);
        assert!(far >= 20.0 + MOUNTAIN_HEIGHT.0 - HILL_HEIGHT, "far land is only {far} high");
    }

    #[test]
    fn surround_mesh_shares_the_map_edge_heights() {
        let t: Vec<f32> = (0..W * D).map(|i| (i % 7) as f32).collect();
        let mesh = build_surround_mesh(&t);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(pos)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("no positions");
        };
        for (i, &(x, z)) in perimeter().iter().enumerate() {
            assert_eq!(pos[i][1], t[idx(x, z)]);
        }
    }
}
