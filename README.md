# hex-terrain

A Bevy 0.19 sandbox: a hexagonal grid under an all-sky starfield, with an orbit camera. The grid is a
hexagon of side 4 — 37 locations — addressable in axial, cube and doubled coordinates, following
[Red Blob Games' hexagon guide](https://www.redblobgames.com/grids/hexagons/).

Design notes and accumulated knowledge live in `spec/` — start at `spec/home.md`.

- **Right-drag** to orbit, **scroll** to zoom, **left-click** to select a hexagon, **Escape** to quit.
- The panel top right reports the selected hexagon in axial, cube and doubled coordinates plus world
  position, and carries four controls: cycle the per-hex labels (axial / cube / doubled / off), toggle
  pointy-top against flat-top, show or hide the axis compass, and reset the view to look straight down
  with everything in frame.

Set `HEX_TERRAIN_SCREENSHOT=<path>` to save a PNG of the scene and exit — useful for checking a change
without a window manager in the way.

## Native

    cargo run

Bevy is linked dynamically on native targets, which cuts an edit-rebuild cycle from ~40s to
~2s. The binary therefore needs `libbevy_dylib.so` from `target/debug/deps/` at runtime —
`cargo run` handles that, but copying the bare binary elsewhere will not work.

## Web

    trunk serve --open                      # dev server on 127.0.0.1:8080

    trunk build --release                   # static files in dist/
    python3 -m http.server -d dist 8080     # or any static server

## Regenerating the skybox

`assets/textures/starmap_cubemap.png` is committed, so neither of the above needs this.
To rebuild it (needs ImageMagick with the OpenEXR delegate, plus numpy):

    python3 tools/make_skybox.py --check          # geometry asserts only
    python3 tools/make_skybox.py --exposure 0.25  # download sources, reproject, write PNG

Skybox imagery: *Deep Star Maps 2020*, NASA/Goddard Space Flight Center Scientific
Visualization Studio, incorporating ESA/Gaia data. Public domain.
