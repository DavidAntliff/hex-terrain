//! Left-click selection: cursor → terrain surface → hex.
//!
//! Deliberately arithmetic rather than mesh picking. Inverting the layout is the reference's
//! `pixel_to_hex` plus cube rounding, which the grid needs anyway, and it keeps selection working
//! regardless of how — or whether — a hex is rendered.
//!
//! Only the surface faces are selectable: the top of a raised hex, the floor of a sunken one. A ray
//! that crosses a prism's wall lands outside that hex's footprint and is rejected, so it carries on
//! to whatever surface lies beyond.

use bevy::prelude::*;
use bevy::{math::primitives::InfinitePlane3d, picking::hover::Hovered};

use super::layout::HexLayout;
use super::GridModel;
use crate::hex::{Axial, TerrainGrid};

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

    // Hitting nothing clears the selection, which is less surprising than keeping a stale one.
    selected.0 = pick_surface(ray, &layout, &grid);
}

/// The hex whose surface the ray meets first, if any.
///
/// Each location contributes one horizontal face at whatever it presents — its cap, or the water
/// standing over it, which is what a click on a lake lands on. A hit counts only if it falls inside
/// that location's own footprint, which is what makes walls transparent to selection.
///
// ponytail: a linear scan over every location, and walls do not occlude — only surfaces are
// tested, so at a grazing angle a low cap hidden behind a taller neighbour can still be picked.
// Test the six wall quads per hex if that ever shows.
fn pick_surface(ray: Ray3d, layout: &HexLayout, grid: &TerrainGrid) -> Option<Axial> {
    let plane = InfinitePlane3d::new(layout.plane.normal());
    grid.iter()
        .filter_map(|location| {
            let surface = layout.surface_centre(location.coord, location.data.surface());
            let distance = ray.intersect_plane(surface, plane)?;
            let point = ray.get_point(distance);
            let hit = layout.world_to_hex(point).round().to_axial();
            (hit == location.coord).then_some((distance, location.coord))
        })
        .min_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, coord)| coord)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::{undulating, Grid, Location, Terrain};

    fn down_at(point: Vec3) -> Ray3d {
        Ray3d::new(point + Vec3::Y * 50.0, Dir3::NEG_Y)
    }

    #[test]
    fn looking_straight_down_picks_the_hex_below_whatever_its_height() {
        for height_scale in [0.5, 4.0] {
            let layout = HexLayout::pointy(1.3).with_height_scale(height_scale);
            let grid = Grid::hexagon(3, undulating);
            for location in grid.iter() {
                let coord = location.coord;
                let surface = layout.surface_centre(coord, location.data.surface());
                assert_eq!(pick_surface(down_at(surface), &layout, &grid), Some(coord));
                // Off-centre, but still inside the hex.
                let corner = layout.corners(coord)[0] + layout.elevation(location.data.surface());
                let probe = surface + (corner - surface) * 0.8;
                assert_eq!(pick_surface(down_at(probe), &layout, &grid), Some(coord));
            }
        }
    }

    #[test]
    fn the_nearest_surface_wins_and_walls_are_not_selectable() {
        // Two hexes side by side on the east-west axis: a tall column at the origin, a pit next
        // door. `+q` is due east on a pointy layout, so the ray can come in low from the east.
        let layout = HexLayout::pointy(1.0);
        let (column, pit) = (Axial::ZERO, Axial::new(1, 0));
        let mut grid: TerrainGrid = Grid::new();
        grid.insert(Location::new(column, Terrain { height: 3.0, water: None }));
        grid.insert(Location::new(pit, Terrain { height: -1.0, water: None }));

        // Straight down over the column: the column, not the plane it stands on.
        let top = layout.surface_centre(column, 3.0);
        assert_eq!(pick_surface(down_at(top), &layout, &grid), Some(column));

        // Down into the pit: its floor, reached through the opening.
        assert_eq!(
            pick_surface(down_at(layout.surface_centre(pit, -1.0)), &layout, &grid),
            Some(pit)
        );

        // A shallow ray from the east, aimed at the column's top: it crosses the pit's airspace
        // above the floor and the column's east wall, and neither of those is a surface.
        let aimed = |from: Vec3, at: Vec3| Ray3d::new(from, Dir3::new(at - from).unwrap());
        let east = top + Vec3::new(20.0, 4.0, 0.0);
        assert_eq!(pick_surface(aimed(east, top), &layout, &grid), Some(column));

        // Aimed just over the column and away: nothing, rather than the hex underneath.
        let past = aimed(east, top + Vec3::new(-20.0, 4.1, 0.0));
        assert_eq!(pick_surface(past, &layout, &grid), None);
    }

    /// A flooded location is caught at the water rather than at the sea bed, so clicking a lake
    /// selects it where it looks like it should.
    #[test]
    fn a_submerged_location_is_picked_at_the_waterline() {
        let layout = HexLayout::pointy(1.0);
        let sunk = Axial::ZERO;
        let mut grid: TerrainGrid = Grid::new();
        grid.insert(Location::new(sunk, Terrain { height: -1.0, water: Some(0.25) }));

        let waterline = layout.surface_centre(sunk, 0.25);
        assert_eq!(pick_surface(down_at(waterline), &layout, &grid), Some(sunk));

        // A ray that stops above the sea bed but below the water still finds it, because the water
        // is what it meets — the old behaviour would have needed it to reach all the way down.
        let just_under = waterline + Vec3::Y * 0.1;
        let shallow = Ray3d::new(just_under + Vec3::new(8.0, 0.4, 0.0), Dir3::NEG_X);
        assert_eq!(pick_surface(shallow, &layout, &grid), None, "above the surface");
        let onto = Ray3d::new(
            waterline + Vec3::new(8.0, 4.0, 0.0),
            Dir3::new(waterline - (waterline + Vec3::new(8.0, 4.0, 0.0))).unwrap(),
        );
        assert_eq!(pick_surface(onto, &layout, &grid), Some(sunk));
    }
}
