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
