// Terrain ground-cover shader — extends Bevy's StandardMaterial (see `TerrainExt` in flood.rs).
//
// Like the water shader, Bevy still does all the lighting; this only picks the
// ground's COLOUR (and roughness) per pixel, from three things it can see:
//
//   1. Height — low ground near the stream bed is sand (darker where it's
//               usually wet), higher ground is grass.
//   2. Slope  — steep faces can't hold soil, so they show bare rock.
//   3. Noise  — large blotches of dry grass / dirt break up the bands so the
//               boundaries wander naturally, and fine grain gives texture up close.
//
// Blending per pixel (instead of per vertex on the CPU) is what makes this work:
// a terrain cell is 4 world units wide, far too coarse to carry grain or a
// natural-looking edge between sand and grass.

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
};

// Must match `TerrainSettings` in flood.rs field-for-field.
struct TerrainSettings {
    wet_sand: vec4<f32>,
    sand: vec4<f32>,
    grass: vec4<f32>,
    dry_grass: vec4<f32>,
    rock: vec4<f32>,
    // x = sand line (0..1 of the level's height range): grass starts above it.
    // y = rock slope: surfaces with normal.y below this are bare rock.
    // z = grain strength (light/dark variation), w = unused.
    params: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> ground: TerrainSettings;

// --- Noise (same gradient noise as water.wesl) -------------------------------

fn hash_grad(cell: vec2<f32>) -> vec2<f32> {
    let h = fract(sin(dot(cell, vec2(127.1, 311.7))) * 43758.5453);
    let a = h * 6.2831853;
    return vec2(cos(a), sin(a));
}

// 2D gradient noise in roughly [-1, 1].
fn gnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = dot(hash_grad(i), f);
    let b = dot(hash_grad(i + vec2(1.0, 0.0)), f - vec2(1.0, 0.0));
    let c = dot(hash_grad(i + vec2(0.0, 1.0)), f - vec2(0.0, 1.0));
    let d = dot(hash_grad(i + vec2(1.0, 1.0)), f - vec2(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * 1.4;
}

// Fine surface grain. Zoomed out, a pixel covers several grains and the noise
// would shimmer/alias, so each octave fades to its average (0) once `px` — the
// world size of one pixel — gets close to the octave's size.
fn grain(p: vec2<f32>, px: f32) -> f32 {
    let k0 = 1.0 - smoothstep(0.5, 1.5, px / 3.0);
    let k1 = 1.0 - smoothstep(0.5, 1.5, px / 0.9);
    return k0 * gnoise(p / 3.0) + k1 * 0.6 * gnoise(p / 0.9 + vec2(41.7, 13.1));
}

// --- Fragment --------------------------------------------------------------

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let p = in.world_position.xyz;
    let n = pbr_input.world_normal;
    let px = length(fwidth(p.xz));

    // Large, slow blotches (~60-unit patches) that nudge the band edges and
    // choose between lush and dry grass. UV.y is 0 on the map and grows to 1 far
    // out on the surround land (surround.rs): there the patches get ~6×
    // bigger and fainter, or the same 60-unit pattern repeated for kilometres
    // reads as camouflage. (Cross-fade to a second, bigger noise rather than
    // scaling one by distance — that warps the patches into radial streaks.)
    let far = in.uv.y;
    let near_blotch = gnoise(p.xz / 60.0) + 0.5 * gnoise(p.xz / 23.0 + vec2(7.3, 2.9));
    let far_blotch = gnoise(p.xz / 380.0 + vec2(3.1, 8.7)) + 0.5 * gnoise(p.xz / 140.0 + vec2(5.5, 1.2));
    let blotch = mix(near_blotch, 0.7 * far_blotch, smoothstep(0.0, 0.5, far));
    let fine = grain(p.xz, px);

    // 1. Height, 0 (lowest) .. 1 (highest) over this level's range — baked into
    //    the mesh's UV.x on the CPU, so switching levels needs no uniform update.
    //    Wobbled by the patches so the sand line isn't a contour line.
    let hw = in.uv.x + 0.04 * blotch + 0.01 * fine;
    let sand_line = ground.params.x;

    var color = mix(ground.wet_sand.rgb, ground.sand.rgb, smoothstep(0.0, sand_line * 0.6, hw));
    let grass = mix(ground.grass.rgb, ground.dry_grass.rgb, smoothstep(-0.2, 0.6, blotch));
    color = mix(color, grass, smoothstep(sand_line - 0.03, sand_line + 0.05, hw));

    // 2. Slope → rock. normal.y is 1 on flat ground and falls toward 0 on a cliff.
    let rock_line = ground.params.y + 0.05 * blotch;
    let rockiness = 1.0 - smoothstep(rock_line - 0.06, rock_line + 0.06, n.y);
    // Rock faces are near-vertical, so grain from xz would smear into streaks;
    // sample it across the face (horizontal position + height) instead.
    let strata = grain(vec2(p.x + 0.618 * p.z, p.y * 2.5), px);
    let rock = ground.rock.rgb * (1.0 + 0.18 * strata);
    color = mix(color, rock, rockiness);

    // 3. Grain: a little light/dark variation everywhere so it reads as a
    //    surface rather than flat paint.
    color *= 1.0 + ground.params.z * fine;

    pbr_input.material.base_color = vec4(color, 1.0);
    // Wet sand is a touch shinier; rock and grass are matte.
    let wetness = 1.0 - smoothstep(0.0, sand_line * 0.6, hw);
    pbr_input.material.perceptual_roughness = mix(0.95, 0.7, wetness * (1.0 - rockiness));

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
