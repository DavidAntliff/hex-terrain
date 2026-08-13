---
tags: [scene, camera, skybox, spec]
type: spec
status: implemented
updated: 2026-08-13
---
# Spec: Scene shell

The shell every other feature is displayed in: a daylight sky that also lights the scene, an orbit
camera, clean exit, and the native and web build paths. What the scene *contains* is other specs'
business — currently [[hex-grid]].

## Requirements

### Goal (definition of done)

`cargo run` opens a window showing the scene's contents under a daylight sky, over ground that
hazes into that sky at the horizon rather than meeting it at a line; right-drag orbits the camera
about the origin, the scroll wheel zooms, Escape exits cleanly. The same scene, built to wasm and
served by any static web server, renders identically in Firefox and Chrome.

### Constraints

- **One dependency for the shell.** `bevy` only. No third-party camera-controller, math, or asset
  crates — behaviour that costs a few dozen lines is written here instead. Amended once, by
  agreement: `serde` and `serde_json` are admitted for the scene report, which is
  [[instrumentation]]'s and is not part of the shell. The reasoning is on that page; the rule here
  is unchanged for everything the shell itself does, and there is still no CLI crate.
- **Web parity is non-negotiable.** Anything added must build for `wasm32-unknown-unknown`.
  This rules out several conveniences; see Design discussion.
- **A fresh clone must run** with no asset-generation step.
- **WebGL2 is the web baseline** — it is the Bevy default feature. Do not require `webgpu`. This
  is what rules out compute shaders, and with them Bevy's procedural `Atmosphere` and every
  runtime-generated environment map; see Design discussion.
- **Skybox imagery must be freely licensed** and attributed where required.
- The camera orbits the **origin only**; there is no pan.
- **The scene is lit in physical units.** Illuminance in lux, sky luminance in cd/m², and the
  camera's `Exposure` set for daylight — no level tuned against the one beside it.

### Functional requirements

In scope: the sky, lighting, orbit + zoom + exit input, the native and web build paths, the tooling
that generates a skybox texture, and which of the named scenes the shell loads.

Out of scope since it grew past a single screenshot: driving and reading the app from a script —
aiming the camera, capturing a batch, reporting scene state. That is [[instrumentation]].

Out of scope, deliberately: whatever the scene displays (see [[hex-grid]]), clouds, time of day,
and any gameplay.

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

**Sky imagery — a generated daylight sky, replacing the starfield.** The scene's sky was an
all-sky star map: NASA SVS *Deep Star Maps 2020*, reprojected by `tools/make_skybox.py` and
committed as a 10.8 MB cubemap. It was chosen over a JWST deep field, which does not exist as an
all-sky panorama and would have shown a discontinuity at every face boundary. What displaced it
was [[water]]: water is a mirror, and a night sky gives a mirror nothing to reflect, so the water
read as painted blue however it was shaded. Decision: a **daylight sky**, and since the imagery is
now analytic rather than photographic, generated in `src/sky.rs` at startup rather than committed.
Locked. The starfield generator and its asset stay in the tree, unwired, for a night mode; see
[[skybox-pipeline]].

**The sky model — Preetham, not a hand-drawn gradient.** A zenith-to-horizon interpolation is
thirty lines and looks close enough from directly overhead. It has no real horizon glow, which is
precisely the part of the sky that water reflects at grazing angles, so the cheap version fails at
the one job the sky was replaced to do. Decision: **Preetham et al.'s analytic daylight model**
(1999) — coefficients only, no data files, and horizon brightening and the solar halo fall out of
it rather than being painted on. Locked.

**Rejected: Bevy's procedural `Atmosphere`.** Physically the best answer, and
`AtmosphereEnvironmentMapLight` would have lit the scene from it directly, compositing over a
starfield by time of day. But `bevy_pbr/src/atmosphere/` builds its LUTs with compute pipelines,
and the environment-map generator disables itself where compute is unavailable — which is WebGL2.
Native-only, so it fails web parity. Rejected on that ground alone; revisit only if the web
baseline ever moves to WebGPU.

**The horizon is a haze, not a line.** Ground meets sky over several degrees of aerial
perspective, fading towards the sky's own colour at the same azimuth. Two reasons: it is what the
horizon looks like from altitude, which is the viewpoint the scene is composed for; and because
both sides of the blend are the same value at the join, there is no seam to place and nothing to
re-match when the sun moves. The width is a knob, and it is the one that decides whether the scene
reads as a hilltop or an aircraft. Locked.

**The sky lights the scene, through a gradient rather than the cubemap itself.**
`EnvironmentMapLight::hemispherical_gradient` takes three colours and needs no prefiltered KTX2
map, so the `toktx`/`basisu` tooling this spec once recorded as the blocker is not needed at all.
The three colours are sampled from the same `Sky`, so the reflection and the sky behind it agree
by construction. A prefiltered version of the full cubemap would reflect the sun's own halo as
well, but generating one needs compute — the same WebGL2 wall as the atmosphere. Locked.

**Bevy linked dynamically on native targets.** Statically linking Bevy in a debug build
produces a ~1.7 GB binary and a ~40 s edit-rebuild cycle. The `dynamic_linking` feature cuts
that to ~2 s, but does not build for wasm at all, so it must be scoped with
`[target.'cfg(not(target_family = "wasm"))'.dependencies]` rather than enabled globally.
Measurements and the rejected alternatives are in [[build-performance]]. Locked.

**Web tooling — trunk.** It builds, bundles the assets, and fetches the wasm-bindgen version
matching Bevy, which removes a real version-mismatch failure mode. The alternative — invoking
`wasm-bindgen` directly behind a small script — requires keeping `wasm-bindgen-cli` pinned to
Bevy's `wasm-bindgen` by hand on every upgrade. Locked.

**A named scene, chosen by one positional argument.** A grid built to isolate one rendering question
is worth keeping, and worth having beside the sandbox rather than instead of it, so scenes are a
named set and the shell loads one of them. The shell owns only the *choice*; what a scene contains,
and every height and water level in it, is the model's — see [[terrain]].

Selection is `std::env::args().nth(1)` against a table, defaulting to the sandbox. Not a CLI crate:
the one-dependency constraint stands, and one positional name is the whole interface. Not an
environment variable either, despite `HEX_TERRAIN_SCREENSHOT` being the existing precedent — that
one configures a *mode* the app can be put into from a script, whereas this picks what to run, which
is what an argument is for. The web build has no argv and therefore always gets the default, which
is a real limitation rather than an oversight: a scene worth showing in a browser has to become the
default, or be selectable in the panel. Locked.

## Implementation details

`src/main.rs` is app wiring: plugins, resources, lighting, the camera, and the sky. The sky model
lives in `src/sky.rs`, the camera in `src/camera.rs`, and everything a script drives the app with in
`src/probe/` — see [[instrumentation]].

- `setup` — spawns a `DirectionalLight` at `lux::DIRECT_SUNLIGHT`, a `GlobalAmbientLight` of zero,
  and the camera carrying `Camera3d`, a `Transform` from `place()`, `Exposure::SUNLIGHT`, the
  `Skybox`, an `EnvironmentMapLight` and `Orbit`.
- `SUN_DIR` — one constant, aiming both the `DirectionalLight` and the sky's sun, so the sky's sun
  and the highlight the water throws off it cannot drift apart.
- `sky::Sky` — `sun`, `turbidity`, `ground` and `haze`, and two outputs: `cubemap()` for the
  `Skybox`, and `gradient_colours()` for the `EnvironmentMapLight`. Load-bearing details:
  - **Values are cd/m²**, the units Bevy's photometric pipeline already works in, so `brightness`
    and `intensity` are both plain `1.0` and exposure lives on the camera alone. There is no second
    scale to keep in step with the first.
  - **`ground` is a fraction of the horizon's brightness**, not a colour in chosen units. Putting a
    stored value through a luminance scale once sank the ground to 5e-6, which fills the whole
    frame with black in any downward view and is indistinguishable from a skybox that failed to
    load. A fraction cannot go wrong that way, and a test pins the ratio.
  - **The cubemap is `Rgba16Float`** — a cloudless sky is one large smooth gradient, the worst case
    for 8-bit banding, and this is the one HDR format WebGL2 filters without an extension. The
    `f32`→`f16` conversion is hand-rolled: `half` is not a dependency and will not become one for
    fifteen lines, and Rust's own `f16` is unstable.
  - The image is built with its cube view descriptor already set, so **no `patch_cubemap` step
    exists any more**. A generated image needs no load-state polling, no
    `reinterpret_stacked_2d_as_array`, and no re-upload guard — the whole system was deleted.
- `place(&Orbit) -> Transform` — the single place spherical coordinates become a transform,
  shared by `setup` and `orbit` so the first frame is already correct.
- `orbit` — reads `ButtonInput<MouseButton>`, `AccumulatedMouseMotion` and
  `AccumulatedMouseScroll`. Pitch is clamped just short of ±π/2, where `looking_at`
  degenerates. Scroll is normalised by `AccumulatedMouseScroll::unit`, because browsers report
  pixel deltas roughly 50× larger than a desktop mouse's line deltas — without this, zoom is
  unusable in the browser while feeling fine natively.
- `exit_on_escape` — writes `AppExit::Success`, which is the graceful path: Bevy finishes the
  frame, drops the world and closes the window itself.
- `named_scene` — resolves the first argument through `hex::scenes::build`, or exits 2 listing the
  names. Called **before** `App::new()`, so a mistyped name costs the message and not a GPU
  initialisation. The scenes themselves are model data in `src/hex/scenes.rs`: a `SCENES` table of
  `(name, fn() -> TerrainGrid)`, `DEFAULT`, and the grid radius, which lives there rather than in
  `view` because a grid's extent is dimensionless.
  - Tests must not call it. Under `cargo test` the first argument is the test-name filter, so it
    would exit the harness.
- `ProbePlugin` — the scripted-observation mode, specified in [[instrumentation]]. It lives here in
  outline only because the shell adds it and because the reason for its existence is the shell's:
  capturing the window through the X server is unreliable, since a window on an inactive workspace
  is unmapped and yields a blank image, so scripted visual verification has to come from inside the
  app. `main` additionally reads `HEX_TERRAIN_WINDOW` to pin the window size, the window being its
  to configure.

The crate is a **library plus a binary** (`src/lib.rs`, `src/main.rs`). The split is what makes
the model a real API boundary rather than a convention, and it keeps the compiler honest — public
items in a binary-only crate read as dead code, which buries genuine warnings.

Supporting files: `index.html` (trunk entry point, copies `assets/`), `Cargo.toml` (the
target-scoped dynamic-linking dependency and the dev-profile opt-levels), and — no longer wired to
anything, kept for a night mode — `tools/make_skybox.py` and `assets/textures/starmap_cubemap.png`
(see [[skybox-pipeline]]).

API specifics this code depends on are recorded in [[bevy-0-19-api]].

## Verification plan

Performed:

- `cargo test` — one test presses Escape through `ButtonInput` and asserts
  `App::should_exit() == Some(AppExit::Success)`, covering the key code, the system
  registration, and the message reaching the app.
- `cargo test` over `sky` — the cube faces point where the table claims (sampling the poles, which
  is the only way the projection can be silently wrong); the sun is the brightest point in the map;
  the zenith is bluer and darker than the horizon; and the ground-to-sky haze is monotonic with no
  step hidden in it and a ground-to-horizon ratio that is neither a black void nor brighter than
  the sky.
- `cargo run` — window opens on Vulkan. Screenshots confirm the sky at two camera pitches: from
  above, dark grey ground filling the frame; near-horizontal, blue overhead grading through a warm
  horizon band into ground, with no line anywhere and no seam at a cube-face boundary.
- `trunk build --release`, served by a static web server — confirmed rendering in Firefox on
  WebGL2, which is the check the whole sky and [[water]] design is shaped around.
- `python3 tools/make_skybox.py --check` — the projection asserts pass. Still run, though the
  starfield is no longer the scene's sky.
- Scene selection — `cargo run` gives the sandbox unchanged; `cargo run -- two-lakes` gives that
  scene; `cargo run -- nope` prints `unknown scene "nope"; one of: sea, two-lakes` and exits 2
  without opening a window. A test asserts every registered name builds a grid of 37 locations.

Not verified: a physical Escape keypress, and mouse-drag/scroll behaviour by hand. No
key-injection tool was available, so the input paths are covered by the headless test and by
inspection only. **Anyone changing the input code should confirm the feel manually.**

Not verified this time round: Chrome. `WGPU_BACKEND=gl` is worth knowing about as a cheap native
proxy for the GLSL path, and worth knowing it may not be available — neither the NVIDIA driver nor
a software Mesa fallback offered wgpu a GL adapter here.

## Implementation status

**status:** implemented — spec and code agree. No known divergences.

Deliberate omissions, each marked with a `ponytail:` comment at the relevant site in the code:

- `Orbit` has no `target` field; the camera cannot pan.
- Generated sky faces are 256². `sky::FACE` can go higher, at a startup cost, if the sky ever
  gains a feature sharper than its haze band — a solar disc, or clouds.
- The environment map is a three-colour hemispherical gradient rather than a prefiltered version of
  the sky itself, so the water reflects the sky's grading but not its solar halo.
- The starfield cubemap is still committed and its generator still works, but nothing loads either.
- A scene can only be chosen at startup, and only natively. There is no in-app scene switcher, so
  the web build shows the default and nothing else.

The camera's default `radius` and the compass placement are tuned together so that both fit the
default view. The horizontal extent of the view depends on window aspect ratio, so a widget placed
to the side of the grid falls off-screen in a portrait window — which is why the compass sits south
of the grid. Changing either constant means re-checking the framing.

Known rough edges that are not spec divergences: the release wasm bundle is ~51 MB plus ~11 MB
of assets, untuned; and the committed cubemap is 10.8 MB of binary in git history.

## Related

- [[hex-grid]]: what the scene currently displays
- [[terrain]]: what the named scenes hold, and what `two-lakes` was built to show
- [[skybox-pipeline]]: how the cubemap asset is generated
- [[bevy-0-19-api]]: the API facts this implementation relies on
- [[build-performance]]: why the dependency is target-scoped, with the measurements
