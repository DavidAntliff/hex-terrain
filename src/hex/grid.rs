//! The grid model: a set of hex locations, each carrying arbitrary data.
//!
//! Deliberately **dimensionless** — a `Grid` knows which hexes exist and what data they hold, and
//! nothing about scale, orientation, or world positions. Ask
//! [`crate::view::layout::HexLayout`] for positions.
//!
//! Storage is a hash map keyed by [`Axial`]. The reference's Recommendations table offers array
//! storage only for rhombus-shaped axial maps; for any other shape — ours is a hexagon — it
//! recommends axial or cube coordinates with hash storage.

use std::collections::HashMap;

use super::coords::Axial;

/// One hex of the grid, plus whatever data is attached to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location<T> {
    pub coord: Axial,
    pub data: T,
}

impl<T> Location<T> {
    pub fn new(coord: Axial, data: T) -> Self {
        Self { coord, data }
    }
}

/// A plane of hex locations. Not necessarily rectangular, or even contiguous.
#[derive(Debug, Clone, Default)]
pub struct Grid<T> {
    locations: HashMap<Axial, Location<T>>,
}

impl<T> Grid<T> {
    pub fn new() -> Self {
        Self {
            locations: HashMap::new(),
        }
    }

    /// Builds a hexagon-shaped grid of the given radius, centred on the origin.
    ///
    /// `radius` counts rings around the centre, so radius 3 is a hexagon of side 4: 37 locations
    /// in rows of 4, 5, 6, 7, 6, 5, 4. The bounds are the reference's: `q` spans `-radius..=radius`
    /// and `r` is clamped so that `|s| <= radius` too, which is what makes the shape a hexagon
    /// rather than a rhombus.
    pub fn hexagon(radius: i32, mut data: impl FnMut(Axial) -> T) -> Self {
        let mut locations = HashMap::new();
        for q in -radius..=radius {
            let r_min = (-radius).max(-q - radius);
            let r_max = radius.min(-q + radius);
            for r in r_min..=r_max {
                let coord = Axial::new(q, r);
                locations.insert(coord, Location::new(coord, data(coord)));
            }
        }
        Self { locations }
    }

    pub fn insert(&mut self, location: Location<T>) -> Option<Location<T>> {
        self.locations.insert(location.coord, location)
    }

    pub fn remove(&mut self, coord: Axial) -> Option<Location<T>> {
        self.locations.remove(&coord)
    }

    pub fn get(&self, coord: Axial) -> Option<&Location<T>> {
        self.locations.get(&coord)
    }

    pub fn get_mut(&mut self, coord: Axial) -> Option<&mut Location<T>> {
        self.locations.get_mut(&coord)
    }

    pub fn contains(&self, coord: Axial) -> bool {
        self.locations.contains_key(&coord)
    }

    pub fn len(&self) -> usize {
        self.locations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.locations.is_empty()
    }

    /// Iterates the locations. Order is unspecified — sort by coordinate if order matters.
    pub fn iter(&self) -> impl Iterator<Item = &Location<T>> {
        self.locations.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Location<T>> {
        self.locations.values_mut()
    }

    pub fn coords(&self) -> impl Iterator<Item = Axial> + '_ {
        self.locations.keys().copied()
    }

    /// The neighbours of `coord` that actually exist in this grid.
    pub fn neighbours(&self, coord: Axial) -> impl Iterator<Item = &Location<T>> {
        coord
            .neighbours()
            .into_iter()
            .filter_map(|c| self.locations.get(&c))
            .collect::<Vec<_>>()
            .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn hexagon_of_side_four_has_the_expected_rows() {
        let grid = Grid::hexagon(3, |_| ());
        assert_eq!(grid.len(), 37);

        // Rows are constant r. A pointy-top hexagon of side 4 reads 4,5,6,7,6,5,4 top to bottom,
        // and r increases downward, so ordering by r gives exactly that.
        let mut per_row: BTreeMap<i32, usize> = BTreeMap::new();
        for coord in grid.coords() {
            *per_row.entry(coord.r).or_default() += 1;
        }
        let counts: Vec<usize> = per_row.values().copied().collect();
        assert_eq!(counts, vec![4, 5, 6, 7, 6, 5, 4]);
    }

    #[test]
    fn hexagon_is_centred_on_the_origin() {
        let grid = Grid::hexagon(3, |_| ());
        assert!(grid.contains(Axial::ZERO));
        // Every location is within `radius` of the centre, and the corners are present.
        for coord in grid.coords() {
            assert!(coord.distance(Axial::ZERO) <= 3);
        }
        assert!(grid.contains(Axial::new(3, 0)));
        assert!(grid.contains(Axial::new(-3, 0)));
        assert!(grid.contains(Axial::new(0, 3)));
        assert!(!grid.contains(Axial::new(3, 1)));
    }

    #[test]
    fn data_is_attached_per_location() {
        let grid = Grid::hexagon(1, |c| c.q * 10 + c.r);
        assert_eq!(grid.get(Axial::new(1, -1)).map(|l| l.data), Some(9));
        assert_eq!(grid.get(Axial::new(9, 9)), None);
    }

    #[test]
    fn interior_hexes_have_six_neighbours_and_edges_have_fewer() {
        let grid = Grid::hexagon(3, |_| ());
        assert_eq!(grid.neighbours(Axial::ZERO).count(), 6);
        // A corner of the hexagon is missing three of its neighbours.
        assert_eq!(grid.neighbours(Axial::new(3, 0)).count(), 3);
    }

    #[test]
    fn insert_and_remove_round_trip() {
        let mut grid: Grid<i32> = Grid::new();
        assert!(grid.is_empty());
        grid.insert(Location::new(Axial::new(2, 2), 7));
        assert_eq!(grid.get(Axial::new(2, 2)).map(|l| l.data), Some(7));
        assert_eq!(grid.remove(Axial::new(2, 2)).map(|l| l.data), Some(7));
        assert!(grid.is_empty());
    }
}
