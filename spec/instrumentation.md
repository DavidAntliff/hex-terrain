---
tags: [instrumentation, tooling, camera, screenshot, spec]
type: spec
status: implemented
updated: 2026-08-13
---
# Spec: Scripted observation

How the app is driven and read by something that is not a person: aiming the camera, capturing a
batch of frames, and dumping the scene's state as data. The scene shell itself is [[scene]]'s
business; this is the instrumentation bolted to it.

## Requirements

### Goal (definition of done)

A single non-interactive invocation can aim the camera at any pose, produce one image per pose, and
write a machine-readable record of what the scene contained at each capture — and `cargo run` with
no environment set behaves exactly as it did before.

The two failures this exists to prevent, both previously real:

- Reaching a view meant **editing `Orbit::default()` in source and restoring it** afterwards. See
  [[log]] → the daylight sky entry, where that is recorded as the way a low-pitch screenshot was
  obtained.
- A screenshot shows a symptom and never a cause. "The water is missing" and "the water plates
  carry no vertices" are the same image, and telling them apart cost another run.

### Constraints

- **The app stays human-runnable.** This is an additional mode, not a new front door. Every
  variable is absent by default, and when all are absent the plugin does not even register a system.
- **Web parity**, per [[scene]]. Everything must build for `wasm32-unknown-unknown`.
  `std::env::var` compiles there and returns `Err`, so the whole mechanism self-disables.
- **Env vars configure a mode; argv picks what to run.** [[scene]] reasons this out for the scene
  name. Aim, capture and report are modes, so all are variables.
- **The camera orbits the origin only.** A pose is therefore exactly three numbers.
- **Existing invocations keep working.** `HEX_TERRAIN_SCREENSHOT=<path>` alone must still write
  exactly that path, with no index inserted.
- **No CLI crate.** Parsing stays hand-rolled; see Design discussion.

### Functional requirements

In scope: naming a camera pose from outside the app, capturing several frames per run, sampling
over time for animated content, pinning the window size, and reporting scene state as JSON.

Out of scope, deliberately: camera interpolation, a headless renderer, capturing log output, and
any in-app UI for any of this.

## Design discussion

**Poses are named as well as numbered.** A preset table (`top`, `iso`, `low`, `fit`) covers what is
actually asked for, and `yaw,pitch,radius` in degrees covers the rest. Names because the common
request is "show me it from above", not a triple; numbers because a fixed table cannot reach an
arbitrary angle. Both are ten lines. Locked.

**Angles are degrees at the boundary, radians inside.** The conversion happens once in
`parse_pose`, so `place()` and every existing caller keep working in radians and nothing else in the
codebase learns about degrees. Locked.

**Out-of-range values are clamped, not rejected.** A pitch of 120° means "as far over as the camera
goes". This is what `orbit` and `framing::reset_view` already do to values arrived at by dragging,
and a script that overshoots wants a picture rather than an error. An unparseable *name* is still
fatal — that is a typo, not an overshoot. Locked.

**`fit` is deferred rather than computed.** Framing the whole scene needs the live projection and
aspect ratio, which `framing::reset_view` already has; `Pose::Fit` sets `ResetViewRequested` and
lets it run. Duplicating `content_half_extent`/`framing_distance` here would be a second copy of a
thing with five tests on it. Locked.

**Rejected: camera interpolation between poses.** Asked for during design and declined. The
scripted path reads still images; a tween is only ever visible to a human watching in real time, and
a flythrough is a demo feature rather than a diagnostic one. Discrete poses give the same coverage
in fewer frames. Revisit only if a human-facing demo is wanted, at which point it belongs beside the
camera controls and not here.

**Rejected: a headless renderer.** `ScheduleRunnerPlugin` with no window would make runs independent
of the window manager, which is the one real weakness of the current approach. But the whole reason
this captures its own framebuffer is that it needs a real GPU surface — capturing the window through
the X server is unreliable, since a window on an inactive workspace is unmapped and yields a blank
image. Rendering to an offscreen target of fixed size is the proper fix and is a much larger change.

**The report is JSON, with typed structs.** Flat `key = value` text was the first proposal and was
rejected as imprecise: nesting, arrays and nulls all have to be encoded by hand, and the escaping is
a liability. The types are derived rather than assembled with `serde_json::json!` so the schema is
self-documenting and cannot drift silently as fields are added. Locked.

**`serde` is the one dependency exception**, agreed explicitly against [[scene]]'s "one dependency"
constraint, which has been amended to say so. The distinction that makes it worth it: a typed schema
for machine-read output is load-bearing in a way a camera controller or an argument parser was not —
those were behaviour worth a few dozen lines, this is a contract. **No CLI crate**: one pose string
and one scene name are the whole interface, and `clap` would earn nothing.

**The serialisation types live in their own module** (`src/probe/report.rs`), and nothing in
`src/hex/` or `src/view/` derives `Serialize`. This is the same boundary `CLAUDE.md` draws for
`Resource`: the model stays a plain dimensionless model and the report owns both the shapes and the
conversion into them. It is also why the module is a directory — the plugin and the schema are
separate concerns that happen to ship together.

**One report file per shot, not one array per run.** A run that dies on shot 3 still leaves shots
0–2 readable. To stdout the same documents go out one per line, which is JSON Lines and parses as it
streams.

**The report is the index.** Filenames carry only a running counter (`-00`, `-01`); which pose and
which tick produced each is in the report's `run` object. No encoding scheme has to be invented for
the filename, and the counter stays correct however poses and ticks multiply out.

**Window size is a request, and the report records what was actually rendered.** Pinning matters
because an image diff between two screenshots of different sizes measures the window manager rather
than the scene. Two things had to be true for the pin to hold, and a third could not be made true:

- **Equal min/max size hints.** A tiling WM gives a tiled window whatever its layout dictates.
  i3 auto-floats a window whose minimum and maximum sizes are equal, which takes it out of the
  layout — verified, and it is what makes the requested aspect ratio hold exactly.
- **A scale-factor override**, so the resize constraints are interpreted in a fixed
  logical-to-physical ratio.
- **The numbers are still logical pixels**, and the PNG comes out at that times the display's scale
  factor. This is not adjustable from the `Window` descriptor; see [[bevy-0-19-api]] for the call
  that does it. Rather than pretend, the report carries the real physical size, so a script can
  *check* that two runs are comparable instead of assuming it.

## Implementation details

`src/camera.rs` — pose naming, beside `Orbit` because presets are camera knowledge:

- `Pose { Fit, At(Orbit) }`, `parse_pose(&str) -> Option<Pose>`, `pose_names()` for the error
  message. `PRESETS` is a `&[(&str, f32, f32, f32)]` table in degrees, following the `scenes::SCENES`
  idiom.
- `iso` restates `Orbit::default()` in degrees, so a test pins the two together.

`src/probe/mod.rs` — the plugin and the sequencer:

- `ProbePlugin::build` returns immediately unless one of `HEX_TERRAIN_CAMERA`,
  `HEX_TERRAIN_SCREENSHOT` or `HEX_TERRAIN_REPORT` is set. `FrameTimeDiagnosticsPlugin` is added
  only when a report was asked for.
- `Shot { pose, name, tick, delay }`; `build_shots` is the poses crossed with the ticks, poses
  varying slowest. `Schedule` holds the list, the cursor and the output paths.
- `run_schedule` is the state machine: settle `SETTLE_FRAMES` (120) → aim → wait → capture → advance
  → wait `SAVE_FRAMES` (60) for the asynchronous PNG write → `AppExit::Success`. `REPOSE_FRAMES` (8)
  covers what depends on the view after a pose change — the shadow cascade re-rendering, reflections
  settling — and is short because nothing is loading by then.
  - It takes a **`ParamSet`**: aiming needs the camera mutably and the report needs it immutably,
    which Bevy rejects as conflicting even though a pose is always followed by a wait and the two
    never happen on one frame.
  - The realised gap between captures is `interval + 2` frames, the two being the state machine's
    own advance and capture steps. Sampling an animation does not care, so it is not corrected.
- `indexed_path` inserts `-NN` before the extension, and only when there is more than one shot.
- A bad *value* prints what was valid and exits 2, following `main::named_scene`. Absence is never
  an error.

`src/probe/report.rs` — the schema. `Report { run, window, camera, layout, model, render,
diagnostics }`, `ReportSources` as a `SystemParam` gathering what it reads, and `Report::collect`.
Load-bearing details:

- `render` separates `cells` / `walls` / `water`, each `{ entities, vertices, triangles }`.
  **`entities` is what separates "nothing was spawned" from "something was spawned and it is
  empty"** — different bugs that look identical in a PNG. Triangles come from the index buffer where
  there is one, because an indexed mesh's vertex count says nothing about how many triangles it
  draws.
- A cell's cap carries no marker of its own — it is the meshed child that is neither `HexWall` nor
  `WaterSurface` — so it is found through `Children` on `HexCell`.
- `model.water_levels` is the **distinct levels, ascending**: a lower bound on the number of bodies,
  not the count. `grid_render::water_plates` knows the real partition but computes it per location
  and does not expose it, and duplicating it here was not worth it.

`src/main.rs` — `pinned_resolution` and `size_hints` read `HEX_TERRAIN_WINDOW`; the window is
`main`'s to configure, and the pin is useful with nothing else set.

The interface, all optional:

| Variable | Value | Effect |
|---|---|---|
| `HEX_TERRAIN_CAMERA` | `;`-separated poses | Aims the camera; one capture per pose. |
| `HEX_TERRAIN_SCREENSHOT` | path | A PNG per capture. |
| `HEX_TERRAIN_REPORT` | path, or `-` for stdout | A JSON report per capture. |
| `HEX_TERRAIN_INTERVAL` | `<frames>x<count>` | Capture `count` times per pose, `frames` apart. |
| `HEX_TERRAIN_WINDOW` | `<W>x<H>` | Pins the window, in logical pixels. |

## Verification plan

Performed.

`cargo test` — 75 tests, up from 66. The nine new ones are pure, with no `App`:

- Every name `pose_names()` advertises parses; `fit` reaches `Pose::Fit`; case and surrounding space
  are tolerated, both being easy to introduce from a shell.
- `yaw,pitch,radius` is read as degrees and converted to the radians `place` expects.
- `top` is exactly `TOP_DOWN_PITCH`, the pole `place` is built to survive.
- `iso` agrees with `Orbit::default()`, so the table cannot drift from the view the app opens with.
- Pitch and radius clamp at both ends; yaw does not clamp, being an angle about a full circle.
- Garbage returns `None`, including the two-field, four-field and empty cases.
- `indexed_path` puts the index before the extension, and appends where there is no extension, where
  the only dot is in a directory earlier in the path, and for a dotfile.
- `parse_interval` defaults to one capture, tolerates spacing and case, and floors the count at 1.
- `build_shots` crosses poses with ticks, poses slowest, first tick waiting on the repose.

Native runs, on Vulkan (RTX 3050, driver 580.142), i3:

- `HEX_TERRAIN_CAMERA='top;iso;low;fit'` with screenshot and report over `two-lakes` — four PNGs and
  four reports; each report's `camera` matches the pose that produced it (90°/51.57°/8°/90°), all
  four pass `python3 -m json.tool`. `top` shows both lakes from overhead; `low` gives the grazing
  view of the horizon haze — **the view that previously required editing the default in source**.
- `HEX_TERRAIN_REPORT=-` over `terraces` — two JSON Lines records, each independently parseable,
  reporting three distinct water levels `[-0.3, 0.0, 0.55]`, which is what that scene is built to
  hold. `image` is `null` when no screenshot was asked for.
- `HEX_TERRAIN_INTERVAL=30x4` over `sea` — captures at frames 130, 162, 194, 226. Consecutive pairs
  differ by ~27–30k pixels and the first and last by ~50k, the difference confined to the water: the
  ripples are animating and drift further apart with time.
- **Comparability.** Two identical runs at a pinned 1280x720 both rendered 2560×1440 and differed by
  6667 pixels of 3.69M (0.18%) — the ripple floor recorded in [[log]], not a whole-frame difference.
  Without the pin the WM gave 3840×2320 on one invocation and 2392×845 on another, which is the
  failure the pin removes.
- Backwards compatibility — `HEX_TERRAIN_SCREENSHOT=<path>` alone writes exactly that path, no index.
- A plain `cargo run` logs nothing from `probe` and is unchanged.
- Bad values — `HEX_TERRAIN_CAMERA=nonsense` exits 2 listing `top, iso, low, fit`; bad `INTERVAL`
  and `WINDOW` values print their expected form.

Web — `cargo check --target wasm32-unknown-unknown` passes, and `trunk build --release` succeeds.
The variables are inert there.

**Not verified**: the browser at run time was not re-checked for this change (the mechanism is
inert on web, and [[scene]] covers the rendering path itself); behaviour under any window manager
other than i3; and whether `HEX_TERRAIN_WINDOW` yields exact physical pixels on a non-HiDPI display,
where the scale factor is 1 and the question does not arise.

## Implementation status

**status:** implemented — spec and code agree. No known divergences.

Deliberate omissions, each marked with a `ponytail:` comment at the site:

- No interpolation between poses; they are discrete.
- `model.water_levels` counts distinct levels rather than bodies, because the partition is not
  exposed.
- Log output is not captured into the report. `stderr` already reaches any script that ran
  `cargo run`, so this only becomes worth doing if a failure proves invisible there.

Known limitation, not a divergence: `HEX_TERRAIN_WINDOW` is logical pixels, so the image comes out
scaled by the display. The report carries the physical size, which is what makes this checkable
rather than surprising.

## Related

- [[scene]]: the shell this instruments, and where the screenshot mechanism used to be specified
- [[bevy-0-19-api]]: the window-sizing and query-conflict facts this relies on
- [[hex-grid]]: the debug panel, which is the interactive counterpart to the report
- [[terrain]], [[water]]: what the `model` and `render` sections of a report describe
