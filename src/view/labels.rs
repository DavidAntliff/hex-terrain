//! Per-hex coordinate labels, in whichever system is currently selected.

use bevy::prelude::*;

use super::layout::HexLayout;
use super::world_label::{world_label, WorldLabel};
use super::GridModel;
use crate::hex::Axial;

const LABEL_SIZE: f32 = 11.0;
const LABEL_COLOR: Color = Color::srgb(0.85, 0.89, 0.95);

/// Which coordinate system the hex labels show.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelMode {
    #[default]
    Axial,
    Cube,
    Doubled,
}

impl LabelMode {
    pub fn next(self) -> Self {
        match self {
            Self::Axial => Self::Cube,
            Self::Cube => Self::Doubled,
            Self::Doubled => Self::Axial,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Axial => "axial",
            Self::Cube => "cube",
            Self::Doubled => "doubled",
        }
    }

    /// The label for one hex: two numbers for axial and doubled, three for cube.
    pub fn format(self, coord: Axial) -> String {
        match self {
            Self::Axial => format!("{},{}", coord.q, coord.r),
            Self::Cube => {
                let c = coord.to_cube();
                format!("{},{},{}", c.q, c.r, c.s)
            }
            Self::Doubled => {
                let d = coord.to_doubled();
                format!("{},{}", d.col, d.row)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_cycles_through_all_three_and_returns() {
        let mut mode = LabelMode::Axial;
        let mut seen = Vec::new();
        for _ in 0..3 {
            seen.push(mode.name());
            mode = mode.next();
        }
        assert_eq!(seen, ["axial", "cube", "doubled"]);
        assert_eq!(mode, LabelMode::Axial, "cycling should return to the start");
    }

    #[test]
    fn labels_show_the_coordinates_of_the_selected_system() {
        // Two components for axial and doubled, three for cube. Hand-checked against the
        // conversions: axial (1,-1) is cube (1,-1,0) and doubled (col 1, row -1).
        let coord = Axial::new(1, -1);
        assert_eq!(LabelMode::Axial.format(coord), "1,-1");
        assert_eq!(LabelMode::Cube.format(coord), "1,-1,0");
        assert_eq!(LabelMode::Doubled.format(coord), "1,-1");
        // The origin reads as zero in every system.
        assert_eq!(LabelMode::Axial.format(Axial::ZERO), "0,0");
        assert_eq!(LabelMode::Cube.format(Axial::ZERO), "0,0,0");
        assert_eq!(LabelMode::Doubled.format(Axial::ZERO), "0,0");
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
    for coord in grid.coords() {
        commands.spawn((
            HexLabel { coord },
            world_label(
                layout.hex_to_world(coord),
                mode.format(coord),
                LABEL_SIZE,
                LABEL_COLOR,
            ),
        ));
    }
}

pub fn update_label_text(mode: Res<LabelMode>, mut labels: Query<(&HexLabel, &mut Text)>) {
    if !mode.is_changed() {
        return;
    }
    for (label, mut text) in &mut labels {
        **text = mode.format(label.coord);
    }
}

/// Re-anchors the labels if the layout changes, so they track a rescaled grid.
pub fn sync_label_anchors(
    layout: Res<HexLayout>,
    mut labels: Query<(&HexLabel, &mut WorldLabel)>,
) {
    if !layout.is_changed() {
        return;
    }
    for (label, mut anchor) in &mut labels {
        anchor.anchor = layout.hex_to_world(label.coord);
    }
}
