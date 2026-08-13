---
tags: [camera, controls, input, spec]
type: spec
status: implemented
updated: 2026-08-13
---
# Spec: Camera controls

How the camera is moved, by hand and from a script. Promoted out of [[scene]], which now owns only
the shell the camera lives in. What the camera *looks at* is other specs' business — see
[[hex-grid]] and [[terrain]].

This is a **debugging instrument**, not the game's camera. Its job is to let a person or a script
get an eye anywhere in the scene, quickly, using controls nobody has to learn.

## Requirements

### Goal (definition of done)

Holding the right mouse button flies the camera with `WASD`/`QE` and mouse look; releasing it
leaves the camera where it was flown to. Middle-drag turns the view about whatever the cursor was
over when the drag began, without the view snapping to face it. Shift+middle-drag pans, the wheel
zooms towards the cursor, left-click still selects a hexagon, and `HEX_TERRAIN_CAMERA` can name any
eye point and look-at point in the scene. No mode is switched, at any point, by anything.

### Constraints

- **No camera mode.** There is no state that says which camera is in charge, no toggle, and
  nothing to get stuck in. The controls are transient: a button is held or it is not.
- **The `Transform` is the camera's state.** `Orbit` describes where the camera is; it never
  drives it except when a pose is *commanded* (startup, a scripted pose, reset view).
- **Left-click stays selection.** Nothing in the camera may take the plain left button.
- **Web parity** — everything here must build and work for `wasm32-unknown-unknown`, per [[scene]].
- **No third-party camera crate.** A Bevy feature is not a third-party crate; see Design discussion.
- **No binding the window manager eats.** See the Alt discussion below — this is a real constraint
  here, not a hypothetical.

### Functional requirements

In scope: the bindings, the pivot, the scripted pose grammar's free form, and how the two
controllers are kept out of each other's way.

Out of scope: interpolation between poses (rejected in [[instrumentation]]), collision with the
terrain, a gameplay camera, and any in-app UI for the camera — there is nothing for it to control.

## Bindings

| Input | Action |
|---|---|
| **Left click** | select a hexagon |
| **Right drag** | fly: mouse looks, `WASD` moves, `E`/`Q` up and down, `Shift` runs, wheel changes speed |
| **Middle drag** | turn about the point under the cursor |
| **Shift + middle drag** | pan |
| **Wheel** | zoom towards the cursor |
| **Escape** | quit |

## Design discussion

**Transient controls, not modes.** The original design had a mode toggle in the debug panel
switching between an orbit camera and a free camera. Rejected: with the pivot design below, the two
are the *same* camera differing only in which button is down, so a mode would be a state variable
with no state in it — plus a widget, a keybinding, and a class of "why won't it move" bugs. Every
editor with a flythrough does it transiently. Locked.

**The right button flies.** Unity, Godot and Unreal all bind hold-right-button to flythrough; it is
also `FreeCamera`'s own default `mouse_key_cursor_grab`. The camera previously orbited on
right-drag, so orbiting had to move. Taking the widely-used binding for the widely-used meaning is
worth more than preserving one project's habit, particularly for a debugging tool. Locked.

**Turning is on the middle button, not Unity's `Alt`+left.** `Alt`+left is the natural pair to
right-to-fly, and it was bound as an alias at first — but **i3 sets `floating_modifier Mod1`**, and
it was then confirmed by hand that the drag never reaches the app. Not a quirk of this setup: `Alt`
or `Super`+drag is the near-universal Linux window-move binding, so an application cannot rely on
it. Middle-drag is Blender's binding, equally standard, and no window manager wants it. The alias
was **removed** rather than left in place, along with the `Alt` guard it needed in
`select_on_click` — a documented binding that silently does nothing is worse than one binding that
works. Locked.

**The pivot is latched at the press.** Re-picking it each frame as the cursor moves makes the pivot
slide out from under the camera, and a rotation degrades into a slow crawl across the scene. One
pick, at the press, held for the drag. The wheel is the exception: it re-picks every event, which is
exactly what makes zoom converge on what the cursor is over.

**`rebase` is a read-back, not a write-back.** This is the load-bearing decision, and the one that
is easy to get wrong. The obvious cursor-pivot orbit re-derives `yaw`/`pitch`/`radius` about the new
pivot and then writes `place()`. But `place` always points the camera **at** its target, so the view
**snaps** the moment the pivot is off centre — which, with the pivot taken from under the cursor, is
most of the time. Near a screen edge the snap is tens of degrees and the control is unusable.

Instead every interaction manipulates the `Transform` directly — `turn_about` rotates position *and*
orientation about the pivot, so whatever you were looking at stays where it was — and `Orbit` is
read back out afterwards by `rebase`. `rebase` is the exact inverse of `place`'s translation, so
nothing drifts, and this is what lets flying and turning compose with no mode: fly anywhere, then
turn about the cursor from there. Locked, with a round-trip test pinning the inverse.

A consequence worth stating: `rebase` **clamps nothing**. A lossy inverse would leave the reported
pose disagreeing with the camera, and after a flight the camera can genuinely be beyond
`MAX_RADIUS`. The limits belong on the input that moves the camera, not on the description of where
it ended up.

**Pitch is limited by testing the step, not the pose.** The first attempt refused a turn when the
view direction came within a whisker of vertical. That is wrong: past the pole the view is *equally*
steep, so the test cannot tell just-short-of-vertical from just-past-it, and a fast drag sails
through and lands upside down. The working test is whether the step would put the camera's up vector
below the horizon. The yaw is kept when the pitch is refused, so a diagonal drag still swings round.

**Pan is computed, not tuned.** At the pivot's distance the viewport is `2·d·tan(fov/2)` world units
tall, so a one-pixel drag moves the camera by exactly one pixel's worth of ground. The ground keeps
up with the cursor at any zoom, and there is no sensitivity constant to re-tune when the projection
changes. The pivot travels with the camera; left behind, it would be off screen by the end of the
pan and the next turn would swing about a point nobody can see.

**`free_camera` is a Bevy feature, not a dependency.** [[scene]]'s one-dependency constraint bans
third-party camera-controller crates. `bevy_camera_controller` is first-party, enabled by
`features = ["free_camera"]` on the `bevy` dependency already present, so the constraint holds
unchanged. [[scene]] previously recorded these controllers as rejected — correctly, for *orbiting a
fixed target*, which is still hand-written here. Flying is the part they do well and there is no
reason to write it twice.

**The controller is gated rather than left running.** `FreeCamera` adjusts its fly speed on **every**
scroll event, whether or not the mouse is grabbed — so left enabled it would silently ramp the speed
while the wheel was zooming — and its movement keys would answer `WASD` when the camera is not being
flown. `fly_on_right_button` sets `FreeCameraState::enabled` from the right button, which makes it
inert otherwise and releases the cursor grab for us. Two useful side effects: its `M` cursor-grab
toggle and its Numpad axis snaps become unreachable, which is what a transient-only design wants.

**Gating another plugin's systems has to be ordered against the input, not just against them.**
`fly_on_right_button` was first registered as a bare `PreUpdate` system. `PreUpdate` is correct —
`RunFixedMainLoop` follows it, so the controller sees the flag on the frame it is set — but systems
within a schedule are unordered, so it could run *before* `bevy_input` had processed the frame's
buttons and therefore read the previous frame's state. The controller was then enabled exactly one
frame late, which is one frame too late for the `just_pressed` that its cursor grab hangs on.

**The failure was quiet, and that is the part worth remembering.** Everything driven by `pressed`
— `WASD`, `Q`/`E`, `Shift`, the wheel — worked perfectly; only mouse-look never engaged, because it
is the one behaviour behind an edge rather than a level. Nothing warned, and it was found by a
person flying the camera. `.after(InputSystems)` is the fix.

**Yaw and pitch are re-seeded on the press.** `FreeCamera` latches its own `yaw`/`pitch` from the
transform exactly once, on its first run. After a turn drag or a scripted pose its copy is stale and
the view would jump back to it the moment the mouse moved. One re-seed at the press fixes it; there
is no way to ask the controller to do this itself.

**The free pose is not a second kind of pose.** `free:x,y,z@tx,ty,tz` resolves through `rebase` into
an ordinary `Orbit`, so `Pose` gains no variant, `place` gains no branch, the probe's aiming code is
untouched, and the report stays meaningful. It also inherits `place`'s definition at the poles,
which is exactly the case a hand-written eye point tends to ask for (`free:0,10,0@0,0,0`).

## Implementation details

All in `src/camera.rs` except the ray, which is `src/view/selection.rs`'s.

- `Orbit { yaw, pitch, radius, target }` — a camera position in spherical coordinates about
  `target`. A component on the camera, and the type a scripted pose parses to.
- `place(&Orbit) -> Transform` — the commanded direction. Builds the rotation directly rather than
  through `looking_at`, which has no valid up vector looking straight down, so a pitch of exactly
  ±π/2 is legal. Used by `setup`, the probe's aim, and `framing::reset_view` — **never** by the
  interactive controls.
- `rebase(position, target) -> Orbit` — the inverse. Guarded at the pole, where the division can
  overshoot 1.0 by an ulp and `asin` returns NaN, and where yaw is undefined.
- `orbit` — the `Update` system for turning, panning and zooming. Returns immediately while the
  right button is down, and **writes the `Transform` only when something moved**: that is what lets
  a scripted pose survive the frame it was set in, and what keeps it out of `FreeCamera`'s way.
- `turn_about` — rotation about the pivot, position and orientation together. Yaw about world `+Y`,
  pitch about the camera's own right axis, which is what makes a vertical drag pitch rather than
  roll however far round the view has been swung.
- `Pivot(Vec3)` — a resource, latched at the press. Defaults to the origin, matching
  `Orbit::default().target`.
- `fly_on_right_button` — registered as **`PreUpdate`, `.after(bevy::input::InputSystems)`**. Both
  halves are load-bearing and one of them was got wrong first time; see below. It also zeroes the
  leftover velocity on release, which would otherwise be re-applied as a lurch on the next press.
- `parse_pose` — a preset name, `yaw,pitch,radius` in degrees about the origin, or
  `free:x,y,z@tx,ty,tz` in world units. See [[instrumentation]].
- `view::selection::pick_surface` returns the hit **point** as well as the coord; `pick_point` adds
  the fallbacks — the grid plane beyond the grid's edge, then the previous pivot for a ray aimed at
  the sky. One ray routine serves both selection and the pivot rather than two that can disagree
  about what was hit.

Values, all in `src/camera.rs`: `LOOK_SENSITIVITY` 0.005 rad/px, `ZOOM_SENSITIVITY` 0.1 per notch,
`MIN_RADIUS` 3, `MAX_RADIUS` 200, `PIXELS_PER_LINE` 50 (browsers report pixel deltas roughly 50×
a desktop mouse's line deltas — without it, zoom is unusable in a browser while feeling fine
natively). `FreeCamera`'s `walk_speed`/`run_speed` are set to 3 and 9 in `main::setup`, against
defaults of 5 and 15 that cross this seven-unit grid in under a second.

API specifics are recorded in [[bevy-0-19-api]].

## Verification plan

Performed:

- `cargo test` — 80 tests, up from 75.
  - `rebase_inverts_place` — the invariant the no-mode design rests on, over two targets and six
    poses including both poles and a radius beyond `MAX_RADIUS`.
  - `turning_about_an_off_centre_pivot_does_not_re_aim_the_camera` — the pivot distance is kept, the
    heading turns by the angle asked for, and the view does **not** end up facing the pivot. This is
    the snap the design exists to avoid, so it is pinned rather than left to inspection.
  - `the_view_cannot_be_turned_past_vertical` — twenty steps into each pole; the up vector stays
    above the horizon and the view still reaches vertical rather than the guard freezing it short.
  - `a_free_pose_is_an_eye_point_and_what_it_looks_at`, plus seven malformed `free:` forms in
    `garbage_is_rejected`.
  - `a_pivot_falls_back_to_the_plane_and_then_to_the_last_one` — the two fallbacks in `pick_point`.
  - `pick_surface`'s existing tests extended to check the returned point, not only the coord.
- Scripted — `HEX_TERRAIN_CAMERA='iso;free:12,6,-12@0,0,0;free:2.5,1.2,2.5@0,0.3,0'` against
  `two-lakes` at 1280×720. Three images; reported `translation` matches each requested eye point to
  float precision, and `target` matches. The third is a view from ground level between two prisms,
  which was not reachable at all before this change.
- `cargo clippy --all-targets` — clean.

By hand, since there is no key-injection tool here ([[scene]] records the same limitation) — a
person flew the camera and reported back:

- Middle-drag turn, Shift+middle-drag pan, wheel zoom and left-click selection all behave. The
  snap-free turn about an off-centre pivot holds in practice, not only in the round-trip test.
- Fly on right-drag: `WASD`, `Q`/`E`, `Shift` and wheel-for-speed all behave.
- **`Alt`+left-drag never arrived**, as suspected. The alias was removed rather than documented.
- **Mouse-look did not work at all**, which is what exposed the ordering bug above.

Still outstanding:

- Mouse-look after the ordering fix, and whether it feels un-inverted at the shipped sensitivity.
- The browser: whether right-button capture is usable there, or raises the context menu on the
  canvas.

## Implementation status

**status:** implemented — spec and code agree, with the hand-verification above outstanding.

Deliberate omissions:

- No collision. The camera flies through terrain and can end up inside a prism, which for an
  inspection tool is a feature.
- No pose interpolation — see [[instrumentation]].
- A left-click while the cursor is grabbed selects whatever is at the locked cursor position. Left
  alone rather than special-cased; it is harmless and arguably useful as a crosshair.
- `reset_view` (`fit`, and the panel button) returns the target to the origin. `content_half_extent`
  measures the grid symmetrically about `layout.origin`, so framing anything else would need a
  different measurement, and there is nothing asking for one.

## Related

- [[scene]]: the shell the camera lives in, and where this was promoted from
- [[instrumentation]]: naming a pose from a script, and the report the camera is recorded in
- [[hex-grid]]: click selection, which shares the ray this pivots on
- [[bevy-0-19-api]]: `FreeCamera`'s bindings and the schedule ordering this relies on
