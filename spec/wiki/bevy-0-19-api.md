---
tags: [bevy, api, concept]
type: concept
updated: 2026-08-12
---
# Bevy 0.19 API facts

Facts verified against the vendored Bevy 0.19.0 source in the cargo registry, not recalled from
older versions. Bevy renames things freely between releases and 0.17–0.19 moved several items
this project uses. When in doubt, read the registry source: it is on disk and authoritative,
which is faster and more reliable than searching for documentation of the wrong version.

## Events are now Messages

The buffered-event API was renamed. There is **no `EventWriter`** in 0.19 — the type is
`MessageWriter`, and the method is `write`, not `send`:

```rust
fn exit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}
```

`AppExit` derives `Message` (`bevy_app/src/app.rs`) with variants `Success` and
`Error(NonZero<u8>)`. `App::should_exit() -> Option<AppExit>` reads the message buffer, which
makes exit behaviour testable headlessly without a window.

## Skybox

`Skybox` lives in `bevy::light` and its image field is **optional**:

```rust
pub struct Skybox {
    pub image: Option<Handle<Image>>,
    pub brightness: f32,
    pub rotation: Quat,
}
```

A PNG carries no cubemap metadata, so a loaded stacked strip must be reinterpreted before it
can be used — the idiom is in Bevy's own `examples/3d/skybox.rs`:

```rust
image.reinterpret_stacked_2d_as_array(layers).expect(...);   // returns Result
image.texture_view_descriptor = Some(TextureViewDescriptor {
    dimension: Some(TextureViewDimension::Cube),
    ..default()
});
```

The skybox does **not** light the scene; `EnvironmentMapLight` is a separate component.

**A cubemap built in code needs none of the reinterpretation above.** Construct the `Image` with
six array layers and set `texture_view_descriptor` at construction, and there is no load state to
poll, no re-upload guard, and no bind-group nudge — the whole `patch_cubemap` idiom disappears.
`Rgba16Float` is the format to reach for: it is what Bevy's own `hemispherical_gradient_cubemap`
uses, and the one HDR format WebGL2 filters without an extension.

Bevy will silently draw **no** skybox rather than fail if the image is missing from
`RenderAssets<GpuImage>`. It warns only about a wrong view dimension
(`sanity_check_skybox_image_and_warn`), so a black sky is not evidence of a broken cubemap — check
what the texels actually contain before suspecting the pipeline. A **uniform** debug texture cannot
distinguish a pipeline fault from a sampling one, since it looks correct however it is sampled.

## Environment maps need no KTX2

`EnvironmentMapLight` lives in `bevy_light`, and 0.19 ships two constructors that build the map on
the CPU — so the `toktx`/`basisu` prefiltering step is **not** required to light a scene from a sky:

```rust
EnvironmentMapLight::solid_color(&mut images, color)
EnvironmentMapLight::hemispherical_gradient(&mut images, top, mid, bottom)
```

The gradient is a **1×1×6** cubemap — ideal as an environment map, and visibly faceted if used as a
skybox, so it is not a substitute for one.

`GeneratedEnvironmentMapLight` and `AtmosphereEnvironmentMapLight` will filter an arbitrary cubemap
at runtime, which is the better-looking answer — but both go through
`EnvironmentMapGenerationPlugin`, which **disables itself where compute is unavailable**
(`light_probe/generate.rs`, logging "Disabling EnvironmentMapGenerationPlugin because compute is
not supported on this platform"). That means not on WebGL2.

## What WebGL2 rules out

Verified against the 0.19 source, because each of these looks available until it isn't:

- **Screen-space reflections.** `ScreenSpaceReflections` exists in `bevy_pbr::ssr` and
  `examples/3d/ssr.rs` is a water demo, but the component's own doc says: *"Screen-space reflections
  are presently unsupported on WebGL 2 because of a bug whereby Naga doesn't generate correct GLSL
  when sampling depth buffers."* It also requires deferred rendering, which forfeits
  `specular_tint` and `diffuse_transmission`.
- **Sampling the depth prepass texture** at all, for the same Naga reason.
- **Procedural `Atmosphere`.** `bevy_pbr/src/atmosphere/` builds its LUTs with compute pipelines.
  `AtmosphereSettings` also `#[require(Hdr)]`.
- **Runtime-generated environment maps**, per the section above.

There is no `ScreenSpaceReflectionsBundle` in 0.19 — bundles were replaced by required components.

## Material extensions

`ExtendedMaterial<B, E>` and `MaterialExtension` are in `bevy::pbr` but **not in its prelude**.
`ShaderRef` moved with the 0.19 crate split and is now `bevy::shader::ShaderRef`, not in
`bevy_render::render_resource`.

- Bindings **start at 100**; 0-99 belong to the base material.
- Every shader entry point defaults to the base material's, so an extension can override
  `fragment_shader()` alone and inherit the prepass and deferred paths.
- `opaque_render_method()` and `reads_view_transmission_texture()` come from the **base only** — an
  extension cannot opt into transmission.
- The reference to copy is `examples/shader/extended_material.rs`; `examples/3d/ssr.rs` is the same
  pattern applied to water.

A mesh attribute the base material ignores is a free channel to the fragment shader: an untextured
`StandardMaterial` never reads `uv`, and `VertexOutput` carries it whenever the mesh has `UV_0`, so
a per-vertex scalar needs no custom attribute and no `specialize()` override.

## Exposure and photometric units

`Exposure` defaults to `EV100_BLENDER` (9.7), several stops darker than daylight. `Exposure::SUNLIGHT`
is `ev100: 15.0`, and `bevy::light::light_consts::lux` names the matching illuminances —
`DIRECT_SUNLIGHT` is 100 000, an order of magnitude above `AMBIENT_DAYLIGHT`. With those two set
together, `Skybox::brightness` and `EnvironmentMapLight::intensity` are both `1.0` for a sky
expressed in cd/m², and no level needs tuning against another.

## Mouse input

`AccumulatedMouseMotion { delta: Vec2 }` and `AccumulatedMouseScroll { unit: MouseScrollUnit,
delta: Vec2 }` are resources reset every frame — no message reader needed for camera control.

**`unit` is not decoration.** Browsers report `MouseScrollUnit::Pixel` with deltas roughly 50×
larger than the `Line` deltas a desktop mouse produces. Code that ignores `unit` feels correct
natively and is unusable on the web.

## Camera controllers

0.19 ships first-party controllers in `bevy_camera_controller`, feature-gated as `free_camera`
and `pan_camera`. **Neither orbits a target**, so an orbit camera is still hand-written. The
crate's own docs suggest copying a controller and modifying it when the provided behaviour
doesn't fit. `pan_camera` is 2D only — `PanCamera` goes on a `Camera2d`.

Enabling either is `features = ["free_camera"]` on the `bevy` dependency, and `bevy` re-exports it
as `bevy::camera_controller::free_camera`. The `bevy_camera_controller` crate is not vendored into
the cargo registry until the feature is switched on, so its source cannot be read beforehand — until
then `examples/camera/free_camera_controller.rs` in the `bevy` crate is the only local
documentation, and it does not show the whole component.

**`FreeCamera` holds the settings, `FreeCameraState` the runtime state**, the latter a required
component of the former. Defaults: `WASD` to move, `E`/`Q` up and down (world axes, per
`VerticalMovementAxis::World`), `ShiftLeft` to run, **`MouseButton::Right` to grab the cursor while
held**, `KeyM` to toggle the grab, `Numpad1`/`3`/`7` (with `ControlLeft` reversing) to snap to an
axis, and the wheel scaling `speed_multiplier` exponentially. `walk_speed` 5.0 and `run_speed` 15.0
are world units per second, which is quick for a scene a few units across. Mouse look is
**non-inverted** as shipped: `pitch -= delta.y * …`, so moving the mouse forward looks up.

Three behaviours worth knowing before wiring it up, all read out of
`bevy_camera_controller-0.19.0/src/free_camera.rs`:

- **Scroll is consumed unconditionally.** `state.speed_multiplier *= exp(scroll_factor * scroll)`
  runs whether or not the cursor is grabbed, so a controller left enabled quietly ramps its fly
  speed while the wheel is doing something else entirely. `scroll_factor: 0.0` disables that, or
  gate the whole controller.
- **`FreeCameraState::enabled` is a clean off switch.** The system early-returns on it *after*
  releasing the cursor grab, and before reading scroll, keys or motion — so everything past that
  point, including the `KeyM` toggle and the Numpad snaps, is unreachable while it is false.
- **`yaw`/`pitch` are latched from the transform exactly once**, guarded by a private `initialized`
  flag on the first run. Move the camera by any other means afterwards and the controller's copy is
  stale, so the view jumps back to it on the next mouse motion. Both fields are `pub`, so re-seeding
  them from `transform.rotation.to_euler(EulerRot::YXZ)` is the fix.

**`FreeCameraPlugin` schedules its systems in `RunFixedMainLoop`**, not `Update`. The main schedule
order is `First → PreUpdate → RunFixedMainLoop → Update → SpawnScene → PostUpdate → Last`
(`bevy_app/src/main_schedule.rs`, `MainScheduleOrder::default`), so a system that must influence the
controller on the same frame as an input edge belongs in **`PreUpdate`** — from `Update` it is
always a frame late.

## Gating a plugin on input: order against `InputSystems`, not just the schedule

`bevy_input` updates `ButtonInput` in `PreUpdate`, inside the `InputSystems` set
(`bevy_input/src/lib.rs`). Systems within a schedule are otherwise **unordered**, so a `PreUpdate`
system reading `ButtonInput` without `.after(bevy::input::InputSystems)` may run first and see the
*previous* frame's state. Anything it then enables is enabled one frame late.

**One frame late is invisible for `pressed` and fatal for `just_pressed`.** A level survives the
delay; an edge is gone by the time the downstream system looks. Cost here: `FreeCamera`'s cursor
grab hangs on `just_pressed`, so gating it from an unordered `PreUpdate` system left every
`pressed`-driven behaviour — movement keys, run, wheel — working perfectly while mouse-look silently
never engaged. No warning, and a diff that looks right. If a gate feeds a `just_pressed` consumer,
order it after `InputSystems`.

## Gizmos

Gizmo line width and depth bias are **per config group**, not per call, so a scene needing both thin
and thick lines needs two groups: `#[derive(Default, Reflect, GizmoConfigGroup)]` plus
`app.init_gizmo_group::<T>()`, then `Gizmos<T>` as a system parameter.

**Coplanar gizmo lines z-fight with the mesh they trace.** Outlining a flat tile at the same height
as its face needs a negative `depth_bias`, or the lines lose the depth test. The failure is
direction-dependent and so easy to miss: at an oblique angle depth varies along each line and enough
of it wins to look correct, but from directly overhead an interior edge has a face on both sides at
identical depth and vanishes completely. What survives are the edges bordering empty space, so the
result looks like a correct silhouette with no internal structure.

**But `depth_bias` shifts normalized depth, which is steeply non-linear**, so the useful range is
far smaller than it looks. `-0.1` is a huge pull toward the camera: it is invisible against a flat
sheet with nothing to punch through, and the moment the scene gains any depth the same outlines
draw straight through the geometry standing in front of them. `-0.002` still beats a coplanar face
and stops there. Tune it against the deepest view, not the flattest.

## Cameras

**`Transform::looking_at` has no valid up vector at the poles.** A camera directly above its target
looking down has a view direction parallel to `+Y`, so `looking_at(target, Vec3::Y)` degenerates.
Building the rotation from yaw and pitch instead — `Quat::from_euler(EulerRot::YXZ, yaw, -pitch, 0)`,
with the position at `rotation * Vec3::Z * radius` — is well defined everywhere, allows a pitch of
exactly ±π/2, and yields north-up at the pole. It is also no more code than the `looking_at` version.

`Projection::Perspective(PerspectiveProjection { fov, aspect_ratio, .. })`: `fov` is the **vertical**
field of view, and Bevy's `camera_system` keeps `aspect_ratio` in step with the window. Both are
needed to frame content: the vertical extent depends on the field of view alone, the horizontal one
also on the aspect ratio — which is why a widget placed beside the subject falls off-screen in a
portrait window, and why a hand-tuned camera distance cannot be correct for all window shapes.

## Lights and shadows

**The field is `shadow_maps_enabled`, not `shadows_enabled`** (`bevy_light/src/directional_light.rs`).
It defaults to `false`, so shadows are off until asked for. `contact_shadows_enabled` is a separate
flag, and `soft_shadow_size` is behind the `experimental_pbr_pcss` feature.

**`CascadeShadowConfigBuilder` is in no prelude** — import it from `bevy::light`. Its defaults are
4 cascades (1 on WebGL2) reaching 150 world units, chosen to match Unity/Unreal/Godot. For a scene
a few units across that spends nearly all of the shadow map on empty space; one cascade over a
distance that actually bounds the scene is both sharper and identical to what the web build gets.

**Ambient light is two types.** `GlobalAmbientLight` is the resource; `AmbientLight` is a component
that `#[require(Camera)]`, i.e. per-view. Both are `{ color, brightness, affects_lightmapped_meshes }`.

## `StandardMaterial::depth_bias` will not stop z-fighting

Its own doc comment says it "affects render ordering and depth write operations using the
`wgpu::DepthBiasState::Constant` field" (`bevy_pbr/src/pbr_material.rs`). Only the first half is
true. The value is copied into the render-phase item's **sort key** (`bevy_pbr/src/material.rs`,
where `PreparedMaterial` is queued), while the mesh pipeline hardcodes

```rust
bias: DepthBiasState { constant: 0, slope_scale: 0.0, clamp: 0.0 }   // bevy_pbr/src/render/mesh.rs
```

so nothing reaches the rasterizer. Reordering two **opaque** draws changes nothing, since the depth
test decides. Two coplanar opaque meshes therefore z-fight however large a `depth_bias` is set, and
the fix has to be geometric: separate them by an epsilon. Gizmos are the exception — their
`depth_bias` is a real depth offset, which is why the grid outlines can be biased over the faces
they trace.

**Which way the epsilon points is a decision, not a detail.** Whichever mesh is nudged towards the
camera wins every exact tie. Water separated from terrain by lifting the *water* makes ground lying
exactly at its own water level read as submerged, which is not a rounding artefact but a visible
patch of zero-depth water over dry land. Nudging the water *down* instead gives the tie to the
ground. Either separation stops the z-fighting; only one of them is right.

## `WindowResolution` takes integers

`Window { resolution: (1600.0, 900.0).into(), .. }` does not compile. The `From` impls are
`(u32, u32)`, `[u32; 2]` and `UVec2` (`bevy_window/src/window.rs`) — logical pixels are integral
here. Write `(1600u32, 900u32).into()`. `WindowResolution` is **not in the prelude**, unlike `Window`
itself; import it from `bevy::window`.

### Asking for a size is not getting one

Pinning a size is what makes two screenshots pixel-comparable — without it a tiling window manager
hands out whatever geometry it likes and an image diff between two runs is meaningless. Setting
`resolution` alone does not achieve it. Three separate things are in the way, and only two can be
fixed from the `Window` descriptor:

- **The window manager.** A tiling WM ignores the requested geometry for a tiled window. The lever
  that works is `WindowResizeConstraints` with `min_width == max_width` and `min_height ==
  max_height`: i3 auto-floats a window whose minimum and maximum sizes are equal, which takes it out
  of the layout. Measured: without it, two invocations gave 3840×2320 and 2392×845; with it, both
  gave the requested aspect ratio exactly.
- **The scale factor**, which decides what units the resize constraints are read in.
  `WindowResolution::new` stores *physical* pixels but defaults `scale_factor` to 1.0;
  `with_scale_factor_override(1.0)` fixes the ratio so constraints and resolution are the same
  numbers.
- **The creation-time multiply, which cannot be fixed.** `bevy_winit/src/system.rs` calls
  `resolution.set_scale_factor_and_apply_to_physical_size(winit_window.scale_factor())`
  unconditionally on window creation, multiplying the requested physical size by the backend's
  factor **whether or not `scale_factor_override` is set**. On a 2× display, asking for `1280x720`
  yields a 2560×1440 framebuffer. So the request behaves as *logical* pixels, and the honest move is
  to read the size back — `window.resolution.physical_width()` / `physical_height()` — and record
  it, rather than assume the request held. See [[instrumentation]], whose report carries it.

## A `ParamSet` is the answer to B0001 across a `SystemParam`

A system that both writes a component and reads it through a bundled `SystemParam` panics at
startup with `error[B0001] … accesses component(s) … in a way that conflicts with a previous system
parameter`, even when the two accesses provably never happen on the same frame — Bevy resolves
conflicts from the signature, not from the control flow. Wrapping them as
`ParamSet<(Query<&mut T>, MyParams)>` and reaching through `params.p0()` / `params.p1()` is the fix;
a whole custom `SystemParam` nests inside a `ParamSet` unchanged. Note that the panic message is
useless by default — every name reads `<Enable the debug feature to see the name>`.

## Mesh primitives

`Extrusion<T: Primitive2d>` exists and `Extrusion<RegularPolygon>` is `Meshable` and `Into<Mesh>`
(`bevy_mesh/src/primitives/extrusion.rs`). Three things to know before reaching for it: it extrudes
along **Z** and is centred on the origin (`half_depth` either side), its cap comes from
`EllipseMeshBuilder`, which starts at `FRAC_PI_2` and so is always pointy-top in XY, and it is
closed at both ends. A Y-up prism therefore needs a rotation and an offset, a flat-top hexagon is
out of reach, and an open-ended prism is not expressible at all. Rejected for the hex terrain, whose
caps and walls come from `corner_offsets` instead.

For a hand-built mesh, **wind each face so that `(v1 - v0) × (v2 - v0)` is the normal it stores** —
that cross product is what back-face culling compares against. `cull_mode: None` hides the mistake
completely, so a mesh built under it can be wound backwards for as long as it stays double-sided
and will disappear the moment culling is turned on. A cheap unit test comparing the two per triangle
catches it without a renderer.

**Scaling a mesh to zero on one axis renders it black.** The normal transform is the inverse
transpose of the model matrix, which a zero scale makes degenerate, so the shading normals come out
unusable. Anything that drives a scale component from data will eventually feed it a zero — a height
field does so the moment a value lands exactly on its base plane. Keep data out of the scale where
you can (a height is better expressed as a vertex position), and floor the scale at a small positive
value where you cannot.

## UI widgets

`bevy_ui_widgets` ships **headless** widgets — behaviour and accessibility, no drawing:

- `Button` emits `Activate { entity }`, observable with `On<Activate>`. Keyboard activation is
  included, so a focused button responds to Enter and Space.
- `Checkbox` emits `ValueChange<bool> { source, value, .. }` and reads its state from the `Checked`
  component. It computes the next value from `Has<Checked>`, so **`Checked` must be maintained** or
  every click reports `true`. Add the provided `checkbox_self_update` observer to the entity for
  that, and set the initial `Checked` yourself. Rendering the box is the caller's job.

- `Slider` reports `ValueChange<f32>` and keeps `SliderValue` up to date through the provided
  `slider_self_update` observer, exactly like `Checkbox`. It requires `SliderValue`, `SliderRange`
  and `SliderStep`, and expects a descendant marked `SliderThumb` — but it **never moves the
  thumb**: positioning it is the caller's, and `SliderRange::thumb_position(value)` gives the 0..1
  fraction to do it with. Reduce the travel by the thumb's own width or it overhangs the ends.

Both use `bevy_picking` under the hood, which is what makes a UI click also reachable by
world-space picking code — see `Hovered` below.

## Change detection counts insertion as a change

`Res<T>::is_changed()` is true on a system's first run, because inserting the resource sets its
changed tick along with its added tick. A system gated on `if !thing.is_changed() { return; }`
therefore fires once at startup, before the user has touched anything.

Usually harmless, and load-bearing when the gated work is *building* something from the resource.
It bites when the work **overwrites** state some other source of truth also owns: a system writing a
resource's value into the model wipes whatever the model was initialised with, on the first frame,
before any of it reaches the screen. `Res<T>::is_added()` distinguishes the two, so the guard reads

```rust
if !thing.is_changed() || thing.is_added() { return; }
```

Same trait (`DetectChanges`) for `ResMut`, `Ref` and `Mut`, and the same reasoning applies to
`Changed<T>` query filters, which also match components on the frame they are inserted.

## Other

- **`Assets::get_mut` returns a guard**, not a plain `&mut`, so it needs `let mut image = …`.
  It also flags the asset modified on access — check a condition through the immutable `get`
  first, or a per-frame `get_mut` will re-upload the texture every frame.
- **Examples are written in BSN.** 0.19's `examples/3d/3d_scene.rs` uses the new scene notation
  (`bsn_list!`, `asset_value`, `template_value`). Plain `commands.spawn((..))` still works and
  is what `examples/3d/skybox.rs` uses — don't conclude from one example that the old API is
  gone.
- **Web backend**: `webgl2` is a default feature; `webgpu` is opt-in. Cubemaps and skyboxes work
  on WebGL2. Temporal anti-aliasing does not, and Bevy's examples `cfg`-gate it accordingly.

## Related

- [[scene]]: the spec whose implementation depends on all of the above
- [[water]]: the spec the WebGL2 exclusions above shaped
- [[build-performance]]: build-time consequences of depending on Bevy
