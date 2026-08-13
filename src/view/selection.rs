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

// A system's parameters are the list of what it reads and writes, not an argument list a caller
// has to get right, so the usual reason to keep them few does not apply.
#[allow(clippy::too_many_arguments)]
pub fn select_on_click(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
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

    // Alt+left starts the camera's turn drag. Without this, beginning one would also re-select
    // whatever the pivot landed on.
    if keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]) {
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
    selected.0 = pick_surface(ray, &layout, &grid).map(|(_, coord)| coord);
}

/// Where the ray meets the terrain first, and which hex that surface belongs to.
///
/// Each location contributes one horizontal face at whatever it presents — its cap, or the water
/// standing over it, which is what a click on a lake lands on. A hit counts only if it falls inside
/// that location's own footprint, which is what makes walls transparent to selection.
///
/// The point is returned as well as the coord because the camera pivots on it; selection wants only
/// the coord. One ray routine serves both rather than two that can disagree about what was hit.
///
// ponytail: a linear scan over every location, and walls do not occlude — only surfaces are
// tested, so at a grazing angle a low cap hidden behind a taller neighbour can still be picked.
// Test the six wall quads per hex if that ever shows.
pub fn pick_surface(ray: Ray3d, layout: &HexLayout, grid: &TerrainGrid) -> Option<(Vec3, Axial)> {
    let plane = InfinitePlane3d::new(layout.plane.normal());
    grid.iter()
        .filter_map(|location| {
            let surface = layout.surface_centre(location.coord, location.data.surface());
            let distance = ray.intersect_plane(surface, plane)?;
            let point = ray.get_point(distance);
            let hit = layout.world_to_hex(point).round().to_axial();
            (hit == location.coord).then_some((distance, point, location.coord))
        })
        .min_by(|(a, ..), (b, ..)| a.total_cmp(b))
        .map(|(_, point, coord)| (point, coord))
}

/// The world point under the ray: the terrain it meets, or the grid plane stretching away beyond
/// the grid's edge.
///
/// `fallback` covers a ray aimed at the sky, which meets neither. Returning it rather than `None`
/// is what lets the camera always have a pivot instead of handling an absent one — and keeping the
/// previous pivot is the right answer anyway, since a drag begun on empty sky should carry on
/// turning about whatever it was turning about before.
pub fn pick_point(ray: Ray3d, layout: &HexLayout, grid: &TerrainGrid, fallback: Vec3) -> Vec3 {
    if let Some((point, _)) = pick_surface(ray, layout, grid) {
        return point;
    }
    let plane = InfinitePlane3d::new(layout.plane.normal());
    match ray.intersect_plane(layout.origin, plane) {
        Some(distance) => ray.get_point(distance),
        None => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::{undulating, Grid, Location, Terrain};

    fn down_at(point: Vec3) -> Ray3d {
        Ray3d::new(point + Vec3::Y * 50.0, Dir3::NEG_Y)
    }

    /// The hex alone, which is all selection cares about.
    fn hex(ray: Ray3d, layout: &HexLayout, grid: &TerrainGrid) -> Option<Axial> {
        pick_surface(ray, layout, grid).map(|(_, coord)| coord)
    }

    #[test]
    fn looking_straight_down_picks_the_hex_below_whatever_its_height() {
        for height_scale in [0.5, 4.0] {
            let layout = HexLayout::pointy(1.3).with_height_scale(height_scale);
            let grid = Grid::hexagon(3, undulating);
            for location in grid.iter() {
                let coord = location.coord;
                let surface = layout.surface_centre(coord, location.data.surface());
                assert_eq!(hex(down_at(surface), &layout, &grid), Some(coord));
                // Off-centre, but still inside the hex.
                let corner = layout.corners(coord)[0] + layout.elevation(location.data.surface());
                let probe = surface + (corner - surface) * 0.8;
                assert_eq!(hex(down_at(probe), &layout, &grid), Some(coord));

                // The point comes back too, and it is on the surface that was hit — this is what
                // the camera pivots on, so a coord that is right with a point that is wrong would
                // otherwise go unnoticed.
                let (point, _) = pick_surface(down_at(surface), &layout, &grid).unwrap();
                assert!(point.distance(surface) < 1e-3, "{point:?} vs {surface:?}");
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
        assert_eq!(hex(down_at(top), &layout, &grid), Some(column));

        // Down into the pit: its floor, reached through the opening.
        assert_eq!(
            hex(down_at(layout.surface_centre(pit, -1.0)), &layout, &grid),
            Some(pit)
        );

        // A shallow ray from the east, aimed at the column's top: it crosses the pit's airspace
        // above the floor and the column's east wall, and neither of those is a surface.
        let aimed = |from: Vec3, at: Vec3| Ray3d::new(from, Dir3::new(at - from).unwrap());
        let east = top + Vec3::new(20.0, 4.0, 0.0);
        assert_eq!(hex(aimed(east, top), &layout, &grid), Some(column));

        // Aimed just over the column and away: nothing, rather than the hex underneath.
        let past = aimed(east, top + Vec3::new(-20.0, 4.1, 0.0));
        assert_eq!(hex(past, &layout, &grid), None);
    }

    /// The camera's pivot always exists. Off the grid it lands on the plane the grid sits in; aimed
    /// at the sky it keeps whatever pivot the camera already had.
    #[test]
    fn a_pivot_falls_back_to_the_plane_and_then_to_the_last_one() {
        let layout = HexLayout::pointy(1.0);
        let grid: TerrainGrid = Grid::hexagon(2, undulating);
        let held = Vec3::new(-9.0, 9.0, -9.0);

        // Well outside a side-2 grid, so no location's footprint contains it.
        let outside = Vec3::new(40.0, 0.0, 40.0);
        let onto_plane = pick_point(down_at(outside), &layout, &grid, held);
        assert!(onto_plane.distance(outside) < 1e-3, "{onto_plane:?}");

        let sky = Ray3d::new(Vec3::new(0.0, 5.0, 0.0), Dir3::Y);
        assert_eq!(pick_point(sky, &layout, &grid, held), held);

        // And a ray that does meet terrain still gets the terrain, not the plane under it.
        let raised = grid.iter().max_by(|a, b| {
            a.data.surface().total_cmp(&b.data.surface())
        }).unwrap();
        let surface = layout.surface_centre(raised.coord, raised.data.surface());
        let hit = pick_point(down_at(surface), &layout, &grid, held);
        assert!(hit.distance(surface) < 1e-3, "{hit:?} vs {surface:?}");
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
        assert_eq!(hex(down_at(waterline), &layout, &grid), Some(sunk));

        // A ray that stops above the sea bed but below the water still finds it, because the water
        // is what it meets — the old behaviour would have needed it to reach all the way down.
        let just_under = waterline + Vec3::Y * 0.1;
        let shallow = Ray3d::new(just_under + Vec3::new(8.0, 0.4, 0.0), Dir3::NEG_X);
        assert_eq!(hex(shallow, &layout, &grid), None, "above the surface");
        let onto = Ray3d::new(
            waterline + Vec3::new(8.0, 4.0, 0.0),
            Dir3::new(waterline - (waterline + Vec3::new(8.0, 4.0, 0.0))).unwrap(),
        );
        assert_eq!(hex(onto, &layout, &grid), Some(sunk));
    }
}
