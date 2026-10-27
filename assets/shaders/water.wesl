// Water surface shader — extends Bevy's StandardMaterial (see `WaterExt` in flood.rs).
//
// Bevy still does all the lighting (sun highlights, sky reflections from the
// atmosphere environment map, Fresnel). This shader only changes the INPUTS to
// that lighting, per pixel:
//
//   1. Ripples  — a procedural noise "height" is differentiated into a slope that
//                 tilts the surface normal. The noise scrolls along the local
//                 current (passed in through the mesh's UV channel), so the
//                 sparkle visibly flows downstream.
//   2. Depth    — the depth prepass tells us how far the view ray travels through
//                 water before hitting the bed. Thin water is clear (you see the
//                 sand), thick water is murky green-blue (Beer–Lambert absorption).
//   3. Shoreline — where that thickness falls to zero the water fades out, so the
//                 edge blends into the terrain instead of ending in a hard line.

//
// Written in WESL (Bevy 0.20+): `import` statements come first, module paths
// follow the file layout inside each crate, and `@if(FLAG)` replaces `#ifdef`.

import bevy_pbr::render::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::globals,
    view_transformations::depth_ndc_to_view_z,
};
@if(DEPTH_PREPASS) import bevy_pbr::prepass::utils::prepass_depth;

// Must match `WaterSettings` in flood.rs field-for-field.
struct WaterSettings {
    shallow_color: vec4<f32>, // colour + opacity of a thin film of water
    deep_color: vec4<f32>,    // colour + opacity once the view ray is "full" of water
    // x = absorption per world unit of water the ray crosses
    // y = shoreline fade distance (world units)
    // z = ripple strength (how far the normal tilts)
    // w = ripple size (world units per noise cell)
    params: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> water: WaterSettings;

// --- Noise -----------------------------------------------------------------

// Pseudo-random unit gradient for an integer lattice point.
fn hash_grad(cell: vec2<f32>) -> vec2<f32> {
    let h = fract(sin(dot(cell, vec2(127.1, 311.7))) * 43758.5453);
    let a = h * 6.2831853;
    return vec2(cos(a), sin(a));
}

// 2D gradient noise in roughly [-1, 1].
fn gnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f); // smoothstep fade
    let a = dot(hash_grad(i), f);
    let b = dot(hash_grad(i + vec2(1.0, 0.0)), f - vec2(1.0, 0.0));
    let c = dot(hash_grad(i + vec2(0.0, 1.0)), f - vec2(0.0, 1.0));
    let d = dot(hash_grad(i + vec2(1.0, 1.0)), f - vec2(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y) * 1.4;
}

// Ripple height at noise-space point q: three octaves, each slowly drifting in
// its own direction so even still water keeps shimmering.
fn ripple_height(q: vec2<f32>, t: f32) -> f32 {
    var h = gnoise(q + vec2(0.31, 0.17) * t);
    h += 0.5 * gnoise(q * 2.13 + vec2(-0.23, 0.41) * t);
    h += 0.25 * gnoise(q * 4.37 + vec2(0.52, -0.37) * t);
    return h;
}

// Slope (dh/dx, dh/dz) of the ripple height, by central differences.
fn ripple_slope(q: vec2<f32>, t: f32) -> vec2<f32> {
    let e = 0.08;
    let dx = ripple_height(q + vec2(e, 0.0), t) - ripple_height(q - vec2(e, 0.0), t);
    let dz = ripple_height(q + vec2(0.0, e), t) - ripple_height(q - vec2(0.0, e), t);
    return vec2(dx, dz) / (2.0 * e);
}

// "Flow map" ripples: two copies of the noise are dragged along the current,
// each restarting every PERIOD seconds (otherwise the pattern would smear out
// forever). They're half a period apart and cross-faded, so whenever one copy
// jumps back, it's invisible. A short PERIOD keeps fast water from stretching
// the noise into hair-like streaks.
fn flowing_slope(p: vec2<f32>, flow: vec2<f32>, t: f32) -> vec2<f32> {
    let PERIOD = 0.8;
    let ph0 = fract(t / PERIOD);
    let ph1 = fract(t / PERIOD + 0.5);
    let w1 = abs(ph0 - 0.5) * 2.0; // weight of copy 1: 1 exactly when copy 0 resets
    let q = p / water.params.w;
    let fq = flow / water.params.w;
    let s0 = ripple_slope(q - fq * ph0 * PERIOD, t);
    let s1 = ripple_slope(q - fq * ph1 * PERIOD + vec2(37.1, 11.7), t);
    return mix(s0, s1, w1);
}

// --- Fragment --------------------------------------------------------------

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // 1. Ripples. The mesh's UV holds the current in world units / second.
    var flow = in.uv;
    let speed = length(flow);
    if speed > 25.0 {
        flow *= 25.0 / speed; // cap how far a ripple can be dragged per cycle
    }
    // Fast water is choppier than a still pool.
    let strength = water.params.z * (0.6 + min(speed / 25.0, 1.4));
    let slope = flowing_slope(in.world_position.xz, flow, globals.time);
    var n = normalize(pbr_input.world_normal + vec3(-slope.x, 0.0, -slope.y) * strength);
    // A steep ripple can tilt the reflection below the horizon, where it would
    // pick up the dark ground under the sky and show as a black blob. Real water
    // never reflects what's beneath it, so ease the normal back toward the smooth
    // surface until the reflected ray points at the sky.
    for (var k = 0; k < 4; k++) {
        if reflect(-pbr_input.V, n).y >= 0.08 {
            break;
        }
        n = normalize(mix(n, pbr_input.world_normal, 0.5));
    }

    // 2. How much water lies between the surface and whatever is behind it.
    var thickness = 1.0e4;
    @if(DEPTH_PREPASS) {
        let scene_ndc = prepass_depth(in.position, 0u);
        if scene_ndc > 0.0 { // 0 = nothing drawn behind (reverse-Z far plane)
            let scene_z = depth_ndc_to_view_z(scene_ndc);
            let water_z = depth_ndc_to_view_z(in.position.z);
            thickness = max(water_z - scene_z, 0.0); // view-space z is negative ahead of the camera
        }
    }
    let absorb = 1.0 - exp(-thickness * water.params.x);
    let body = mix(water.shallow_color, water.deep_color, absorb);

    // Glancing views reflect more than they transmit (Fresnel), so make the
    // surface more opaque there — otherwise the blend would wash the
    // reflection out.
    let fresnel = pow(1.0 - clamp(dot(n, pbr_input.V), 0.0, 1.0), 5.0);
    var alpha = mix(body.a, 1.0, fresnel);

    // 3. Fade out right at the shore, and on near-vertical triangles (the mesh
    //    stretched up the side of a block or wall — real water is never that steep).
    alpha *= smoothstep(0.0, water.params.y, thickness);
    alpha *= smoothstep(0.3, 0.65, pbr_input.world_normal.y);

    // 4. Light it in two passes, because real water's colour and its shine
    //    react to ripples differently:
    //    * BODY — the colour comes from light scattered *inside* the water, so it
    //      ignores tiny surface ripples. Lit with the smooth surface normal and no
    //      reflection (reflectance 0). Lighting this with the rippled normal is what
    //      made dark "seed"-like flecks wherever a ripple faced away from the sun.
    //    * SHINE — the sky reflection and sun glints DO follow every ripple. Lit
    //      with the rippled normal and a black base colour (reflection only).
    var body_input = pbr_input;
    body_input.material.base_color = vec4(body.rgb, alpha);
    body_input.material.reflectance = vec3(0.0);
    let body_lit = apply_pbr_lighting(body_input);

    var shine_input = pbr_input;
    shine_input.N = n;
    shine_input.material.base_color = vec4(0.0, 0.0, 0.0, alpha);
    let shine_lit = apply_pbr_lighting(shine_input);

    var out: FragmentOutput;
    out.color = vec4(body_lit.rgb + shine_lit.rgb, alpha);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
