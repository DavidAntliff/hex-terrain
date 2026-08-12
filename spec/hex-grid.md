---
tags: [hex, grid, coordinates, spec]
type: spec
status: implemented
updated: 2026-08-12
---
# Spec: Hexagonal grid

The coordinate systems and grid model everything else in the project is built on, plus the view
that makes the coordinate conventions visible: labels, an axis compass, click selection and a debug
readout. Follows <https://www.redblobgames.com/grids/hexagons/>.

## Requirements

### Goal (definition of done)

A grid of hex locations, each addressable in axial, cube and doubled coordinates and convertible to
a world position, displayed as 37 hexagons (a hexagon of side 4) meeting edge to edge with contrasting
outlines, in either orientation. Left-click selects a hex and marks it with a bold outline; a top-right
panel reports the selected hex in all three coordinate systems plus world xyz. Controls cycle the
per-hex labels through the systems and off, toggle pointy/flat, show or hide the compass, and frame
the whole scene from directly overhead. A compass shows where the six half-axes point. The **centre
hex is the origin of every system**, world position included.

### Constraints

- **The model is dimensionless.** The grid and its locations know nothing of world units, scale,
  orientation or plane. Only the projection layer does. This is the constraint most likely to be
  violated by a careless addition, and the one the two-scale round-trip test exists to catch.
- **One scaling factor, in one place.** Hex size is a property of the projection, not the model and
  not the renderer.
- **Axial is the storage format**; cube is for algorithms. Both are always available.
- **Orientation is a parameter, and doubled coordinates must follow it.** Pointy-top and flat-top are
  both selectable at runtime, and the doubled variant switches with them — a grid cannot be showing
  flat-top hexagons while reporting doublewidth coordinates.
- **Locations carry arbitrary data**, typed generically so the payload can grow without changing
  the grid.
- The grid is **not necessarily rectangular** — the shape is a parameter, and the storage must not
  assume otherwise.
- Inherited from [[scene]]: `bevy` is the only dependency, and everything must build for wasm.

### Functional requirements

In scope: the three coordinate systems and their conversions, neighbours, distance, fractional
rounding, the grid container and its hexagon-shaped constructor, the hex↔world projection, and the
view (faces, outlines, selection, labels, compass, readout).

Out of scope: terrain data itself (the payload is an empty placeholder), pathfinding, ranges and
rotations, and offset coordinates.

## Design discussion

**Model and view are separate layers, and the projection is its own thing.** `hex/` holds the model
and knows nothing about rendering; `view/layout.rs` holds `HexLayout`, the only code where world
units exist; the rest of `view/` draws. An earlier draft had `Grid` own the layout — the leak is
easy to introduce and hard to notice, because everything still works until you want a second
projection of the same grid. The reference supports the split: its `Layout` is deliberately
separate from `Hex`, since distance, neighbours and ranges never need it. Locked.

The model is not merely Bevy-free by accident: `Grid` deliberately does **not** derive `Resource`.
The `GridModel` newtype in `view/mod.rs` is the bridge, so the compiler enforces the boundary rather
than a convention doing it.

**Hash storage, not an array.** The reference's Recommendations table gives array storage only for
rhombus-shaped axial maps; for "any other shaped maps" — ours is a hexagon — it recommends
axial/cube with hash storage. So `HashMap<Axial, Location<T>>`. Locked.

**Orientation is a model concept, because doubled coordinates depend on it.** Pointy versus flat is
otherwise pure rendering — the topology is identical either way — so `Orientation` began in the
projection layer. That was wrong: doubled coordinates pair with an orientation (*doublewidth* with
pointy-top, *doubleheight* with flat-top), and with the parameter out of reach the model silently
assumed pointy. `Orientation` therefore lives in `hex/orientation.rs`, which is legitimate because it
is **dimensionless** — the model's constraint is world units, not this. Its projection matrices stay
with the projection. Every doubled conversion takes an orientation, so the two cannot disagree.
Locked.

The same treatment would apply to offset coordinates (evenr/oddr versus evenq/oddq) if they were ever
added, which is a point in the design's favour.

**Offset coordinates omitted.** The same Recommendations table shows offset's only advantages are
rectangular array storage and matching a rectangular map orientation, neither of which applies, and
its costs are no vector arithmetic and position-dependent neighbour tables. Rejected, deliberately.

**The cube invariant is enforced by construction.** `Cube::new(q, r)` derives `s = -q-r`; there is
no constructor taking three components. `q + r + s == 0` therefore cannot be violated, rather than
being asserted after the fact.

**Selection is arithmetic, not mesh picking.** A ray against the grid plane followed by
`world_to_hex` and cube rounding reuses code the grid needs anyway, and keeps selection independent
of how hexes are rendered. It does mean guarding against clicks that belong to the UI — see
Implementation details. Locked.

**Faces are meshes, outlines are gizmos.** Gizmos are immediate-mode, so changing which hex is
highlighted costs nothing and needs no material swapping. Line width and depth bias are per config
group, hence three groups: thin grid lines, the thick highlight, and the compass.

**Camera framing is computed, not tuned.** The reset-view button derives its distance from the
camera's actual vertical field of view and aspect ratio. A constant cannot be right for all window
shapes: the horizontal extent of the view scales with the aspect ratio, so a hand-picked distance that
frames the scene in a landscape window clips it in a portrait one. This replaced two hand-tuned
constants that had already been adjusted three times by observation.

**Controls are cycling buttons plus a checkbox, not radio lists.** `bevy_ui_widgets` has radio groups,
but a cycle is one entity and one observer arm, and this is a debug panel. "No labels" is a fourth
`LabelMode` rather than a separate flag, so a single piece of state governs the labels and there is
nothing to disagree with.

## Coordinate conventions

Derived by inverting the layout rather than read off the reference's interactive diagrams, and
asserted in tests.

The reference's 2D `+y` (down-screen, "south") maps to **`+z`**, so `hex → world = (x₂, 0, y₂)` on
Bevy's ground plane. Viewed from above the grid then looks exactly like the website's pictures: `+q`
due east, `+r` down and to the right.

The direction in which each cube coordinate increases is the gradient of that coordinate — for `q`,
`(b0/size.x, b1/size.y)`:

| axis | world direction | compass |
|---|---|---|
| +q | (+0.866, 0, −0.5) | ENE |
| +r | (0, 0, +1) | due south |
| +s | (−0.866, 0, −0.5) | WNW |

The three are 120° apart and each points at a **vertex** of the hexagon — in either orientation, which
is why the compass needs no special-casing when the orientation is toggled. That is what makes it
self-checking: its arrows must land on the corners of the reference hexagon drawn beneath them, and a
test asserts it for both orientations.

Flat-top rotates all of this by 30°: `+q` becomes due east, and the doubled variant switches to
doubleheight.

Note the distinction that makes this confusing: stepping one hex in `+r` moves *south-east* (it
changes `s` too), whereas the `r` **axis** points due south. The compass shows axes; the labels show
coordinates.

## Implementation details

```
src/lib.rs           library root — the model/view split
src/hex/coords.rs    Axial · Cube · Doubled · FractionalCube, conversions, neighbours, distance
src/hex/grid.rs      Grid<T> · Location<T>, hexagon constructor, lookups
src/hex/mod.rs       Terrain payload placeholder, TerrainGrid alias
src/view/layout.rs   HexLayout — scale, origin, orientation, plane; hex↔world, corners, axes
src/view/grid_render.rs  unit hex mesh, per-hex entities, outlines, highlight
src/view/selection.rs    cursor ray → plane → hex
src/view/labels.rs       LabelMode and the per-hex labels
src/view/compass.rs      the axis compass
src/view/debug_ui.rs     readout panel and the four controls
src/view/framing.rs      camera framing maths for the reset-view button
src/hex/orientation.rs   Orientation — pointy/flat, and which doubled variant follows
src/view/world_label.rs  UI text pinned to a world position
```

Details that are load-bearing:

- **`HexLayout::corner_offsets` is the single source of hex geometry.** The face mesh, the outlines
  and the compass all derive from it, so they cannot drift apart.
- **The mesh is built at circumradius 1** and scaled by `Transform`, so changing scale never
  rebuilds it. Spacing comes from `hex_to_world` under the same layout, which is why faces keep
  touching edge to edge at any scale.
- **UI clicks are excluded from world picking** by skipping selection while any UI node is
  `Hovered`. Without it, pressing the label button would also select the hex behind it.
- **World-anchored labels are UI nodes** repositioned each frame from `Camera::world_to_viewport`,
  hidden with `Display::None` when the projection fails. One mechanism serves both the hex labels
  and the compass.
- The material is `double_sided` with no culling: the grid is a single flat sheet, and showing it
  from underneath beats having it vanish.
- **Both outline gizmo groups carry a negative `depth_bias`**, because the lines are exactly coplanar
  with the faces they trace. Omitting it is a bug that hides: at an oblique angle enough of each line
  wins the depth test to look right, but from directly overhead every interior edge has a face on both
  sides at identical depth and vanishes, leaving only the grid's silhouette. The reset-view button is
  what exposed it.
- **Changing orientation moves everything**, so `sync_cells` rewrites the shared mesh asset in place
  (corner angles change, which a `Transform` cannot express) as well as recomputing every transform;
  the labels re-anchor and re-format, and the compass labels re-anchor and change identity. A scale
  change still needs no mesh rebuild.
- **`place` builds the camera rotation directly** instead of using `looking_at`, which has no valid up
  vector when looking straight down. That is what makes a pitch of exactly ±π/2 available, and it is
  no more code.

## Verification plan

`cargo test` — 36 tests, all pure. The ones that carry weight:

- Model: cube invariant holds for every constructed coordinate; axial↔cube↔doubled round-trips **in
  both orientations**; `col + row` always even; each orientation doubles its own axis, checked on a
  coordinate where the two variants genuinely differ; stepping along a row moves the doubled axis by
  two; neighbours distinct and all at distance 1; rounding a hex centre returns that hex, and rounding
  never breaks the invariant.
- Grid: `hexagon(3)` gives **37** locations in rows of **4,5,6,7,6,5,4**; every location within
  radius; interior hexes have 6 neighbours and corners 3.
- Projection: world round-trip for all 37 hexes **at four scales** and on **both planes** — the
  test that fails if scale leaks into the model; round-trip from points 80% of the way to every
  corner; neighbouring hexes share **exactly two** corners (faces touch, without gaps or overlap);
  a vertex points due north for pointy-top and due east for flat-top; the axis arrows are unit
  length, 120° apart, mutually opposed, and land on hexagon vertices.
- Projection, orientation: the flat-top layout puts a corner due east where pointy-top puts one due
  north, and still round-trips.
- Framing: the computed extent contains every hex corner in both orientations; it scales with the
  layout; enabling the compass only ever grows it, southward; and at the computed distance every corner
  projects inside the frustum at aspect ratios from 0.5 to 3.0 — with no more slack than the intended
  margin, which is what "just visible" means. A narrower window demands more distance.
- Wiring: the app's grid is the one this spec describes, and the centre hex is the origin of all
  three coordinate systems and of world space, in either orientation.

Visually, via `HEX_TERRAIN_SCREENSHOT`: 37 hexes meeting edge to edge with cyan outlines, axial
labels with `0,0` at the centre, the compass showing `-r` north / `+q` ENE / `+s` WNW with its
arrows on the reference hexagon's vertices. With a hex preset as selected, the bold amber outline
and the readout panel were both confirmed, and the readout's numbers hand-checked: axial (1,−1) →
cube (1,−1,0), doubled (col 1, row −1), world (0.87, 0.00, −1.50).

Flat-top was confirmed the same way, and is the check that the orientation parameter really reaches
the coordinates: the readout names *doubleheight*, and the centre column of labels reads
`0,-6 / 0,-4 / 0,-2 / 0,0 / 0,2 / 0,4 / 0,6` — the row doubling, with the column held. The compass
rotated with it, `+q` landing due east. World (1.50, 0.00, −0.87) for axial (1,−1) matches the
flat-top matrix by hand. Labels-off and compass-off were confirmed, as was the top-down reset framing
in both a wide and a tall window.

**Not verified:** actual mouse interaction — the click-to-select ray, the camera drag, and pressing any
of the four controls. No pointer or key injection tool is available on this machine, so every state was
reached by presetting resources rather than by clicking, and the raycast itself is covered only by the
round-trip tests over the same projection code it uses. Someone should click around and confirm the
feel, particularly that clicking a control does **not** also change the selection — that guard has
never been exercised for real.

## Implementation status

**status:** implemented — spec and code agree. No known divergences.

Deliberate omissions:

- `Terrain` is an empty struct. Elevation and biome land there; the grid is already generic over it.
- No pathfinding, ranges, rotations or line drawing. The reference covers them; add when needed.
- `GridPlane::Xy` exists and is tested but nothing constructs it in the app: it is there so the
  model's plane-agnosticism is real rather than aspirational.
- The reset view frames symmetrically about the origin, because the camera always looks there. Content
  off to one side — the compass — therefore costs empty space on the opposite side. Tight framing needs
  an orbit `target`, which is the same missing field that rules out panning.
- The initial view is still a hand-picked oblique angle rather than the computed framing, because the
  projection's aspect ratio is not yet valid during `Startup`. The reset button is one click away.

## Related

- [[hex-coordinates]]: the formulas as implemented, and the derivations behind them
- [[scene]]: the shell this is displayed in
- [[bevy-0-19-api]]: the Bevy APIs the view depends on
