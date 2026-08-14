---
tags: [biomes, terrain, shader, texturing, spec]
type: spec
status: implemented
updated: 2026-08-14
---
# Spec: Biomes and procedural surface colour

Five biomes over the terrain surface, the transitions between them, and the procedural shading that
keeps a run of one biome from reading as repeated tiles. The geometry is [[terrain]]'s; this is what
colours it.

## Requirements

### Goal (definition of done)

The surface reads as landscape rather than as extruded tiles, which decomposes into three claims
that can each be checked:

1. **Five biomes are distinguishable and correctly placed.** The `biomes` scene shows sand,
   grassland, woodland, rock and snow, with every adjacent pair present, and the bands move when
   their thresholds move.
2. **A transition between two unlike biomes is an irregular, interlocking boundary**, not a linear
   crossfade — grass fingers into sand and sand pockets appear in grass — and the surface stays
   closed across it. Closure is a test, not an eyeball: two cells sharing a corner or an edge emit
   bitwise identical blend weights and identical biome identities for it, exactly as they already
   agree on height.
3. **A run of one biome carries variation that does not repeat with the hex lattice.** Nothing in
   the shading may be a function of position *within* a cell alone.

All three hold in a browser on WebGL2, not only natively.

### Constraints

- **The model stays dimensionless.** Biome derivation lives in `src/hex/` and knows nothing of world
  units. Every quantity with a size — noise wavelength, blend width, bump strength — lives in
  `src/view/` and reaches the shader from there.
- **Inherited from [[scene]]**: `bevy` is the only dependency, and everything builds for wasm, where
  **WebGL2 is the baseline**. No compute shaders, and no sampling the depth prepass — the constraint
  that shaped [[water]].
- **WGSL→GLSL translation happens in the browser at runtime.** A successful wasm build proves
  nothing; a construct with no GLSL ES 3.00 equivalent fails there and nowhere else. `WGPU_BACKEND=gl`
  is recorded in [[log]] as unavailable on the development machine, so there is no cheaper proxy than
  an actual browser.
- **The surface must stay closed.** Any per-vertex quantity shared by two cells has to be bitwise
  identical computed from either side. [[terrain]] establishes this for heights, and the same
  canonical ordering has to carry anything new.
- **Per-vertex data rides an attribute the base material ignores**, rather than a custom attribute
  that would need a vertex layout to specialize. This is the trick [[water]] already uses to carry
  depth in `uv.x`.
- **Caps share one mesh asset** moved by a `Transform`, so nothing per-cell can live on cap
  vertices. Anything varying per cell on a cap has to come from its material.
- **The blend width is `HexLayout::inset`**, not a second knob. The wall ring already *is* the
  transition band, and its width is already a CLI argument and a panel slider.
- **Material bindings start at 100**; 0–99 belong to the base `StandardMaterial`. See
  [[bevy-0-19-api]].
- **No texture assets.** The release wasm is already ~14 MB over the wire.

### Functional requirements

In scope: five biomes derived from height and water level; a colour per biome; large-scale procedural
variation in world space; rock exposed by slope; three-way blending across the wall ring with a
noise-perturbed boundary; normal perturbation from the noise field; and a debug slider for each
tunable that genuinely needs eyeball tuning.

Out of scope: pre-baked tiling textures (a decision point after the procedural work, not a
commitment); per-cell authored biomes; moisture, latitude or any second axis besides elevation;
vegetation or any added geometry; seasonal or time-varying appearance.

## Design discussion

**Biome is derived, not stored.** A `Biome` is a pure function of a location's height and water
level, computed where it is needed rather than held on `Terrain`. Two reasons. Nothing authors
biomes yet — there is no generator and no editor — so a stored field would be written by the same
formula at construction and merely cached. And `Terrain` has no `..default()` construction anywhere
in the tree, so every one of its ~20 exhaustive struct literals across `scenes.rs`, `hex/mod.rs` and
four test modules would have to change for a field nothing yet sets independently.

The derivation is dimensionless and belongs to the model regardless, so it lives in `src/hex/`, ready
to become a generator's default rather than something a generator has to displace. **Open**: when a
generator or an editor exists, an `Option<Biome>` on `Terrain` overriding the derivation is the
obvious extension, and the derivation becomes the `None` case.

**The cap/wall split is already the structure this needs.** [[terrain]] recorded the reason for
keeping them as separate meshes: *"It also lets the two carry different materials, and later a colour
blend across the wall between two biomes, which a per-cell combined mesh could not express as
cheaply."* That is exactly this. The cap is single-biome interior, the wall ring is the transition
band, and the ring divides into a **bridge** between two cells and a **wedge** between three — a
two-way and a three-way blend region, already built. Locked.

**Rejected: re-anchoring the render unit to the wedge triangle.** Making the primitive the
intersection of three hexes — the dual triangulation — is attractive, because a triangle with one
hex per corner gives a three-way weight from hardware interpolation for free. It was declined. The
wedge already *is* that triangle, cut into thirds, and the lattice vertex is already its exact
centroid in plan: the three cap corners are inset along directions 120° apart, which sum to zero. So
the geometry has already placed a vertex where the `(⅓,⅓,⅓)` weight belongs, and the interpolation
can be had without moving anything. Against that, re-anchoring would cost the per-location entity
that makes one location's visibility and one location's material a single component; water, whose
level is per-cell and flat by definition, could not follow onto a primitive spanning three cells; the
grid rim would need a new rule for degenerate triangles where [[terrain]]'s "average over what is
present" already suffices; and rebuild granularity would worsen, since a triangle owned by nobody
must be reassigned an owner anyway. Locked as rejected.

**Rejected for now: world position → hex coordinate in the shader, with biomes in a data texture.**
This needs no vertex attributes at all, works identically on caps and walls, and decouples blend
width from geometry entirely. It was declined because it duplicates `HexLayout::world_to_hex` in
WGSL — a second copy of the projection, in a second language, with the tests on only one of them.
Revisit if the blend ever needs to reach *inside* the caps, which the inset knob currently makes
unnecessary. If revisited, pass the inverse matrix as a uniform rather than restating it in the
shader.

**The blend function matters more than the palette.** A straight `mix` between two biome colours is a
crossfade, and reads as an airbrushed gradient — recognisably artificial at exactly the place the eye
goes. Perturbing the weights by the same noise field the shading already evaluates, then sharpening
and renormalising, makes the boundary irregular and interlocking for about five lines. Locked.

**Same-biome variation is the harder half, and the one that decides the result.** Blending boundaries
well does nothing if every grassland cap is identical; the surface still reads as tiles, just tiles
with soft edges. The fix is noise in *world* space, which is deliberately not a function of position
within a cell and so cannot align with the lattice. This is why it arrives in step 2, before any
blending work.

**What that noise has to be was got wrong twice, and both corrections are worth keeping.** The
obvious reading is that the job is large-scale variation across the map, at a wavelength of several
hexes. Two things are wrong with it.

*The wavelength.* Built that way the effect is nearly invisible, because each cap is very nearly
uniform **within itself**, and uniformity within one face is precisely what reads as flat — a cap
differing from its neighbour does not help, since the eye takes each cap as one surface either way.
Only the octaves *shorter than a cap* put texture inside a cap, and the persistence is what pays
them. Measured over a cap's worth of surface, as a share of the total swing:

| wavelength | persistence | octaves | inside a cap | between caps |
|---|---|---|---|---|
| 2.5 | 0.60 | 5 | 26% | 42% |
| 1.6 | 0.80 | 6 | 37% | 32% |

The second is the first setting at which the texture within a cap outweighs the step between caps,
which is what "mottled" means.

*The amplitude.* Summed value noise is an **average of independent values**, so it concentrates about
its mean exactly as any other average does, and the nominal `-1..1` is a range it essentially never
visits. Uncorrected, an amplitude setting names a tail it never reaches: a nominal 22% moved typical
brightness by 4%, and measured against a tint-off frame came to a peak of **3 parts in 255** — which
is nothing a person can see, and was reported as working before anyone looked hard. Dividing the
swing by two of its standard deviations makes the setting mean what it says. That divisor has to be
re-measured whenever the octave count or persistence moves.

Measured on `biomes` at 1280x720, against the same frame with the tint at zero: within-cap variation
rose from 0.99 to 3.56 levels of 255 across the two corrections, and peak difference from 8 to 16.

**One shader, two paths, selected by the mesh.** Caps carry no `ATTRIBUTE_COLOR` and take their
colour from a per-biome material; walls carry one and blend from a palette uniform. `#ifdef
VERTEX_COLORS` — a shader def the mesh pipeline sets from the vertex layout — picks the path, so one
`terrain.wgsl` serves both and there is no second material type.

**Attribute budget.** The wall mesh's `ATTRIBUTE_COLOR` carries three blend weights, and `uv.x`
carries the three biomes they belong to packed as `a + 8b + 64c`. The packed value is constant
across a triangle, so interpolating it returns it exactly — which is what lets identities share a
channel with anything interpolated at all. The colour channel's fourth component was reserved for an
ambient-occlusion scalar, which was built and then removed; see below.
Biome identities pack into `uv.x` as `a + 8*b + 64*c`, where the walls currently write an unused
placeholder. Identities are constant across a triangle, so interpolating them returns them exactly.

**The vertex-colour trap.** `pbr_fragment.wgsl` multiplies base colour by vertex colour under `#ifdef
VERTEX_COLORS` (verified in the vendored `bevy_pbr-0.19.0` source). With weights in `COLOR.xyz` that
multiply is meaningless — but harmless, because the extension overwrites
`pbr_input.material.base_color` outright afterwards and reads `in.color` itself. Stated here and at
the site, because it looks like a bug worth fixing and is not.

**Work proceeds cheapest-first, gated on screenshots.** Per-biome colour, then world-space noise,
then slope rock, then blending, then normal perturbation. Each is independently visible, and each is
captured through [[instrumentation]] at a fixed pose and pinned window size before the next begins,
so a step that does not earn its complexity can be dropped rather than discovered later underneath
four others.

## Implementation details

**Model** — `src/hex/biome.rs`: a `Biome` enum of five variants, a `Bands` struct of ascending
dimensionless height thresholds, and `Biome::at(&Terrain, &Bands)`. Ground under water is sand, so a
shoreline emerges as beach. Re-exported from `src/hex/mod.rs`. No Bevy types, no world units, and no
change to `Terrain`.

**Scene** — a `biomes` entry in `src/hex/scenes.rs` whose heights ramp across the grid so that all
five bands appear with every adjacent pair on screen. This is the fixed subject for every comparison
capture.

**View** — `src/view/grid_render.rs`: a biome colour palette beside the existing `CAP_FILL` and
`WALL_FILL`; per-biome cap and wall material handles on `SharedAssets`; a `HexCap` marker component,
symmetric with `HexWall` and `HexSkirt`, which also replaces the negative filter `src/probe/report.rs`
currently uses to find a cap; a `BiomeBands` resource shaped like the existing `SeaLevel`; and a
`sync_biomes` system gated on the model or the bands changing — `sync_cells` guards on the layout
alone, so a model-only change reaches nothing without it.

**Shader** — `assets/shaders/terrain.wgsl`, fragment-only, following `water.wgsl` exactly:
`pbr_input_from_standard_material` → mutate → `apply_pbr_lighting` →
`main_pass_post_lighting_processing`. It ships automatically, since `index.html` copies the whole
`assets/shaders` directory. `TerrainMaterial` is an `ExtendedMaterial<StandardMaterial,
TerrainExtension>` with a settings struct mirroring the WGSL struct field for field, registered
through `MaterialPlugin` in `src/view/mod.rs`.

A single `TerrainLook` resource holds every shader tunable, written into the material handles by one
`sync_look` system, so later steps add fields rather than plumbing. The grid's up axis reaches the
shader as a uniform taken from `HexLayout::plane`, keeping `GridPlane` knowledge in the view.

**Blend weights** — assigned by vertex role in `wall_mesh`: a cap corner is `(1,0,0)`, a bridge end
`(½,½,0)`, a lattice vertex `(⅓,⅓,⅓)`. At the rim, fewer than three cells present means weights over
what is present — the same rule `mean_height` uses, needing no special case, and the reason a
missing neighbour must be *dropped* rather than replaced by this cell's own biome. Replacing it
would have each of two cells weight itself double at a shared corner and the seam would part.

**A canonical ordering turned out not to be needed, which was not the expectation.** The plan was to
sort the three cells by `(q, r)` as `mean_height` does, so that two locations listing the same wedge
agree on which weight belongs to which cell. It is unnecessary, for two reasons that hold together:
the weights at every *shared* point are symmetric — a half each along an edge, a third each at a
lattice vertex — so which cell occupies which slot cannot change their sum; and the shader keys its
perturbation to the **biome identity** rather than to the slot, so the one place ordering could have
leaked back in does not. Both cells therefore compute the same colour from differently ordered
inputs. A test asserts it rather than the argument being trusted.

**Panel** — sliders follow the existing five-point recipe: resource, `init_resource`, a `Control`
variant, a `spawn_slider` call, and arms in `on_slider_changed` and `update_captions`, the latter
being an exhaustive match that will not compile without one.

## Verification plan

Per step: `cargo test`, then a capture through [[instrumentation]] at a fixed pose set and pinned
window size over both `biomes` and `sea`, reviewed before the next step starts.

Tests, all pure and needing no `App`:

- `Biome::at` at each band boundary, and the submerged-is-sand rule.
- **The closure test**, modelled on `terrain`'s existing agreement test: blend weights sum to one at
  every vertex, and two cells sharing a corner or an edge emit bitwise identical weights and
  identical packed identities for it.
- The existing winding and no-overhang tests continue to cover the wall mesh unchanged.

End to end: `cargo check --target wasm32-unknown-unknown`, `trunk build --release`, and **a real
browser on WebGL2** — the check that counts, per the constraint above.

## Implementation status

**status:** implemented — all five steps are built and agree with this spec, and the browser check
below has now been made. No divergence known.

- **Step 1, per-biome colour: done.** `Biome`, `Bands` and `Biome::at` in `src/hex/biome.rs`; the
  `biomes` scene; per-biome cap and wall materials; `HexCap`; `BiomeBands` and `sync_biomes`; the
  shoreline and snow-line sliders.
- **Step 2, procedural tint: done.** `assets/shaders/terrain.wgsl`, `TerrainMaterial`, `TerrainLook`
  and `sync_look`; the tint amplitude and scale sliders. **One claim in Design discussion above was
  wrong and has been corrected in place** rather than left to mislead: the noise's coarsest octave
  has to be about one hexagon, not several, or it is invisible. The correction is recorded there
  with what it was measured against.
- **Step 3, slope-driven rock: done.** A face steeper than the panel's threshold reads as bare
  rock whatever biome it belongs to, mixed in the shader from `1 - dot(normal, up)` with the up axis
  passed in from `HexLayout::plane`. Measured on `biomes`, it moves 6.2% of the visible terrain, by a
  median of 8 levels of 255 and a 99th percentile of 94 — a strong, localised effect.
- **Crevice ambient occlusion: built, measured, removed.** A per-vertex occlusion term rode in
  `uv.y` and attenuated `diffuse_occlusion`, which is the physically correct channel — it dims the
  indirect light a shut-in surface loses while leaving direct sun alone. It is invisible here, and
  the reason is a property of the scene rather than of the code: **this scene is lit mostly by its
  directional sun**, so the indirect term is small. Occluding *all* of a surface's indirect light —
  a constant 80%, ignoring the geometry entirely — moves the median terrain pixel by **zero** levels
  of 255 and the most extreme by 42. The geometric term the creases actually produce is about a
  third of that on a fifth of the wall vertices, which came to three levels. Cut rather than
  reproduced by darkening albedo instead: that would dim a crease in full sunlight too, which is
  what makes vertex AO look painted on. Revisit only if the lighting stops being sun-dominated.
- **Step 4, the blend across the wall: done.** `wall_mesh` writes a weight triple per vertex and the
  three biomes it blends, and the shader mixes them out of a palette uniform, perturbing the weights
  by noise and sharpening before renormalising so the boundary interlocks rather than crossfading.
  Walls collapsed from five materials to **one**, since a wall no longer has a colour of its own.
  `WALL_SHADE` and `biome_wall_fill` went with them: a wall now takes the same colours as the caps
  it joins, and what makes it read darker is the tilt of its normal, which the renderer already
  does. Measured on `biomes`, the blend moves 28% of the visible terrain at a median of 10 levels of
  255, and the perturbation a further 15% at a median of 7.
- **Step 5, normal perturbation: done, and the largest single gain of the five.** The tint field's
  slope, found by forward differences, tilts the normal; the gradient is projected onto the surface
  first so it works on a wall as well as a level cap and needs to know nothing about which plane the
  grid lies in. It fades with distance for the reason [[water]] fades its ripples — detail finer than
  a pixel is shimmer rather than texture. Measured on `biomes` against the same frame with the bump
  at zero: half the visible terrain changes, and the local luminance variation within a face rises
  from 19.4 to 20.7. The sample step matters and the intuition was backwards: **0.02 beat 0.06 and
  0.14**, because it is the finest octaves that read as a surface, not the broad tilt.
- **Verified in a browser on WebGL2 on 2026-08-14** — Chrome on ANGLE over Intel UHD (TGL GT1),
  `WebGL 2.0 (OpenGL ES 3.0 Chromium)`. Caps, walls, the three-way blend, the rock and the bump all
  render, and the frame matches the native one. The WGSL translates; nothing in it needed changing.
- **What that check first appeared to fail was not the shader at all.** Caps and walls drew nothing
  under `trunk serve`, and the shader looked guilty. It never ran: the dev server answers the
  `.meta` probe with its index page and a `200`, Bevy fails to deserialize that and abandons the
  asset, and the shader is then never fetched. `AssetPlugin { meta_check: AssetMetaCheck::Never }`
  in `main.rs` is the fix; the diagnosis and the one-line console check that finds it in a minute
  are in [[build-performance]]. Nothing here is WebGL2-specific — any host that 200s a missing path
  does the same, and a native run cannot reproduce it because the absent file reports missing.

## Related

- [[terrain]]: the surface this colours, and the cap/wall split that makes the transition possible
- [[water]]: the material-extension pattern this copies, and the WebGL2 constraint that shaped it
- [[hex-grid]]: the grid and the debug panel the sliders are added to
- [[instrumentation]]: how each step's comparison captures are taken
- [[scene]]: the one-dependency and WebGL2-baseline constraints inherited here
- [[bevy-0-19-api]]: material extension bindings, vertex colours, and what WebGL2 rules out
- [[skirt]]: the other user of per-vertex colours on a generated mesh
