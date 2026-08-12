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
a world position, displayed as 37 pointy-top hexagons (a hexagon of side 4) meeting edge to edge
with contrasting outlines. Left-click selects a hex and marks it with a bold outline; a top-right
panel reports the selected hex in all three coordinate systems plus world xyz; a button cycles the
per-hex labels between systems; and a compass shows where the six half-axes point. The **centre hex
is the origin of every system**, world position included.

### Constraints

- **The model is dimensionless.** The grid and its locations know nothing of world units, scale,
  orientation or plane. Only the projection layer does. This is the constraint most likely to be
  violated by a careless addition, and the one the two-scale round-trip test exists to catch.
- **One scaling factor, in one place.** Hex size is a property of the projection, not the model and
  not the renderer.
- **Axial is the storage format**; cube is for algorithms. Both are always available.
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
rotations, offset coordinates, and flat-top rendering — the layout supports flat-top, but nothing
uses it.

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

**Doubled coordinates: doublewidth only.** Pointy-top pairs with *doublewidth* (`col = 2q + r`,
`row = r`); doubleheight is the flat-top counterpart and would be dead code. Locked.

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
highlighted costs nothing and needs no material swapping. Line width is per config group, hence
three groups: thin grid lines, the thick highlight, and the compass.

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

The three are 120° apart and, for a pointy-top layout, each points at a **vertex** of the hexagon.
That is what makes the compass self-checking: its arrows must land on the corners of the reference
hexagon drawn beneath them, and a test asserts it.

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
src/view/debug_ui.rs     readout panel and the mode button
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

## Verification plan

`cargo test` — 28 tests, all pure. The ones that carry weight:

- Model: cube invariant holds for every constructed coordinate; axial↔cube↔doubled round-trips;
  `col + row` always even; neighbours distinct and all at distance 1; rounding a hex centre returns
  that hex, and rounding never breaks the invariant.
- Grid: `hexagon(3)` gives **37** locations in rows of **4,5,6,7,6,5,4**; every location within
  radius; interior hexes have 6 neighbours and corners 3.
- Projection: world round-trip for all 37 hexes **at four scales** and on **both planes** — the
  test that fails if scale leaks into the model; round-trip from points 80% of the way to every
  corner; neighbouring hexes share **exactly two** corners (faces touch, without gaps or overlap);
  a vertex points due north for pointy-top and due east for flat-top; the axis arrows are unit
  length, 120° apart, mutually opposed, and land on hexagon vertices.
- Wiring: the app's grid is the one this spec describes, and the centre hex is the origin of all
  three coordinate systems and of world space.

Visually, via `HEX_TERRAIN_SCREENSHOT`: 37 hexes meeting edge to edge with cyan outlines, axial
labels with `0,0` at the centre, the compass showing `-r` north / `+q` ENE / `+s` WNW with its
arrows on the reference hexagon's vertices. With a hex preset as selected, the bold amber outline
and the readout panel were both confirmed, and the readout's numbers hand-checked: axial (1,−1) →
cube (1,−1,0), doubled (col 1, row −1), world (0.87, 0.00, −1.50).

**Not verified:** actual mouse interaction — the click-to-select ray, the drag, and pressing the
button. No pointer or key injection tool is available on this machine, so the selection path was
exercised by presetting the resource rather than by clicking, and the raycast itself is covered only
by the round-trip tests over the same projection code it uses. Someone should click around and
confirm the feel, particularly that clicking the button does not change the selection.

## Implementation status

**status:** implemented — spec and code agree. No known divergences.

Deliberate omissions:

- `Terrain` is an empty struct. Elevation and biome land there; the grid is already generic over it.
- No pathfinding, ranges, rotations or line drawing. The reference covers them; add when needed.
- `Orientation::Flat` exists and is tested but nothing constructs it in the app.
- `GridPlane::Xy` likewise: it exists so the model's plane-agnosticism is real and tested, not
  aspirational.

## Related

- [[hex-coordinates]]: the formulas as implemented, and the derivations behind them
- [[scene]]: the shell this is displayed in
- [[bevy-0-19-api]]: the Bevy APIs the view depends on
