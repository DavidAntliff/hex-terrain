#!/usr/bin/env python3
"""Build the skybox cubemap from NASA SVS Deep Star Maps 2020 (public domain).

Reprojects an equirectangular (plate carree) all-sky map into 6 cube faces stacked
vertically in wgpu layer order, which is what `Skybox` wants after
`reinterpret_stacked_2d_as_array`.

Run once and commit the output; the EXR sources are cached in tools/cache (gitignored).
ImageMagick does the image I/O (EXR decode, HDR->sRGB, PNG encode); numpy does the
reprojection.

    python3 tools/make_skybox.py [--exposure 6.0] [--check]
"""

import argparse
import subprocess
import urllib.request
from pathlib import Path

import numpy as np

SVS = "https://svs.gsfc.nasa.gov/vis/a000000/a004800/a004851"
# Screened together, dimmest first: diffuse galactic band, 1.7B faint stars, bright stars.
# At 4k the faint-star layer averages into a glow; the hiptyc layer is what puts visible
# points of light in the sky.
LAYERS = ["milkyway_2020_4k.exr", "starmap_2020_4k.exr", "hiptyc_2020_4k.exr"]

# ponytail: 1024 faces match the 4k source's local resolution. Bump both this and the
# source layers to *_8k.exr if it looks soft — costs web load time.
FACE = 1024

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / "tools" / "cache"
OUT = ROOT / "assets" / "textures" / "starmap_cubemap.png"

# wgpu cubemap layer order. Each entry maps face-local (u, v) in [-1, 1], v pointing
# down, to a direction on the unit cube.
FACES = [
    ("+X", lambda u, v: (np.ones_like(u), -v, -u)),
    ("-X", lambda u, v: (-np.ones_like(u), -v, u)),
    ("+Y", lambda u, v: (u, np.ones_like(u), v)),
    ("-Y", lambda u, v: (u, -np.ones_like(u), -v)),
    ("+Z", lambda u, v: (u, -v, np.ones_like(u))),
    ("-Z", lambda u, v: (-u, -v, -np.ones_like(u))),
]
AXES = {"+X": (1, 0, 0), "-X": (-1, 0, 0), "+Y": (0, 1, 0),
        "-Y": (0, -1, 0), "+Z": (0, 0, 1), "-Z": (0, 0, -1)}


def face_grid(size):
    """(u, v) at pixel centres; u increases rightwards, v downwards."""
    t = (np.arange(size) + 0.5) / size * 2 - 1
    return np.meshgrid(t, t)


def to_lon_lat(dirs):
    x, y, z = (np.asarray(c, dtype=np.float64) for c in dirs)
    n = np.sqrt(x * x + y * y + z * z)
    x, y, z = x / n, y / n, z / n
    return np.arctan2(x, -z), np.arcsin(np.clip(y, -1.0, 1.0))


def sample(equirect, dirs):
    """Nearest-neighbour lookup. Bilinear would dim single-pixel stars."""
    lon, lat = to_lon_lat(dirs)
    h, w, _ = equirect.shape
    col = np.clip(((lon / (2 * np.pi) + 0.5) * w).astype(np.int64), 0, w - 1)
    row = np.clip(((0.5 - lat / np.pi) * h).astype(np.int64), 0, h - 1)
    return equirect[row, col]


def check():
    """The whole risk here is an axis swap or sign flip. These asserts catch both."""
    zero = np.zeros((1, 1))
    for name, f in FACES:
        centre = np.array([np.asarray(c).item() for c in f(zero, zero)])
        assert np.allclose(centre, AXES[name]), f"{name} centre points at {centre}"

    u, v = face_grid(64)
    for name, f in FACES:
        x, y, z = (np.asarray(c) for c in f(u, v))
        assert np.all(x * x + y * y + z * z >= 1.0), f"{name} has a degenerate direction"

    # The poles must land on the first and last row of the source, and the four side
    # faces must between them span every longitude.
    probe = np.zeros((180, 360, 3), np.uint8)
    probe[0] = 1
    probe[-1] = 2
    assert sample(probe, FACES[2][1](zero, zero))[0, 0, 0] == 1, "+Y is not the north pole"
    assert sample(probe, FACES[3][1](zero, zero))[0, 0, 0] == 2, "-Y is not the south pole"

    lons = np.concatenate([to_lon_lat(f(u, v))[0].ravel()
                           for name, f in FACES if name[1] != "Y"])
    assert lons.min() < -np.pi + 0.1 and lons.max() > np.pi - 0.1, "longitudes do not wrap"
    print("geometry checks passed")


def fetch(name):
    path = CACHE / name
    if not path.exists():
        CACHE.mkdir(parents=True, exist_ok=True)
        print(f"downloading {name} ...")
        urllib.request.urlretrieve(f"{SVS}/{name}", path)
    return path


def read_ppm(path):
    data = path.read_bytes()
    assert data[:2] == b"P6", f"{path} is not a binary PPM"
    fields, pos = [], 2
    while len(fields) < 3:
        if data[pos : pos + 1].isspace():
            pos += 1
        elif data[pos : pos + 1] == b"#":
            pos = data.index(b"\n", pos) + 1
        else:
            end = pos
            while not data[end : end + 1].isspace():
                end += 1
            fields.append(int(data[pos:end]))
            pos = end
    w, h, maxval = fields
    assert maxval == 255, f"expected 8 bits per channel, got maxval {maxval}"
    return np.frombuffer(data, np.uint8, w * h * 3, pos + 1).reshape(h, w, 3)


def write_png(arr, path):
    h, w, _ = arr.shape
    ppm = CACHE / "cubemap.ppm"
    ppm.write_bytes(b"P6\n%d %d\n255\n" % (w, h) + np.ascontiguousarray(arr).tobytes())
    path.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["convert", str(ppm), str(path)], check=True)
    ppm.unlink()


def main():
    ap = argparse.ArgumentParser()
    # The one knob that needs eyeballing: too high and the sRGB curve lifts the faint-star
    # floor into a grey wash, too low and the galactic band disappears.
    ap.add_argument("--exposure", type=float, default=0.25)
    ap.add_argument("--check", action="store_true", help="run the asserts and stop")
    args = ap.parse_args()

    check()
    if args.check:
        return

    flat = CACHE / "equirect.ppm"
    layers = [str(fetch(name)) for name in LAYERS]
    # Screen the layers pairwise. Not `-flatten`, which composites onto a white canvas.
    cmd = ["convert", layers[0]]
    for layer in layers[1:]:
        cmd += [layer, "-compose", "screen", "-composite"]
    cmd += ["-evaluate", "multiply", str(args.exposure),
            "-colorspace", "sRGB", "-depth", "8", str(flat)]
    subprocess.run(cmd, check=True)
    equirect = read_ppm(flat)
    print(f"source {equirect.shape[1]}x{equirect.shape[0]}, exposure {args.exposure}")

    u, v = face_grid(FACE)
    strip = np.concatenate([sample(equirect, f(u, v)) for _, f in FACES], axis=0)
    assert strip.shape == (FACE * 6, FACE, 3), strip.shape

    write_png(strip, OUT)
    print(f"wrote {OUT.relative_to(ROOT)} ({FACE}x{FACE * 6}, "
          f"{OUT.stat().st_size / 1e6:.1f} MB), mean level {strip.mean():.1f}")


if __name__ == "__main__":
    main()
