//! Named starting grids, so a demo can be picked without an edit.
//!
//! A scene is a whole grid, water included — dimensionless model data, with nothing in it about how
//! any of it is drawn. Water is a level per location or none, written here directly or by [`flood`];
//! the renderer displays whatever it finds, and a real flooding algorithm will write the same field.

use std::cmp::Ordering;

use super::{Grid, SEA_LEVEL, Terrain, TerrainGrid, flood, undulating};

/// Hexagon rings around the centre. Radius 3 is a hexagon of side 4: 37 locations.
pub const RADIUS: i32 = 3;

/// The scene built when none is named.
pub const DEFAULT: &str = "sea";

/// Builds a scene's grid from nothing. A plain `fn` pointer, so the table below can be a `const`.
pub type Scene = fn() -> TerrainGrid;

/// Every scene there is, by name.
pub const SCENES: &[(&str, Scene)] = &[
    ("sea", sea),
    ("two-lakes", two_lakes),
    ("terraces", terraces),
    ("biomes", biomes),
];

/// Builds the named scene, or `None` if there is no such scene.
pub fn build(name: &str) -> Option<TerrainGrid> {
    SCENES
        .iter()
        .find(|(scene, _)| *scene == name)
        .map(|(_, build)| build())
}

pub fn names() -> impl Iterator<Item = &'static str> {
    SCENES.iter().map(|(name, _)| *name)
}

/// The default: undulating ground flooded to [`SEA_LEVEL`], one body over the whole grid, which is
/// what the debug panel's sea-level slider drives.
fn sea() -> TerrainGrid {
    let mut grid = Grid::hexagon(RADIUS, undulating);
    flood(&mut grid, SEA_LEVEL);
    grid
}

/// The floor of both basins.
const BASIN: f32 = -0.6;
/// The land bridge's ground, above both water levels so neither body can drain into the other.
const BRIDGE: f32 = 0.95;
const HIGH: f32 = 0.55;
const LOW: f32 = 0.0;

/// Two bodies of water at different levels, divided by a land bridge one hex wide.
///
/// The axial line `q = 0` is a straight run of seven hexes, and no `q = -1` hex is adjacent to any
/// `q = +1` hex, so the two basins touch nothing but the bridge. The bridge stands above both levels,
/// which is what makes this terrain a real terrain could hold rather than a contrivance.
///
/// It exists to show what the renderer does with it. A location beside two bodies draws a plate for
/// each — it does not pick one — but a plate covers the location's *whole* hexagon, and the bridge's
/// wall dips to the mean of the two heights either side on both flanks. So the higher water is
/// exposed over the lower body's shore, and ends at the hexagon boundary in a wall of water.
fn two_lakes() -> TerrainGrid {
    Grid::hexagon(RADIUS, |coord| match coord.q.cmp(&0) {
        Ordering::Less => Terrain {
            height: BASIN,
            water: Some(HIGH),
        },
        Ordering::Equal => Terrain {
            height: BRIDGE,
            water: None,
        },
        Ordering::Greater => Terrain {
            height: BASIN,
            water: Some(LOW),
        },
    })
}

/// Three bodies at three levels, divided by two land bridges one hex wide.
///
/// The columns run `q ≤ -1` high body, `q = 0` bridge, `q = 1` middle body, `q = 2` bridge,
/// `q = 3` low body. Neither bridge column is adjacent to a body it does not divide, and each stands
/// above both of the levels it does — so, as in [`two_lakes`], no body can drain into another.
///
/// It exists for the case [`two_lakes`] cannot show. The first bridge is **tall**: its wall towards
/// the middle body stands above that body's level, so only the high body reaches it. The second is
/// **low**: its wall dips below both adjacent levels, so both bodies reach it and each is confined to
/// its own half of the cell. That is the closest two levels can legitimately come to each other, and
/// where they meet is a step in the water that no rendering rule can remove — only place correctly.
fn terraces() -> TerrainGrid {
    /// The bodies, from the high side, as `(bed, level)`.
    const BODIES: [(f32, f32); 3] = [(-0.6, 0.55), (-0.6, 0.0), (-0.8, -0.3)];
    /// The bridges dividing them, above both of the levels either side.
    const BRIDGES: [f32; 2] = [0.95, 0.1];

    Grid::hexagon(RADIUS, |coord| match coord.q {
        q if q <= -1 => body(BODIES[0]),
        0 => Terrain {
            height: BRIDGES[0],
            water: None,
        },
        1 => body(BODIES[1]),
        2 => Terrain {
            height: BRIDGES[1],
            water: None,
        },
        _ => body(BODIES[2]),
    })
}

/// The elevations `biomes` ramps between, low column to high.
const RAMP: (f32, f32) = (-0.5, 1.0);
/// Where its outlier peak stands, and how high. Deep in the low ground, so the biome it reaches is
/// as far as possible from the ones around it.
const PEAK: (i32, i32) = (-2, 1);

/// A ramp across every biome band, for looking at how the surface is coloured.
///
/// Height rises linearly with `q`, so the seven columns of the grid cross all five bands in order
/// and every consecutive pair of biomes meets along a straight edge. Flooded to [`SEA_LEVEL`], which
/// submerges the low columns — the sea bed and the strand above it are both sand, so the water line
/// is the one biome boundary here that is not an elevation threshold.
///
/// A ramp alone only ever puts *consecutive* biomes next to each other, which is all a continuous
/// height field can produce. The outlier peak is what supplies the other case: a stepped terrain can
/// stand snow directly against sand across a single cliff, and a transition that only has to blend
/// its immediate neighbour is not the one worth checking.
fn biomes() -> TerrainGrid {
    let (low, high) = RAMP;
    let columns = (2 * RADIUS) as f32;
    let mut grid = Grid::hexagon(RADIUS, |coord| {
        let step = (coord.q + RADIUS) as f32 / columns;
        Terrain {
            height: low + (high - low) * step,
            water: None,
        }
    });
    if let Some(peak) = grid.get_mut(super::Axial::new(PEAK.0, PEAK.1)) {
        peak.data.height = high;
    }
    flood(&mut grid, SEA_LEVEL);
    grid
}

/// A location under a body of water: its bed, and the level standing over it.
fn body((bed, level): (f32, f32)) -> Terrain {
    Terrain {
        height: bed,
        water: Some(level),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::Axial;

    #[test]
    fn every_scene_is_a_hexagon_of_side_four() {
        for name in names() {
            let grid = build(name).expect("a registered scene builds");
            assert_eq!(grid.len(), 37, "{name}");
            assert!(grid.contains(Axial::ZERO), "{name}");
        }
        assert!(build(DEFAULT).is_some(), "the default names a scene");
        assert!(build("no-such-scene").is_none());
    }

    /// The demo is only fair if its data is data a real terrain could hold: the bridge stands above
    /// both bodies, so neither drains into the other, and the basins touch only through it. Without
    /// that, the artefact it shows could be dismissed as nonsense input.
    #[test]
    fn the_bridge_divides_two_levels_it_stands_above() {
        // Two levels, or the assertions below pass on one body and prove nothing.
        const { assert!(HIGH > LOW) };
        let grid = two_lakes();

        for location in grid.iter().filter(|l| l.coord.q == 0) {
            assert!(location.data.water.is_none(), "the bridge is dry");
            assert!(
                location.data.height > HIGH,
                "the bridge stands above both bodies"
            );
            let around: Vec<Option<f32>> = grid
                .neighbours(location.coord)
                .map(|l| l.data.water)
                .collect();
            assert!(
                around.contains(&Some(HIGH)) && around.contains(&Some(LOW)),
                "{:?} should border both bodies, borders {around:?}",
                location.coord
            );
        }

        // Away from the bridge, neighbours are always in the same basin, so there is no path for one
        // body to reach the other.
        for location in grid.iter().filter(|l| l.coord.q != 0) {
            for neighbour in grid.neighbours(location.coord).filter(|l| l.coord.q != 0) {
                assert_eq!(
                    location.data.water, neighbour.data.water,
                    "{:?} and {:?} are in different basins",
                    location.coord, neighbour.coord
                );
            }
        }
    }

    /// The `biomes` scene earns its place only if it actually shows what it is for: all five
    /// biomes, every consecutive pair meeting somewhere, and at least one pair that a ramp alone
    /// could never produce.
    #[test]
    fn the_biomes_scene_shows_every_biome_and_a_non_consecutive_pair() {
        use crate::hex::{Bands, Biome};
        let bands = Bands::default();
        let grid = biomes();
        let biome_at = |coord| Biome::at(&grid.get(coord).expect("in the grid").data, &bands);

        let present: Vec<Biome> = Biome::ALL
            .into_iter()
            .filter(|b| grid.iter().any(|l| Biome::at(&l.data, &bands) == *b))
            .collect();
        assert_eq!(present, Biome::ALL.to_vec(), "not every biome appears");

        let mut gaps = Vec::new();
        for location in grid.iter() {
            let ours = Biome::at(&location.data, &bands);
            for neighbour in grid.neighbours(location.coord) {
                let theirs = biome_at(neighbour.coord);
                gaps.push(ours.index().abs_diff(theirs.index()));
            }
        }
        assert!(
            gaps.contains(&1),
            "no two neighbouring cells are consecutive biomes"
        );
        assert!(
            gaps.iter().any(|gap| *gap > 1),
            "every transition is between consecutive biomes, so the peak is not doing its job"
        );
    }

    /// What `terraces` adds: one bridge only the higher body reaches, and one that both bodies
    /// either side reach — the closest two levels can legitimately come to one another.
    #[test]
    fn terraces_puts_one_body_on_the_tall_bridge_and_two_on_the_low_one() {
        let grid = terraces();
        let at = |q, r| grid.get(Axial::new(q, r)).expect("in the grid").data;

        // No body touches a body at another level, so none can drain into another.
        for location in grid.iter() {
            for neighbour in grid.neighbours(location.coord) {
                if let (Some(ours), Some(theirs)) = (location.data.water, neighbour.data.water) {
                    assert_eq!(
                        ours, theirs,
                        "{:?} and {:?} are different bodies, touching",
                        location.coord, neighbour.coord
                    );
                }
            }
        }

        // A wall between two locations sits at the mean of their heights, which is what decides
        // whether a body reaches across it onto the bridge.
        let wall = |a: Terrain, b: Terrain| (a.height + b.height) / 2.0;
        let level = |t: Terrain| t.water.expect("a body");
        let (high, tall, middle, low_bridge, low) =
            (at(-1, 0), at(0, 0), at(1, 0), at(2, 0), at(3, 0));

        assert!(
            tall.height > level(high) && tall.height > level(middle),
            "the tall bridge stands above both levels it divides"
        );
        assert!(wall(tall, high) < level(high), "the high body reaches it");
        assert!(
            wall(tall, middle) > level(middle),
            "the middle body does not"
        );

        assert!(
            low_bridge.height > level(middle) && low_bridge.height > level(low),
            "the low bridge stands above both levels it divides"
        );
        assert!(
            wall(low_bridge, middle) < level(middle),
            "the middle body reaches it"
        );
        assert!(
            wall(low_bridge, low) < level(low),
            "and so does the low body"
        );
    }
}
