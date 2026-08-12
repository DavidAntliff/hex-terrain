//! Hex coordinate systems and the conversions between them.
//!
//! Three representations, following <https://www.redblobgames.com/grids/hexagons/>:
//!
//! - [`Axial`] — two components, the storage format. Hashable, so it keys the grid map.
//! - [`Cube`] — three components constrained to `q + r + s == 0`. Most algorithms are simplest
//!   here, because the third component makes the symmetry explicit.
//! - [`Doubled`] — row/column addressing, in the variant the [`Orientation`] implies.
//!
//! Everything here is **dimensionless**: these are grid coordinates, with no notion of world units
//! or scale. Turning a coordinate into a position is the projection layer's job
//! (`crate::view::layout`).
//!
//! Axial and cube are orientation-independent — the grid's topology is the same however the
//! hexagons are drawn. Doubled is the exception: "row" and "column" only mean something once an
//! orientation is chosen, so its conversions take one.

use super::Orientation;

/// Axial coordinates: the storage format.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Axial {
    pub q: i32,
    pub r: i32,
}

/// Cube coordinates, maintaining `q + r + s == 0`.
///
/// The invariant holds by construction: `s` is always derived, never supplied.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cube {
    pub q: i32,
    pub r: i32,
    pub s: i32,
}

/// Doubled coordinates: row/column addressing where one axis steps by two.
///
/// Which axis doubles depends on the [`Orientation`] — *width* for pointy-top, *height* for
/// flat-top — so every conversion takes one. `col + row` is always even in both variants.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Doubled {
    pub col: i32,
    pub row: i32,
}

/// The six neighbour directions, in axial form, ordered counter-clockwise from east.
///
/// For a pointy-top layout these are E, NE, NW, W, SW, SE. The order is the one the reference
/// uses, so `direction(i)` matches its diagrams.
pub const DIRECTIONS: [Axial; 6] = [
    Axial { q: 1, r: 0 },
    Axial { q: 1, r: -1 },
    Axial { q: 0, r: -1 },
    Axial { q: -1, r: 0 },
    Axial { q: -1, r: 1 },
    Axial { q: 0, r: 1 },
];

impl Axial {
    pub const ZERO: Self = Self { q: 0, r: 0 };

    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    pub const fn to_cube(self) -> Cube {
        Cube::new(self.q, self.r)
    }

    /// Doubled coordinates in the variant `orientation` implies.
    pub const fn to_doubled(self, orientation: Orientation) -> Doubled {
        match orientation {
            // Doublewidth: columns step by two, rows are the axial row.
            Orientation::Pointy => Doubled {
                col: 2 * self.q + self.r,
                row: self.r,
            },
            // Doubleheight: rows step by two, columns are the axial column.
            Orientation::Flat => Doubled {
                col: self.q,
                row: 2 * self.r + self.q,
            },
        }
    }

    /// The neighbour in direction `index`, taken modulo 6.
    pub const fn neighbour(self, index: usize) -> Self {
        let d = DIRECTIONS[index % 6];
        Self::new(self.q + d.q, self.r + d.r)
    }

    pub fn neighbours(self) -> [Self; 6] {
        core::array::from_fn(|i| self.neighbour(i))
    }

    /// Steps along the grid between two hexes.
    pub const fn distance(self, other: Self) -> i32 {
        self.to_cube().distance(other.to_cube())
    }
}

impl Cube {
    pub const ZERO: Self = Self { q: 0, r: 0, s: 0 };

    /// Builds a cube coordinate, deriving `s` so that `q + r + s == 0` cannot be violated.
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r, s: -q - r }
    }

    pub const fn to_axial(self) -> Axial {
        Axial::new(self.q, self.r)
    }

    /// Half the L1 norm — the standard cube distance.
    pub const fn distance(self, other: Self) -> i32 {
        let dq = (self.q - other.q).abs();
        let dr = (self.r - other.r).abs();
        let ds = (self.s - other.s).abs();
        (dq + dr + ds) / 2
    }
}

impl Doubled {
    pub const fn new(col: i32, row: i32) -> Self {
        Self { col, row }
    }

    /// Inverse of [`Axial::to_doubled`]; pass the same orientation.
    pub const fn to_axial(self, orientation: Orientation) -> Axial {
        match orientation {
            Orientation::Pointy => Axial::new((self.col - self.row) / 2, self.row),
            Orientation::Flat => Axial::new(self.col, (self.row - self.col) / 2),
        }
    }
}

/// A non-integral cube coordinate, as produced by projecting a world position onto the grid.
///
/// [`Self::round`] resolves it to the containing hex.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FractionalCube {
    pub q: f32,
    pub r: f32,
    pub s: f32,
}

impl FractionalCube {
    pub fn new(q: f32, r: f32) -> Self {
        Self { q, r, s: -q - r }
    }

    /// Rounds to the nearest hex.
    ///
    /// Rounding each component independently can break `q + r + s == 0`, so the component that
    /// moved furthest is recomputed from the other two — the reference's approach.
    pub fn round(self) -> Cube {
        let mut q = self.q.round();
        let mut r = self.r.round();
        let s = self.s.round();

        let dq = (q - self.q).abs();
        let dr = (r - self.r).abs();
        let ds = (s - self.s).abs();

        if dq > dr && dq > ds {
            q = -r - s;
        } else if dr > ds {
            r = -q - s;
        }

        Cube::new(q as i32, r as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every hex in a small patch, for exhaustive round-trip checks.
    fn sample() -> impl Iterator<Item = Axial> {
        (-5..=5).flat_map(|q| (-5..=5).map(move |r| Axial::new(q, r)))
    }

    #[test]
    fn cube_invariant_always_holds() {
        for a in sample() {
            let c = a.to_cube();
            assert_eq!(c.q + c.r + c.s, 0, "{c:?} breaks q+r+s==0");
        }
    }

    #[test]
    fn axial_cube_round_trip() {
        for a in sample() {
            assert_eq!(a.to_cube().to_axial(), a);
        }
    }

    #[test]
    fn axial_doubled_round_trip_in_both_orientations() {
        for orientation in [Orientation::Pointy, Orientation::Flat] {
            for a in sample() {
                let d = a.to_doubled(orientation);
                assert_eq!(
                    (d.col + d.row) % 2,
                    0,
                    "{d:?} should have even col+row ({orientation:?})"
                );
                assert_eq!(d.to_axial(orientation), a, "round trip failed ({orientation:?})");
            }
        }
    }

    #[test]
    fn each_orientation_doubles_its_own_axis() {
        // The whole point of the parameter: pointy-top doubles the column, flat-top the row. Using
        // the wrong variant produces numbers that match no visible row or column.
        let coord = Axial::new(1, -1);
        assert_eq!(coord.to_doubled(Orientation::Pointy), Doubled::new(1, -1));
        assert_eq!(coord.to_doubled(Orientation::Flat), Doubled::new(1, -1));

        // A coordinate where the two variants genuinely differ.
        let coord = Axial::new(2, 1);
        assert_eq!(coord.to_doubled(Orientation::Pointy), Doubled::new(5, 1));
        assert_eq!(coord.to_doubled(Orientation::Flat), Doubled::new(2, 4));
    }

    #[test]
    fn stepping_along_a_row_moves_the_doubled_axis_by_two() {
        // The signature of doubled coordinates, and the clearest check that each orientation is
        // paired with the right variant.
        let east = Axial::ZERO.neighbour(0); // +q, along a pointy-top row
        let pointy = east.to_doubled(Orientation::Pointy);
        assert_eq!((pointy.col, pointy.row), (2, 0), "pointy row step should double the column");

        // For flat-top, the column-wise neighbour is the one that steps the doubled row by two.
        let south = Axial::new(0, 1);
        let flat = south.to_doubled(Orientation::Flat);
        assert_eq!((flat.col, flat.row), (0, 2), "flat column step should double the row");
    }

    #[test]
    fn neighbours_are_distinct_and_adjacent() {
        for a in sample() {
            let n = a.neighbours();
            for (i, &x) in n.iter().enumerate() {
                assert_eq!(a.distance(x), 1, "{x:?} should be adjacent to {a:?}");
                for &y in &n[i + 1..] {
                    assert_ne!(x, y, "duplicate neighbour of {a:?}");
                }
            }
        }
    }

    #[test]
    fn distance_is_symmetric_and_zero_on_self() {
        for a in sample() {
            assert_eq!(a.distance(a), 0);
            for b in sample() {
                assert_eq!(a.distance(b), b.distance(a));
            }
        }
    }

    #[test]
    fn rounding_a_hex_centre_returns_that_hex() {
        for a in sample() {
            let c = a.to_cube();
            let f = FractionalCube::new(c.q as f32, c.r as f32);
            assert_eq!(f.round(), c);
        }
    }

    #[test]
    fn rounding_never_breaks_the_cube_invariant() {
        // Nudges that would break the invariant if components were rounded independently.
        for (q, r) in [(0.4, 0.4), (0.5, 0.5), (-0.4, 0.6), (1.5, -0.5), (2.49, 2.49)] {
            let c = FractionalCube::new(q, r).round();
            assert_eq!(c.q + c.r + c.s, 0, "rounding ({q}, {r}) broke the invariant");
        }
    }
}
