---
tags: [skybox, assets, tooling, concept]
type: concept
updated: 2026-08-13
---
# Skybox pipeline

How `tools/make_skybox.py` turns NASA all-sky imagery into a starfield cubemap. Run once;
the output `assets/textures/starmap_cubemap.png` (1024×6144, 10.8 MB) is committed, so a fresh
clone needs neither the tool nor the sources.

**Nothing currently loads it.** The scene's sky is now generated at runtime — a daylight sky from
`src/sky.rs`, because [[water]] needs something worth reflecting and a night sky gives a mirror
nothing. The tool and its asset are kept, working, for a night mode. The face convention below is
still the one in use: `src/sky.rs` writes to the same layout deliberately, so the two skies are
interchangeable. See [[scene]] for the decision.

Because nothing loads it, `index.html` no longer copies it into the web bundle — 10.8 MB against a
50 MB deploy. Wiring a night mode up therefore means widening that `copy-dir` link as well as
loading the asset, or it will 404 on the web only. See [[build-performance]] → *Web build*.

```bash
python3 tools/make_skybox.py --check            # geometry asserts only, no downloads
python3 tools/make_skybox.py --exposure 0.25    # download, composite, reproject, write PNG
```

## Source imagery

NASA SVS *Deep Star Maps 2020* — 1.7 billion stars from Hipparcos-2, Tycho-2 and Gaia DR2,
public domain, credited to NASA/Goddard SVS with ESA/Gaia. Three equirectangular
(plate carrée) 4k EXR layers are screened together, dimmest first:

| Layer | What it contributes |
|---|---|
| `milkyway_2020_4k.exr` | the diffuse galactic band |
| `starmap_2020_4k.exr` | 1.7 B faint stars |
| `hiptyc_2020_4k.exr` | bright named stars as distinct points |

The third layer matters more than it looks. At 4k each pixel covers several arcminutes, so the
faint-star layer averages into a smooth glow rather than resolving individual stars — the result
without `hiptyc` reads as grey fog, not sky. Resolving real stars from the star map alone would
need the 16k source (423 MB).

Only the 4k tier is EXR-and-large; the JPEG previews NASA offers are 1024×512, too low for a
cubemap.

## Why numpy plus ImageMagick

The projection is written by hand because nothing on the machine does equirect→cubemap:
`ffmpeg` (whose `v360` filter would do it in one command) is absent, as is PIL. What is present
is ImageMagick with an OpenEXR delegate and numpy. So ImageMagick does image I/O — EXR decode,
screen compositing, HDR→sRGB, PNG encode — and numpy does the per-pixel reprojection, exchanging
data through 8-bit binary PPM.

## The projection

Six 1024² faces are stacked vertically in wgpu layer order (+X, −X, +Y, −Y, +Z, −Z), each
mapping face-local `(u, v)` in [−1, 1] with `v` pointing **down** to a direction on the unit
cube, then to `lon = atan2(x, -z)`, `lat = asin(y)`.

Sampling is **nearest neighbour**, deliberately. Bilinear would average single-pixel stars with
their black neighbours and dim them; nearest is simultaneously less code and the better-looking
result. This is the rare case where the lazy choice is also the correct one.

## Exposure is a calibration knob, not a constant

The EXR data is linear HDR and must be tonemapped to 8-bit, which is a judgement call, so it is
exposed as `--exposure` rather than buried. Measured means over the output image:

| exposure | mean level | verdict |
|---|---|---|
| 0.25 | 20 | **default** — dark sky, visible band |
| 0.5 | 31 | brighter than wanted |
| 1.0 | 46 | washed out |
| 6.0 | 110 | uniform grey |

The sRGB transfer curve is what makes this sharp: it lifts near-black strongly, so a modest
exposure increase turns the faint-star floor into visible fog across the whole sky.

## Verification

`--check` runs the asserts before any downloading, so it is a fast sanity check:

- each face's centre direction equals its expected axis — catches an axis swap or sign flip,
  which is essentially the only way the projection can be wrong;
- the +Y and −Y face centres sample the first and last row of the source, confirming the
  lon/lat convention is hooked up the right way round;
- the four side faces between them span every longitude;
- the output is exactly 1024×6144.

Visual confirmation is the Magellanic Clouds appearing on the −Y face and the galactic band
crossing face boundaries without a discontinuity.

## Traps

- **`-flatten` composites onto a white canvas.** Using `convert A B C -compose screen -flatten`
  produced a near-uniform bright image (mean 136, and a suspiciously tiny 14 KB PNG, because
  near-uniform data compresses). Screen the layers **pairwise** with repeated `-composite`
  instead, or pass an explicit `-background black`.
- **A tiny output PNG is a symptom, not a win.** Both failure modes above — white wash and grey
  wash — compress far smaller than a correct starfield. Check the mean level, which the script
  prints.
- **No JWST all-sky image exists.** Webb images deep patches of sky. Any "JWST skybox" is
  necessarily a deep field repeated across the cube faces, with discontinuities at every face
  boundary. See [[scene]] → Design discussion.

## Related

- [[scene]]: the spec that locks the cubemap decision, and that replaced this sky with a daylight one
- [[water]]: why the sky changed
- [[bevy-0-19-api]]: how the stacked PNG is reinterpreted as a cubemap at load time
