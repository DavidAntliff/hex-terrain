# hex-terrain

A Bevy 0.19 sandbox: one cube at the origin, an all-sky starfield, orbit camera.

- **Right-drag** to orbit, **scroll** to zoom.

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
