---
tags: [terrain, elevation, spec]
type: spec
status: implemented
updated: 2026-08-12
---
# Spec: Terrain height

Elevation as per-location data, and the surface that makes it visible: each location shows a level
cap at its own height, and the caps are joined to one another by inclined walls. The first real
payload on [[hex-grid]]'s `Location`, which until now carried an empty placeholder.

## Requirements

### Goal (definition of done)

Every location carries a height. The grid renders as a **continuous surface**: a level cap per
location, inset from its hexagon, with the ring between the two joining it to its neighbours' caps.
A location below the grid plane is a dip in that surface and nothing more — no geometry anywhere
refers to the plane. A single scale converts a height into world units. In the static scene the
heights come from a sinusoid along each axis, crossing zero. Labels sit on the caps, clicking
selects a location through its own hexagon, and the terrain casts shadows against enough ambient
fill that shadowed walls stay legible.

### Constraints

- **The model is dimensionless**, inherited from [[hex-grid]] and unchanged by this spec. A height
  is a signed level, not a distance; what it is worth in world units belongs to `HexLayout`.
- **One height scale, in one place** — the projection, beside the in-plane hex size. Neither the
  model nor the renderer may hold a second one.
- **Nothing refers to the grid plane.** It is notional: what a location's surface is at depends only
  on its own height and its neighbours'.
- **A location's mesh covers exactly its own hexagon** — no more, so that hiding or adding one is a
  clean hexagon's worth of surface, and no less, so that the grid has no gaps.
- **The surface is closed between neighbours.** Two locations meeting at a point of the lattice must
  put their geometry at exactly the same place, not merely a close one.
- **Geometry stays derived from `HexLayout::corner_offsets`**, so caps, walls and outlines cannot
  drift apart, and both orientations keep working.
- Inherited from [[scene]]: `bevy` is the only dependency, and everything must build for wasm,
  where WebGL2 is the baseline.

### Functional requirements

In scope: the height attribute and its placeholder generator, the world-unit height scale, the cap
and wall meshes and the rule that joins them, cap-anchored labels and outlines, picking by
footprint, and shadows with ambient fill.

Out of scope: real terrain generation (the sinusoid is a placeholder), height editing at runtime,
biome or any other payload, per-height colouring, and water.

## Design discussion

**Height is signed and dimensionless, and lives on `Terrain`.** `Grid<T>` was already generic and
`Grid::hexagon` already took a `fn(Axial) -> T`, so the generator drops in with no signature
change anywhere. Locked.

**The height scale is a second knob on `HexLayout`, not a variant of the first.** Vertical
exaggeration is the normal thing to want from terrain, so elevation and in-plane size are
deliberately independent. `mesh_scale` and `elevation` are the only places either is applied.
Locked.

**The surface is stitched between locations, not extruded from each.** This replaced an earlier
build in which every location was an independent prism — a column standing on the grid plane, or a
pit sunk into it whose walls ran back up to that plane. Those pit walls were the tell: the grid
plane is notional, so geometry that reaches it is an artefact of the meshing. Drawing the surface
*between* caps removes the reference entirely, and with it the whole column/pit distinction — the
sign of a height becomes nothing but a vertex position. Locked.

**A cap is inset and its wall is the ring around it.** Shrinking the cap while leaving the spacing
alone is what opens up a visible wall between neighbours, and gives it an incline even where two
heights match. Locked.

**The wall has two kinds of piece, because the gaps have two shapes.** An inset hexagon leaves a
strip along each edge and a triangle at each corner. So the wall is six **bridges**, each half of
the ramp to a neighbour's cap, and six **wedges**, each a third of the triangle joining the three
caps meeting at a lattice vertex, cut at its centroid. Locked.

**A bridge is level along its length, at the mean of the two heights either side.** This is the
rule the construction turns on, and getting it wrong is what a first attempt did: it put the wall's
far edge on the lattice *vertices*, at the mean over the three cells meeting there. That looks like
a simplification — the ring then tiles with six quads and the wedges vanish — but it is wrong twice
over. A bridge between two equal neighbours was dragged up at whichever end a tall third cell
touched, raising a wall where the ground should have been flat; and the quad was left non-planar,
so its two triangles shaded differently and a diagonal crease ran across every wall, worst exactly
where the height difference was largest.

With the pairwise mean along edges and the triple mean only at the vertices, every piece is planar
by construction — a bridge spans two parallel level edges, a wedge is a piece of the plane through
three points — so the walls are flat-shaded without creases, and equal neighbours are joined by
level ground. Locked.

**Both means are over the locations *present*.** Substituting a stand-in height for an absent
neighbour would have each side of a boundary substitute a different one, and the seam would split
along the whole rim. Averaging over what is there also gives the grid's edge its level lip and makes
a lone location a flat plate, with no special case. Locked.

**Ownership is per location, and the split is the midline.** Every cell emits the same eighteen
triangles covering exactly its own hexagon. The alternative — one stitched mesh for the whole grid,
or a canonical rule handing each wall to one of its two cells — is fewer triangles and one draw
call, but a cell's mesh then stops being its own territory: hiding one leaves a ragged hole, and
adding one rewrites a mesh shared with everything else. Per-location ownership keeps visibility,
material and growth to a single cell, and it removes the canonical-ownership rule that the stitched
version needs and that is easy to get subtly wrong on a grid that is not convex.

It does **not** remove the coupling: a wall's outer edge depends on the neighbours' heights, so
adding a location still rebuilds it and its up-to-six neighbours. That is inherent — anything that
joins two caps continuously must know both heights — and only overlapping independent plates avoid
it, at the cost of the inset and the inclined walls. Locked.

**Cap and wall are separate meshes.** The cap is identical for every location, so one asset serves
them all, moved by a `Transform`; only the wall depends on the neighbours. It also lets the two
carry different materials, and later a colour blend across the wall between two biomes, which a
per-cell combined mesh could not express as cheaply.

**Selection stays arithmetic**, as [[hex-grid]] locks in — it intersects a plane at each location's
own cap height and keeps a hit only where it lands inside that location's **full** hexagon. Testing
the full hexagon rather than the inset cap is deliberate: a location's territory is its hexagon, so
a click on the wall belongs to the cell it is nearer. The imprecision is that over the wall the
tested plane sits at the cap's height rather than on the incline — a few pixels, and only at
grazing angles.

**Rejected: `Extrusion<RegularPolygon>`.** Bevy 0.19 has it, and it is `Meshable`. But it is
centred on the origin, extrudes along `+Z`, its cap is always pointy-top in the XY plane, and it
is closed on both ends. Even for the prisms it would have meant a rotation, an offset, no flat-top
orientation, and a second source of hex geometry competing with `corner_offsets`. Recorded in
[[bevy-0-19-api]] so it is not re-proposed.

**Rejected: a skirt and an underside.** The surface is a bare shell: the grid's edge is a level lip
with nothing beneath it. With back-face culling that means it is see-through from below, which the
orbit camera can reach, and a raised rim cell shows sky underneath it at a low angle. Accepted
deliberately, in exchange for the geometry staying purely a function of the heights. A skirt down
to a base elevation and a flat bottom would close it, at the cost of a base depth that is a fudge
and a boundary case in every builder.

**Shadows: one cascade, not four.** The default cascade configuration reaches 150 world units,
which spends nearly all of the shadow map on empty space around a grid a few units across. One
cascade over 60 units is both sharper here and what WebGL2 is limited to, so the web build gets
the same picture rather than a quietly different one.

## Implementation details

```
src/hex/mod.rs           Terrain { height }, and `undulating` — the placeholder generator
src/view/layout.rs       height_scale · elevation · surface_centre · mesh_scale · corner_directions
src/view/grid_render.rs  INSET, the cap and wall meshes, the fence rule, per-cell entities, outlines
src/view/selection.rs    pick_surface — ray against every cap plane, nearest first
src/view/labels.rs       labels anchored on the caps
src/main.rs              HEIGHT_SCALE, the grid's generator, shadows and ambient fill
```

Details that are load-bearing:

- **`surface_centre` is the single answer to "where is this location's terrain".** Labels, outlines
  and picking all call it, so they cannot disagree.
- **A cell is a parent entity with a cap child and a wall child.** The parent carries the transform
  and the visibility both inherit, which is what makes hiding or restyling one location a
  single-component change.
- **Both meshes are built in the unit frame with dimensionless heights**, so the cell's transform —
  `(size.x, height_scale, size.y)` — supplies the hex size and the height scale together, and
  neither has to be rebuilt when either changes. Only an orientation change moves the corner angles,
  which a transform cannot express.
- **`corner_directions` is where this is most likely to break.** The corner index that a neighbour
  direction maps to differs by exactly one between pointy-top and flat-top, so a mistake shows only
  in one orientation. `corner_directions_are_geometric` re-derives it from the corner positions
  rather than trusting the arithmetic. The derivation is in [[hex-coordinates]].
- **Every face is wound so its geometric normal is the normal it stores**, which is what back-face
  culling reads. `corner_offsets` runs clockwise seen from the `+normal` side on either plane, so
  an upward-facing fan iterates it backwards. A test asserts this over both orientations and both
  planes; the flat-sheet mesh this feature replaced was wound the other way and got away with it
  only because the material disabled culling.
- **The corner mean is summed in coordinate order.** Floating-point addition is not associative, so
  the three cells meeting at a lattice point would otherwise each get an answer an ulp apart and
  crack the surface at exactly the vertices hardest to inspect. A bridge needs no such care, since
  addition of its two heights is commutative.
- **A bridge's far edge stops short of the lattice vertex**, at the midpoint of this cap's corner
  and the neighbour's — which lies on the shared edge, inset from the vertex by the same fraction.
  The wedge fills what is left, out to the vertex itself. Together they cover the cell's hexagon
  exactly.
- **The outline gizmos' depth bias had to shrink by fifty times**, to `-0.002`. It is still needed,
  because an outline is exactly coplanar with the cap it traces, but depth bias shifts normalized
  depth — which is steeply non-linear — so the old `-0.1` pulled lines far enough forward to draw
  straight through the cells in front of them. See [[bevy-0-19-api]].
- **Picking tests cap planes only, and walls do not occlude.** At a grazing angle a low cap hidden
  behind a taller neighbour can still be picked; marked in the source as a known ceiling.
- Heights are static, so `sync_cells` only runs when the layout changes, and then rebuilds
  everything rather than working out which field moved.

## Verification plan

`cargo test` — 48 tests, all pure. The ones that carry weight here:

- Model: the generator stays within `-1..=1` across the grid and produces **both** signs, so the
  scene really exercises dips as well as rises.
- Projection: a surface position round-trips to its own coordinate at three scales × three height
  scales × three heights including a negative one — the height-aware sibling of the two-scale round
  trip, and what fails if the height scale leaks into the model. `elevation` is linear and
  sign-preserving, independent of hex size, and follows the plane's normal. `corner_directions` is
  re-derived from the corner positions in both orientations.
- Mesh: every triangle's geometric normal matches its stored normal, over both planes × both
  orientations × every location. Caps are exactly level and nothing faces downwards, since a terrain
  does not overhang. Each cell's geometry reaches exactly the circumradius and no further, so a
  location's mesh really is its own hexagon. A location with no neighbours comes out as a flat plate
  of 24 wall triangles.
- The seam: wherever two locations meet, their shared bridge and corner heights agree **bitwise**.
  This is the test the whole scheme rests on — it exercises the corner mapping, both mean rules and
  the summation order at once, and it is why the corner mean is summed in coordinate order.
- A bridge between two equal-height neighbours is level, and stays level with a cell twice their
  height on one of the corners they share. This is the regression test for the artefact above: it
  fails against the lattice-vertex rule and passes against the pairwise one.
- Selection: looking straight down picks the hex below at two height scales, at the centre and 80%
  of the way to a corner; a shallow ray crossing a low cell's airspace and a tall one's east wall
  picks the tall one; a ray passing just over it and away picks nothing.

Visually, via `HEX_TERRAIN_SCREENSHOT`, all confirmed: the default oblique view (a continuous
undulating surface, each cap outlined and labelled, inclined walls between them, no geometry at the
grid plane); directly overhead (every interior outline survives the reduced depth bias, and the
surface has no gaps in plan); a low angle (the walls read as walls, and the bare shell's lip is
plainly a knife edge); and flat-top, which is the orientation a wrong `corner_directions` would tear
apart. Sampling the rendered image put the darkest terrain tone at `(30,35,47)` against a lit
`(91,100,123)` — what "shadowed but not black" was tuned to.

**Not verified:** actual mouse interaction. As [[hex-grid]] already records, no pointer injection
is available on this machine, so picking is covered by its unit tests over synthetic rays and not by
a real click. Clicking a cap, a wall, and empty sky are the three cases worth confirming by hand.

## Implementation status

**status:** implemented — spec and code agree. No known divergences.

Deliberate omissions:

- The heights are a placeholder sinusoid. Real generation is the next thing this spec grows.
- **The surface has no underside.** See the rejected skirt above: from below it is see-through, and
  a raised rim cell shows sky beneath it at a low angle.
- One inset for the whole grid, not per location; and no per-height colouring or height in the debug
  readout — the readout's world position is still the hex centre on the plane, not the cap.
- `view/framing.rs` still computes the camera's extent from plane-level corners, ignoring height.
  Harmless for the top-down reset view; an oblique computed framing would need the extra extent.
- Walls do not occlude picking (above).
- Per-location meshes rule out chunking, which is the answer at ~10k locations where the entity and
  asset counts stop being free. A deliberate fork in the road, not an oversight.

## Related

- [[hex-grid]]: the grid, the projection and the view this extends
- [[hex-coordinates]]: the corner↔direction relationship the walls are built on
- [[bevy-0-19-api]]: the shadow, gizmo and primitive facts this depends on
- [[scene]]: the shell it is displayed in, and the lighting it adds to
