// Water: a ripple field over a plate, coloured by how deep it is.
//
// An extension to the standard material rather than a material of its own, so everything that
// makes water look like water — Fresnel, the environment reflection, the sun's specular — comes
// from the stock lighting. This shader only supplies two things the standard material cannot know:
// a normal broken up by ripples, and a colour that pales into the shallows.
//
// Nothing here displaces geometry. The plate stays the seven-vertex fan it is drawn as, because a
// perturbed normal is what produces glitter, and at lake scale the surface really is flat.

#import bevy_pbr::{
    mesh_view_bindings::{view, globals},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

struct WaterSettings {
    // Colour of water shallow enough to see the bottom through.
    shallow: vec3<f32>,
    // Depth, in the model's height units, by which the colour has reached the deep water of the
    // base material.
    shallow_depth: f32,
    // Peak slope of the ripple field. A slope, not a height: nothing is displaced, so an amplitude
    // in world units would mean nothing.
    amplitude: f32,
    // Length of the longest ripple, in world units.
    wavelength: f32,
    // How fast the pattern travels, in world units per second.
    speed: f32,
    // Distance at which ripples have faded to half strength.
    fade: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: WaterSettings;

const TAU: f32 = 6.2831853;

// Six crossing wave trains. Each contributes the gradient of `a·sin(k·(d·p) + c·k·t)`, the slope
// the surface would have; summing gradients rather than heights is what lets the normal be built
// without ever evaluating a height, and without displacing a single vertex.
//
// Two details keep it from reading as a woven texture, which is what a handful of sine waves
// otherwise looks like:
//
//   - Directions step by the **golden angle**, so no two trains are ever near-parallel or
//     near-perpendicular and none of them line up into a plaid.
//   - Each train travels at its own speed, from the deep-water dispersion relation `c = √(gλ/2π)`:
//     short ripples genuinely do lag long ones. The trains therefore slide out of phase with each
//     other forever instead of returning to the same pattern on a cycle.
fn ripple_slope(p: vec2<f32>, t: f32) -> vec2<f32> {
    const GOLDEN_ANGLE: f32 = 2.3999632;
    const TRAINS: i32 = 6;

    var slope = vec2(0.0);
    var total = 0.0;
    for (var i = 0; i < TRAINS; i++) {
        let angle = f32(i) * GOLDEN_ANGLE;
        let dir = vec2(cos(angle), sin(angle));
        // Each train shorter, and contributing less slope, than the one before it.
        let scale = pow(0.68, f32(i));
        let weight = pow(0.78, f32(i));

        let k = TAU / (water.wavelength * scale);
        let phase = dot(p, dir) * k + t * water.speed * sqrt(scale) * k;
        slope += dir * weight * cos(phase);
        total += weight;
    }
    return slope * (water.amplitude / total);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let world_position = in.world_position.xyz;
    let distance = length(view.world_position.xyz - world_position);

    // Ripples fade out with distance, and the surface is made rougher as they go. Without this the
    // pattern turns into crawling shimmer as soon as a ripple is smaller than a pixel — which from
    // any altitude is most of the water on screen. Trading the detail for roughness keeps the
    // glitter, which is the part that reads at that range anyway.
    let near = water.fade / (water.fade + distance);
    let slope = ripple_slope(world_position.xz, globals.time) * near;

    // The plate is level, so its tangent frame is the world axes and the perturbed normal needs no
    // TBN matrix: the slope *is* the gradient in world units.
    pbr_input.N = normalize(vec3(-slope.x, 1.0, -slope.y));
    pbr_input.material.perceptual_roughness =
        mix(0.28, pbr_input.material.perceptual_roughness, near);

    // Water depth arrives in `uv.x`, put there per vertex by the renderer, which knows every
    // terrain height and so does not need the depth buffer — unavailable on WebGL2 — to work it
    // out. Depth reaches zero exactly where the terrain rises through the plate, so the shallows
    // meet the shoreline with no seam.
    let depth = max(in.uv.x, 0.0);
    let deep = pbr_input.material.base_color;
    let shoaling = clamp(depth / water.shallow_depth, 0.0, 1.0);
    pbr_input.material.base_color = vec4(mix(water.shallow, deep.rgb, shoaling), deep.a);

    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
