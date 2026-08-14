//! What kind of ground a location is, derived from what the model already holds.
//!
//! A biome is a **function of elevation and water**, not a field on [`Terrain`]. Nothing authors
//! biomes yet — there is no generator and no editor — so a stored field would only ever cache what
//! this computes, and every construction of a `Terrain` would have to name it. When something does
//! author them, this becomes the default rather than something the generator has to displace.
//!
//! Dimensionless, like the rest of the model: the thresholds are in the same units as
//! [`Terrain::height`], and what a biome *looks* like belongs to the renderer.

use super::Terrain;

/// The kinds of ground the terrain is drawn as, in ascending order of elevation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Biome {
    /// Shore and sea bed. The default because it is what unclassified low ground is.
    #[default]
    Sand,
    Grass,
    Woodland,
    Rock,
    Snow,
}

impl Biome {
    /// Every biome, in the order their bands stack. The index into this is what the renderer keys
    /// its per-biome assets on, so it is also the packing order used on the wire to the shader.
    pub const ALL: [Biome; 5] = [
        Biome::Sand,
        Biome::Grass,
        Biome::Woodland,
        Biome::Rock,
        Biome::Snow,
    ];

    /// This biome's position in [`Self::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// The biome of a location: sand wherever water stands over the ground, and otherwise whichever
    /// band the ground's elevation falls in.
    ///
    /// **Submerged ground is sand** so that a shoreline emerges as beach rather than as whatever the
    /// bed's elevation would otherwise make it — the sea bed and the strand above it are the same
    /// material, and the water line is where it stops being wet rather than where it changes.
    ///
    /// Bands out of order are not rejected. The cascade simply skips whichever bands it inverts,
    /// which costs those biomes rather than misclassifying anything, and a slider dragged past its
    /// neighbour corrects itself on the way back.
    pub fn at(terrain: &Terrain, bands: &Bands) -> Biome {
        if terrain.water.is_some_and(|level| level > terrain.height) {
            return Biome::Sand;
        }
        let height = terrain.height;
        if height < bands.grass {
            Biome::Sand
        } else if height < bands.woodland {
            Biome::Grass
        } else if height < bands.rock {
            Biome::Woodland
        } else if height < bands.snow {
            Biome::Rock
        } else {
            Biome::Snow
        }
    }
}

/// The elevations the biomes change at, each the lower bound of the biome it names, in the same
/// dimensionless units as [`Terrain::height`].
///
/// Four thresholds for five biomes: below the first is sand, above the last is snow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bands {
    pub grass: f32,
    pub woodland: f32,
    pub rock: f32,
    pub snow: f32,
}

impl Default for Bands {
    /// Pitched against [`super::undulating`], which spans roughly `-1..=1` and is flooded to
    /// [`super::SEA_LEVEL`]. The sand band is deliberately narrow — a strand is a thin thing, and
    /// most of what is drawn as sand is the sea bed under the water rather than dry ground.
    fn default() -> Self {
        Self {
            grass: 0.05,
            woodland: 0.30,
            rock: 0.55,
            snow: 0.80,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dry(height: f32) -> Terrain {
        Terrain {
            height,
            water: None,
        }
    }

    #[test]
    fn each_band_starts_at_its_threshold() {
        let bands = Bands::default();
        // A threshold belongs to the biome above it, so a location exactly on one has already
        // changed. Checked at the boundary rather than around it, because that is the only value
        // the comparison can get wrong.
        assert_eq!(Biome::at(&dry(bands.grass - 0.01), &bands), Biome::Sand);
        assert_eq!(Biome::at(&dry(bands.grass), &bands), Biome::Grass);
        assert_eq!(Biome::at(&dry(bands.woodland), &bands), Biome::Woodland);
        assert_eq!(Biome::at(&dry(bands.rock), &bands), Biome::Rock);
        assert_eq!(Biome::at(&dry(bands.snow), &bands), Biome::Snow);
    }

    #[test]
    fn the_extremes_land_in_the_outer_bands() {
        let bands = Bands::default();
        assert_eq!(Biome::at(&dry(-10.0), &bands), Biome::Sand);
        assert_eq!(Biome::at(&dry(10.0), &bands), Biome::Snow);
    }

    /// Water wins over elevation, however high the ground under it stands — otherwise a flooded
    /// mountain basin would draw its bed as snow through the water.
    #[test]
    fn ground_under_water_is_sand_whatever_its_height() {
        let bands = Bands::default();
        for height in [-1.0, 0.0, 0.5, 2.0] {
            let submerged = Terrain {
                height,
                water: Some(height + 0.1),
            };
            assert_eq!(Biome::at(&submerged, &bands), Biome::Sand, "at {height}");
        }
    }

    /// `Terrain::surface` guards against water below the ground it claims to cover; so does this,
    /// and it has to agree — ground standing clear of its own water is dry land, not sea bed.
    #[test]
    fn ground_standing_above_its_water_is_classified_by_height() {
        let bands = Bands::default();
        let exposed = Terrain {
            height: 0.9,
            water: Some(-0.5),
        };
        assert_eq!(Biome::at(&exposed, &bands), Biome::Snow);

        // And the exact tie: water level with the ground covers nothing.
        let level = Terrain {
            height: 0.9,
            water: Some(0.9),
        };
        assert_eq!(Biome::at(&level, &bands), Biome::Snow);
    }

    #[test]
    fn every_biome_is_at_its_own_index_in_all() {
        for (index, biome) in Biome::ALL.into_iter().enumerate() {
            assert_eq!(biome.index(), index, "{biome:?}");
        }
    }

    /// The default bands have to be usable: ascending, so every one of the five is reachable.
    #[test]
    fn the_default_bands_ascend_and_reach_every_biome() {
        let bands = Bands::default();
        assert!(bands.grass < bands.woodland);
        assert!(bands.woodland < bands.rock);
        assert!(bands.rock < bands.snow);

        let reached: Vec<Biome> = [-1.0, 0.1, 0.4, 0.6, 1.0]
            .into_iter()
            .map(|h| Biome::at(&dry(h), &bands))
            .collect();
        assert_eq!(reached, Biome::ALL.to_vec());
    }
}
