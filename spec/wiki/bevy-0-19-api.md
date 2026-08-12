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

The skybox does **not** light the scene; `EnvironmentMapLight` is a separate component
requiring a prefiltered map. A KTX2 cubemap carries the metadata natively and would remove the
reinterpretation code entirely, at the cost of needing `toktx`/`basisu` to generate.

## Mouse input

`AccumulatedMouseMotion { delta: Vec2 }` and `AccumulatedMouseScroll { unit: MouseScrollUnit,
delta: Vec2 }` are resources reset every frame — no message reader needed for camera control.

**`unit` is not decoration.** Browsers report `MouseScrollUnit::Pixel` with deltas roughly 50×
larger than the `Line` deltas a desktop mouse produces. Code that ignores `unit` feels correct
natively and is unusable on the web.

## Camera controllers

0.19 ships first-party controllers in `bevy_camera_controller`, feature-gated as `free_camera`
and `pan_camera`. **Neither orbits a fixed target**, so an orbit camera is still hand-written.
The crate's own docs suggest copying a controller and modifying it when the provided behaviour
doesn't fit.

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

## UI widgets

`bevy_ui_widgets` ships **headless** widgets — behaviour and accessibility, no drawing:

- `Button` emits `Activate { entity }`, observable with `On<Activate>`. Keyboard activation is
  included, so a focused button responds to Enter and Space.
- `Checkbox` emits `ValueChange<bool> { source, value, .. }` and reads its state from the `Checked`
  component. It computes the next value from `Has<Checked>`, so **`Checked` must be maintained** or
  every click reports `true`. Add the provided `checkbox_self_update` observer to the entity for
  that, and set the initial `Checked` yourself. Rendering the box is the caller's job.

Both use `bevy_picking` under the hood, which is what makes a UI click also reachable by
world-space picking code — see `Hovered` below.

## Other

- **`Assets::get_mut` returns a guard**, not a plain `&mut`, so it needs `let mut image = …`.
  It also flags the asset modified on access — check a condition through the immutable `get`
  first, or a per-frame `get_mut` will re-upload the texture every frame.
- **`GlobalAmbientLight`** is the ambient-light resource name (inserted with
  `commands.insert_resource`).
- **Examples are written in BSN.** 0.19's `examples/3d/3d_scene.rs` uses the new scene notation
  (`bsn_list!`, `asset_value`, `template_value`). Plain `commands.spawn((..))` still works and
  is what `examples/3d/skybox.rs` uses — don't conclude from one example that the old API is
  gone.
- **Web backend**: `webgl2` is a default feature; `webgpu` is opt-in. Cubemaps and skyboxes work
  on WebGL2. Temporal anti-aliasing does not, and Bevy's examples `cfg`-gate it accordingly.

## Related

- [[scene]]: the spec whose implementation depends on all of the above
- [[build-performance]]: build-time consequences of depending on Bevy
