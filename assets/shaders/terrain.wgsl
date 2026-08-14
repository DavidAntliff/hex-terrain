// Terrain: a biome's flat colour, weathered into something that reads as ground.
//
// An extension to the standard material rather than a material of its own, so the lighting, the
// shadows and the environment reflection all come from stock PBR. Three things the material cannot
// know are supplied here: that two caps of the same biome should not be the same colour, and that a
// steep enough face is bare rock whatever grows on the flat ground above it.
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
    // Every biome's colour, indexed the way `Biome::index` orders them, linear. Only a wall reads
    // it — a cap carries its one biome in its own base colour — but both share this struct.
    palette: array<vec4<f32>, 5>,
    // The colour of exposed rock, **linear** — it is mixed against a base colour the material has
    // already converted out of sRGB, so it has to arrive in the same space.
    rock: vec3<f32>,
    // How far the tint may swing either side of the material's own colour, as a fraction of it.
    tint_amplitude: f32,
    // The grid's up axis in world space, from `HexLayout::plane`. Passed in rather than assumed,
    // because which axis is up is the view's to know and it is not always +Y.
    up: vec3<f32>,
    // Size of the largest noise feature, in world units. Wants to be around one hexagon or less:
    // features larger than a cap only shift whole caps against each other, and what reads as
    // texture is the detail *within* one. Drag it long for regional variation instead.
    tint_wavelength: f32,
    // Where rock starts taking over from the biome, and over how much slope it finishes. Both in
    // units of `1 - dot(N, up)`: level is 0, a 45 degree face is 0.29, and vertical is 1.
    rock_onset: f32,
    rock_width: f32,
    // How far the noise may push a blend weight either side of what the geometry says, and the
    // exponent the weights are raised to before being normalised again.
    blend_noise: f32,
    blend_sharpness: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> terrain: TerrainSettings;

// How many octaves the tint sums, and how much of the previous one's amplitude each keeps.
//
// The persistence is 0.8, far above the textbook 0.5, and it is the number that decides whether
// this reads as mottling or merely as one flat cap being darker than the next. Only the octaves
// *shorter than a cap* put variation inside a cap, and persistence is what pays them: at 0.5 they
// carry so little of the swing that the effect is a per-cap brightness shift and nothing else.
//
// Measured over a cap's worth of surface, as a share of the total swing:
//
//   wavelength 2.5, persistence 0.60, 5 octaves -> 26% inside a cap, 42% between caps
//   wavelength 1.6, persistence 0.80, 6 octaves -> 37% inside a cap, 32% between caps
//
// The second is the first setting where the texture within a cap outweighs the step between caps,
// which is what "mottled" means. The sixth octave reaches about a twentieth of a hexagon.
const OCTAVES: i32 = 6;
const PERSISTENCE: f32 = 0.8;

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

// Octaves of value noise, each `PERSISTENCE` of the amplitude and about twice the frequency of the
// one before, normalised back to `0..1`.
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

// How much of the nominal `-1..1` the noise's swing actually occupies, per unit of amplitude.
//
// This is not a fudge. Summed value noise is an **average of independent values**, so it
// concentrates about its mean exactly as any other average does, and the nominal `-1..1` is a range
// it essentially never visits. Measured over the grid at the settings above, `(fbm - 0.5) * 2` has a
// standard deviation of 0.15 and reaches its ends only at the rarest points.
//
// Left uncorrected, `tint_amplitude` names a tail it almost never reaches: a nominal 22% moved
// typical brightness by 4%, which on screen came to a peak difference of 3 parts in 255 — nothing a
// person can see. Dividing by two standard deviations makes the setting mean what it says at the
// extremes and about half of it across most of the surface, with only the thin tails clamped.
//
// **It has to be re-measured whenever `OCTAVES` or `PERSISTENCE` moves**, since both change how
// hard the sum concentrates.
const SWING_PER_AMPLITUDE: f32 = 0.30;

// Three biomes mixed by weight, with the boundary between them broken up.
//
// A straight `mix` of the weights the geometry gives is a linear crossfade, and reads as an
// airbrushed gradient at exactly the place the eye goes. Perturbing each weight by noise and then
// sharpening makes the boundary interlock instead — one biome fingers into the next, and pockets of
// each appear inside the other.
//
// **The perturbation is keyed to the biome, not to the slot it happens to occupy.** Two locations
// meeting at a seam list the same three cells in different orders, and a weight is only well
// defined up to that ordering; keying the noise to the identity makes both sides compute the same
// colour without either having to know how the other ordered them. The weights themselves are
// symmetric at every shared point — a half each along an edge, a third each at a lattice vertex —
// so nothing else about the ordering matters either.
fn blend(world_position: vec3<f32>, weights: vec3<f32>, ids: vec3<u32>) -> vec3<f32> {
    let p = world_position / terrain.tint_wavelength;
    // One octave is enough: this only has to make an edge wander, not carry detail of its own.
    let jitter = vec3(
        value_noise(p + vec3(f32(ids.x) * 37.1, 0.0, 0.0)),
        value_noise(p + vec3(f32(ids.y) * 37.1, 0.0, 0.0)),
        value_noise(p + vec3(f32(ids.z) * 37.1, 0.0, 0.0)),
    );

    var w = max(weights + (jitter - 0.5) * terrain.blend_noise, vec3(0.0));
    w = pow(w, vec3(terrain.blend_sharpness));
    w = w / max(w.x + w.y + w.z, 1e-5);

    return w.x * terrain.palette[ids.x].rgb
        + w.y * terrain.palette[ids.y].rgb
        + w.z * terrain.palette[ids.z].rgb;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let base = pbr_input.material.base_color;

#ifdef VERTEX_COLORS
    // A wall, which spans up to three locations and so cannot have one colour. The weights arrive
    // per vertex and the three biomes they belong to are packed into `uv.x`, constant across the
    // triangle and therefore interpolated exactly.
    //
    // Note that the standard material has already multiplied `base` by these weights, which is
    // meaningless. That product is discarded here rather than prevented; preventing it would mean
    // a specialized vertex layout for no gain.
    let packed = u32(in.uv.x + 0.5);
    let ids = vec3<u32>(packed % 8u, (packed / 8u) % 8u, (packed / 64u) % 8u);
    let surface = blend(in.world_position.xyz, in.color.rgb, ids);
#else
    // A cap, which is one biome throughout and carries it in its own base colour.
    let surface = base.rgb;
#endif

    // Rock last of the three, because it overrides whichever material the ground turned out to be.
    // A steep face is bare rock whatever grows on the flat ground above it, which is why this is a
    // slope test and not a biome — the `Rock` biome is a band of elevation and only looks similar
    // by coincidence. Caps need no exemption: they are level, so the mix never fires on one.
    let slope = 1.0 - dot(pbr_input.N, terrain.up);
    let bare = smoothstep(terrain.rock_onset, terrain.rock_onset + terrain.rock_width, slope);
    let material = mix(surface, terrain.rock, bare);

    // Multiplicative, and in linear space, so the material's hue survives and only its brightness
    // moves. Mixing towards a second colour instead would make everything drift towards that one
    // colour wherever the noise is high.
    let n = fbm(in.world_position.xyz / terrain.tint_wavelength);
    let swing = clamp((n - 0.5) * 2.0 / SWING_PER_AMPLITUDE, -1.0, 1.0);
    let tint = 1.0 + swing * terrain.tint_amplitude;
    pbr_input.material.base_color = vec4(material * tint, base.a);

    pbr_input.material.base_color =
        alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
