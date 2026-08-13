//! Hex terrain: a hexagonal grid model and a Bevy view onto it.
//!
//! Two layers, deliberately separated:
//!
//! - [`hex`] — the **model**. Coordinate systems and the grid of locations, entirely dimensionless:
//!   no world units, no scale, no rendering. Usable and testable with no Bevy app.
//! - [`view`] — the **view**. Owns the projection into world space ([`view::HexLayout`]) and
//!   everything drawn from it.
//!
//! Following <https://www.redblobgames.com/grids/hexagons/>.

pub mod camera;
pub mod hex;
pub mod probe;
pub mod sky;
pub mod view;
