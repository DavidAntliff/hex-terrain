---
tags: [scene, camera, skybox, spec]
type: spec
status: implemented
updated: 2026-08-12
---
# Spec: Scene shell

The shell every other feature is displayed in: an all-sky starfield, an orbit camera, clean exit,
and the native and web build paths. What the scene *contains* is other specs' business — currently
[[hex-grid]].

## Requirements

### Goal (definition of done)

`cargo run` opens a window showing the scene's contents against a starfield in every direction;
right-drag orbits the camera about the origin, the scroll wheel zooms, Escape exits cleanly. The
same scene, built to wasm and served by any static web server, renders identically in Firefox and
Chrome.

### Constraints

- **One dependency.** `bevy` only. No third-party camera-controller, math, or asset crates —
  behaviour that costs a few dozen lines is written here instead.
- **Web parity is non-negotiable.** Anything added must build for `wasm32-unknown-unknown`.
  This rules out several conveniences; see Design discussion.
- **A fresh clone must run** with no asset-generation step: the generated skybox texture is
  committed.
- **WebGL2 is the web baseline** — it is the Bevy default feature. Do not require `webgpu`.
- **Skybox imagery must be freely licensed** and attributed where required.
- The camera orbits the **origin only**; there is no pan.

### Functional requirements

In scope: the skybox, lighting, orbit + zoom + exit input, the native and web build paths, the
tooling that generates the skybox texture, and the scripted-screenshot mechanism used to verify
any of it.

Out of scope, deliberately: whatever the scene displays (see [[hex-grid]]), lighting the scene
*from* the skybox, and any gameplay.

## Design discussion

**Camera controller — write it, don't take one.** Bevy 0.19 ships first-party controllers in
`bevy_camera_controller` (`free_camera`, `pan_camera`), but neither orbits a fixed target, so
neither meets the requirement. A third-party orbit-camera crate would satisfy it but violates
the one-dependency constraint for roughly twenty lines of work. Decision: a local `Orbit`
component holding `yaw`/`pitch`/`radius`, with one system writing the `Transform`. Locked.

**Skybox — a real cubemap, not a textured sphere.** An inverted sphere with an equirectangular
texture is fewer lines, but it is finite geometry: it clips as the camera zooms out and needs
either a zoom clamp tied to its radius or a per-frame follow system. Bevy's `Skybox` component
is infinitely distant and interacts correctly with any far plane. Decision: `Skybox` with a
cubemap, accepting the offline reprojection step described in [[skybox-pipeline]]. Locked.

**Skybox imagery — an all-sky star map, not JWST.** A James Webb image was the original
request, but no JWST all-sky panorama exists: Webb images deep patches of sky, not the full
sphere. The alternatives were a JWST deep field repeated across all six cube faces (genuine
Webb imagery, but with discontinuities at every face boundary) or a true 360° star map from
another source. Decision: NASA SVS *Deep Star Maps 2020* — real Gaia/Tycho astrometry, public
domain, seamless. Locked, on the understanding that the imagery is not Webb's.

**Bevy linked dynamically on native targets.** Statically linking Bevy in a debug build
produces a ~1.7 GB binary and a ~40 s edit-rebuild cycle. The `dynamic_linking` feature cuts
that to ~2 s, but does not build for wasm at all, so it must be scoped with
`[target.'cfg(not(target_family = "wasm"))'.dependencies]` rather than enabled globally.
Measurements and the rejected alternatives are in [[build-performance]]. Locked.

**Web tooling — trunk.** It builds, bundles the assets, and fetches the wasm-bindgen version
matching Bevy, which removes a real version-mismatch failure mode. The alternative — invoking
`wasm-bindgen` directly behind a small script — requires keeping `wasm-bindgen-cli` pinned to
Bevy's `wasm-bindgen` by hand on every upgrade. Locked.

## Implementation details

`src/main.rs` is app wiring: plugins, resources, lighting, the camera, and the skybox. The camera
lives in `src/camera.rs` and the screenshot mechanism in `src/screenshot.rs`.

- `setup` — spawns a `DirectionalLight`, a `GlobalAmbientLight` resource, and the camera carrying
  `Camera3d`, a `Transform` from `place()`, the `Skybox` component, and `Orbit`.
- `place(&Orbit) -> Transform` — the single place spherical coordinates become a transform,
  shared by `setup` and `orbit` so the first frame is already correct.
- `patch_cubemap` — a PNG carries no cubemap metadata, so once the image loads this calls
  `reinterpret_stacked_2d_as_array` and sets the view dimension to `Cube`. Two details are
  load-bearing and easy to regress: it checks the layer count through an immutable
  `Assets::get` **before** taking `get_mut`, because `get_mut` flags the asset modified and
  would re-upload the texture every frame; and it reassigns `skybox.image` afterwards so the
  render world rebuilds its bind group.
- `orbit` — reads `ButtonInput<MouseButton>`, `AccumulatedMouseMotion` and
  `AccumulatedMouseScroll`. Pitch is clamped just short of ±π/2, where `looking_at`
  degenerates. Scroll is normalised by `AccumulatedMouseScroll::unit`, because browsers report
  pixel deltas roughly 50× larger than a desktop mouse's line deltas — without this, zoom is
  unusable in the browser while feeling fine natively.
- `exit_on_escape` — writes `AppExit::Success`, which is the graceful path: Bevy finishes the
  frame, drops the world and closes the window itself.
- `ScreenshotOnDemandPlugin` — `HEX_TERRAIN_SCREENSHOT=<path>` renders 120 frames, saves a PNG of
  the framebuffer, waits for the asynchronous write, then exits. It exists because capturing the
  window through the X server is unreliable: a window on an inactive workspace is unmapped and
  yields a blank image, so scripted visual verification has to come from inside the app.

The crate is a **library plus a binary** (`src/lib.rs`, `src/main.rs`). The split is what makes
the model a real API boundary rather than a convention, and it keeps the compiler honest — public
items in a binary-only crate read as dead code, which buries genuine warnings.

Supporting files: `index.html` (trunk entry point, copies `assets/`), `Cargo.toml` (the
target-scoped dynamic-linking dependency and the dev-profile opt-levels),
`tools/make_skybox.py` (see [[skybox-pipeline]]), and the committed
`assets/textures/starmap_cubemap.png`.

API specifics this code depends on are recorded in [[bevy-0-19-api]].

## Verification plan

Performed:

- `cargo test` — one test presses Escape through `ButtonInput` and asserts
  `App::should_exit() == Some(AppExit::Success)`, covering the key code, the system
  registration, and the message reaching the app.
- `cargo run` — window opens on Vulkan, screenshot confirms the scene contents lit against the
  Milky Way band, with no seams at cube-face boundaries. Confirmed the app runs untouched without
  exiting.
- `trunk build --release`, served by a static web server — screenshots confirm an identical
  render in **both Chrome and Firefox**, with the wasm, JS, and cubemap all fetched `200`. A
  `404` on `starmap_cubemap.png.meta` is Bevy's normal asset-meta probe, not a fault.
- `python3 tools/make_skybox.py --check` — the projection asserts pass.

Not verified: a physical Escape keypress, and mouse-drag/scroll behaviour by hand. No
key-injection tool was available, so the input paths are covered by the headless test and by
inspection only. **Anyone changing the input code should confirm the feel manually.**

## Implementation status

**status:** implemented — spec and code agree. No known divergences.

Deliberate omissions, each marked with a `ponytail:` comment at the relevant site in the code:

- The skybox does not light the scene. Doing so needs `EnvironmentMapLight` with a prefiltered
  KTX2 environment map, which requires `toktx`/`basisu` tooling that is not currently used.
- `Orbit` has no `target` field; the camera cannot pan.
- Cubemap faces are 1024². The generator's `FACE` constant can go to 2048 at the cost of web load
  time.

The camera's default `radius` and the compass placement are tuned together so that both fit the
default view. The horizontal extent of the view depends on window aspect ratio, so a widget placed
to the side of the grid falls off-screen in a portrait window — which is why the compass sits south
of the grid. Changing either constant means re-checking the framing.

Known rough edges that are not spec divergences: the release wasm bundle is ~51 MB plus ~11 MB
of assets, untuned; and the committed cubemap is 10.8 MB of binary in git history.

## Related

- [[hex-grid]]: what the scene currently displays
- [[skybox-pipeline]]: how the cubemap asset is generated
- [[bevy-0-19-api]]: the API facts this implementation relies on
- [[build-performance]]: why the dependency is target-scoped, with the measurements
