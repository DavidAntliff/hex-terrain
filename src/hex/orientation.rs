//! Which way up the hexagons sit.
//!
//! This lives in the model rather than the view because it is **dimensionless** — a discrete choice
//! about which direction rows run in, carrying no world units. The grid's *topology* does not depend
//! on it at all: neighbours, distance, and axial/cube coordinates are identical either way, and
//! pointy versus flat is a 30° rotation of the rendering.
//!
//! Two things do depend on it, and both are presentational:
//!
//! - [`crate::hex::Doubled`] coordinates, because "row" and "column" only mean something once an
//!   orientation is chosen. Offset coordinates would be the same, if they are ever added.
//! - The projection matrices, which live with the projection in [`crate::view::layout`].

/// The orientation of every hexagon in a grid.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Orientation {
    /// A vertex at the top. Rows run east–west. Pairs with doubled *width* coordinates.
    #[default]
    Pointy,
    /// An edge at the top. Columns run north–south. Pairs with doubled *height* coordinates.
    Flat,
}

impl Orientation {
    pub fn other(self) -> Self {
        match self {
            Self::Pointy => Self::Flat,
            Self::Flat => Self::Pointy,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Pointy => "pointy",
            Self::Flat => "flat",
        }
    }

    /// Which doubled-coordinate variant this orientation implies.
    pub fn doubled_variant(self) -> &'static str {
        match self {
            Self::Pointy => "doublewidth",
            Self::Flat => "doubleheight",
        }
    }
}
