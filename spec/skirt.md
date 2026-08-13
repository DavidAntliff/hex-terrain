---
tags: [terrain, rendering, spec]
type: spec
status: implemented
updated: 2026-08-13
---
# Spec: Terrain skirt

What closes the terrain from below. Each location extends downwards as a hexagonal prism of its own
length, and where the grid ends under water the prism carries the cut face of that water. Reverses
the *"Rejected: a skirt and an underside"* decision in [[terrain]], whose surface this hangs from.

## Requirements

### Goal (definition of done)

No view of the grid shows through it. From below it is a solid with a broken, stepped underside
rather than a smooth copy of the terrain; at a grazing angle no rim location has sky beneath it. At
the rim, water standing over a location is drawn as an opaque cut face graduated by depth, meeting
the ground where the ground rises through it. A debug-panel checkbox, unticked at startup, hides
every skirt and gives back the bare shell.

### Constraints

- **Nothing extends upwards.** A skirt is only ever below the surface it hangs from.
- **The skirt hangs from the wall's own outer rim**, bitwise. Any other rule is a crack in a solid
  that exists to have none.
- **A location's skirt covers exactly its own hexagon**, inherited from [[terrain]] — no more, so
  hiding a location stays a clean hexagon, and no less, so the solid has no gaps.
- **No prism may turn inside out.** Every bottom clears every piece of ground it hangs from, on any
  data the scenes produce.
- **The variation is deterministic**, and stable across runs and toolchains: the same grid must give
  the same underside every time.
- **The model stays dimensionless.** All of this is view-side; a skirt depth is a height, not a
  distance, and world units stay in `HexLayout`.
- **The cut face is opaque**, for [[water]]'s reason: the plates are opaque so the depth buffer can
  cut the shoreline, and a translucent cut against opaque water would be the odd one out.
- Inherited from [[scene]]: `bevy` is the only dependency, and everything builds for wasm, where
  WebGL2 is the baseline.

### Functional requirements

In scope: the prism geometry and its bottom, the per-location length variation, the water
cross-section at the rim, the material and vertex colours that shade both, and the hide toggle.

Out of scope: bedrock strata or any other texture on the cut; a skirt that follows a chunk boundary
rather than a location; caves, overhangs, or anything else that makes the underside a surface in its
own right; and hiding an individual location's skirt, which nothing yet asks for.

## Design discussion

**A closed prism on every location, not a rim wall.** The alternative is to emit sides only where a
location has no neighbour, plus one bottom per location — about a third of the triangles. It does
not work: the bottoms are at different heights by design, so every step between two neighbours is a
hole you can see up through. Closing those steps needs a second rule comparing two bottoms and
emitting a wall of the right sign, on top of the rim rule — which is exactly the *"boundary case in
every builder"* [[terrain]] rejected a skirt to avoid. The closed prism has no rule at all: every
location, six edges, one bottom, and the deeper neighbour's own side closes the step. It costs
~54 triangles a location, ~2k for the grid. Locked.

**One mesh per location, not one for the grid.** A single grid-wide skirt would be one entity and
one draw call, and could skip the buried interior sides. It was rejected because it breaks the
ownership rule [[terrain]] locks for cap, wall and plate alike: a location's geometry is its own, so
hiding one leaves a clean hexagon rather than a ragged hole, and adding one does not rewrite a mesh
shared with everything else. The skirt would have been the one piece of the surface not owned by a
location. The triangle saving is real and not worth that; at the scale where it would matter, the
answer [[terrain]] already names is chunking, and one grid-wide mesh is the opposite of chunking.
Locked.

**The bottom is a common floor plus a per-location step, not a depth below each cap.** Measuring
down from a location's own height mirrors the terrain, which is the smooth underside the variation
exists to avoid, and it is unsafe: a location's boundary dips towards its lower neighbours, so a cap
next to a deep one can hang below its own bottom. A floor `SKIRT_BASE` under the lowest ground in
the grid, stepped by `wobble ∈ -2..=2` times `SKIRT_STEP`, cannot invert as long as
`2·SKIRT_STEP < SKIRT_BASE` — asserted at compile time — and still leaves the shallowest prism a
third of the base to stand on. Locked.

**The variation is a hand-rolled integer hash of the coordinate.** It has to be deterministic to be
a property of the terrain rather than of the run. `DefaultHasher` is explicitly not stable between
toolchains, and a crate is out under [[scene]]'s one-dependency rule; the usual xor-shift-multiply
finalizer is four lines and spreads neighbouring coordinates apart instead of banding them. Locked.

**The water's cut face is confined to the rim**, unlike the prism. Inland the ground is all there is
to see, whatever stands over it. At the rim the cut comes from `water_plates` — the same function
the surfaces themselves come from — rather than from the location's own `Terrain::water`, so it
reaches exactly as far around the rim as the water does. That difference is not academic: under the
one-ring rule a *dry* location carries a flooded neighbour's plate where its own boundary dips under
that level, and testing its own water would leave that plate's edge cut off in mid-air at the grid's
boundary. Locked.

**The cut is shaded by vertex colour, not by the water shader.** `water.wgsl` sets the fragment
normal to point straight up, which is right for a level plate and wrong for a vertical face, so
reusing `WaterMaterial` would light the cut as though it faced the sky. The ramp is three
multiplications on the CPU and needs no shader at all: the same two colours and the same
`WATER_SHALLOW_DEPTH` the surface shoals over, so the cut agrees with the surface it cuts.

The two say the same thing from different directions — on a plate the argument is how deep the water
is at that point, on the cut it is how far down the column the vertex sits, and both mean *the first
`WATER_SHALLOW_DEPTH` of water is the pale part*. Those two constants were literals inside the
`WaterSettings` the material is built with; they are now named constants used by both. Locked.

**One mesh and one material for the whole skirt.** Rock and water are the same geometry with
different colours, so splitting them into two children would buy two entities and two materials for
nothing. The standard material multiplies its base colour by the vertex colour, so a white base lets
the mesh carry both. Locked.

**The toggle hides, rather than shows.** The skirt is what makes the grid a solid, so it is what the
scene should look like by default; the checkbox exists to get the bare shell back for inspection.
Hiding is a `Visibility` on the skirt children, which is the [[terrain]] ownership rule paying off —
no mesh is rebuilt and nothing else in the cell is touched.

**Rejected: culling the buried interior sides.** About two thirds of the geometry here is interior
sides that are never seen. Culling them means each location consulting its neighbours' bottoms,
which is the branch the closed prism exists to avoid, for triangles that are free at this size.
Recorded in the source as a `ponytail:` with the upgrade path.

## Implementation details

```
src/view/grid_render.rs  SKIRT_BASE · SKIRT_STEP · wobble · skirt_bottom · lowest_height ·
                         edge_profile · skirt_mesh · shoaled · HexSkirt · HideSkirt ·
                         sync_skirts · sync_skirt_visibility · Faces colours
src/view/debug_ui.rs     Control::HideSkirt and its checkbox
src/view/mod.rs          the resource and the two systems
src/probe/report.rs      NotACap — the skirt is excluded from the cap count
```

Details that are load-bearing:

- **`edge_profile` is the seam.** It returns the five points where a location's surface meets its own
  hexagon along one edge — corner, bridge end, edge midpoint, bridge end, corner — and both the wall
  and the skirt are built from it, so neither can drift from the other. The wall ignores the
  midpoint; the skirt needs it, because it splits the edge exactly where `Pieces` splits it and so
  lets a cut face be granted the halves its body reaches.
- **The prism is wound outward by construction.** `corner_offsets` runs clockwise seen from `+normal`,
  so a triangle taken along an edge and then downwards faces away from the centre; the bottom fan is
  a cap wound the other way. The existing winding test covers the skirt over both planes and both
  orientations.
- **A cut face's top sits at `level − WATER_TIE_BREAK`**, the same place the plate does, so no hairline
  of section shows above the water it is cutting.
- **A cut strip whose ground already stands above the level collapses** to zero height rather than
  inverting: each top vertex is clamped to its own ground, and a strip with both ends clear is
  skipped outright.
- **Vertex colours are linear.** `StandardMaterial` multiplies its own base colour — already converted
  out of sRGB — by the vertex colour, so a colour written as sRGB comes out several stops dark. See
  [[bevy-0-19-api]].
- **`Faces` carries colours only when asked.** The channel is empty for caps, walls and plates, and
  the attribute is inserted only when it is not, so those meshes keep their vertex layout and their
  pipeline.
- **The skirt rebuilds on a model change as well as a layout one**, unlike the wall: the sea-level
  slider moves the model's water, and a rim skirt cuts through it. Ordered after `apply_sea_level`,
  as `sync_water` is.
- **A cell's cap is still identified negatively** in the probe's report — the meshed child that is
  none of the wall, the skirt or a plate. Every new kind of child has to be excluded there or it is
  counted as a cap.

## Verification plan

`cargo test` — 81 tests, all pure. The ones that carry weight here:

- The skirt hangs from `edge_profile` and so does the wall, checked **bitwise** per location, edge and
  point, over both planes and both orientations. This is the crack test.
- Two locations either side of an edge produce the same line to hang from: heights bitwise, positions
  to a tolerance, as `cells_agree_on_every_shared_edge_and_corner` also allows since they are reached
  by adding different centres.
- Every bottom clears every point of its own boundary, over every registered scene — the invariant
  `2·SKIRT_STEP < SKIRT_BASE` buys, which is itself a compile-time assertion.
- Every skirt triangle is wound to match the normal it stores (the existing test, extended), and each
  reaches exactly the circumradius and no further.
- The cut face is confined to the rim: an interior location of a flooded grid emits no water-coloured
  vertex, a rim location does, every one of them sits at or below `level − WATER_TIE_BREAK`, its
  colour is the ramp evaluated at its own depth, and draining the grid returns it to all rock.
- `wobble` is stable at fixed coordinates and spends its whole `-2..=2` range over a 37-cell grid.

Visually, via `HEX_TERRAIN_CAMERA`, all confirmed on `sea` and `two-lakes`: from below the grid is a
closed solid with a stepped underside and no sky through it; at a grazing angle the rim is a row of
prisms of differing length rather than a knife edge; and at a low oblique angle a submerged rim
location shows its water as a pale band under the surface darkening with depth, over the rock below.

**Not verified:** the checkbox itself, by hand or otherwise — as [[hex-grid]] records, no pointer
injection is available on this machine, so it shares that gap with every other control in the panel.
The systems either side of it are covered: the resource has a default, and the caption and visibility
both derive from it.

## Implementation status

**status:** implemented.

Deliberate omissions:

- The buried interior sides are not culled (above).
- One `SKIRT_BASE` and one `SKIRT_STEP` for the whole grid, as `INSET` is — nothing yet wants a
  location to carry its own.
- The cut face is flat-shaded and matte, with none of the surface's ripple or specular. It is a cut
  through water, not a face of it.
- Hiding is all-or-nothing. Per-location hiding is a `Visibility` away but nothing asks for it.
- The skirt is rebuilt whole when the model or the layout changes, matching `sync_water` and
  `sync_cells`.

## Related

- [[terrain]]: the surface this hangs from, and the rejection it reverses
- [[water]]: the shoaling ramp the cut face shares with the plates
- [[hex-grid]]: the ownership rule and the debug panel the toggle joins
- [[bevy-0-19-api]]: vertex colours, and the material behaviour the cut relies on
