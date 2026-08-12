---
tags: [terrain, elevation, spec]
type: spec
status: implemented
updated: 2026-08-12
---
# Spec: Terrain height

Elevation as per-location data, and the prisms that make it visible: raised hexes stand as columns
above the grid plane, sunken ones open into it as pits. The first real payload on
[[hex-grid]]'s `Location`, which until now carried an empty placeholder.

## Requirements

### Goal (definition of done)

Every location carries a height. The grid renders as one hexagonal prism per location, each with a
top and a bottom: a positive height stands on the grid plane as a closed column, a negative one
sinks into it as a pit — floored, walled, and **open at the rim**, so nothing caps the hole. A
single scale converts a height into world units. In the static scene the heights come from a
sinusoid along each axis, crossing zero so both forms appear. Labels sit on the terrain surface,
clicking selects only through that surface, and the terrain casts shadows against enough ambient
fill that shadowed faces stay legible.

### Constraints

- **The model is dimensionless**, inherited from [[hex-grid]] and unchanged by this spec. A height
  is a signed level, not a distance; what it is worth in world units belongs to `HexLayout`.
- **One height scale, in one place** — the projection, beside the in-plane hex size. Neither the
  model nor the renderer may hold a second one.
- **The grid plane must not intersect a pit.** A pit is open at elevation zero.
- **Walls are not selectable.** Only the surface face — a column's top, a pit's floor — is.
- **Geometry stays derived from `HexLayout::corner_offsets`**, so faces and outlines cannot drift
  apart, and both orientations keep working.
- Inherited from [[scene]]: `bevy` is the only dependency, and everything must build for wasm,
  where WebGL2 is the baseline.

### Functional requirements

In scope: the height attribute and its placeholder generator, the world-unit height scale, the
column and pit meshes, surface-anchored labels and outlines, surface-only picking, and shadows
with ambient fill.

Out of scope: real terrain generation (the sinusoid is a placeholder), height editing at runtime,
biome or any other payload, per-height colouring, and water.

## Design discussion

**Height is signed and dimensionless, and lives on `Terrain`.** `Grid<T>` was already generic and
`Grid::hexagon` already took a `fn(Axial) -> T`, so the generator drops in with no signature
change anywhere. Zero is the grid plane; the sign chooses the form. Locked.

**The height scale is a second knob on `HexLayout`, not a variant of the first.** Vertical
exaggeration is the normal thing to want from terrain, so elevation and in-plane size are
deliberately independent. `mesh_scale` and `elevation` are the only places either is applied.
Locked.

**A pit is a different mesh, not a mirrored column.** The obvious economy — one prism, negative
scale on the elevation axis — fails twice over: a negative scale mirrors the mesh so every face
turns the wrong way out, and it would keep the cap at elevation zero that a pit must not have.
Two shared meshes instead, both anchored on the plane, so one transform rule still serves both and
the sign only selects which handle a cell spawns with. Locked.

**A pit is open at the rim.** A closed prism hanging below the plane would look like a lid over
the hole, so the pit mesh omits that face and winds its walls inward, to be seen from inside. The
consequence is that a pit is not a watertight solid — acceptable, because the grid plane is
notional and a pit is always bounded by its neighbours' walls, which start at elevation zero.
Locked.

**Zero height is guarded in the projection, not the model.** A height of exactly zero is ordinary
data — any field that crosses the plane produces one, and this generator puts a whole diagonal
there — but scaling a mesh to zero along an axis makes its normal transform degenerate and the
cell renders black. `HexLayout::MIN_ELEVATION` floors the elevation component so such a hex draws
as a thin tile. The alternative, clamping heights in the generator, would put a rendering
artefact's fix in the model and leave the next generator to rediscover it. Locked.

**Selection stays arithmetic**, as [[hex-grid]] locks in — it just intersects each location's own
surface plane instead of one shared plane, and keeps a hit only where it lands inside that
location's own footprint. Rejecting the rest is what makes walls transparent to picking, which is
the requirement, and pits selectable through their opening, which comes free. Mesh picking would
have needed a backend plugin and a normal test to reject wall hits, for a worse answer.

**Rejected: `Extrusion<RegularPolygon>`.** Bevy 0.19 has it, and it is `Meshable`. But it is
centred on the origin, extrudes along `+Z`, its cap is always pointy-top in the XY plane, and it
is closed on both ends. Using it would mean a rotation, an offset, no flat-top orientation, no
open pit, and a second source of hex geometry competing with `corner_offsets`. Recorded in
[[bevy-0-19-api]] so it is not re-proposed.

**Shadows: one cascade, not four.** The default cascade configuration reaches 150 world units,
which spends nearly all of the shadow map on empty space around a grid a few units across. One
cascade over 60 units is both sharper here and what WebGL2 is limited to, so the web build gets
the same picture rather than a quietly different one.

## Implementation details

```
src/hex/mod.rs           Terrain { height }, and `undulating` — the placeholder generator
src/view/layout.rs       height_scale · elevation · surface_centre · mesh_scale · MIN_ELEVATION
src/view/grid_render.rs  Form, the column and pit meshes, per-cell transform and outlines
src/view/selection.rs    pick_surface — ray against every surface face, nearest first
src/view/labels.rs       labels anchored on the surface
src/main.rs              HEIGHT_SCALE, the grid's generator, shadows and ambient fill
```

Details that are load-bearing:

- **`surface_centre` is the single answer to "where is this hex's terrain".** Labels, outlines and
  picking all call it, so they cannot disagree about a column's top or a pit's floor.
- **Both meshes are anchored at elevation zero** — a column occupies `0..=1`, a pit `-1..=0` — so
  one transform serves both, with the height's *magnitude* on the elevation axis.
- **Every face is wound so its geometric normal is the normal it stores**, which is what back-face
  culling reads. `corner_offsets` runs clockwise seen from the `+normal` side on either plane, so
  an upward-facing fan iterates it backwards. A test asserts this over both orientations, both
  planes and both forms; the previous flat-sheet mesh was wound the other way and got away with it
  only because the material disabled culling.
- **The material now culls back faces.** Each prism is a solid, and the shadow pass wants it.
- **The outline gizmos' depth bias had to shrink by fifty times**, to `-0.002`. It is still needed,
  because an outline is exactly coplanar with the surface it traces, but depth bias shifts
  normalized depth — which is steeply non-linear — so the old `-0.1` pulled lines far enough
  forward to draw straight through the prisms in front of them. See [[bevy-0-19-api]].
- **Picking tests surface faces only, and walls do not occlude.** At a grazing angle a pit floor
  hidden behind a taller neighbour can still be picked; marked in the source as a known ceiling.
- Heights are static, so a cell never changes form, and `sync_cells` only recomputes transforms and
  rewrites the two shared meshes when the layout changes.

## Verification plan

`cargo test` — 46 tests, all pure. The ones that carry weight here:

- Model: the generator stays within `-1..=1` across the grid and produces **both** signs, so the
  scene really exercises pits.
- Projection: a surface position round-trips to its own coordinate at three scales × three height
  scales × three heights including a negative one — the height-aware sibling of the two-scale round
  trip, and what fails if the height scale leaks into the model. `elevation` is linear and
  sign-preserving, independent of hex size, and follows the plane's normal. `mesh_scale` takes the
  magnitude only, and floors a zero height at `MIN_ELEVATION`.
- Mesh: every triangle's geometric normal matches its stored normal, over both orientations, both
  planes and both forms. A column has 24 triangles with 6 of them capping it at the rim; a pit has
  18 and **none** at the rim. Both surfaces face up. Column walls point away from the axis, pit
  walls toward it.
- Selection: looking straight down picks the hex below at two height scales, at the centre and 80%
  of the way to a corner; a shallow ray crossing a pit's airspace and a column's east wall picks
  the column; a ray passing just over the column and away picks nothing.

Visually, via `HEX_TERRAIN_SCREENSHOT`: 37 prisms undulating about the plane, each outlined at its
own surface, labels on the surfaces, and directional shadows cast across neighbouring cells.
Confirmed from the default oblique view and from directly overhead — the angle that exposes
coplanar-outline bugs, where every interior edge survived the reduced depth bias. Sampling the
rendered image put the darkest terrain tone at `(30,35,47)` against a lit `(91,100,123)`, which is
what "shadowed but not black" was tuned to: at the previous ambient brightness it was `(20,23,33)`.

**Not verified:** actual mouse interaction. As [[hex-grid]] already records, no pointer injection
is available on this machine, so surface picking is covered by its unit tests over synthetic rays
and not by a real click. Clicking a column top, a column's wall, into a pit, and empty sky are the
four cases worth confirming by hand.

## Implementation status

**status:** implemented — spec and code agree. No known divergences.

Deliberate omissions:

- The heights are a placeholder sinusoid. Real generation is the next thing this spec grows.
- `view/framing.rs` still computes the camera's extent from base-plane corners, ignoring height.
  Harmless for the top-down reset view; an oblique computed framing would need the extra extent.
- No rim outline around a pit's opening, no height in the debug readout, and no per-height
  colouring — the readout's world position is still the base centre, not the surface.
- Walls do not occlude picking (above).

## Related

- [[hex-grid]]: the grid, the projection and the view this extends
- [[bevy-0-19-api]]: the shadow, gizmo and primitive facts this depends on
- [[scene]]: the shell it is displayed in, and the lighting it adds to
