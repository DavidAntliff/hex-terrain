//! Framing the camera on the whole scene.
//!
//! Answers "how far back must a top-down camera sit for all of this to be visible", from the
//! camera's actual vertical field of view and aspect ratio rather than a hand-tuned constant. This
//! is what makes the view robust to window shape — a widget placed to the side of the grid falls
//! off-screen in a portrait window, and guessing a radius cannot fix that in general.

use bevy::prelude::*;

use super::compass::{self, ShowCompass};
use super::layout::HexLayout;
use super::GridModel;
use crate::camera::{place, Orbit, MAX_RADIUS, MIN_RADIUS, TOP_DOWN_PITCH};
use crate::hex::Axial;

/// A little breathing room, so "just visible" is not "clipped at the edge".
const MARGIN: f32 = 1.06;

/// Set by the reset-view button; consumed by [`reset_view`].
#[derive(Resource, Default)]
pub struct ResetViewRequested(pub bool);

/// Half-extent of everything that must be visible, in grid-plane coordinates measured from the
/// layout origin.
///
/// Symmetric about the origin because the camera always looks there — see the `ponytail` note on
/// [`Orbit`]. Content off to one side therefore costs empty space on the other.
pub fn content_half_extent(
    layout: &HexLayout,
    coords: impl Iterator<Item = Axial>,
    show_compass: bool,
) -> Vec2 {
    let mut half = Vec2::ZERO;

    // Corners, not centres: the outermost hex's far edge is what has to fit.
    for coord in coords {
        for corner in layout.corners(coord) {
            half = half.max(layout.plane.to_plane(corner - layout.origin).abs());
        }
    }

    if show_compass {
        let (centre, radius) = compass::bounds(layout);
        let centre = layout.plane.to_plane(centre - layout.origin).abs();
        half = half.max(centre + Vec2::splat(radius));
    }

    half
}

/// Distance a camera needs to fit `half_extent` in view, given a vertical field of view and an
/// aspect ratio (width / height).
///
/// The vertical axis is governed by the field of view alone; the horizontal one is widened by the
/// aspect ratio, which is why a portrait window is the tighter case horizontally.
pub fn framing_distance(half_extent: Vec2, fov: f32, aspect_ratio: f32) -> f32 {
    let half_height_per_unit = (fov * 0.5).tan();
    let vertical = half_extent.y / half_height_per_unit;
    let horizontal = half_extent.x / (half_height_per_unit * aspect_ratio);
    vertical.max(horizontal) * MARGIN
}

/// Puts the camera exactly overhead, far enough out to show the whole grid — and the compass, when
/// it is switched on.
pub fn reset_view(
    mut requested: ResMut<ResetViewRequested>,
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    show_compass: Res<ShowCompass>,
    camera: Single<(&mut Orbit, &mut Transform, &Projection)>,
) {
    if !requested.0 {
        return;
    }
    requested.0 = false;

    let (mut orbit, mut transform, projection) = camera.into_inner();
    let Projection::Perspective(perspective) = projection else {
        // Only the perspective camera is framed here; nothing else is used in this app.
        return;
    };

    let half_extent = content_half_extent(&layout, grid.coords(), show_compass.0);
    orbit.yaw = 0.0;
    orbit.pitch = TOP_DOWN_PITCH;
    // Back to the origin, wherever a pan or a flight has left the camera: `content_half_extent`
    // measures the grid symmetrically about `layout.origin`, so that is the point it frames.
    orbit.target = Vec3::ZERO;
    orbit.radius = framing_distance(half_extent, perspective.fov, perspective.aspect_ratio)
        .clamp(MIN_RADIUS, MAX_RADIUS);

    *transform = place(&orbit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::{Grid, Orientation};

    fn coords() -> Vec<Axial> {
        Grid::hexagon(3, |_| ()).coords().collect()
    }

    #[test]
    fn the_extent_contains_every_hex_corner() {
        for orientation in [Orientation::Pointy, Orientation::Flat] {
            let layout = HexLayout::pointy(1.3).with_orientation(orientation);
            let half = content_half_extent(&layout, coords().into_iter(), false);
            for coord in coords() {
                for corner in layout.corners(coord) {
                    let p = layout.plane.to_plane(corner - layout.origin).abs();
                    assert!(
                        p.x <= half.x + 1e-4 && p.y <= half.y + 1e-4,
                        "{corner:?} outside {half:?} ({orientation:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn the_extent_scales_with_the_layout() {
        let small = content_half_extent(&HexLayout::pointy(1.0), coords().into_iter(), false);
        let large = content_half_extent(&HexLayout::pointy(2.0), coords().into_iter(), false);
        assert!((large.x / small.x - 2.0).abs() < 1e-3);
        assert!((large.y / small.y - 2.0).abs() < 1e-3);
    }

    #[test]
    fn enabling_the_compass_only_ever_grows_the_extent() {
        let layout = HexLayout::pointy(1.0);
        let without = content_half_extent(&layout, coords().into_iter(), false);
        let with = content_half_extent(&layout, coords().into_iter(), true);
        assert!(with.x >= without.x && with.y >= without.y);
        // The compass sits south of the grid, so it is the vertical extent that must grow.
        assert!(with.y > without.y, "compass should extend the framing southward");
    }

    #[test]
    fn everything_fits_the_frustum_at_the_computed_distance() {
        // The definition of "just visible", checked against the frustum directly: at the computed
        // distance every corner projects inside the view rectangle, in portrait and landscape.
        for aspect in [0.5, 1.0, 1.78, 3.0] {
            for show_compass in [false, true] {
                let layout = HexLayout::pointy(1.0);
                let fov = std::f32::consts::FRAC_PI_4;
                let half = content_half_extent(&layout, coords().into_iter(), show_compass);
                let distance = framing_distance(half, fov, aspect);

                let visible_half_height = distance * (fov * 0.5).tan();
                let visible_half_width = visible_half_height * aspect;
                for coord in coords() {
                    for corner in layout.corners(coord) {
                        let p = layout.plane.to_plane(corner - layout.origin).abs();
                        assert!(
                            p.x <= visible_half_width && p.y <= visible_half_height,
                            "{corner:?} clipped at aspect {aspect}"
                        );
                    }
                }

                // "Just" visible: no more than the margin's worth of slack on the tighter axis.
                let slack = (visible_half_height / half.y).min(visible_half_width / half.x);
                assert!(slack <= MARGIN + 1e-3, "framing is looser than intended: {slack}");
            }
        }
    }

    #[test]
    fn a_narrower_window_needs_more_distance() {
        let layout = HexLayout::pointy(1.0);
        let half = content_half_extent(&layout, coords().into_iter(), false);
        let fov = std::f32::consts::FRAC_PI_4;
        let portrait = framing_distance(half, fov, 0.6);
        let landscape = framing_distance(half, fov, 1.8);
        assert!(
            portrait > landscape,
            "portrait {portrait} should need more room than landscape {landscape}"
        );
    }
}
