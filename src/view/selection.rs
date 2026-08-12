//! Left-click selection: cursor → grid plane → hex.
//!
//! Deliberately arithmetic rather than mesh picking. Inverting the layout is the reference's
//! `pixel_to_hex` plus cube rounding, which the grid needs anyway, and it keeps selection working
//! regardless of how — or whether — a hex is rendered.

use bevy::prelude::*;
use bevy::{math::primitives::InfinitePlane3d, picking::hover::Hovered};

use super::layout::HexLayout;
use super::GridModel;
use crate::hex::Axial;

/// The active hex, if any.
#[derive(Resource, Default, Debug, PartialEq)]
pub struct Selected(pub Option<Axial>);

pub fn select_on_click(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    hovered_ui: Query<&Hovered, With<Node>>,
    mut selected: ResMut<Selected>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }

    // A click on the UI belongs to the UI: without this, pressing the button would also select
    // whatever hex happens to sit behind it.
    if hovered_ui.iter().any(|hovered| hovered.0) {
        return;
    }

    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let (camera, camera_transform) = *camera;
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor) else {
        return;
    };

    let plane = InfinitePlane3d::new(layout.plane.normal());
    let Some(distance) = ray.intersect_plane(layout.origin, plane) else {
        return;
    };

    let coord = layout
        .world_to_hex(ray.get_point(distance))
        .round()
        .to_axial();

    // Clicking off the grid clears the selection, which is less surprising than keeping a stale one.
    selected.0 = grid.contains(coord).then_some(coord);
}
