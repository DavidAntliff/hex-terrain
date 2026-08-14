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

## [2026-08-13] feature | a daylight sky, and water that reflects it

- The water read as painted blue, and [[terrain]] had already named the cause: a smooth plane with
  no environment map has nothing to reflect. So the fix started upstream of any shader — the
  starfield was replaced by a generated **daylight sky** (`src/sky.rs`), and an
  `EnvironmentMapLight` taken from the same model now lights the scene.
- The sky is **Preetham's analytic daylight model**, evaluated on the CPU into a 256² cubemap at
  startup. No asset, no download, no generator to run: unlike the starfield, an analytic sky has no
  source imagery. Below the horizon, ground **hazes** into the sky's own colour at the same azimuth
  over several degrees — aerial perspective, so the horizon has no line in it. Recorded in [[scene]].
- **Everything is in physical units now**: `lux::DIRECT_SUNLIGHT`, sky luminance in cd/m²,
  `Exposure::SUNLIGHT` on the camera, and `brightness`/`intensity` both 1.0. The ambient fill went
  to zero — the environment map does that job with the colours actually overhead.
- **A black sky is not a broken skybox.** An hour went on this: the sky rendered pure black while
  the terrain lit correctly. The cubemap data was right, the pipeline was right, and a uniform red
  test texture rendered fine — which proved nothing, because a uniform texture looks correct however
  it is sampled. The camera was looking *down*, so the whole frame was ground, and the ground
  colour had been put through the cd/m² scale as though it were a luminance: dark grey became
  5e-6. `Sky::ground` is now a **fraction of the horizon's brightness**, which cannot go wrong that
  way, and a test pins the ratio.
- `patch_cubemap` **deleted**. An `Image` built in code sets its own cube view descriptor, so there
  is no load state to poll and no re-upload guard — `src/main.rs` came out shorter than it went in.
- Water gained an `ExtendedMaterial` shader ([[water]]): six ripple trains at golden-angle
  directions travelling at their real dispersion speeds, so the pattern never settles into the plaid
  four even waves produced; ripples fading with distance into roughness, because sub-pixel ripples
  are crawling shimmer; and a shoaling colour.
- **Water depth comes from the model, not the depth buffer** — WebGL2 cannot sample one. A plate's
  seven vertices are the points `wall_mesh` already places terrain at, so `corner_height` gives the
  ground under each, and at a shoreline the corner mean *is* the terrain's wedge height: the depth
  there is exactly zero and the shallows meet the shoreline with nothing lined up by hand. It rides
  in `uv.x`, a channel an untextured standard material ignores.
- SSR, procedural `Atmosphere` and runtime-generated environment maps were all considered and all
  fail WebGL2 — two on compute, two on a Naga bug sampling depth textures. Recorded in
  [[bevy-0-19-api]] so nobody prices them again.
- Verified: 61 tests pass, screenshots at two pitches and two sea levels, and the wasm build
  rendering in Firefox — the check the whole design is shaped around. `WGPU_BACKEND=gl` would have
  been a cheap native proxy for it but no GL adapter was available, NVIDIA or Mesa.

## [2026-08-13] feature | named scenes, and a bridge between two water levels

- **Scenes are a named table**, `src/hex/scenes.rs`: `(name, fn() -> TerrainGrid)`, picked by
  `cargo run -- <name>`, defaulting to `sea` — the sandbox, unchanged. Model data, so a scene is a
  whole grid including its water and nothing about how any of it is drawn. `GRID_RADIUS` moved out of
  `view` on the way past: a grid's extent is dimensionless. Parsing is `std::env::args().nth(1)`
  against the table, called before `App::new()` so a mistyped name costs a message rather than a GPU
  init. See [[scene]] for why not a CLI crate and not an environment variable.
- **A test must never read `argv`.** Under `cargo test` the first argument is the test-name filter, so
  a test calling the scene picker exits the harness on `cargo test some_filter`. The main-binary test
  goes through `scenes::build(scenes::DEFAULT)` instead.
- **`apply_sea_level` was erasing scene-authored water before it was ever drawn.** Inserting a
  resource counts as changing it, so `Res<SeaLevel>::is_changed()` is true on the first `Update` and
  the startup flood overwrote every hand-authored level. Guarded with `|| sea.is_added()`. Anything
  that writes the model from a resource's change detection has the same hazard.
- **The `two-lakes` scene, and what it shows.** Two bodies at 0.55 and 0.0 either side of a land
  bridge one hex wide (the axial line `q = 0`, whose `q = -1` and `q = +1` neighbours are never
  adjacent to each other), with the bridge's ground at 0.95 — above both, so neither body can drain
  into the other and the data is sensible by [[terrain]]'s own rule.
  - The reassuring half: `water_levels` returns **both** levels for the bridge, and `sync_water`
    spawns a plate for each. The renderer does not pick one.
  - The artefact: a plate covers the location's *whole* hexagon, and a bridge cell's wall dips to the
    **mean** of the two heights either side — `mean(0.95, -0.6) = 0.175` at the edges, `0.433` at the
    corners, both under 0.55. So the higher plate is exposed all the way round each bridge cap,
    including the flank facing the *lower* body, where it stands 0.55 above it. The lower body's own
    cells never see the higher level, so it ends at the hexagon boundary in a wall of water — 0.825
    world units at `HEIGHT_SCALE` 1.5, against a hexagon circumradius of 1.
  - On screen the bridge reads as a chain of islands standing in the higher water, with the lower
    water visibly below it in the same frame.
  - This contradicts [[terrain]]'s claim that a step needs "ground between them below both", so "the
    data has to be sensible rather than the renderer policing it". Ground above both levels is not
    sufficient, because the *wall* between two caps is not ground above both. Reconciling that spec
    is pending.
- Verified: 64 tests, including one recording the artefact numerically so confirming it does not rest
  on pixels, and screenshots at four camera angles. The camera default was edited to reach a
  low-pitch view and restored — the shell has no way to aim it from outside.

## [2026-08-13] fix | a water level reaches only the part of a location its own body touches

The artefact above, fixed in the renderer alone — the model still holds one level per location and
nothing else. `water_levels` is replaced by `water_plates`, returning each level with the **pieces**
of the hexagon it covers, and two rules decide them:

- **Reach.** A location's hexagon divides into six sectors, one per edge, and each sector halves at
  the midpoint of that edge. A location's own water covers all twelve halves; a neighbour's covers the
  two along their shared edge and the one in each adjoining sector that reaches a shared corner. A
  body can no longer cross a location and come out over another body's shore.
- **Submergence.** A piece is dropped when every height under it stands above the level, since a
  buried plate draws nothing. Those heights are the cap, the bridge along the edge, and the piece's
  one corner; the terrain between them is planar, so nothing lower hides in between.

Why the **half**-sector and not the sector: a half touches exactly two neighbours — the one across its
edge and the one sharing its corner — so it can only be claimed by two bodies at once if those two
bodies are adjacent, and two bodies at different levels never are, or one would drain into the other.
A whole sector touches three, which leaves the ambiguity in place. Twelve pieces is therefore not a
finer approximation, it is the granularity at which the problem disappears for any sensible terrain.

Details worth keeping:

- **The edge midpoint pays for itself twice.** It is what halves a sector, and it puts a vertex over
  the *bridge*. The old chord ran corner to corner and interpolated between two corner means, missing
  the bridge underneath at a different height — so the shallows along an edge were shaded from a depth
  the water did not have there. Depths are now exact at every vertex again.
- **Both bodies can legitimately reach the same bridge, on disjoint pieces.** A wall along an edge is
  the mean of two heights, but a corner is the mean of three, so where a bridge meets *two* cells of
  the lower body that corner falls to `mean(0.95, -0.6, -0.6) = -0.08` — under the lower level, and so
  genuinely its shore. A first attempt at the test asserted only one body reaches the bridge and was
  wrong, not the code.
- The `terraces` scene exists for that case: three levels over two bridges, the second low enough that
  the bodies either side both reach it and each is confined to its own half of the cell. Where they
  meet is a step no rendering rule can remove — only place correctly.
- Verified: 66 tests, including one over every registered scene asserting that no location ever draws
  two levels over the same piece — which is the artefact in its general form. Screenshots A/B at three
  camera angles, with the old rule temporarily restored to shoot the same frames: the higher water is
  gone from the bridge's lower flank, the bridge no longer reads as islands standing in the upper body,
  and in `sea` several plates that hung past the grid's boundary lip disappeared with no shoreline
  opening up anywhere.

Divergences this opens, both pending reconciliation: [[terrain]]'s one-ring rule is still true but is
now stated too coarsely — the reach is per half-sector, not per hexagon — and its remark that "the data
has to be sensible rather than the renderer policing it" no longer describes what the renderer does.
[[water]]'s constraint that "the plate is a seven-vertex fan and stays one" is violated by the six edge
midpoints, though its *reason* — do not subdivide to displace a surface for waves — still holds.

## [2026-08-13] note | the pale stripe across `sea` is the shoreline, not an artefact

Raised as a suspected bug and it is not one. `undulating` is `0.5 * (sin(0.9q) + sin(0.9r))`, so on the
diagonal `q + r = 0` the two terms cancel: the seven locations from `(3,-3)` to `(-3,3)` are at a
**bitwise** zero, not merely near it — `sin` is odd and `r = -q` negates its argument exactly. A test in
`hex::tests` now pins that, because ground exactly at a water level is the renderer's awkward case and
this generator hands it over seven times in the default scene.

The stripe is therefore the *shoreline* of the default scene, and it is straight because the
generator's zero contour is straight. It looks like a band rather than a line for a second reason worth
knowing: `shallow_depth` is 0.55, so the pale-to-deep ramp spans the first 0.55 of depth, which on this
terrain is about two hexes either side of the contour. Draining the scene entirely (`flood` to -2.0)
removes the stripe, which is what proved it was water rather than a lighting or shadow seam.

Submergence now also tests `floor < level` rather than `floor <= level`, matching the strictness of
`flood`: a piece whose *lowest* ground is exactly at the level has nothing under it to cover. An `AE`
pixel diff of the same frame before and after shows that removes two small wedges and nothing else, so
it was not the explanation on its own.

**The explanation was the direction of the epsilon.** A piece is drawn when its *edge* is submerged,
and it reaches inward from that edge to the location's centre — so on a location lying exactly at the
water level, six of its twelve pieces are drawn and every one of them covers a cap at exactly the
level. `WATER_LIFT` put the plate 0.002 *above* it, winning the tie, so half of each of those seven
locations was covered in zero-depth water, which is the palest the shoaling colour goes. A probe
printing `water_plates` for the diagonal showed it: `(0,0)` is `height=0.0 water=None` and draws
pieces 3-8, every one with `cap=0.0`.

The constant is now `WATER_TIE_BREAK` and the plate sits `level - WATER_TIE_BREAK`, giving the tie to
the **ground**: land level with the water reads as land. Both directions stop the z-fighting the
epsilon exists for; only one of them is right, which is now recorded in [[bevy-0-19-api]]. The outline
clearance keeps its old sign — it wants to be above everything, and the plate it competes with has
moved further below it. Verified by a pixel diff of the two directions in the same frame at a pinned
window size: the change is confined to the wedges over that diagonal.

Two smaller things learned in passing. `WindowResolution` has no `From<(f32, f32)>`, only integer
forms — worth pinning a size for any screenshot A/B, since otherwise a tiling window manager hands out
different geometry per run and the diff is meaningless. And the ripple animation makes two runs differ
by a speckle everywhere there is water, so an `AE` count is a floor on the real difference, not the
difference itself.

## [2026-08-13] note | [[terrain]] and [[water]] marked stale

Flagged rather than rewritten, since reconciling the wording needs agreement. Both are the deficient
side of the contradiction — the code is doing what it should, and the prose predates the cases that
disprove it. [[terrain]] carries three: the one-ring rule is stated too coarsely now that reach is per
half-sector; "the data has to be sensible rather than the renderer policing it" fails in both
directions; and "ground exactly at the water line reads as submerged" was a bug, not a property.
[[water]]'s is the constraint pinning a plate to a seven-vertex fan, whose *reason* still stands even
though the count does not. Statuses are mirrored in [[home]], which repeats them inline.

## [2026-08-13] feature | the app can be aimed and read from a script

**Camera poses are nameable from outside**, `src/camera.rs`: `parse_pose` takes a preset (`top`,
`iso`, `low`, `fit`) or `yaw,pitch,radius` in degrees, and `Pose::Fit` defers to
`framing::reset_view` rather than duplicating the framing maths it already owns. Degrees convert to
radians at the boundary, so `place` and every existing caller are untouched. Out-of-range values
clamp rather than reject, matching what dragging already does; an unknown *name* still exits 2.

- This closes something the log recorded two entries ago: reaching a low-pitch view previously meant
  editing `Orbit::default()` in source and putting it back, because the shell had no way to aim the
  camera from outside. `HEX_TERRAIN_CAMERA=low` is now the whole operation.

**`src/screenshot.rs` became `src/probe/`**, since it now owns aiming and reporting as well as
capture. `HEX_TERRAIN_CAMERA` takes a `;`-separated list and `HEX_TERRAIN_INTERVAL=<frames>x<count>`
repeats each pose, so one launch yields many frames — the launch being the expensive part of any
visual check. A single capture still writes exactly the path given; only a batch gets an index.

**A JSON report, `src/probe/report.rs`**, is the half a screenshot cannot show. `serde` and
`serde_json` are the first dependencies past `bevy`, admitted by agreement and scoped to this: the
serialisation types live in their own module and nothing in `src/hex/` or `src/view/` derives
`Serialize`, which is the same boundary `Resource` is kept behind. Reporting `entities` alongside
`vertices` per mesh kind is deliberate — "nothing was spawned" and "something empty was spawned" are
different bugs and identical pictures.

- `model.water_levels` is distinct levels, not bodies: `water_plates` knows the real partition but
  computes it per location and does not expose it. On `terraces` it reports `[-0.3, 0.0, 0.55]`,
  which is the three levels that scene exists to hold.

**Pinning the window turned out to need three things**, two of which worked. Equal min/max
`WindowResizeConstraints` make i3 auto-float the window and take it out of the tiling layout, which
is what makes the aspect ratio hold exactly; a scale-factor override fixes the units the constraints
are read in; but `bevy_winit` multiplies the requested physical size by the backend scale factor at
window creation regardless of that override, so on a 2× display `1280x720` renders 2560×1440. The
request is therefore *logical* pixels, and rather than pretend otherwise the report carries the size
actually rendered — so comparability is checkable instead of assumed. Recorded in
[[bevy-0-19-api]] along with the `ParamSet` needed to read and write the camera in one system.

- Verified: 75 tests (was 66), the nine new ones covering pose parsing, index suffixing, interval
  parsing and shot expansion. Four-pose batch over `two-lakes` gives four PNGs and four reports whose
  `camera` blocks match the poses asked for; `HEX_TERRAIN_REPORT=-` gives parseable JSON Lines;
  `HEX_TERRAIN_INTERVAL=30x4` captures at frames 130/162/194/226 with the difference confined to the
  animating water. Two identical pinned runs differ by 6667 px of 3.69M — the ripple floor, not a
  whole-frame difference — where unpinned runs differed in size outright. `cargo run` alone is
  unchanged and logs nothing. `trunk build --release` succeeds at 52 MB; the variables are inert on
  web.
- New spec [[instrumentation]]. [[scene]] edited by agreement in two places: the screenshot mechanism
  is now a pointer, and its "one dependency" constraint is scoped to the shell with the serde
  exemption named.

## [2026-08-13] tooling | one build directory shared by every worktree

**A checked-in `.cargo/config.toml` sets `build.build-dir` to `{cargo-cache-home}/hex-terrain-build`**,
so Bevy's intermediates are compiled and stored once for the whole project instead of once per
worktree. The working process in `CLAUDE.md` puts every task in its own worktree, and each was
paying a full Bevy build and 3.8–20 GB of `target/` for the privilege — with four worktrees alive
at once by the end of this change, on a disk already 97 % full.

- **kache was working and was not the answer.** 48.2 % hit rate, ~37 min of compile avoided in 24 h,
  and the 1.6 GiB `bevy_dylib` compile genuinely in its store. But a compile cache does not stop
  cargo materialising ~14 GB of artefacts into each new `target/`; the duplication is structural.
  Measured: a fresh worktree costs **2 min 12 s and 7.2 GB** with kache alone, **11.8 s and ~0** with
  the shared directory. Details and the hardlink mechanism in [[build-performance]].
- **`build-dir`, not `target-dir`**, so each worktree keeps its own `target/` and
  `./target/debug/hex-terrain` still means *this* worktree's binary. The template variable rather
  than a relative path, so nothing depends on worktrees being siblings.
- The cost is a lock: concurrent builds in two worktrees serialise. Recorded as a trap.
- Verified: `cargo run -- two-lakes` renders and captures from the shared directory, so the
  dynamic-linking search path still resolves; alternating builds between two worktrees settle at
  0.3 s with no ping-pong recompilation; `trunk build --release` succeeds, putting wasm
  intermediates in the shared directory too and emitting a 52,238,516-byte `dist/*_bg.wasm` against
  the 52,336,672 already recorded — unchanged, as it should be.

## [2026-08-13] feature | a skirt under the terrain, and a cut through its water

- The surface is no longer a shell. Every location hangs a **closed hexagonal prism** from the
  boundary of its own hexagon: six sides down from the wall's outer rim, and a bottom facing down.
  New spec [[skirt]]; [[terrain]]'s *"Rejected: a skirt and an underside"* reversed by agreement, and
  its "no underside" omission with it.
- **The bottom is a common floor plus a per-location step.** `SKIRT_BASE = 0.6` under the grid's
  lowest ground, `± 2 × SKIRT_STEP = 0.12` from a hash of the coordinate. Measuring down from each
  cap instead — the obvious reading of "extend downwards a distance" — is both a smooth copy of the
  terrain and unsafe: a location's own boundary dips towards its lower neighbours, so a cap beside a
  deep one can hang below its own bottom. `2·SKIRT_STEP < SKIRT_BASE` is a `const _: () = assert!`.
- **Closing every location, rather than only the rim, is what removes the boundary case** that got a
  skirt rejected in the first place. Rim-only sides plus per-location bottoms leaves a hole at every
  step between two neighbours' undersides, and closing those needs a second rule with a sign to get
  wrong. ~54 triangles a location, ~2k for the grid — the interior sides are buried and not culled.
- **The wall's outer rim is now a shared function**, `edge_profile`: five points along one edge —
  corner, bridge end, edge midpoint, bridge end, corner. The wall ignores the midpoint; the skirt
  needs it, because it splits the edge exactly where `Pieces` does and so lets a water cut be granted
  the halves the body reaches. Both build from it, checked bitwise, since a crack here is a hole in
  something whose whole point is being closed.
- **The water cut is taken from `water_plates`, not from the location's own `Terrain::water`.** Under
  the one-ring rule a *dry* rim location carries a flooded neighbour's plate wherever its boundary
  dips below that level; testing its own water would leave that plate cut off in mid-air at the grid's
  edge.
- **`water.wgsl` cannot shade the cut**: it sets the fragment normal to point straight up, which is
  right for a level plate and lights a vertical face as though it faced the sky. Vertex colours
  instead, ramped on the CPU with the same two colours and the same `WATER_SHALLOW_DEPTH` the surface
  shoals over — now named constants rather than literals inside `WaterSettings`. Vertex colours turn
  out to *replace* the base colour before multiplying back into it, and to be linear; recorded in
  [[bevy-0-19-api]] against the vendored 0.19 source.
- The probe's cap count identifies a cap negatively, as the meshed child that is neither wall nor
  plate. A third kind of child silently became a cap until it was excluded — worth knowing before the
  fourth.
- Verified: 81 tests (was 75), six new — the bitwise seam against the wall, neighbours agreeing on
  the line they hang from, no prism inverting over any scene, reach confined to the hexagon, the cut
  confined to the rim with the right ramp at every vertex, and the hash stable and spending its whole
  range. Visually on `sea` and `two-lakes`: closed and stepped from below, prisms of differing length
  along the rim at a grazing angle, and a pale band darkening with depth under a submerged rim
  location. The `hide skirt` checkbox is wired like the compass one and, like every control here, has
  never been clicked — no pointer injection on this machine.

## [2026-08-13] feature | editor-style camera controls

- New spec [[camera-controls]], promoted out of [[scene]], which had carried the camera since the
  tree was created and had a stub in [[home]] anticipating exactly this. Right-drag flies
  (`WASD`/`QE`, `Shift` to run), middle-drag or `Alt`+left-drag turns about the point under the
  cursor, `Shift`+middle-drag pans, the wheel zooms towards the cursor. Left-click still selects.

**No camera mode, and that is the whole design.** The change was asked for as a free camera plus a
UI toggle between it and the orbit camera. Both were dropped: with the pivot design below the two
are the *same* camera differing only in which button is down, so a mode would have been a state
variable with no state in it, plus a widget and a class of "why won't it move" bugs. Right-drag had
to give up orbiting to become the fly button, which is what Unity, Godot and Unreal all bind it to
— and what `FreeCamera` already defaults to.

**`rebase` is a read-back, not a write-back** — the load-bearing decision. Deriving
`yaw`/`pitch`/`radius` about the new pivot and writing `place()` is the obvious implementation and
it is wrong: `place` points the camera *at* its target, so the view **snaps** whenever the pivot is
off centre, which with a cursor-picked pivot is most of the time and near a screen edge is tens of
degrees. Instead `turn_about` rotates position and orientation about the pivot together, and `Orbit`
is read back out afterwards. Being the exact inverse of `place`'s translation, it clamps nothing —
a lossy inverse would leave the report disagreeing with the camera, and a flight can genuinely end
up beyond `MAX_RADIUS`. A round-trip test pins it.

**Two traps, both caught by a test rather than by reading.**

- The pitch limit cannot test *how steep the view is*: past the pole the view is equally steep, so
  the check passes and the horizon lands upside down. It has to test whether the *step* would put
  the camera's up vector below the horizon.
- `FreeCamera` latches its own `yaw`/`pitch` from the transform exactly once, and consumes scroll
  whether or not the cursor is grabbed. Gating `FreeCameraState::enabled` on the right button fixes
  the second and kills the `M` toggle and Numpad snaps for free; re-seeding yaw/pitch at the press
  fixes the first. The gate must be in `PreUpdate` — `FreeCameraPlugin` runs in `RunFixedMainLoop`,
  which precedes `Update`, so from `Update` the press is always a frame late. All in
  [[bevy-0-19-api]].

**`Alt`+left-drag alone was not viable**: i3 sets `floating_modifier Mod1`, and this app auto-floats
itself whenever `HEX_TERRAIN_WINDOW` pins its size, so the window manager eats the drag before the
app sees it. Middle-drag is Blender's binding and no WM wants it, so both are bound — one `||`.

- `free_camera` is a **Bevy feature**, not a third-party crate, so [[scene]]'s one-dependency
  constraint holds unchanged; that page's note recording the built-in controllers as rejected was
  correct only for orbiting a target, which is still hand-written. The crate is not vendored into
  the cargo registry until the feature is enabled, so its source cannot be read in advance.
- `HEX_TERRAIN_CAMERA` gained `free:x,y,z@tx,ty,tz`. It resolves through `rebase` into an ordinary
  `Orbit`, so `Pose` gained no variant, `place` gained no branch and the probe's aiming code was
  untouched — and it inherits `place`'s definition at the poles, which is what a hand-written eye
  point usually wants. The report's `camera` block gained `target`, without which the spherical
  fields no longer say where the camera is. [[instrumentation]] amended by agreement: its "a pose is
  exactly three numbers" constraint was the third place orbit-around-origin was locked in.
- `pick_surface` now returns the hit point as well as the coord, so selection and the camera pivot
  share one ray routine rather than two that can disagree about what was hit.
- Verified: 80 tests (was 75); `cargo clippy --all-targets` clean; wasm builds.
  `HEX_TERRAIN_CAMERA='iso;free:12,6,-12@0,0,0;free:2.5,1.2,2.5@0,0.3,0'` over `two-lakes` puts the
  camera at each eye point to float precision, the last standing between two prisms at ground level
  — a view no three-number pose could reach.

**Hand-testing found what the tests could not, and the bug was a scheduling one.**
`fly_on_right_button` was registered as a bare `PreUpdate` system. That schedule is right —
`RunFixedMainLoop` follows it — but systems within a schedule are unordered, so it could run before
`bevy_input` had processed the frame's buttons and read the previous frame's state instead. The
controller was then enabled exactly one frame late, which is one frame too late for the
`just_pressed` its cursor grab depends on.

**The failure was silent and lopsided**: everything driven by `pressed` — `WASD`, `Q`/`E`, `Shift`,
wheel-for-speed — worked perfectly, and only mouse-look never engaged, that being the one behaviour
behind an edge rather than a level. Nothing warned. `.after(bevy::input::InputSystems)` is the fix,
and the general rule — gate a `just_pressed` consumer and you must order against the input, not
merely against the consumer — is now in [[bevy-0-19-api]].

## [2026-08-13] tooling | the web build publishes itself to GitHub Pages

- `.github/workflows/pages.yml` builds the release wasm on every push to `main` and deploys it as a
  Pages artefact, live at https://davidantliff.github.io/hex-terrain/. `dist/` stays gitignored —
  nothing built is committed. Recorded in [[build-performance]] with the two non-obvious parts:
  `--public-url /hex-terrain/`, without which a subpath site fetches its wasm from the domain root,
  and `CARGO_BUILD_BUILD_DIR: target`, which keeps CI out of the shared build directory so the cache
  action still sees the artefacts it caches.
- `index.html` now copies `assets/shaders` rather than all of `assets/`, which stops shipping the
  11 MB starmap cubemap that [[skybox-pipeline]] records as unwired. `data-target-path` preserves the
  runtime path. The deployed bundle went ~63 MB → ~50 MB with no change to the wasm.
- Release wasm size still has had no attention, and needs less than it looked: **Pages serves the
  wasm gzipped, 13.6 MB over the wire for the 52 MB file**, measured against the live site. Any
  `wasm-opt` work is therefore against a 14 MB download, not a 52 MB one. `data-wasm-opt="z"` remains
  the cheapest lever and is release-only, so it costs dev builds nothing.
- The cold run took 12m10s and saved a 473 MiB cache; the next push, docs-only, took 3m19s. A
  `Cargo.lock` or rustc change, or 7 days of no pushes, resets it to cold. Figures in
  [[build-performance]].
- Verified: built locally with `--public-url /hex-terrain/`, served from that subpath, and loaded in
  Chrome — the scene renders, `water.wgsl` returns 200, and only the expected WebGL2 downlevel
  warnings appear. `trunk serve` was **not** re-run; the change alters where trunk writes the asset,
  not how, and the release build exercises the same pipeline.

## [2026-08-14] feature | the inset becomes a knob, and argv gets a parser

- **`--scene` and `--inset`, parsed by `clap`.** The shell had one positional argument and a
  hand-rolled parser, which [[scene]] had locked as "not a CLI crate". A second knob broke every
  premise that decision rested on — ordering, absence and range all become real cases, and the
  valid scene names have to be printed in two places — so the constraint was amended by agreement,
  as `serde` was before it. `--scene` carries a `PossibleValuesParser` built from `scenes::names()`,
  so `--help` and the bad-name error are both generated and `scenes::build` can no longer fail.
- **The rule that bites: a test must never call `Cli::parse`.** Under `cargo test` the first
  argument is the test-name filter, so `cargo test some_filter` would exit the harness. Every test
  goes through `try_parse_from`. This is the same trap `named_scene` carried, restated for `clap`.
- **`const INSET` became `HexLayout::inset`.** Tuning it by eye used to cost an edit and a link. On
  the layout it costs nothing to wire: every mesh builder is already handed a `&HexLayout`, and
  `sync_cells` and `sync_skirts` already rebuild on `layout.is_changed()`, so the shared cap, every
  wall and every skirt follow a change with no new systems. The alternative — a `SeaLevel`-style
  resource — meant threading an `f32` through five functions and their tests. The inset survives
  `HexLayout::unit()` where `size` and `height_scale` do not, because it is a fraction of the
  circumradius and the meshes are built in that unit frame; a test pins it.
- **`probe` was re-reading `std::env::args().nth(1)`** to learn the scene name for the report. With
  two arguments that is simply wrong — it would have reported `--scene`. `ProbePlugin::for_scene`
  now takes the name `main` already resolved, and `report::Layout` gained `inset`.
- **Percent at the edges, fraction in the middle.** `--inset 25` and the panel slider both read in
  percent; `HexLayout::inset` holds `0.25`. Two divisions by 100, at the parser and at the
  observer, rather than a percentage threaded through the mesh builders.
- `clap` costs **+246,651 bytes of release wasm (0.47%)** — the one controlled before/after in
  [[build-performance]]. Left unconditional rather than scoped to non-wasm: 0.5% is not worth a
  second code path.
- Verified: 92 tests (up from 86), `cargo clippy --all-targets` clean, `cargo check --target
  wasm32-unknown-unknown`, `trunk build --release`. `--help`, `--scene nope`, `--inset 80` and
  `--inset wide` all exit 2 without opening a window; `cargo test the_inset` does not exit the
  harness. Screenshots at 8% and 25% show caps, walls, skirts and outlines moving together, and the
  report reads `layout.inset` 0.08 and 0.25 with `run.scene` correct. **Not verified:** dragging
  the inset slider — it has only been preset, as no pointer-injection tool exists here.

## [2026-08-14] change | a quieter default view

- The startup view now shows neither the compass nor the per-hex coordinates, and the cap inset
  starts at 25%: `ShowCompass` derives `Default` (false), `LabelMode`'s `#[default]` moved to
  `Off`, `DEFAULT_INSET` is `0.25`. All three remain togglable from the panel and `--inset`.
- Verified: `cargo test` — 92 tests pass, `cargo clippy --all-targets` clean, and
  `HEX_TERRAIN_REPORT=- HEX_TERRAIN_CAMERA=top cargo run` reports `layout.inset` `0.25`,
  `labels` `off` and `compass` `false` with no flags. Not verified: the window by eye.

## [2026-08-14] change | grid outlines off by default, behind a panel checkbox

- The cyan per-hex outlines now start hidden. `ShowGridLines` (defaults false, in `grid_render`)
  gates `draw_outlines`; a `grid lines` checkbox in the panel turns them on, the same three-line
  pattern as `compass` and `hide skirt`.
- The selected hex keeps its amber outline regardless — the check is per location, before the
  corners are computed, so hidden outlines also cost nothing to build.
- `probe`'s `layout.grid_lines` reports the state, which is what made this verifiable headlessly;
  `hide_skirt` still is not reported.
- Verified: `cargo test` — 92 tests pass, `cargo clippy --all-targets` clean, and screenshots
  from `HEX_TERRAIN_CAMERA=top` with the resource forced true and left at its default show the
  outlines present and absent, with the report reading `grid_lines` `true` and `false` to match.
  Not verified: clicking the checkbox itself, as no pointer-injection tool exists here.

## [2026-08-14] feature | five biomes, derived from elevation rather than stored

- **A biome is a function, not a field.** `Biome::at(&Terrain, &Bands)` in `src/hex/biome.rs`
  classifies a location from the height and water it already has. Nothing authors biomes — there is
  no generator and no editor — so a field on `Terrain` would be written by this same formula at
  construction and merely cached. It would also not be free: **no `Terrain` literal in the tree uses
  `..default()`**, so all ~20 exhaustive constructions across `scenes.rs`, `hex/mod.rs` and four test
  modules would have to name a value nothing yet varies. When something does author biomes, an
  `Option<Biome>` overriding this is the extension, and this becomes the `None` case. See [[biomes]].
- **Submerged ground is sand, whatever its elevation.** The sea bed and the strand above it are the
  same material, so the water line is where the ground stops being wet rather than where it changes
  kind. Without that rule a flooded mountain basin draws its bed as snow through the water.
- **`With` does not prove two queries disjoint; `Without` does.** `sync_biomes` takes mutable
  `MeshMaterial3d<StandardMaterial>` twice, once for caps and once for walls. Bevy rejects that on
  `With<HexCap>` / `With<HexWall>` alone, because an entity could carry both markers — the pair has
  to exclude each other explicitly. Recorded in [[bevy-0-19-api]].
- **The cap got a marker it should always have had.** `probe/report.rs` identified a cap by what it
  was *not* — `(Without<HexWall>, Without<HexSkirt>, Without<WaterSurface>)` — so every new kind of
  cell child had to be remembered there or be silently counted as a cap. `HexCap` makes it positive,
  and is what `sync_biomes` needed anyway to repaint one location's cap without touching the rest.
- **A model-only change reached nothing before this.** `sync_cells` guards on `layout.is_changed()`,
  which is right for geometry — no model change moves any of it — but a biome is the one thing a
  location presents that the layout has no part in. Hence a system and a guard of its own.
- **A ramp only ever makes *consecutive* biomes adjacent**, which is all a continuous height field
  can produce. The `biomes` scene therefore carries an outlier peak at `(-2,1)`, standing snow
  directly against the low ground: a stepped terrain can do that across one cliff, and a transition
  that only ever meets its immediate neighbour is not the one worth testing. A test asserts both that
  every biome appears and that some neighbouring pair is non-consecutive.
- Verified: 99 tests (92 lib, up 7, plus 7 in the binary), `cargo clippy --all-targets` clean,
  screenshots of `biomes` at `top`/`iso`/`low` and `sea` at `iso`, pinned to 1280x720. **Not
  verified:** dragging the two new sliders, for the same reason the inset slider was not — there is
  no pointer-injection tool here. The panel's own control count in `debug_ui.rs` was already one
  stale before this change; it is now nine in both that doc comment and `README.md`.

## [2026-08-14] feature | a procedural tint, and the octave that actually matters

- **A second material extension, following [[water]]'s pattern exactly.**
  `assets/shaders/terrain.wgsl` is fragment-only, binds at 100, and calls
  `pbr_input_from_standard_material` → mutate → `apply_pbr_lighting` →
  `main_pass_post_lighting_processing`. It ships with no build change: `index.html` already copies
  the whole `assets/shaders` directory. Caps and walls moved from `StandardMaterial` to
  `TerrainMaterial`, ten of them, differing only in base colour.
- **Two mistakes worth recording, both of which shipped looking like they worked.**
  - *The wavelength.* The intuition is that large-scale variation across the map is what stops a run
    of one biome reading as tiles, so the first build used a wavelength of 4.5 world units (about two
    hexes) with the textbook persistence of 0.5. What actually reads as flat is a cap being uniform
    **within itself** — a cap differing from its neighbour does not help, because the eye takes each
    cap as one surface regardless. Only the octaves shorter than a cap put texture inside one, and
    persistence is what pays them. Measured over a cap's worth of surface, as a share of the swing:
    `2.5 / 0.60 / 5 octaves` gives 26% inside a cap against 42% between caps; `1.6 / 0.80 / 6` gives
    37% against 32%, the first setting where the texture within a cap outweighs the step between them.
  - *The amplitude.* Summed value noise is an **average of independent values**, so it concentrates
    about its mean like any other average, and the nominal `-1..1` is a range it essentially never
    visits — measured, `(fbm - 0.5) * 2` has a standard deviation of 0.15. So `tint_amplitude` named a
    tail it never reached: a nominal 22% moved typical brightness by 4%, and against a tint-off frame
    came to a peak of **3 parts in 255**. The swing is now divided by two of its standard deviations,
    which makes the setting mean what it says; that divisor must be re-measured whenever the octave
    count or persistence moves.
  - **The lesson under both is the method, not the numbers**: "it looks subtle but it's there" was
    wrong twice, and only a pixel diff against a control frame settled it. Within-cap variation went
    0.99 → 2.23 → 3.56 levels of 255 across the two fixes, peak 8 → 12 → 16.
- **Noise is 3D, not 2D on the ground plane.** Two reasons: a cap and the wall below it would
  otherwise share a tint and the wall would be vertically streaked, and `GridPlane` means there is no
  fixed "ground plane" to project onto. Sampling the full world position sidesteps both.
- **An integer avalanche, not `fract(sin(...))`.** The sine trick's quality depends on how the driver
  evaluates `sin` at large arguments and bands visibly on some GPUs. The lattice index is shifted by
  a large positive constant before conversion so nothing depends on how a negative value casts to
  `u32`, and only the top 24 bits reach the float, which is what an `f32` holds exactly.
- **Octaves step by 2.03 and are displaced, not doubled.** Exact doubling lands every octave's
  lattice on the same integer planes, where they reinforce into a visible grid.
- `TerrainLook` **is** the shader's settings struct rather than a parallel copy, so a field cannot be
  added to one and forgotten in the other.
- Verified: 99 tests, `cargo clippy --all-targets` clean, `cargo check --target
  wasm32-unknown-unknown`. A controlled A/B on `biomes` at 1280x720 `iso`, tint 0% against tint 22%
  with everything else equal. **Not verified:** the shader in a browser on WebGL2 — per [[water]] the
  WGSL→GLSL translation happens there at runtime, so this is the check that counts and it is deferred
  to the end of the procedural work. Three Bevy systems now carry
  `#[allow(clippy::too_many_arguments)]`; each argument is a resource genuinely read, and a
  `SystemParam` would hide the dependency list without shortening it.

## [2026-08-14] feature | rock by slope, and an ambient occlusion that could not work here

- **A steep face is bare rock, whatever biome it stands in.** `1 - dot(normal, up)` against a
  threshold in the shader, with the up axis passed in as a uniform from `HexLayout::plane` rather
  than assumed to be `+Y`. Caps need no exemption: they are level, so their slope is zero and the
  mix never fires. Measured on `biomes`: 6.2% of the visible terrain, median 8 levels of 255,
  99th percentile 94.
- **Ambient occlusion was built, measured and removed. The measurement is the point.** A per-vertex
  crease term rode in `uv.y` and attenuated `PbrInput::diffuse_occlusion`, which is the correct
  channel — it dims the indirect light a shut-in surface loses and leaves direct sun alone. It was
  invisible, and not because of a bug: **this scene is lit mostly by its directional sun.** Occluding
  *all* indirect light on every surface — a hardcoded 80%, geometry ignored — moves the median
  terrain pixel by **zero** levels of 255, with a maximum of 42. So the ceiling on any physically
  honest AO here is a handful of levels, and the geometric term produced three. Deleted rather than
  faked by darkening albedo, which would dim a crease in full sunlight too. Worth revisiting only if
  the lighting stops being sun-dominated.
- **`max` over an image diff is a trap, and it cost several rounds.** A single antialiased edge pixel
  put the peak difference at 173 of 255 between two frames that were, in the body of the terrain,
  identical — which read as "the effect is strong" when the median was 0. Percentiles over a mask of
  terrain pixels are the statistic; the peak is noise. The same mistake in the other direction is
  what made the [[biomes]] tint look like it worked in the first place.
- **Screenshots from `probe` are deterministic**, because the settle and repose frame counts are
  fixed and `globals.time` follows the frame count. Two runs of unchanged code are byte-identical
  even with the water animating, so a zero diff really does mean nothing changed — which is what
  proved the occlusion uniform was inert rather than merely subtle.
- **To find out whether a per-vertex channel reaches the fragment shader, render it as albedo.**
  `base_color = vec3(in.uv.y)` settled in one frame that `uv.y` was arriving correctly and moved the
  search to the lighting, after two rounds of guessing at the uniform layout.
- Verified: 99 tests, `cargo clippy --all-targets` clean. `WALL_SHADE` stays at 0.78 and is now
  documented as a deliberate thumb on the scale, since the measurement above shows the lighting
  cannot supply that difference on its own.

## [2026-08-14] feature | three biomes blended across a wall, and an ordering that was not needed

- **A wall carries its blend, a cap carries its biome.** `wall_mesh` writes a weight triple per
  vertex into the colour channel and packs the three biomes it blends into `uv.x` as `a + 8b + 64c`.
  Weights come from what each vertex stands on: a cap corner is this location's alone, a bridge end
  is the midpoint of two caps, and the lattice vertex is the point three meet at — already the
  centroid of the three cap corners in plan, because they are inset along directions 120° apart
  which sum to zero. One triple serves all four triangles of a sector.
- **The canonical ordering the design called for turned out to be unnecessary.** The plan was to sort
  the three cells of a wedge by `(q, r)`, as `mean_height` does for heights, so two locations agree
  on which weight belongs to which cell. Two facts make it moot: the weights at every *shared* point
  are symmetric — a half each along an edge, a third each at a lattice vertex — so the slot a cell
  occupies cannot change their sum; and the shader perturbs each weight by noise keyed to the
  **biome identity** rather than to the slot, which is the one place ordering could have leaked back
  in. Keying the noise that way is deliberate and load-bearing, not incidental.
- **The seam test compares the resolved mix, not the triple.** `neighbours_resolve_the_same_biome_mix_on_every_shared_vertex`
  buckets every wall vertex by world position, unpacks the identities, accumulates weight per biome,
  and asserts two locations reaching the same point agree **bitwise** — which they do, across all
  four layouts. That is the same standard `cells_agree_on_every_shared_edge_and_corner` holds heights
  to. Confirmed to bite: replacing the lattice vertex's `(⅓,⅓,⅓)` with an asymmetric triple fails it
  with the offending pair and position.
- **Packing identities into `uv.x` is safe because they are constant across a triangle.** An
  interpolated attribute returns a constant exactly, so the unpack cannot land between two values.
- **The wall lost its own colour, and two constants with it.** `WALL_SHADE` and `biome_wall_fill` are
  gone: a wall now takes the same palette the caps do, and what makes it read darker is the tilt of
  its normal, which the renderer was already doing. Five wall materials collapsed to one, since
  nothing about a wall's material varies per location any more.
- Verified: 100 tests (93 lib, 7 binary), `cargo clippy --all-targets` clean, `cargo check --target
  wasm32-unknown-unknown`. Measured on `biomes` at 1280x720 `iso` against the step-3 frame: the blend
  moves 28.2% of the visible terrain at a median of 10 levels of 255 and a 99th percentile of 59; the
  perturbation on top of it moves a further 15.3% at a median of 7. **Not verified:** a browser on
  WebGL2, still deferred to the end — and `#ifdef VERTEX_COLORS` selecting the wall path inside a
  material extension is exactly the kind of thing that could translate differently there.

## [2026-08-14] feature | the normal follows the noise, and a shared build directory bites

- **Tilting the normal by the tint field's slope is the largest single gain of the five steps.** A
  cap is a flat plate and reads as one under a directional light however its colour varies; once the
  normal follows the same field that tinted it, a dip is darker *and* faces differently, which is
  what the eye reads as texture rather than as a stain. Forward differences, so three extra noise
  evaluations rather than the six a central difference costs — the half-step bias that buys is
  meaningless when there is no true surface being approximated, only a field being borrowed.
- **The gradient is projected onto the surface before it tilts anything.** [[water]] can treat the
  world axes as its tangent frame because a plate is always level; nothing on the terrain can, and
  `GridPlane` means the grid does not have to lie in the same plane from one run to the next.
  Removing the component along `N` is what makes one function serve caps and walls alike.
- **The sample step wanted to be *smaller*, which was the opposite of the guess.** Reasoning that a
  step of 0.02 noise units is about three pixels and therefore "sub-pixel noise", it was swept
  against 0.06 and 0.14. Measured against a bump-free frame, 0.02 changed 50.7% of the terrain at a
  median of 6 levels of 255, against 43.5%/5 and 31.0%/4. The finest octaves are what read as a
  surface; the broad tilt is what reads as nothing.
- **It fades with distance**, the same guard [[water]] puts on its ripples and for the same recorded
  reason: below a pixel the pattern becomes crawling shimmer, and from any altitude most of the
  terrain on screen is past that point. Not verified in motion — the captures are still frames.
- **The build directory is shared between worktrees, and that can serve the wrong binary.**
  `.cargo/config.toml` points every worktree at `~/.cargo/hex-terrain-build`; see
  [[build-performance]]. After an accidental `cargo run` from the primary checkout, `cargo run` in a
  worktree reported `Finished` in 0.24s and then ran a binary that **rejected a scene the worktree's
  own source defines** — `--scene biomes` came back "possible values: sea, two-lakes, terraces".
  `touch`ing a source file and rebuilding fixed it. The symptom to watch for is a build that finishes
  suspiciously fast after changing only assets, and the cheap guard is to assert the screenshot
  actually appeared rather than trusting the run.
- **This is the likeliest explanation for the phantom measurement in the previous entry**, where a
  control frame differed from every other by 173 of 255 for no reason the code could account for.
  Recorded as likely rather than proven: the frame was not kept and the state cannot be reproduced.
- Verified: 100 tests, `cargo clippy --all-targets` clean, `cargo check --target
  wasm32-unknown-unknown`. Frame rate is vsync-capped at 60 both with and without the bump, so that
  only shows the three extra noise evaluations do not breach 60 at this scene size — it is not a
  measurement of their cost.
