//! Per-hex coordinate labels, in whichever system is currently selected — or none.

use bevy::prelude::*;

use super::GridModel;
use super::layout::HexLayout;
use super::world_label::{WorldLabel, world_label};
use crate::hex::{Axial, Orientation};

const LABEL_SIZE: f32 = 11.0;
const LABEL_COLOR: Color = Color::srgb(0.85, 0.89, 0.95);

/// Which coordinate system the hex labels show, `Off` included.
///
/// "No labels" is a mode rather than a separate on/off flag, so there is one piece of state
/// governing the labels instead of two that could disagree.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelMode {
    Axial,
    Cube,
    Doubled,
    #[default]
    Off,
}

impl LabelMode {
    pub fn next(self) -> Self {
        match self {
            Self::Axial => Self::Cube,
            Self::Cube => Self::Doubled,
            Self::Doubled => Self::Off,
            Self::Off => Self::Axial,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Axial => "axial",
            Self::Cube => "cube",
            Self::Doubled => "doubled",
            Self::Off => "off",
        }
    }

    pub fn shows_labels(self) -> bool {
        self != Self::Off
    }

    /// The label for one hex: two numbers for axial and doubled, three for cube.
    ///
    /// Doubled needs the orientation, since which axis doubles depends on it.
    pub fn format(self, coord: Axial, orientation: Orientation) -> String {
        match self {
            Self::Axial => format!("{},{}", coord.q, coord.r),
            Self::Cube => {
                let c = coord.to_cube();
                format!("{},{},{}", c.q, c.r, c.s)
            }
            Self::Doubled => {
                let d = coord.to_doubled(orientation);
                format!("{},{}", d.col, d.row)
            }
            // Never rendered; the labels are hidden in this mode.
            Self::Off => String::new(),
        }
    }
}

/// Marks a label belonging to a grid hex, as opposed to the compass.
#[derive(Component)]
pub struct HexLabel {
    pub coord: Axial,
}

pub fn spawn_labels(
    mut commands: Commands,
    grid: Res<GridModel>,
    layout: Res<HexLayout>,
    mode: Res<LabelMode>,
) {
    for location in grid.iter() {
        let coord = location.coord;
        commands.spawn((
            HexLabel { coord },
            world_label(
                layout.surface_centre(coord, location.data.surface()),
                mode.format(coord, layout.orientation),
                LABEL_SIZE,
                LABEL_COLOR,
            ),
        ));
    }
}

/// Re-formats the labels when the mode changes, or when the layout does — doubled coordinates
/// depend on the orientation, so toggling pointy/flat changes what they should read.
pub fn update_label_text(
    mode: Res<LabelMode>,
    layout: Res<HexLayout>,
    mut labels: Query<(&HexLabel, &mut Text)>,
) {
    if (!mode.is_changed() && !layout.is_changed()) || !mode.shows_labels() {
        return;
    }
    for (label, mut text) in &mut labels {
        **text = mode.format(label.coord, layout.orientation);
    }
}

pub fn sync_label_visibility(
    mode: Res<LabelMode>,
    mut labels: Query<&mut WorldLabel, With<HexLabel>>,
) {
    if !mode.is_changed() {
        return;
    }
    for mut label in &mut labels {
        label.visible = mode.shows_labels();
    }
}

/// Re-anchors the labels so they track a rescaled or reoriented grid, and follow a location's
/// surface up onto the water when it floods.
pub fn sync_label_anchors(
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    mut labels: Query<(&HexLabel, &mut WorldLabel)>,
) {
    if !layout.is_changed() && !grid.is_changed() {
        return;
    }
    for (label, mut anchor) in &mut labels {
        let Some(location) = grid.get(label.coord) else {
            continue;
        };
        anchor.anchor = layout.surface_centre(label.coord, location.data.surface());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_cycles_through_all_four_and_returns() {
        let mut mode = LabelMode::Axial;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen.push(mode.name());
            mode = mode.next();
        }
        assert_eq!(seen, ["axial", "cube", "doubled", "off"]);
        assert_eq!(mode, LabelMode::Axial, "cycling should return to the start");
    }

    #[test]
    fn only_the_off_mode_hides_the_labels() {
        assert!(!LabelMode::Off.shows_labels());
        for mode in [LabelMode::Axial, LabelMode::Cube, LabelMode::Doubled] {
            assert!(mode.shows_labels(), "{mode:?} should show labels");
        }
    }

    #[test]
    fn labels_show_the_coordinates_of_the_selected_system() {
        // Hand-checked: axial (2,1) is cube (2,1,-3); doubled is (5,1) under pointy-top's doubled
        // width and (2,4) under flat-top's doubled height.
        let pointy = Orientation::Pointy;
        let coord = Axial::new(2, 1);
        assert_eq!(LabelMode::Axial.format(coord, pointy), "2,1");
        assert_eq!(LabelMode::Cube.format(coord, pointy), "2,1,-3");
        assert_eq!(LabelMode::Doubled.format(coord, pointy), "5,1");

        // Only the doubled label depends on orientation.
        let flat = Orientation::Flat;
        assert_eq!(LabelMode::Axial.format(coord, flat), "2,1");
        assert_eq!(LabelMode::Cube.format(coord, flat), "2,1,-3");
        assert_eq!(LabelMode::Doubled.format(coord, flat), "2,4");

        // The origin reads as zero in every system, either orientation.
        for orientation in [pointy, flat] {
            assert_eq!(LabelMode::Axial.format(Axial::ZERO, orientation), "0,0");
            assert_eq!(LabelMode::Cube.format(Axial::ZERO, orientation), "0,0,0");
            assert_eq!(LabelMode::Doubled.format(Axial::ZERO, orientation), "0,0");
        }
    }
}
