//! The hex grid model.
//!
//! Pure, dimensionless grid logic: coordinate systems and the locations that make up a grid.
//! Nothing here knows about world units, rendering, or Bevy. Projecting a coordinate into the
//! scene is [`crate::view::layout`]'s responsibility, which keeps the model reusable and testable
//! on its own.
//!
//! Follows <https://www.redblobgames.com/grids/hexagons/>.

pub mod coords;
pub mod grid;
pub mod orientation;

pub use coords::{Axial, Cube, Doubled, FractionalCube, DIRECTIONS};
pub use grid::{Grid, Location};
pub use orientation::Orientation;

/// Per-location payload. The extension point for biome, ownership and anything else a hex needs
/// to carry; so far, elevation and water.
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

/// The level the placeholder terrain floods to, in the same units as a height.
pub const SEA_LEVEL: f32 = 0.0;

/// Placeholder terrain: one sinusoid along each axis, giving a wave that crosses the grid plane,
/// with everything below [`SEA_LEVEL`] under water.
///
/// Centred on zero rather than lifted clear of it, so the grid has troughs as well as peaks. The
/// frequency puts about one full period across a radius-3 grid.
pub fn undulating(coord: Axial) -> Terrain {
    const FREQUENCY: f32 = 0.9; // radians per hex step
    let (q, r) = (coord.q as f32, coord.r as f32);
    let height = 0.5 * ((q * FREQUENCY).sin() + (r * FREQUENCY).sin());
    Terrain {
        height,
        water: (height < SEA_LEVEL).then_some(SEA_LEVEL),
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
    }

    #[test]
    fn water_covers_exactly_what_lies_below_sea_level() {
        let grid = Grid::hexagon(3, undulating);
        for location in grid.iter() {
            assert_eq!(
                location.data.water.is_some(),
                location.data.height < SEA_LEVEL,
                "{:?} is wet without being low, or low without being wet",
                location.coord
            );
        }
        assert!(grid.iter().any(|l| l.data.water.is_some()), "nothing is flooded");
        assert!(grid.iter().any(|l| l.data.water.is_none()), "everything is flooded");
    }
}
