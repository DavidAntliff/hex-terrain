//! A standalone compass showing how the axial/cube axes are oriented.
//!
//! Its own entity with its own [`Transform`], drawn beside the grid, so it can be moved without
//! touching the grid. The six half-axes come from [`HexLayout::axis_arrows`], and a reference
//! hexagon is drawn beneath them: the axes line up with the hexagon's vertices in either
//! orientation, so the drawing checks itself.

use bevy::{gizmos::config::GizmoConfigGroup, prelude::*, reflect::Reflect};

use super::layout::HexLayout;
use super::world_label::{WorldLabel, world_label};

/// Gizmo group for the compass, so its line width is independent of the grid's.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct CompassLines;

/// Where the compass sits: south of a side-4 grid at unit scale, clear of its bottom row.
///
/// South rather than west because the vertical extent of the view depends only on the field of
/// view, whereas the horizontal extent also depends on the window's aspect ratio — placed to the
/// side, the compass falls off-screen in a portrait window. The clearance accounts for flat-top,
/// whose grid reaches slightly further south than pointy-top's.
const COMPASS_OFFSET: Vec3 = Vec3::new(0.0, 0.0, 8.0);
const ARM: f32 = 1.4;
const LABEL_GAP: f32 = 0.4;
const LINE_WIDTH: f32 = 2.5;

const HEX_COLOR: Color = Color::srgb(0.30, 0.34, 0.42);
const Q_COLOR: Color = Color::srgb(0.96, 0.45, 0.45);
const R_COLOR: Color = Color::srgb(0.50, 0.86, 0.55);
const S_COLOR: Color = Color::srgb(0.55, 0.68, 0.98);

/// Whether the compass is drawn. Off unless asked for.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowCompass(pub bool);

/// The compass root. Its transform positions the whole widget.
#[derive(Component)]
pub struct Compass;

/// One of the compass's six axis labels, identified by its index into
/// [`HexLayout::axis_arrows`] so it can be re-anchored when the orientation changes.
#[derive(Component)]
pub struct CompassLabel {
    pub index: usize,
}

/// Centre and in-plane radius of the whole widget, for camera framing.
///
/// Kept here so the constants above stay the single source of the compass's footprint.
pub fn bounds(layout: &HexLayout) -> (Vec3, f32) {
    // The labels sit furthest out; allow for the text itself, which is drawn in screen space and so
    // has no world size to measure.
    const LABEL_ALLOWANCE: f32 = 0.35;
    let reach = (ARM + LABEL_GAP + LABEL_ALLOWANCE).max(layout.size.max_element());
    (layout.origin + COMPASS_OFFSET, reach)
}

pub fn configure_gizmo_width(mut store: ResMut<GizmoConfigStore>) {
    store.config_mut::<CompassLines>().0.line.width = LINE_WIDTH;
}

pub fn spawn_compass(mut commands: Commands, layout: Res<HexLayout>) {
    let centre = layout.origin + COMPASS_OFFSET;
    commands.spawn((Compass, Transform::from_translation(centre)));

    for (index, arrow) in layout.axis_arrows().into_iter().enumerate() {
        commands.spawn((
            CompassLabel { index },
            world_label(
                label_anchor(centre, arrow.direction),
                arrow.label,
                15.0,
                axis_color(arrow.label),
            ),
        ));
    }
}

/// Just beyond the arrow tip, so the text does not sit on the line.
fn label_anchor(centre: Vec3, direction: Vec3) -> Vec3 {
    centre + direction * (ARM + LABEL_GAP)
}

fn axis_color(label: &str) -> Color {
    match label.as_bytes()[1] {
        b'q' => Q_COLOR,
        b'r' => R_COLOR,
        _ => S_COLOR,
    }
}

/// The axis directions rotate with the orientation, so the labels have to follow.
pub fn sync_compass_labels(
    layout: Res<HexLayout>,
    show: Res<ShowCompass>,
    compass: Single<&Transform, With<Compass>>,
    mut labels: Query<(&CompassLabel, &mut WorldLabel, &mut Text, &mut TextColor)>,
) {
    if !layout.is_changed() && !show.is_changed() {
        return;
    }
    let arrows = layout.axis_arrows();
    for (label, mut anchor, mut text, mut color) in &mut labels {
        let arrow = arrows[label.index];
        anchor.anchor = label_anchor(compass.translation, arrow.direction);
        anchor.visible = show.0;
        // Which half-axis sits at a given index can change with orientation, so keep the text and
        // colour in step rather than assuming they are fixed.
        **text = arrow.label.to_string();
        color.0 = axis_color(arrow.label);
    }
}

pub fn draw_compass(
    mut gizmos: Gizmos<CompassLines>,
    layout: Res<HexLayout>,
    show: Res<ShowCompass>,
    compass: Single<&Transform, With<Compass>>,
) {
    if !show.0 {
        return;
    }
    let centre = compass.translation;

    // A reference hexagon at the grid's scale and orientation, for the axes to point at.
    let corners = layout.corner_offsets().map(|offset| centre + offset);
    gizmos.linestrip(
        corners.into_iter().chain(std::iter::once(corners[0])),
        HEX_COLOR,
    );

    for arrow in layout.axis_arrows() {
        gizmos.arrow(
            centre,
            centre + arrow.direction * ARM,
            axis_color(arrow.label),
        );
    }
}
