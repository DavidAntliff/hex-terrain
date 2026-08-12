//! The projection between hex coordinates and world space.
//!
//! This is the **only** place world units exist. The model ([`crate::hex`]) is dimensionless; a
//! `HexLayout` supplies the scale, origin, orientation and plane that turn a coordinate into a
//! position. Keeping the two apart means the same grid can be projected differently — a scene and
//! a flat minimap, say — without the model knowing.
//!
//! The forward and inverse matrices and the corner-angle convention are the reference's:
//! <https://www.redblobgames.com/grids/hexagons/>.

use bevy::math::{Vec2, Vec3};
use bevy::prelude::Resource;

use crate::hex::{Axial, FractionalCube, Orientation};

/// The projection matrices for each orientation.
///
/// [`Orientation`] itself lives in the model, since it is dimensionless and also decides which
/// doubled-coordinate variant applies. These matrices are pure projection, so they live here — the
/// model never needs them.
impl Orientation {
    /// Forward matrix `[f0, f1, f2, f3]`: hex coordinates to plane coordinates.
    const fn forward(self) -> [f32; 4] {
        const SQRT_3: f32 = 1.732_050_8;
        match self {
            Self::Pointy => [SQRT_3, SQRT_3 / 2.0, 0.0, 1.5],
            Self::Flat => [1.5, 0.0, SQRT_3 / 2.0, SQRT_3],
        }
    }

    /// Inverse matrix `[b0, b1, b2, b3]`: plane coordinates back to hex coordinates.
    const fn inverse(self) -> [f32; 4] {
        const SQRT_3_3: f32 = 0.577_350_3; // √3 / 3
        match self {
            Self::Pointy => [SQRT_3_3, -1.0 / 3.0, 0.0, 2.0 / 3.0],
            Self::Flat => [2.0 / 3.0, 0.0, -1.0 / 3.0, SQRT_3_3],
        }
    }

    /// Angle of the first corner, in sixths of a turn.
    const fn start_angle(self) -> f32 {
        match self {
            Self::Pointy => 0.5,
            Self::Flat => 0.0,
        }
    }
}

/// Which world plane the grid lies in, and therefore which axis is elevation.
///
/// Both mappings send the reference's 2D `+y` — down-screen, "south" in its diagrams — to the
/// direction that reads as *down-screen* when the camera looks at the grid, so the rendered grid
/// matches the website's pictures either way.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum GridPlane {
    /// Bevy's ground plane. Elevation is `+y`.
    #[default]
    Xz,
    /// The XY plane, as in most 2D references. Elevation is `+z`.
    Xy,
}

impl GridPlane {
    fn to_world(self, plane: Vec2) -> Vec3 {
        match self {
            Self::Xz => Vec3::new(plane.x, 0.0, plane.y),
            Self::Xy => Vec3::new(plane.x, -plane.y, 0.0),
        }
    }

    /// World offset to in-plane coordinates: the inverse of [`Self::to_world`] for vectors lying
    /// in the plane, discarding any component along the normal.
    pub fn to_plane(self, world: Vec3) -> Vec2 {
        match self {
            Self::Xz => Vec2::new(world.x, world.z),
            Self::Xy => Vec2::new(world.x, -world.y),
        }
    }

    /// The world-space normal of the grid plane.
    pub fn normal(self) -> Vec3 {
        match self {
            Self::Xz => Vec3::Y,
            Self::Xy => Vec3::Z,
        }
    }

    /// Componentwise scale that stretches a unit-sized mesh to `size` within this plane.
    fn mesh_scale(self, size: Vec2) -> Vec3 {
        match self {
            Self::Xz => Vec3::new(size.x, 1.0, size.y),
            Self::Xy => Vec3::new(size.x, size.y, 1.0),
        }
    }
}

/// Projects hex coordinates into world space.
#[derive(Debug, Clone, Copy, PartialEq, Resource)]
pub struct HexLayout {
    pub orientation: Orientation,
    /// Hex circumradius in world units. Two components allow squashed hexes; keep them equal for
    /// regular ones. Set it through [`HexLayout::with_scale`] unless you want them to differ.
    pub size: Vec2,
    pub origin: Vec3,
    pub plane: GridPlane,
}

impl Default for HexLayout {
    fn default() -> Self {
        Self::pointy(1.0)
    }
}

impl HexLayout {
    /// A pointy-top layout on Bevy's ground plane, centred on the world origin.
    ///
    /// `scale` is the hexagon's circumradius — the centre-to-vertex distance — in world units.
    pub fn pointy(scale: f32) -> Self {
        Self {
            orientation: Orientation::Pointy,
            size: Vec2::splat(scale),
            origin: Vec3::ZERO,
            plane: GridPlane::Xz,
        }
    }

    /// The single scaling knob: sets both components of [`Self::size`].
    pub fn with_scale(mut self, scale: f32) -> Self {
        self.size = Vec2::splat(scale);
        self
    }

    pub fn with_orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = orientation;
        self
    }

    pub fn with_origin(mut self, origin: Vec3) -> Self {
        self.origin = origin;
        self
    }

    pub fn with_plane(mut self, plane: GridPlane) -> Self {
        self.plane = plane;
        self
    }

    /// The same layout at unit scale and centred on the origin — the frame a reusable hex mesh is
    /// built in, so that scaling it is a [`bevy::prelude::Transform`] change rather than a mesh
    /// rebuild.
    pub fn unit(&self) -> Self {
        Self {
            size: Vec2::ONE,
            origin: Vec3::ZERO,
            ..*self
        }
    }

    /// Scale to apply to a mesh built by [`Self::unit`] so it matches this layout.
    pub fn mesh_scale(&self) -> Vec3 {
        self.plane.mesh_scale(self.size)
    }

    /// World position of a hex's centre.
    pub fn hex_to_world(&self, coord: Axial) -> Vec3 {
        let [f0, f1, f2, f3] = self.orientation.forward();
        let (q, r) = (coord.q as f32, coord.r as f32);
        let plane = Vec2::new(
            (f0 * q + f1 * r) * self.size.x,
            (f2 * q + f3 * r) * self.size.y,
        );
        self.origin + self.plane.to_world(plane)
    }

    /// Projects a world position onto the grid, giving a fractional coordinate.
    ///
    /// Any component of `world` off the grid plane is ignored, so callers must intersect with the
    /// plane first if they care about it. Call [`FractionalCube::round`] to get the containing hex.
    pub fn world_to_hex(&self, world: Vec3) -> FractionalCube {
        let [b0, b1, b2, b3] = self.orientation.inverse();
        let plane = self.plane.to_plane(world - self.origin);
        let p = Vec2::new(plane.x / self.size.x, plane.y / self.size.y);
        FractionalCube::new(b0 * p.x + b1 * p.y, b2 * p.x + b3 * p.y)
    }

    /// Offsets from a hex centre to its six corners, counter-clockwise.
    ///
    /// The single source of truth for hex geometry: the mesh, the outlines and the compass all
    /// come from here, so they cannot drift apart.
    pub fn corner_offsets(&self) -> [Vec3; 6] {
        core::array::from_fn(|i| {
            let angle =
                std::f32::consts::TAU * (self.orientation.start_angle() + i as f32) / 6.0;
            self.plane.to_world(Vec2::new(
                self.size.x * angle.cos(),
                self.size.y * angle.sin(),
            ))
        })
    }

    /// World positions of a hex's six corners.
    pub fn corners(&self, coord: Axial) -> [Vec3; 6] {
        let centre = self.hex_to_world(coord);
        self.corner_offsets().map(|offset| centre + offset)
    }

    /// The six half-axes of the cube coordinate system, as unit world directions.
    ///
    /// Derived by inverting the layout: the direction in which `q` increases fastest is the
    /// gradient `(b0/size.x, b1/size.y)`, and likewise for `r`; `s = -q-r` gives the third. For a
    /// pointy-top layout these land on the hexagon's vertices, which is what makes the compass
    /// self-checking. Ordered counter-clockwise, so consecutive entries alternate in sign.
    pub fn axis_arrows(&self) -> [AxisArrow; 6] {
        let [b0, b1, b2, b3] = self.orientation.inverse();
        let grad_q = Vec2::new(b0 / self.size.x, b1 / self.size.y);
        let grad_r = Vec2::new(b2 / self.size.x, b3 / self.size.y);
        let grad_s = -grad_q - grad_r;

        let dir = |g: Vec2, sign: f32| self.plane.to_world(g.normalize() * sign);
        [
            AxisArrow::new("+q", dir(grad_q, 1.0)),
            AxisArrow::new("-r", dir(grad_r, -1.0)),
            AxisArrow::new("+s", dir(grad_s, 1.0)),
            AxisArrow::new("-q", dir(grad_q, -1.0)),
            AxisArrow::new("+r", dir(grad_r, 1.0)),
            AxisArrow::new("-s", dir(grad_s, -1.0)),
        ]
    }
}

/// One labelled half-axis of the coordinate system, for the compass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisArrow {
    pub label: &'static str,
    pub direction: Vec3,
}

impl AxisArrow {
    const fn new(label: &'static str, direction: Vec3) -> Self {
        Self { label, direction }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::Grid;

    const EPS: f32 = 1e-3;

    fn grid_coords() -> Vec<Axial> {
        Grid::hexagon(3, |_| ()).coords().collect()
    }

    #[test]
    fn centre_hex_sits_at_the_layout_origin() {
        let layout = HexLayout::pointy(2.0);
        assert!(layout.hex_to_world(Axial::ZERO).abs_diff_eq(Vec3::ZERO, EPS));
    }

    #[test]
    fn world_round_trip_holds_at_several_scales() {
        // Scale living only in the layout is what makes this pass unchanged across scales; if it
        // ever leaks into the model, this is the test that breaks.
        for scale in [0.25, 1.0, 2.7, 40.0] {
            let layout = HexLayout::pointy(scale);
            for coord in grid_coords() {
                let world = layout.hex_to_world(coord);
                let back = layout.world_to_hex(world).round().to_axial();
                assert_eq!(back, coord, "round trip failed at scale {scale}");
            }
        }
    }

    #[test]
    fn world_round_trip_survives_offsets_toward_every_corner() {
        let layout = HexLayout::pointy(1.5);
        for coord in grid_coords() {
            let centre = layout.hex_to_world(coord);
            for corner in layout.corners(coord) {
                // 80% of the way to a corner is still comfortably inside the hex.
                let probe = centre + (corner - centre) * 0.8;
                let back = layout.world_to_hex(probe).round().to_axial();
                assert_eq!(back, coord, "{probe:?} should resolve to {coord:?}");
            }
        }
    }

    #[test]
    fn round_trip_holds_on_the_other_plane_too() {
        let layout = HexLayout::pointy(1.0).with_plane(GridPlane::Xy);
        for coord in grid_coords() {
            let back = layout
                .world_to_hex(layout.hex_to_world(coord))
                .round()
                .to_axial();
            assert_eq!(back, coord);
        }
    }

    #[test]
    fn origin_offset_shifts_everything_uniformly() {
        let offset = Vec3::new(10.0, 0.0, -4.0);
        let plain = HexLayout::pointy(1.0);
        let shifted = plain.with_origin(offset);
        for coord in grid_coords() {
            let expected = plain.hex_to_world(coord) + offset;
            assert!(shifted.hex_to_world(coord).abs_diff_eq(expected, EPS));
            let back = shifted
                .world_to_hex(shifted.hex_to_world(coord))
                .round()
                .to_axial();
            assert_eq!(back, coord);
        }
    }

    #[test]
    fn neighbouring_hexes_share_exactly_two_corners() {
        // The check that faces actually touch edge to edge rather than overlapping or leaving gaps.
        let layout = HexLayout::pointy(1.3);
        let centre = layout.corners(Axial::ZERO);
        for i in 0..6 {
            let neighbour = layout.corners(Axial::ZERO.neighbour(i));
            let shared = centre
                .iter()
                .filter(|c| neighbour.iter().any(|n| c.abs_diff_eq(*n, EPS)))
                .count();
            assert_eq!(shared, 2, "neighbour {i} should share an edge");
        }
    }

    #[test]
    fn pointy_layout_has_a_vertex_pointing_north() {
        // "Pointy-top" means a corner, not an edge, faces up-screen: -z on the ground plane.
        let layout = HexLayout::pointy(1.0);
        let offsets = layout.corner_offsets();
        assert!(
            offsets.iter().any(|o| o.abs_diff_eq(Vec3::new(0.0, 0.0, -1.0), EPS)),
            "expected a corner at due north, got {offsets:?}"
        );
    }

    #[test]
    fn flat_layout_has_an_edge_where_pointy_has_a_vertex() {
        // The orientation's whole effect: flat-top puts a corner due east and an edge due north,
        // exactly swapping pointy-top's arrangement.
        let flat = HexLayout::pointy(1.0).with_orientation(Orientation::Flat);
        let offsets = flat.corner_offsets();
        assert!(
            offsets.iter().any(|o| o.abs_diff_eq(Vec3::new(1.0, 0.0, 0.0), EPS)),
            "expected a corner due east, got {offsets:?}"
        );
        assert!(
            !offsets.iter().any(|o| o.abs_diff_eq(Vec3::new(0.0, 0.0, -1.0), EPS)),
            "flat-top should not have a corner due north"
        );
        // And it still round-trips.
        for coord in grid_coords() {
            assert_eq!(
                flat.world_to_hex(flat.hex_to_world(coord)).round().to_axial(),
                coord
            );
        }
    }

    #[test]
    fn axis_arrows_are_unit_length_and_120_degrees_apart() {
        let layout = HexLayout::pointy(2.0);
        let arrows = layout.axis_arrows();

        for a in arrows {
            assert!((a.direction.length() - 1.0).abs() < EPS, "{a:?} not a unit vector");
        }

        // The three positive axes are 120° apart, so each pair dots to -0.5.
        let positives = ["+q", "+r", "+s"].map(|label| {
            arrows
                .iter()
                .find(|a| a.label == label)
                .expect("axis present")
                .direction
        });
        for i in 0..3 {
            let dot = positives[i].dot(positives[(i + 1) % 3]);
            assert!((dot + 0.5).abs() < EPS, "axes not 120 degrees apart: dot {dot}");
        }

        // Opposite half-axes must actually oppose.
        for (pos, neg) in [("+q", "-q"), ("+r", "-r"), ("+s", "-s")] {
            let find = |l: &str| {
                arrows.iter().find(|a| a.label == l).expect("axis present").direction
            };
            assert!(find(pos).abs_diff_eq(-find(neg), EPS));
        }
    }

    #[test]
    fn axis_arrows_point_at_hexagon_vertices_in_both_orientations() {
        // The self-checking property the compass relies on: the cube axes line up with the
        // hexagon's corners, so the drawn arrows should hit them. True for either orientation,
        // which is why the compass needs no special-casing when the orientation is toggled.
        for orientation in [Orientation::Pointy, Orientation::Flat] {
            let layout = HexLayout::pointy(1.0).with_orientation(orientation);
            let corners = layout.corner_offsets();
            for arrow in layout.axis_arrows() {
                let hit = corners
                    .iter()
                    .any(|c| c.normalize().abs_diff_eq(arrow.direction, EPS));
                assert!(hit, "{} does not point at a vertex ({orientation:?})", arrow.label);
            }
        }
    }

    #[test]
    fn r_increases_toward_the_camera_matching_the_reference_diagrams() {
        // The reference draws +r heading down-screen ("south"). On the ground plane that is +z.
        let layout = HexLayout::pointy(1.0);
        let south = layout.hex_to_world(Axial::new(0, 1));
        assert!(south.z > 0.0, "+r should head toward +z, got {south:?}");
        assert!(south.x.abs() > 0.0, "+r should also shift east on a pointy layout");
        // +q is due east.
        let east = layout.hex_to_world(Axial::new(1, 0));
        assert!(east.x > 0.0 && east.z.abs() < EPS, "+q should be due east, got {east:?}");
    }
}
