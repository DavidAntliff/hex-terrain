---
tags: [hex, coordinates, concept]
type: concept
updated: 2026-08-12
---
# Hex coordinates

The formulas behind `src/hex/` and `src/view/layout.rs`, and the derivations that are not simply
quotable from the reference. Source: <https://www.redblobgames.com/grids/hexagons/>, whose
interactive diagrams carry information its prose does not — anything taken from a diagram had to be
derived instead, and those derivations are recorded here with the tests that confirm them.

## The three systems

| system | form | used for |
|---|---|---|
| axial | `q, r` | storage; hashable, so it keys the grid map |
| cube | `q, r, s` with `q + r + s == 0` | algorithms — the third component makes the symmetry explicit |
| doubled | `col, row` with `col + row` even | an alternative addressing scheme, no arithmetic advantage here |

Conversions, all exact integer arithmetic:

```
axial → cube      q, r, s = q, r, -q-r
cube  → axial     q, r
axial → doubled   col = 2q + r,  row = r          (doublewidth, the pointy-top variant)
doubled → axial   q = (col - row) / 2,  r = row
```

`Cube::new` takes only `q, r` and derives `s`, so the invariant cannot be broken by construction
rather than being checked afterwards.

**Doublewidth goes with pointy-top, doubleheight with flat-top.** Only doublewidth is implemented;
the other would be dead code. Watch out when reading the reference: the *offset* section's
conversions contain `col & 1` terms, and it is easy to lift those by mistake — the doubled
conversions have no bit-twiddling at all.

## Layout matrices

The orientation is entirely captured by a forward matrix, its inverse, and a corner start angle:

| | f0, f1, f2, f3 | b0, b1, b2, b3 | start_angle |
|---|---|---|---|
| pointy | √3, √3/2, 0, 3/2 | √3/3, −1/3, 0, 2/3 | 0.5 |
| flat | 3/2, 0, √3/2, √3 | 2/3, 0, −1/3, √3/3 | 0.0 |

```
hex → plane     x = (f0·q + f1·r)·size.x        plane → hex   q = b0·x' + b1·y'
                y = (f2·q + f3·r)·size.y                      r = b2·x' + b3·y'
                                                where x' = (x - origin.x)/size.x, likewise y'

corner i        angle = 2π·(start_angle + i)/6
                offset = (size.x·cos angle, size.y·sin angle)
```

`size` is the **circumradius** — centre to vertex — not the width across flats.

## Rounding a fractional coordinate

Rounding `q`, `r` and `s` independently can break `q + r + s == 0`. The fix is to round all three,
then recompute whichever moved furthest from the other two. Rounding a hex centre must return that
hex, and rounding anywhere must preserve the invariant — both are tested, the second with nudges
chosen to break a naive implementation (`(0.5, 0.5)`, `(1.5, -0.5)`, `(2.49, 2.49)`).

## Where the axes point (derived)

The reference shows this in an axis legend that yields no text, so it was derived from the layout
instead. The direction in which a coordinate increases fastest is its gradient. From
`q = b0·x' + b1·y'`:

```
∇q = (b0/size.x, b1/size.y)        ∇r = (b2/size.x, b3/size.y)        ∇s = -∇q - ∇r
```

For pointy-top at unit size that gives `∇q = (0.577, −0.333)`, `∇r = (0, 0.667)`,
`∇s = (−0.577, −0.333)` in plane coordinates — 120° apart, at plane angles 330°, 90° and 210°.

Since a pointy-top hexagon's corners are at 30°, 90°, …, 330°, **the cube axes point at the
hexagon's vertices**. That is the property the compass widget is checked against, in code and by
eye.

**The trap.** Stepping one hex in `+r` is not the same as moving along the `r` axis. The step
`(q, r+1)` also decrements `s` and lands *south-east*; the `r` axis points *due south*. Both
statements are true and they look contradictory on a diagram. Consequences: `-r` is north, `+q` is
ENE, `+s` is WNW.

## World mapping

The reference is 2D with `+y` pointing down-screen ("south"). On Bevy's ground plane that becomes
`+z`:

```
Xz plane (Y up, the default)     world = (x₂, 0, y₂)
Xy plane (Z up)                  world = (x₂, -y₂, 0)
```

Both send the reference's south to whatever reads as down-screen for a camera looking at the grid, so
the rendered grid matches the website's pictures either way. Elevation is the remaining axis.

## Storage shape

The Recommendations table settles this: array storage suits offset (rectangular maps) and axial
(rhombus maps); for **any other shape, use axial or cube with hash storage**. A hexagon-shaped map
is "any other shape", so `HashMap<Axial, _>`. Offset coordinates were skipped entirely — their only
wins are rectangular array storage and matching a rectangular map, against the costs of no vector
arithmetic and neighbour tables that vary by row parity.

The hexagon shape itself comes from bounding all three cube components: `q` over `-radius..=radius`
and `r` clamped so `|s| <= radius` too. Without the `s` bound the same loop yields a rhombus.

## Related

- [[hex-grid]]: the spec, including the model/projection split these formulas live in
- [[bevy-0-19-api]]: the Bevy side — meshes, gizmos, UI projection
