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
