---
tags: [biomes, terrain, shader, texturing, spec]
type: spec
status: approved
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
with soft edges. The fix is low-frequency noise in *world* space at a wavelength of several hexes,
which is deliberately not a function of position within a cell and so cannot align with the lattice.
This is why the noise arrives in step 2, before any blending work.

**One shader, two paths, selected by the mesh.** Caps carry no `ATTRIBUTE_COLOR` and take their
colour from a per-biome material; walls carry one and blend from a palette uniform. `#ifdef
VERTEX_COLORS` — a shader def the mesh pipeline sets from the vertex layout — picks the path, so one
`terrain.wgsl` serves both and there is no second material type.

**Attribute budget, fixed up front so the steps do not rework each other.** The wall mesh's
`ATTRIBUTE_COLOR` is `vec4(w0, w1, w2, ao)`: three blend weights and an ambient-occlusion scalar.
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
`(½,½,0)`, a lattice vertex `(⅓,⅓,⅓)`. The three cells meeting at a wedge must be ordered
identically from all three sides or the interpolation cracks along the internal seams; `mean_height`
already solves this for heights by sorting on `(q, r)` before summing, and that ordering is factored
into a shared helper so heights and biomes cannot drift apart. At the rim, fewer than three cells
present means weights over what is present — the same rule `mean_height` uses, needing no special
case.

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

**status:** approved — design agreed, nothing built. Written before the code, so nothing has been
verified yet; this section records what actually lands, step by step, as it does.

## Related

- [[terrain]]: the surface this colours, and the cap/wall split that makes the transition possible
- [[water]]: the material-extension pattern this copies, and the WebGL2 constraint that shaped it
- [[hex-grid]]: the grid and the debug panel the sliders are added to
- [[instrumentation]]: how each step's comparison captures are taken
- [[scene]]: the one-dependency and WebGL2-baseline constraints inherited here
- [[bevy-0-19-api]]: material extension bindings, vertex colours, and what WebGL2 rules out
- [[skirt]]: the other user of per-vertex colours on a generated mesh
