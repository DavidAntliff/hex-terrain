// Terrain: a biome's flat colour, broken up by noise evaluated in world space.
//
// An extension to the standard material rather than a material of its own, so the lighting, the
// shadows and the environment reflection all come from stock PBR. This shader supplies only what
// the material cannot know: that two caps of the same biome should not be the same colour.
//
// The noise is a function of the **world position**, not of position within a cell, which is the
// whole point — anything keyed to the cell would repeat with the lattice and read as tiles no
// matter how well the boundaries between biomes are blended. It is evaluated in three dimensions
// rather than two so that a cap and the wall below it differ, and so that nothing here has to know
// which plane the grid lies in.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

struct TerrainSettings {
    // How far the tint may swing either side of the material's own colour, as a fraction of it.
    tint_amplitude: f32,
    // Size of the largest noise feature, in world units. Wants to be several hexes across: the job
    // is large-scale variation across the map, and detail finer than a cap only reads as noise.
    tint_wavelength: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> terrain: TerrainSettings;

// How many octaves the tint sums, and how much of the previous one's amplitude each keeps.
//
// The persistence is 0.6 rather than the textbook 0.5, and it is load-bearing. What makes a cap read
// as flat is that it is uniform *within itself*, so the octaves shorter than one hexagon are the
// ones doing the work — and at 0.5 the fourth octave carries under 7% of the swing, which at any
// usable amplitude is about one part in a hundred of brightness and simply invisible. At 0.6 it
// carries half again as much, and the surface stops being a flat polygon.
const OCTAVES: i32 = 5;
const PERSISTENCE: f32 = 0.6;

// The lattice is shifted this far into positive territory before its index is hashed, so nothing
// depends on how a negative value converts to `u32`. Far beyond any coordinate this scene reaches,
// even at the finest octave.
const LATTICE_ORIGIN: f32 = 65536.0;

// One lattice corner's value, in `0..1`.
//
// An integer avalanche rather than the usual `fract(sin(...))`: that one's quality depends on how
// the driver evaluates `sin` at large arguments, which differs between GPUs and shows up as visible
// banding on some of them. This is the standard xor-shift-multiply finalizer, and is the same family
// as the skirt's `wobble` on the Rust side.
//
// Only the top 24 bits reach the float, because that is what an `f32` can hold exactly.
fn lattice(cell: vec3<u32>) -> f32 {
    var h = (cell.x * 0x9e3779b9u) ^ (cell.y * 0x85ebca6bu) ^ (cell.z * 0xc2b2ae35u);
    h ^= h >> 16u;
    h *= 0x7feb352du;
    h ^= h >> 15u;
    h *= 0x846ca68bu;
    h ^= h >> 16u;
    return f32(h >> 8u) * (1.0 / 16777216.0);
}

// Trilinear value noise, smoothstepped between lattice corners so the result has no creases at the
// cell boundaries.
fn value_noise(p: vec3<f32>) -> f32 {
    let base = floor(p);
    let f = p - base;
    let w = f * f * (3.0 - 2.0 * f);
    let c = vec3<u32>(base + LATTICE_ORIGIN);

    let x0y0 = mix(lattice(c), lattice(c + vec3(1u, 0u, 0u)), w.x);
    let x1y0 = mix(lattice(c + vec3(0u, 1u, 0u)), lattice(c + vec3(1u, 1u, 0u)), w.x);
    let x0y1 = mix(lattice(c + vec3(0u, 0u, 1u)), lattice(c + vec3(1u, 0u, 1u)), w.x);
    let x1y1 = mix(lattice(c + vec3(0u, 1u, 1u)), lattice(c + vec3(1u, 1u, 1u)), w.x);

    return mix(mix(x0y0, x1y0, w.y), mix(x0y1, x1y1, w.y), w.z);
}

// Octaves of value noise, each half the amplitude and about twice the frequency of the one before,
// normalised back to `0..1`.
//
// The step between octaves is 2.03 and not 2, and each is displaced as well as scaled. Doubling
// exactly would land every octave's lattice on the same integer planes, and they would reinforce
// there into a visible grid; a step slightly off two never lets them line up again.
fn fbm(p: vec3<f32>) -> f32 {
    var sum = 0.0;
    var total = 0.0;
    var amplitude = 1.0;
    var q = p;
    for (var i = 0; i < OCTAVES; i++) {
        sum += amplitude * value_noise(q);
        total += amplitude;
        amplitude *= PERSISTENCE;
        q = q * 2.03 + vec3(17.3, 9.1, 23.7);
    }
    return sum / total;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    // Multiplicative, and in linear space, so the biome's hue survives and only its brightness
    // moves. Mixing towards a second colour instead would make every biome drift towards the same
    // one wherever the noise is high.
    let n = fbm(in.world_position.xyz / terrain.tint_wavelength);
    let tint = 1.0 + (n - 0.5) * 2.0 * terrain.tint_amplitude;
    let base = pbr_input.material.base_color;
    pbr_input.material.base_color = vec4(base.rgb * tint, base.a);

    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
