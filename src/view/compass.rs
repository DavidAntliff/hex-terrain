//! A standalone compass showing how the axial/cube axes are oriented.
//!
//! Its own entity with its own [`Transform`], drawn beside the grid, so it can be moved without
//! touching the grid. The six half-axes come from [`HexLayout::axis_arrows`], and a reference
//! hexagon is drawn beneath them: for a pointy-top layout the axes line up with the hexagon's
//! vertices, so the drawing checks itself.

use bevy::{gizmos::config::GizmoConfigGroup, prelude::*, reflect::Reflect};

use super::layout::HexLayout;
use super::world_label::world_label;

/// Gizmo group for the compass, so its line width is independent of the grid's.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct CompassLines;

/// Where the compass sits: south of a side-4 grid at unit scale, clear of its bottom row.
///
/// South rather than west because the vertical extent of the view depends only on the field of
/// view, whereas the horizontal extent also depends on the window's aspect ratio — placed to the
/// side, the compass falls off-screen in a portrait window.
const COMPASS_OFFSET: Vec3 = Vec3::new(0.0, 0.0, 7.5);
const ARM: f32 = 1.4;
const LINE_WIDTH: f32 = 2.5;

const HEX_COLOR: Color = Color::srgb(0.30, 0.34, 0.42);
const Q_COLOR: Color = Color::srgb(0.96, 0.45, 0.45);
const R_COLOR: Color = Color::srgb(0.50, 0.86, 0.55);
const S_COLOR: Color = Color::srgb(0.55, 0.68, 0.98);

/// The compass root. Its transform positions the whole widget.
#[derive(Component)]
pub struct Compass;

pub fn configure_gizmo_width(mut store: ResMut<GizmoConfigStore>) {
    store.config_mut::<CompassLines>().0.line.width = LINE_WIDTH;
}

pub fn spawn_compass(mut commands: Commands, layout: Res<HexLayout>) {
    let centre = layout.origin + COMPASS_OFFSET;
    commands.spawn((Compass, Transform::from_translation(centre)));

    for arrow in layout.axis_arrows() {
        // Just beyond the arrow tip, so the text does not sit on the line.
        let anchor = centre + arrow.direction * (ARM + 0.4);
        commands.spawn(world_label(
            anchor,
            arrow.label,
            15.0,
            axis_color(arrow.label),
        ));
    }
}

fn axis_color(label: &str) -> Color {
    match label.as_bytes()[1] {
        b'q' => Q_COLOR,
        b'r' => R_COLOR,
        _ => S_COLOR,
    }
}

pub fn draw_compass(
    mut gizmos: Gizmos<CompassLines>,
    layout: Res<HexLayout>,
    compass: Single<&Transform, With<Compass>>,
) {
    let centre = compass.translation;

    // A reference hexagon at the same scale as the grid's, for the axes to point at.
    let corners = layout.corner_offsets().map(|offset| centre + offset);
    gizmos.linestrip(
        corners.into_iter().chain(std::iter::once(corners[0])),
        HEX_COLOR,
    );

    for arrow in layout.axis_arrows() {
        let tip = centre + arrow.direction * ARM;
        gizmos.arrow(centre, tip, axis_color(arrow.label));
    }
}
