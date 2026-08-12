---
tags: [index]
type: index
updated: 2026-08-12
---
# Home: Index

The catalog and map of this project's specifications and developer knowledge. **Read this
first.** Every page is listed with a one-line summary. The authoring and maintenance rules are
in [[conventions]].

## Layout

```
spec/                     ← this tree. The vault root if you open it in Obsidian.
├── home.md               ← THIS PAGE. The index. Read first to orient.
├── conventions.md        ← how to author and maintain everything here (note types, spec
│                           status lifecycle, update triggers, lint).
├── _templates/           ← one template per note type.
├── <feature>.md          ← the specifications. Compile forward: intent → implementation.
└── wiki/                 ← developer knowledge. Compiles upward: code/experience → notes.
    └── log.md            ← append-only record of what was done here.
```

Two directions of knowledge, deliberately kept apart:

- **Specs** (`spec/*.md`) state *what we intend and why*, and exist to catch architectural
  drift. They are written before or alongside the code and carry a `status:`.
- **Wiki** (`spec/wiki/*.md`) records *what is true and what we learned* — API facts,
  measurements, traps. It is derived from the code and from experience.

## Specs

- [[hex-grid]]: the coordinate systems and grid model, plus the view that makes them visible —
  labels, axis compass, click selection, debug readout. `status: implemented`.
- [[terrain]]: elevation as per-location data, drawn as a continuous surface — a level cap per
  location, joined to its neighbours by inclined walls — with picking and shadows.
  `status: implemented`.
- [[scene]]: the scene shell — starfield skybox, orbit camera, clean exit, native and web build
  paths. `status: implemented`.

## Wiki

- [[hex-coordinates]]: the axial/cube/doubled formulas as implemented, the layout matrices, and the
  derivation of where the axes point (with the step-vs-axis trap that catches everyone).
- [[bevy-0-19-api]]: Bevy 0.19 API facts verified against the vendored source — the
  Event→Message rename, skybox cubemaps, mouse input, what the built-in camera controllers do
  and don't cover.
- [[build-performance]]: what makes this project slow to build and what was measured to fix it
  — dynamic linking, the LLD result, the `kache` wrapper, wasm output sizes.
- [[skybox-pipeline]]: how `tools/make_skybox.py` turns NASA all-sky EXRs into a cubemap, the
  exposure knob, and the traps in the ImageMagick and projection steps.
- [[log]]: append-only record.

## 🌱 Stubs

Pages worth writing when the need arises. A `[[link]]` to a page that doesn't exist yet is
fine — it marks one of these.

- `hex-algorithms`: ranges, rotations, line drawing and pathfinding. The reference covers them all;
  none are implemented, and cube coordinates are in place to make them easy.
- `camera-controls`: promote out of [[scene]] if the camera grows beyond orbit+zoom
  (panning, focus targets, keyboard movement).
