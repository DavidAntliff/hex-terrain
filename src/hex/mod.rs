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

/// Per-location payload.
///
/// Empty for now — the extension point for elevation, biome, ownership and anything else a hex
/// needs to carry. It exists as a named type so the grid's payload can grow without changing
/// every signature that mentions it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Terrain;

/// The grid the scene displays: a hexagon of side 4 (radius 3, 37 locations).
pub type TerrainGrid = Grid<Terrain>;
