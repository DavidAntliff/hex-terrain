//! Draws the grid: one hexagonal prism per location, outlined, with the selection picked out.
//!
//! A location's height decides its form. A positive height is a **column** standing on the grid
//! plane: a closed solid, capped top and bottom. A negative height is a **pit** sunk into the
//! plane: floored and walled, but open at the rim, because a lid at elevation zero would seal the
//! hole it is meant to be.
//!
//! Faces are meshes; outlines are gizmos. Gizmos suit the outlines because they are immediate-mode,
//! so switching which hex is highlighted costs nothing, and line width is a per-config-group
//! setting — hence two groups rather than one.

use bevy::{
    asset::RenderAssetUsages,
    gizmos::config::GizmoConfigGroup,
    mesh::PrimitiveTopology,
    prelude::*,
    reflect::Reflect,
};

use super::layout::HexLayout;
use super::selection::Selected;
use super::GridModel;
use crate::hex::Axial;

/// Thin outlines for every hex.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct GridLines;

/// Thick outline for the active hex.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct Highlight;

const FILL: Color = Color::srgb(0.16, 0.19, 0.26);
const EDGE: Color = Color::srgb(0.35, 0.75, 0.85);
const ACTIVE_EDGE: Color = Color::srgb(1.0, 0.78, 0.25);

const EDGE_WIDTH: f32 = 1.5;
const ACTIVE_EDGE_WIDTH: f32 = 6.0;

/// One rendered hex.
#[derive(Component)]
pub struct HexCell {
    pub coord: Axial,
}

/// The two meshes every cell shares, one per [`Form`].
///
/// Held so an orientation change can rewrite them in place: corner angles differ between pointy and
/// flat, which a `Transform` cannot express. Scale and height changes still need no rebuild.
#[derive(Resource)]
pub struct HexMeshes {
    column: Handle<Mesh>,
    pit: Handle<Mesh>,
}

impl HexMeshes {
    fn for_height(&self, height: f32) -> Handle<Mesh> {
        match Form::of(height) {
            Form::Column => self.column.clone(),
            Form::Pit => self.pit.clone(),
        }
    }
}

/// Which side of the grid plane a location's material lies on.
///
/// The sign of the height cannot be expressed as a `Transform` on one mesh: a negative scale
/// mirrors the prism, which turns every face the wrong way out. Two meshes instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Form {
    /// Rises above the plane. Closed: capped at the top and underneath.
    Column,
    /// Sinks below it. Floored and walled, open at the rim.
    Pit,
}

impl Form {
    fn of(height: f32) -> Self {
        if height < 0.0 { Self::Pit } else { Self::Column }
    }
}

/// Both outline groups need a negative depth bias, because the lines are exactly coplanar with the
/// surface faces they trace and would otherwise z-fight with them.
///
/// This is easy to miss: at an oblique angle depth varies along each line and enough of it wins to
/// look correct, but from directly overhead an interior edge shared with an equally high neighbour
/// has a face on both sides at identical depth and disappears entirely.
///
/// The bias has to stay small, though. It shifts normalized depth, which is steeply non-linear, so
/// a tenth of it is a large pull towards the camera — enough that outlines drew straight through
/// the prisms standing in front of them. Just enough to win against a coplanar face is the target.
pub fn configure_gizmo_widths(mut store: ResMut<GizmoConfigStore>) {
    let (grid_lines, _) = store.config_mut::<GridLines>();
    grid_lines.line.width = EDGE_WIDTH;
    grid_lines.depth_bias = -0.002;

    let (highlight, _) = store.config_mut::<Highlight>();
    highlight.line.width = ACTIVE_EDGE_WIDTH;
    highlight.depth_bias = -0.004;
}

/// Spawns a prism per location, all sharing two unit-sized meshes and one material.
pub fn spawn_grid(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    grid: Res<GridModel>,
    layout: Res<HexLayout>,
) {
    let hex_meshes = HexMeshes {
        column: meshes.add(unit_prism_mesh(&layout, Form::Column)),
        pit: meshes.add(unit_prism_mesh(&layout, Form::Pit)),
    };
    let material = materials.add(StandardMaterial {
        base_color: FILL,
        perceptual_roughness: 0.9,
        ..default()
    });

    for location in grid.iter() {
        commands.spawn((
            HexCell {
                coord: location.coord,
            },
            Mesh3d(hex_meshes.for_height(location.data.height)),
            MeshMaterial3d(material.clone()),
            cell_transform(&layout, location.coord, location.data.height),
        ));
    }
    commands.insert_resource(hex_meshes);
}

fn cell_transform(layout: &HexLayout, coord: Axial, height: f32) -> Transform {
    Transform::from_translation(layout.hex_to_world(coord)).with_scale(layout.mesh_scale(height))
}

/// Keeps the prisms in step when the layout changes — its scale, height scale, or orientation.
///
/// Which mesh a cell uses is fixed at spawn: heights are static, so no cell ever changes form.
pub fn sync_cells(
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    hex_meshes: Option<Res<HexMeshes>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cells: Query<(&HexCell, &mut Transform)>,
) {
    if !layout.is_changed() {
        return;
    }

    // Positions move with orientation as well as scale, so both are recomputed here.
    for (cell, mut transform) in &mut cells {
        let Some(location) = grid.get(cell.coord) else {
            continue;
        };
        *transform = cell_transform(&layout, cell.coord, location.data.height);
    }

    // Rewriting the shared assets updates all 37 cells at once.
    if let Some(hex_meshes) = hex_meshes {
        for (handle, form) in [
            (&hex_meshes.column, Form::Column),
            (&hex_meshes.pit, Form::Pit),
        ] {
            if let Some(mut mesh) = meshes.get_mut(handle) {
                *mesh = unit_prism_mesh(&layout, form);
            }
        }
    }
}

pub fn draw_outlines(
    mut grid_lines: Gizmos<GridLines>,
    mut highlight: Gizmos<Highlight>,
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    selected: Res<Selected>,
) {
    for location in grid.iter() {
        let lift = layout.elevation(location.data.height);
        let corners = layout.corners(location.coord).map(|corner| corner + lift);
        // `linestrip` does not close the loop, so repeat the first corner.
        let loop_ = corners.into_iter().chain(std::iter::once(corners[0]));
        if selected.0 == Some(location.coord) {
            highlight.linestrip(loop_, ACTIVE_EDGE);
        } else {
            grid_lines.linestrip(loop_, EDGE);
        }
    }
}

/// A hexagonal prism of circumradius 1 and unit height, anchored on the grid plane: a column
/// occupies elevation `0..=1`, a pit `-1..=0`. One `Transform` scale therefore serves both, with
/// the height's magnitude on the elevation axis.
///
/// Built from [`HexLayout::corner_offsets`] at unit scale — the same function the outlines use —
/// so the faces and the lines cannot disagree.
///
/// Every face is wound so that its geometric normal, `(v1 - v0) × (v2 - v0)`, is the normal it
/// stores; that is what back-face culling reads. Note that `corner_offsets` runs **clockwise**
/// seen from the `+normal` side, on either plane, so a face pointing that way iterates them
/// backwards. A test asserts the whole thing rather than leaving it to inspection.
fn unit_prism_mesh(layout: &HexLayout, form: Form) -> Mesh {
    let corners = layout.unit().corner_offsets();
    let up = layout.plane.normal();

    let mut positions = Vec::with_capacity(72);
    let mut normals = Vec::with_capacity(72);
    let mut face = |tri: [Vec3; 3], normal: Vec3| {
        positions.extend(tri);
        normals.extend([normal; 3]);
    };

    // The terrain surface — a column's top cap, a pit's floor — faces up either way, which is
    // what makes both visible from above the grid.
    let surface = match form {
        Form::Column => up,
        Form::Pit => -up,
    };
    for i in 0..6 {
        face(
            [
                surface,
                corners[(i + 1) % 6] + surface,
                corners[i] + surface,
            ],
            up,
        );
    }

    // A column is closed underneath. A pit is deliberately not closed on top: a cap there would be
    // the grid plane sealing over the hole.
    if form == Form::Column {
        for i in 0..6 {
            face([Vec3::ZERO, corners[i], corners[(i + 1) % 6]], -up);
        }
    }

    // Walls. A column is seen from outside, a pit from inside, so they face opposite ways.
    let floor = match form {
        Form::Column => Vec3::ZERO,
        Form::Pit => -up,
    };
    for i in 0..6 {
        let (a, b) = (corners[i] + floor, corners[(i + 1) % 6] + floor);
        let outward = up.cross(b - a).normalize();
        let (lower, upper) = ([a, b], [a + up, b + up]);
        match form {
            Form::Column => {
                face([lower[0], upper[0], upper[1]], outward);
                face([lower[0], upper[1], lower[1]], outward);
            }
            Form::Pit => {
                face([lower[0], upper[1], upper[0]], -outward);
                face([lower[0], lower[1], upper[1]], -outward);
            }
        }
    }

    // Untextured tiles: UVs exist only to satisfy the standard material's pipeline.
    let uvs = vec![[0.5, 0.5]; positions.len()];

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::Orientation;
    use crate::view::layout::GridPlane;

    const EPS: f32 = 1e-4;

    /// Triangles as `(vertices, stored normal)`.
    fn triangles(mesh: &Mesh) -> Vec<([Vec3; 3], Vec3)> {
        let read = |id| {
            mesh.attribute(id)
                .expect("attribute present")
                .as_float3()
                .expect("float3 attribute")
                .iter()
                .map(|v| Vec3::from_array(*v))
                .collect::<Vec<_>>()
        };
        let positions = read(Mesh::ATTRIBUTE_POSITION);
        let normals = read(Mesh::ATTRIBUTE_NORMAL);
        assert_eq!(positions.len(), normals.len());
        positions
            .chunks_exact(3)
            .zip(normals.chunks_exact(3))
            .map(|(p, n)| {
                assert!(n[0] == n[1] && n[1] == n[2], "flat faces only");
                ([p[0], p[1], p[2]], n[0])
            })
            .collect()
    }

    /// Winding is what back-face culling reads, and getting it backwards is invisible in the code
    /// and glaring on screen. Every triangle's geometric normal must be the one it stores.
    #[test]
    fn every_face_is_wound_to_match_its_normal() {
        for plane in [GridPlane::Xz, GridPlane::Xy] {
            for orientation in [Orientation::Pointy, Orientation::Flat] {
                for form in [Form::Column, Form::Pit] {
                    let layout = HexLayout::pointy(1.0)
                        .with_plane(plane)
                        .with_orientation(orientation);
                    for ([v0, v1, v2], normal) in triangles(&unit_prism_mesh(&layout, form)) {
                        let geometric = (v1 - v0).cross(v2 - v0).normalize();
                        assert!(
                            geometric.abs_diff_eq(normal, EPS),
                            "{form:?} on {plane:?}: face wound {geometric:?}, normal {normal:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_column_is_closed_and_a_pit_is_open_at_the_rim() {
        let layout = HexLayout::pointy(1.0);
        let up = layout.plane.normal();
        let at_rim = |tri: &[Vec3; 3]| tri.iter().all(|v| v.dot(up).abs() < EPS);

        let column = triangles(&unit_prism_mesh(&layout, Form::Column));
        assert_eq!(column.len(), 24, "6 top, 6 bottom, 12 wall");
        assert_eq!(
            column.iter().filter(|(tri, _)| at_rim(tri)).count(),
            6,
            "a column is capped underneath"
        );

        let pit = triangles(&unit_prism_mesh(&layout, Form::Pit));
        assert_eq!(pit.len(), 18, "6 floor, 12 wall — no cap");
        assert_eq!(
            pit.iter().filter(|(tri, _)| at_rim(tri)).count(),
            0,
            "the grid plane must not seal the hole"
        );

        // Both surfaces face up, so both are visible from above the grid.
        for mesh in [column, pit] {
            let surface = mesh
                .iter()
                .filter(|(_, normal)| normal.abs_diff_eq(up, EPS))
                .count();
            assert_eq!(surface, 6, "the surface is a six-triangle fan facing up");
        }
    }

    /// A column is seen from outside and a pit from inside, so their walls face opposite ways.
    #[test]
    fn walls_face_outward_on_a_column_and_inward_on_a_pit() {
        let layout = HexLayout::pointy(1.0);
        let up = layout.plane.normal();
        for (form, sign) in [(Form::Column, 1.0), (Form::Pit, -1.0)] {
            for (tri, normal) in triangles(&unit_prism_mesh(&layout, form)) {
                // Walls are the faces that do not point along the elevation axis.
                if normal.dot(up).abs() > EPS {
                    continue;
                }
                let centroid = (tri[0] + tri[1] + tri[2]) / 3.0;
                let radial = centroid - centroid.dot(up) * up;
                assert!(
                    normal.dot(radial) * sign > 0.0,
                    "{form:?} wall at {centroid:?} faces the wrong way"
                );
            }
        }
    }
}
