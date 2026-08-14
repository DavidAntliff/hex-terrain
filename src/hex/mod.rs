//! The hex grid model.
//!
//! Pure, dimensionless grid logic: coordinate systems and the locations that make up a grid.
//! Nothing here knows about world units, rendering, or Bevy. Projecting a coordinate into the
//! scene is [`crate::view::layout`]'s responsibility, which keeps the model reusable and testable
//! on its own.
//!
//! Follows <https://www.redblobgames.com/grids/hexagons/>.

pub mod biome;
pub mod coords;
pub mod grid;
pub mod orientation;
pub mod scenes;

pub use biome::{Bands, Biome};
pub use coords::{Axial, Cube, DIRECTIONS, Doubled, FractionalCube};
pub use grid::{Grid, Location};
pub use orientation::Orientation;

/// Per-location payload. The extension point for ownership and anything else a hex needs to carry;
/// so far, elevation and water. [`Biome`] is deliberately *not* here — it is derived from these two
/// rather than stored beside them, for the reasons on [`biome`].
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Terrain {
    /// Elevation as a dimensionless level, signed and roughly `-1..=1`. Zero is the grid plane;
    /// positive rises above it, negative sinks below. What a level is worth in world units is
    /// [`crate::view::layout::HexLayout`]'s business, not the model's.
    pub height: f32,
    /// The surface level of the water covering this location, in the same units as `height`, or
    /// `None` where there is none. Per location rather than global, so a mountain lake can stand
    /// above the sea it drains into.
    pub water: Option<f32>,
}

/// The grid the scene displays: a hexagon of side 4 (radius 3, 37 locations).
pub type TerrainGrid = Grid<Terrain>;

impl Terrain {
    /// The level of whatever forms this location's visible surface: the water covering it, or the
    /// ground where nothing does.
    ///
    /// What a location *presents* — what a click lands on, what an outline traces, what a label
    /// sits above — as against `height`, which is only ever the ground. Guards against water below
    /// the ground it is meant to cover, which [`flood`] never produces but nothing else forbids.
    pub fn surface(&self) -> f32 {
        self.water
            .map_or(self.height, |level| level.max(self.height))
    }
}

/// The level the terrain starts flooded to, in the same units as a height.
pub const SEA_LEVEL: f32 = 0.0;

/// Placeholder terrain: one sinusoid along each axis, giving a wave that crosses the grid plane.
///
/// Centred on zero rather than lifted clear of it, so the grid has troughs as well as peaks. The
/// frequency puts about one full period across a radius-3 grid. Dry — [`flood`] decides the water.
pub fn undulating(coord: Axial) -> Terrain {
    const FREQUENCY: f32 = 0.9; // radians per hex step
    let (q, r) = (coord.q as f32, coord.r as f32);
    Terrain {
        height: 0.5 * ((q * FREQUENCY).sin() + (r * FREQUENCY).sin()),
        water: None,
    }
}

/// Fills every location that lies below `level` with water at that level, and drains the rest.
///
/// One body at one level over the whole grid, which is what a single sea level means. A location's
/// water is per-location so that separate bodies can sit at their own levels — a mountain lake
/// above the sea it drains into — but nothing generates those yet.
pub fn flood(grid: &mut TerrainGrid, level: f32) {
    for location in grid.iter_mut() {
        location.data.water = (location.data.height < level).then_some(level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_undulation_stays_in_range_and_crosses_the_plane() {
        let grid = Grid::hexagon(3, undulating);
        let heights: Vec<f32> = grid.iter().map(|l| l.data.height).collect();
        assert!(heights.iter().all(|h| (-1.0..=1.0).contains(h)));
        // Both signs, or there would be no troughs to fill.
        assert!(heights.iter().any(|&h| h > 0.1), "no peaks: {heights:?}");
        assert!(heights.iter().any(|&h| h < -0.1), "no troughs: {heights:?}");

        // And a whole diagonal at *exactly* the plane. `sin` is odd and `r = -q` negates its
        // argument bit for bit, so the two terms cancel to a true zero rather than to something
        // near it. Worth pinning: ground exactly at a water level is the awkward case for the
        // renderer, and this generator hands it seven locations of it every time.
        for q in -3..=3 {
            let height = undulating(Axial::new(q, -q)).height;
            assert_eq!(
                height.to_bits(),
                0.0f32.to_bits(),
                "q={q} is not exactly zero"
            );
        }
    }

    #[test]
    fn flooding_covers_exactly_what_lies_below_the_level() {
        let mut grid = Grid::hexagon(3, undulating);
        assert!(grid.iter().all(|l| l.data.water.is_none()), "starts dry");

        flood(&mut grid, SEA_LEVEL);
        for location in grid.iter() {
            assert_eq!(
                location.data.water,
                (location.data.height < SEA_LEVEL).then_some(SEA_LEVEL),
                "{:?} is wet without being low, or low without being wet",
                location.coord
            );
        }
        assert!(
            grid.iter().any(|l| l.data.water.is_some()),
            "nothing is flooded"
        );
        assert!(
            grid.iter().any(|l| l.data.water.is_none()),
            "everything is flooded"
        );
    }

    #[test]
    fn a_surface_is_the_water_where_there_is_any_and_the_ground_otherwise() {
        assert_eq!(
            Terrain {
                height: 0.3,
                water: None
            }
            .surface(),
            0.3
        );
        assert_eq!(
            Terrain {
                height: -0.4,
                water: Some(0.2)
            }
            .surface(),
            0.2
        );
        // Water below the ground it claims to cover is not something `flood` makes, but the ground
        // still wins if it ever appears.
        assert_eq!(
            Terrain {
                height: 0.5,
                water: Some(-0.5)
            }
            .surface(),
            0.5
        );
    }

    #[test]
    fn the_level_can_rise_and_fall_again() {
        let mut grid = Grid::hexagon(3, undulating);
        flood(&mut grid, 2.0);
        assert!(grid.iter().all(|l| l.data.water == Some(2.0)), "all under");
        flood(&mut grid, -2.0);
        assert!(grid.iter().all(|l| l.data.water.is_none()), "all drained");
    }
}
