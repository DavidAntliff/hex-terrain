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

- [[sandbox-scene]]: the spec whose implementation depends on all of the above
- [[build-performance]]: build-time consequences of depending on Bevy
