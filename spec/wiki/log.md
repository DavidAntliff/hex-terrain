---
tags: [meta, log]
type: log
---
# Log

Append-only, newest at the bottom. Keep the prefix consistent so it stays greppable:
`## [YYYY-MM-DD] <op> | <title>`.

## [2026-08-12] init | spec tree created

- Established `spec/` with [[home]] as the index and [[conventions]] as the operating manual;
  templates for `spec` and `concept` notes under `_templates/`.
- Wrote [[scene]] (`status: implemented`) covering the initial 3D scene, camera, skybox,
  and both build paths, at commit `d650ce8`.
- Compiled three knowledge pages from building it: [[bevy-0-19-api]], [[build-performance]],
  [[skybox-pipeline]].

## [2026-08-12] feature | hexagonal grid

- Wrote [[hex-grid]] (`status: implemented`): axial/cube/doubled coordinates, a generic `Grid<T>`,
  the hex↔world projection, and the view — faces, outlines, click selection, coordinate labels, an
  axis compass and a debug readout. Replaces the placeholder cube.
- Compiled [[hex-coordinates]] from the reference, including the axis directions derived from the
  layout matrix because the website shows them only in an interactive diagram.
- **Renamed** `sandbox-scene` → [[scene]] and narrowed it to the scene *shell*, since what the scene
  contains now belongs to [[hex-grid]]. Inbound links repointed in five pages.
- Structural: the crate became a library plus a binary, so the model/view boundary is a real API
  boundary. `Grid` deliberately does not derive `Resource`; a `GridModel` newtype in the view layer
  is the bridge.
- Added a scripted-screenshot path (`HEX_TERRAIN_SCREENSHOT`), because capturing a window on an
  inactive workspace through X yields a blank image.

## [2026-08-12] feature | orientation parameter, view controls, computed framing

- Made the hexagon orientation a runtime parameter and **moved `Orientation` into the model**
  ([[hex-grid]] → Design discussion). Doubled coordinates depend on it — doublewidth for pointy-top,
  doubleheight for flat-top — and with the parameter confined to the projection layer the model
  silently assumed pointy, which was a latent defect rather than a missing feature. Orientation is
  dimensionless, so the model is the right home; its projection matrices stayed behind.
- Debug panel now carries four controls: label mode (cycling through the three systems **and off**,
  so one piece of state governs the labels), orientation toggle, compass checkbox, and reset view.
- **Camera framing is now computed** from the projection's vertical field of view and aspect ratio
  (`view/framing.rs`) rather than hand-tuned. Two constants had already been adjusted three times by
  observation; a constant cannot be correct for all window shapes.
- Fixed a bug the top-down view exposed: outline gizmos are coplanar with the faces they trace and
  need a negative `depth_bias`. Without it, an oblique view looks correct while a vertical one loses
  every interior edge. Recorded in [[bevy-0-19-api]] with the other gizmo, camera and widget facts.
- `place` no longer uses `looking_at`, which has no valid up vector at the pole; the rotation is built
  from yaw and pitch, so exactly vertical is now reachable.

## [2026-08-12] feature | terrain height, prisms and pits

- Wrote [[terrain]] (`status: implemented`): a signed dimensionless height on `Location`, a
  `height_scale` on the projection, and one hexagonal prism per location — columns above the grid
  plane, **open pits** below it. A pit omits the face at elevation zero, because a lid there is the
  plane sealing the hole it is meant to be.
- The sign cannot be a `Transform`: a negative scale mirrors the prism and turns every face the
  wrong way out, and it keeps the cap a pit must not have. Two shared meshes, selected by sign.
- Turning on back-face culling exposed that the old flat-sheet mesh had been wound backwards all
  along, hidden by `cull_mode: None`. Now every face is wound to match its normal, asserted by a
  test over both orientations, both planes and both forms.
- Two rendering traps, both recorded in [[bevy-0-19-api]]: scaling a mesh to zero on an axis makes
  its normal transform degenerate and the cell renders black — which the generator hits wherever
  the wave crosses zero, hence `HexLayout::MIN_ELEVATION`; and the outlines' `depth_bias` had to
  drop from `-0.1` to `-0.002`, since normalized depth is non-linear enough that the old value drew
  lines straight through the prisms in front of them once the scene had any depth.
- Selection stays arithmetic per [[hex-grid]], now against each location's own surface plane rather
  than one shared plane. Walls are transparent to it, which is the requirement; pits become
  selectable through their opening for free.
- Shadows enabled on the directional light with one cascade over 60 units instead of the default
  four over 150, and ambient fill raised so the darkest terrain tone measures `(30,35,47)` against
  a lit `(91,100,123)` rather than `(20,23,33)`.
- **Edited [[hex-grid]] with agreement** in three places it had gone wrong: the no-cull "flat sheet"
  material, selection being a ray against *the* grid plane, and terrain data being out of scope.

## [2026-08-12] rework | terrain drawn as a stitched surface

- Replaced the prisms with a surface stitched **between** locations: a level cap per location, inset
  from its hexagon, and a ring of six quads joining it to its neighbours' caps. [[terrain]] rewritten
  from Design discussion onward, with the superseded decisions kept as rejected options.
- The reason: a sunken location's walls ran back up to the grid plane. That plane is notional, so
  geometry reaching it was an artefact of the meshing. Nothing in the new build refers to it, and the
  whole column/pit distinction went with it — the sign of a height is now just a vertex position.
- Consequences of that: `Form`, the two shared prism meshes and `HexLayout::MIN_ELEVATION` all
  deleted. `MIN_ELEVATION` existed because a zero *height* collapsed the elevation scale; with
  heights as positions there is no longer a scale to collapse. The hazard itself is unchanged and
  stays recorded in [[bevy-0-19-api]], re-aimed at the global scale.
- **The fence rule** is what closes the surface: each cell's wall reaches the corners of its full
  hexagon at the *mean of the heights present* at that point of the lattice. Averaging over present
  cells rather than substituting for absent ones is the part that matters — it makes the boundary a
  level lip with no special case, and it is why a lone location is a flat plate. Summed in coordinate
  order, because floating-point addition is not associative and the three cells meeting at a point
  would otherwise disagree by an ulp and crack the seam.
- Ownership is **per location**, so each cell's mesh is exactly its own hexagon and hiding or adding
  one is a clean hexagon's worth of surface. Cap and wall are separate meshes: the cap is identical
  everywhere and shared, the wall depends on the neighbours. Both hang off a parent entity that
  carries the transform and the visibility.
- The corner↔direction relationship this is built on differs between the orientations by exactly one
  index, so a mistake would show in flat-top only. Derived and recorded in [[hex-coordinates]], and
  tested by re-deriving it from corner positions rather than by asserting a table.
- The surface is a bare shell by choice: no skirt, no underside, so it is see-through from below.

## [2026-08-12] fix | walls of bridges and wedges

- The wall was six quads reaching to the lattice vertices, each carrying the mean of the three cells
  meeting there. Wrong twice: a bridge between two *equal* locations was dragged up at whichever end
  a tall third cell touched, and the quad was left non-planar so a diagonal crease ran across every
  wall. Both were plainly visible; neither was caught by a test, because the suite checked winding,
  seam agreement and footprint, all of which the broken construction satisfied.
- Now a **bridge** per edge at the pairwise mean, level along its length, and a **wedge** per corner,
  a third of the planar triangle joining the three caps. Both planar by construction. Thirty
  triangles per cell rather than eighteen — the eighteen was the mistake, not a saving.
- Regression test: two equal-height locations with a cell twice their height on a shared corner.

## [2026-08-12] feature | water

- `Terrain` gains `water: Option<f32>`, a surface level in the same units as a height, per location
  so a mountain lake can stand above the sea. The placeholder generator floods below [[terrain]]'s
  sea level.
- Drawn as an opaque full-width hexagon, with the **terrain occluding it** wherever the ground is
  higher — so the coastline falls out of the depth buffer and needs no clipping or shoreline
  geometry. The plate reaches **one ring** past the flooded locations, which is provably exactly
  right: a dry neighbour's half-bridge dips below the water line, and anything further out cannot.
- **`StandardMaterial::depth_bias` does not stop z-fighting** — it only feeds the phase sort key,
  and the mesh pipeline hardcodes a zero rasterizer bias, so it does nothing for opaque draws. The
  coplanar case is real here because a whole diagonal of the placeholder grid sits at exactly zero.
  Fixed geometrically with a hair of lift. Recorded in [[bevy-0-19-api]].
- Honest limitation: it reads as flat blue paint, since a smooth plane under one directional light
  with no environment map has nothing to reflect.

## [2026-08-12] feature | sea-level slider

- The debug panel gains a slider over `-1..=1`, the first control for a continuous value rather than
  a choice between named states. `bevy_ui_widgets::Slider` is headless like the rest: it reports
  `ValueChange<f32>` and maintains `SliderValue`, but never moves the thumb — that is the caller's,
  via `SliderRange::thumb_position`. Recorded in [[bevy-0-19-api]].
- Flooding moved out of the terrain generator into `hex::flood(grid, level)`, so the generator makes
  dry land and the level is applied separately and repeatedly. The slider writes a `SeaLevel`
  resource, that writes the model, and the water surfaces are rebuilt from the model — the level
  never reaches the renderer directly.
- Surfaces are despawned and respawned wholesale on a change, because moving the level changes which
  locations are wet at all, not merely how high the water sits.
- Verified by starting at a raised level rather than by dragging: at `+0.45` only the highest
  locations stay clear and the shoreline is still seamless.

## [2026-08-12] fix | a location presents one surface

- Outlines on submerged locations looked arbitrary: drawn on the sea bed, each was either swallowed
  by the water or showing through it depending on how deep that particular location was, since the
  gizmo depth bias beats a small depth difference and loses to a large one.
- `Terrain::surface` — the water where there is any, the ground where there is not — is now what
  picking, outlines and labels all address. A click on a lake lands on the water, the outline is
  drawn on it, and the label floats on it.
- Outlines float just clear of what they trace, by more than the water's own lift, which also covers
  a location standing exactly at the water line: flooding is strict so the model calls it dry, but a
  neighbour's surface still covers it.
