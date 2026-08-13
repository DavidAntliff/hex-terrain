---
tags: [water, rendering, shader, spec]
type: spec
status: stale
updated: 2026-08-13
---
# Spec: Water surface

How a water plate is shaded so that it reads as water. The plate's *geometry* — one per location,
full width, one ring past the flooded set, cut at the shoreline by the depth buffer — belongs to
[[terrain]] and is not restated here; this spec covers only its appearance.

## Requirements

### Goal (definition of done)

A lake or small sea seen from an aircraft reads as water rather than as blue paint: the sun's
reflection breaks into glitter across a ripple field, the surface takes the sky's colour more
strongly towards grazing angles, and the water pales into the shallows exactly at the shoreline.
No waves — at this scale the surface is flat, and only its normal is disturbed.

### Constraints

- **WebGL2 is the web baseline**, inherited from [[scene]] and binding here more than anywhere
  else. It rules out the depth prepass texture, screen-space reflections, and anything needing
  compute. See Design discussion for what each of those would have bought.
- **One dependency**, `bevy` — inherited from [[scene]]. No water crate, and no new cargo feature:
  every `StandardMaterial` field used is an ungated scalar.
- **The plate stays opaque.** Load-bearing twice over, and the reason transparency is out of scope
  rather than merely deferred. [[terrain]] cuts the shoreline with the depth buffer and no clipping
  geometry, which needs the plate to write depth; and neighbouring plates *overlap* by design under
  the one-ring rule, so alpha blending would darken the overlap twice and draw hexagons on the sea.
- **No geometry changes.** The plate is a seven-vertex fan and stays one: a perturbed normal is
  what produces glitter, and subdividing to displace it would buy nothing at lake scale.
- Everything in [[terrain]] that water already touches — `WATER_LIFT`, the one-ring rule,
  `Terrain::surface` as the address of a location — is untouched.

### Functional requirements

In scope: the water material and its shader, the ripple field, the shoaling colour, and the
per-vertex depth the renderer computes to drive it.

Out of scope, deliberately: reflections of the terrain, refraction and transparency, foam and
shoreline lines, displaced waves, flow, and anything that makes water *behave* rather than look
right — which is where [[terrain]] already drew the line.

## Design discussion

**The problem was upstream of the shader.** [[terrain]] recorded the water as reading like painted
blue and correctly diagnosed why: *a smooth plane under one directional light with no environment
map has nothing to reflect*. Ripples alone would only have made the paint bumpy. The first half of
the fix is therefore not in this spec at all — it is the daylight sky and environment map in
[[scene]]. Shading came second, and is worth much less without it. Locked.

**An `ExtendedMaterial`, not a material of its own.** Fresnel, the environment reflection and the
sun's specular are all things `StandardMaterial` already does correctly, and doing them by hand
would be both more code and worse. The extension overrides `fragment_shader()` only, perturbs
`pbr_input.N` and `.material.base_color`, and hands straight back to `apply_pbr_lighting`. Locked.

**Depth comes from the model, not from the depth buffer.** The usual way to shade shallows is to
sample the depth prepass texture and difference it against the surface — which WebGL2 cannot do:
Naga does not generate correct GLSL for sampling depth textures, the same bug that rules out SSR.
But the depth never needed measuring, because the model holds every height. A plate's seven
vertices are its centre and the six lattice vertices, which are exactly the points `wall_mesh`
already places terrain at, so the existing `corner_height` gives the ground under each one.

This is better than the depth buffer rather than merely equivalent to it: the depth is exact at the
vertices, it costs one subtraction, it works on every backend, and at a shoreline the corner mean
*is* the terrain's own wedge height, so the depth there is exactly zero — the pale water meets the
line the depth buffer cuts with nothing to line up by hand. Locked.

**Depth rides in `uv.x`.** The fan already wrote a UV it never used, `VertexOutput` already carries
it, and an untextured standard material ignores it — so a vertex attribute of its own would buy
nothing but a vertex layout to specialize. The cost is a sharp edge: **a water plate's UVs are not
texture coordinates**. Locked, with the comment saying so at both ends.

**Ripples: six trains, golden-angle directions, real dispersion.** Four evenly-spread sine waves
look like woven fabric, which is what a first attempt produced. Two changes fix it without adding
cost: directions step by the golden angle, so no two trains are near-parallel and none line up into
a plaid; and each train travels at the speed deep-water waves actually travel, `c = √(gλ/2π)`, so
short ripples lag long ones and the trains slide out of phase forever instead of returning to the
same pattern on a cycle. The shader sums *gradients* rather than heights, so a normal falls out
without a height ever being evaluated. Locked.

**Ripples fade with distance, and roughness rises as they go.** Not polish. Once a ripple is
smaller than a pixel the pattern turns into crawling shimmer, and from an aircraft that is most of
the water on screen. Trading the detail for roughness as it recedes keeps the glitter — which is
the part that still reads at that range — and drops the aliasing. Locked.

**Rejected: screen-space reflections.** Bevy 0.19 ships raymarched SSR, and its own `examples/3d/ssr.rs`
is a water demo, so this was the obvious candidate. Its documentation is explicit: *"Screen-space
reflections are presently unsupported on WebGL 2 because of a bug whereby Naga doesn't generate
correct GLSL when sampling depth buffers."* It also requires deferred rendering, which forfeits
`specular_tint` and `diffuse_transmission`. Fails web parity; rejected.

**Rejected for now: planar reflections.** A second camera mirrored about the sea plane, rendered to
a texture the water samples and distorts by its own ripple normal. It would work on WebGL2 and
would give genuine terrain reflections. It costs a full extra scene render, is correct for one
water plane only — so mountain lakes at other levels get nothing — and needs the submerged terrain
clipped or it reflects the lake bed. From an aircraft, reflected terrain occupies a thin band near
the far shore, so this is the worst value per unit of complexity of anything considered. Deferred,
not rejected on principle.

**Rejected: refraction through `specular_transmission`.** Would show the bottom through shallow
water, which is the one cue this design fakes with colour rather than simulating. It needs the
plate to stop being opaque, which breaks the shoreline cut and the overlapping-plate rule above.
Rejected on those grounds, not on cost.

## Implementation details

```
src/view/grid_render.rs   WaterMaterial, WaterExtension, WaterSettings; water_fan_mesh; sync_water
assets/shaders/water.wgsl the fragment shader
src/view/mod.rs           registers MaterialPlugin::<WaterMaterial>
```

- `WaterMaterial` is `ExtendedMaterial<StandardMaterial, WaterExtension>`. The base carries the
  deep colour, `perceptual_roughness: 0.03` — near a mirror, because the ripples are what spread
  the sun into a glitter — and `reflectance: 0.5`, which is already about right for water's IOR of
  1.33. What was missing was never the number; it was something to reflect.
- `WaterSettings` mirrors the WGSL struct of the same name. The two are bound by nothing but
  agreement, so **they move in the same edit**. Bindings start at 100 because 0-99 belong to the
  base material.
- `water_fan_mesh` builds one mesh per plate, carrying that plate's depths. This is why
  `SharedAssets` holds a shared cap mesh but no shared water mesh: no two plates are alike.
- A dry location carrying a flooded neighbour's plate gets **negative** depths over its own ground.
  Not a special case: the plate is buried there, and the shader clamps at zero anyway.
- `sync_water` still throws every surface away and rebuilds on any change — now a mesh each as well
  as an entity each. Marked `ponytail:`; still nothing at 37 locations.

API specifics this code depends on are recorded in [[bevy-0-19-api]].

## Verification plan

Performed:

- `cargo test` — the whole existing suite still passes unchanged, including the four that guard the
  surface: winding against stored normals, nothing facing downwards, bitwise agreement on shared
  edges and corners, and water reaching exactly one ring.
- `cargo test` — one new test on the depth attribute: over a lake bed one unit down the centre
  vertex reads `1.0`; at a shoreline corner, where the corner mean is exactly the water line, it
  reads `0.0`; and a dry shoreline location's buried plate reads `-0.5`.
- `cargo run` with `HEX_TERRAIN_SCREENSHOT` — at sea level `+0.45`, close in and looking towards
  the sun: the sun's reflection breaks into glitter, the shallows pale towards the islands, the
  deep water is blue, and ripple texture is visible in the middle distance. Looking away from the
  sun and further out, ripples fade and the surface takes the sky.
- The ripples strengthen towards grazing angles and vanish looking straight down, which is what
  water does, and falls out of Fresnel rather than being arranged.

- `trunk build --release`, served by a static web server — **confirmed rendering in Firefox on
  WebGL2**. This is the check that counts for the whole design: WGSL→GLSL translation happens in
  the browser at runtime, so the wasm build proves nothing on its own, and a construct with no GLSL
  equivalent fails there and nowhere else.

`WGPU_BACKEND=gl` was the intended cheap proxy for that check and turned out to be unavailable:
neither the NVIDIA driver nor a software Mesa fallback offered wgpu a GL adapter. Recorded because
the next person will reach for it too.

Not verified: Chrome, this time round; the sea-level slider dragged by hand, still; and the ripple
animation in motion — every screenshot is a single frame, so the dispersion that keeps the pattern
from repeating is argued from the arithmetic rather than observed.

## Implementation status

**status:** stale — one constraint is contradicted, and this document is the deficient side of it.
Reconciling the wording needs agreement and has not been done.

**"No geometry changes. The plate is a seven-vertex fan and stays one"** is violated: a plate now has
thirteen vertices — centre, six lattice corners, six edge midpoints — and draws up to twelve triangles,
of which it may draw only some. The constraint's *reason* still stands untouched: nothing is
subdivided in order to displace a surface, and the normal is still what makes the glitter. The
midpoints were added for two other reasons, both in [[terrain]]'s territory rather than this spec's —
they halve a sector so a water level can be granted only the part of a location its own body reaches,
and they put a vertex over the bridge, where the old chord from corner to corner interpolated between
two corner means and missed the different height running underneath. So the shallows along an edge
used to be shaded from a depth the water does not have there; they now are not.

Nothing about the shading this spec actually owns has changed.

Deliberate omissions:

- No reflection of the terrain, by the reasoning above.
- The shallow tint is a colour, not the lake bed seen through water. Nothing is refracted and
  nothing is seen through the surface.
- `WaterSettings` is one set of values for every body of water. A mountain lake and the sea shade
  identically.
- Ripples are uniform across the whole surface: no wind direction, no fetch, no sheltered water in
  the lee of high ground.

## Related

- [[terrain]]: owns the water plate's geometry, the one-ring rule and the depth-buffer shoreline
- [[scene]]: the daylight sky and environment map that give the water something to reflect
- [[bevy-0-19-api]]: `ExtendedMaterial`, and what WebGL2 rules out
