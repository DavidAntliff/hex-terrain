//! Draws the grid as a continuous surface, one cell at a time.
//!
//! A location's territory is its **full** hexagon. Its cap is that hexagon shrunk by [`INSET`] and
//! lifted to the location's height; the ring between the two is its wall. Two concentric similar
//! hexagons have corresponding corners, so that ring tiles exactly with six quads — one per edge,
//! no corner pieces, and no rule deciding which cell owns what. Every cell emits the same eighteen
//! triangles.
//!
//! What joins the cells is the **fence**: the outer ring of each cell sits on the full hexagon's
//! corners, at the mean of the heights of the locations present at that point of the lattice. Two
//! neighbours compute the same mean for each end of their shared edge, so their walls meet
//! vertex-for-vertex and the surface is continuous. Nothing anywhere refers to the grid plane, so a
//! location below it is simply a dip.
//!
//! Cap and wall are separate meshes because the cap is identical for every location — one shared
//! asset moved by a `Transform` — while the wall depends on the neighbours' heights and so is
//! unique per cell. They are children of a per-location entity, which is what makes a single
//! location's visibility, or a single location's material, a one-component change.
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
use crate::hex::{Axial, TerrainGrid};

/// Thin outlines for every hex.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct GridLines;

/// Thick outline for the active hex.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct Highlight;

const CAP_FILL: Color = Color::srgb(0.16, 0.19, 0.26);
const WALL_FILL: Color = Color::srgb(0.13, 0.16, 0.22);
const EDGE: Color = Color::srgb(0.35, 0.75, 0.85);
const ACTIVE_EDGE: Color = Color::srgb(1.0, 0.78, 0.25);

const EDGE_WIDTH: f32 = 1.5;
const ACTIVE_EDGE_WIDTH: f32 = 6.0;

/// How far each cap is shrunk towards its centre, as a fraction of the circumradius. The spacing is
/// untouched, so this is what opens up the wall between neighbours and gives it its incline.
///
/// A fraction, not a world distance: the meshes are built at unit scale and stretched by a
/// `Transform`, which would scale an absolute inset along with everything else.
// ponytail: one inset for the whole grid. Move it onto `Terrain` when a location needs its own.
const INSET: f32 = 0.08;

/// One rendered location: the parent of its cap and its wall.
///
/// Carries the transform and the visibility both children inherit, so hiding a location — or
/// moving one — is a single component away.
#[derive(Component)]
pub struct HexCell {
    pub coord: Axial,
}

/// The child holding a cell's wall ring, which is unique to that cell because it depends on the
/// neighbours' heights.
#[derive(Component)]
pub struct HexWall {
    coord: Axial,
}

/// The one cap mesh every location shares.
///
/// Held so an orientation change can rewrite it in place: corner angles differ between pointy and
/// flat, which a `Transform` cannot express. Scale and height-scale changes need no rebuild.
#[derive(Resource)]
pub struct CapMesh(Handle<Mesh>);

/// Both outline groups need a negative depth bias, because the lines are exactly coplanar with the
/// cap they trace and would otherwise z-fight with it.
///
/// The bias has to stay small. It shifts normalized depth, which is steeply non-linear, so a tenth
/// of it is a large pull towards the camera — enough that outlines drew straight through the cells
/// standing in front of them. Just enough to win against a coplanar face is the target.
pub fn configure_gizmo_widths(mut store: ResMut<GizmoConfigStore>) {
    let (grid_lines, _) = store.config_mut::<GridLines>();
    grid_lines.line.width = EDGE_WIDTH;
    grid_lines.depth_bias = -0.002;

    let (highlight, _) = store.config_mut::<Highlight>();
    highlight.line.width = ACTIVE_EDGE_WIDTH;
    highlight.depth_bias = -0.004;
}

/// Spawns a cell per location: a parent carrying the transform, with a cap and a wall under it.
pub fn spawn_grid(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    grid: Res<GridModel>,
    layout: Res<HexLayout>,
) {
    let cap = meshes.add(cap_mesh(&layout));
    commands.insert_resource(CapMesh(cap.clone()));

    let cap_material = materials.add(StandardMaterial {
        base_color: CAP_FILL,
        perceptual_roughness: 0.9,
        ..default()
    });
    let wall_material = materials.add(StandardMaterial {
        base_color: WALL_FILL,
        perceptual_roughness: 0.95,
        ..default()
    });

    for location in grid.iter() {
        let coord = location.coord;
        commands.spawn((
            HexCell { coord },
            cell_transform(&layout, coord),
            Visibility::default(),
            children![
                (
                    Mesh3d(cap.clone()),
                    MeshMaterial3d(cap_material.clone()),
                    // The cap rides at the location's own height; the parent's scale turns that
                    // dimensionless height into world units along with everything else.
                    Transform::from_translation(layout.plane.normal() * location.data.height),
                ),
                (
                    HexWall { coord },
                    Mesh3d(meshes.add(wall_mesh(&layout, &grid, coord))),
                    MeshMaterial3d(wall_material.clone()),
                ),
            ],
        ));
    }
}

/// Places a cell and stretches its unit-scale meshes to the layout. Heights ride inside the meshes,
/// so this depends on the coordinate alone.
fn cell_transform(layout: &HexLayout, coord: Axial) -> Transform {
    Transform::from_translation(layout.hex_to_world(coord)).with_scale(layout.mesh_scale())
}

/// Keeps the cells in step when the layout changes — its scale, height scale, or orientation.
///
/// The meshes only really need rebuilding when the orientation changes, since a scale change is
/// pure `Transform`.
// ponytail: rebuilds on any layout change rather than tracking which field moved. Thirty-seven
// cells of twelve triangles is nothing; revisit if the grid grows or the layout animates.
pub fn sync_cells(
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    cap_mesh_handle: Option<Res<CapMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cells: Query<(&HexCell, &mut Transform)>,
    walls: Query<(&HexWall, &Mesh3d)>,
) {
    if !layout.is_changed() {
        return;
    }

    // Positions move with orientation as well as scale, so both are recomputed here.
    for (cell, mut transform) in &mut cells {
        *transform = cell_transform(&layout, cell.coord);
    }

    // Rewriting the shared asset updates every cap at once.
    if let Some(handle) = cap_mesh_handle
        && let Some(mut mesh) = meshes.get_mut(&handle.0)
    {
        *mesh = cap_mesh(&layout);
    }

    for (wall, handle) in &walls {
        if let Some(mut mesh) = meshes.get_mut(&handle.0) {
            *mesh = wall_mesh(&layout, &grid, wall.coord);
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
        let corners = cap_corners(&layout, location.coord, location.data.height);
        // `linestrip` does not close the loop, so repeat the first corner.
        let loop_ = corners.into_iter().chain(std::iter::once(corners[0]));
        if selected.0 == Some(location.coord) {
            highlight.linestrip(loop_, ACTIVE_EDGE);
        } else {
            grid_lines.linestrip(loop_, EDGE);
        }
    }
}

/// World positions of a cap's six corners — the inset hexagon the wall hangs from, and the outline
/// the gizmos trace.
fn cap_corners(layout: &HexLayout, coord: Axial, height: f32) -> [Vec3; 6] {
    let centre = layout.surface_centre(coord, height);
    layout.corner_offsets().map(|o| centre + o * (1.0 - INSET))
}

/// The flat inset hexagon every location shares, as a fan of six triangles about its centre.
///
/// Built from [`HexLayout::corner_offsets`] at unit scale — the same function the outlines and the
/// walls use — so none of the three can disagree.
fn cap_mesh(layout: &HexLayout) -> Mesh {
    let corners = layout.unit().corner_offsets().map(|o| o * (1.0 - INSET));
    let up = layout.plane.normal();

    let mut faces = Faces::with_capacity(6);
    for i in 0..6 {
        // `corner_offsets` runs clockwise seen from the `+normal` side, so an upward-facing fan
        // walks it backwards.
        faces.push([Vec3::ZERO, corners[(i + 1) % 6], corners[i]], up);
    }
    faces.build()
}

/// The ring joining one location's cap to its neighbours': six quads from the inset cap edge out to
/// the fence on the full hexagon's corners.
///
/// Built in the unit frame with dimensionless heights, so the cell's `Transform` supplies both the
/// hex size and the height scale and nothing here has to be rebuilt when either changes.
fn wall_mesh(layout: &HexLayout, grid: &TerrainGrid, coord: Axial) -> Mesh {
    let unit = layout.unit();
    let corners = unit.corner_offsets();
    let up = unit.plane.normal();
    let height = grid.get(coord).map_or(0.0, |l| l.data.height);
    let fence: [f32; 6] = core::array::from_fn(|j| fence_height(layout, grid, coord, j));

    let mut faces = Faces::with_capacity(12);
    for j in 0..6 {
        let k = (j + 1) % 6;
        let inner = [corners[j], corners[k]].map(|c| c * (1.0 - INSET) + up * height);
        let outer = [corners[j] + up * fence[j], corners[k] + up * fence[k]];
        faces.push_flat([inner[0], inner[1], outer[1]]);
        faces.push_flat([inner[0], outer[1], outer[0]]);
    }
    faces.build()
}

/// The height of the fence at one corner of a location: the mean over the locations present at that
/// point of the lattice, which is at most this one and the two neighbours sharing the corner.
///
/// Averaging over what is **present**, rather than standing in a value for what is absent, is what
/// keeps the surface closed. Each of the up-to-three cells meeting here computes this same mean, so
/// their walls land on exactly the same point; substituting each cell's own height for a missing
/// neighbour would have them disagree and split the seam open. At the edge of the grid the mean is
/// over fewer cells, which is why a boundary hex ends in a level lip and a lone one is a flat plate.
fn fence_height(layout: &HexLayout, grid: &TerrainGrid, coord: Axial, corner: usize) -> f32 {
    let (a, b) = layout.corner_directions(corner);
    let mut present: Vec<(Axial, f32)> = [coord, coord.neighbour(a), coord.neighbour(b)]
        .into_iter()
        .filter_map(|c| grid.get(c).map(|l| (c, l.data.height)))
        .collect();

    // Summed in coordinate order, not in the order they were found. Floating-point addition is not
    // associative, so without this the three cells meeting here would each get a slightly different
    // answer and crack the surface at exactly the vertices that are hardest to look at.
    present.sort_unstable_by_key(|(c, _)| (c.q, c.r));
    present.iter().map(|(_, h)| h).sum::<f32>() / present.len() as f32
}

/// Accumulates flat-shaded triangles.
///
/// Every face stores the normal its winding implies — `(v1 - v0) × (v2 - v0)` — because that cross
/// product is what back-face culling reads. Getting it backwards is invisible in the source and
/// glaring on screen, so a test checks the two against each other.
struct Faces {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
}

impl Faces {
    fn with_capacity(triangles: usize) -> Self {
        Self {
            positions: Vec::with_capacity(triangles * 3),
            normals: Vec::with_capacity(triangles * 3),
        }
    }

    fn push(&mut self, tri: [Vec3; 3], normal: Vec3) {
        self.positions.extend(tri);
        self.normals.extend([normal; 3]);
    }

    /// Pushes a triangle normalled from its own winding, for faces whose facing is not known up
    /// front — a wall's incline depends on the heights either side of it.
    fn push_flat(&mut self, tri: [Vec3; 3]) {
        let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
        self.push(tri, normal);
    }

    fn build(self) -> Mesh {
        // Untextured tiles: UVs exist only to satisfy the standard material's pipeline.
        let uvs = vec![[0.5, 0.5]; self.positions.len()];
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::{undulating, Grid, Location, Orientation, Terrain};
    use crate::view::layout::GridPlane;

    const EPS: f32 = 1e-4;

    fn grid() -> TerrainGrid {
        Grid::hexagon(3, undulating)
    }

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

    fn layouts() -> Vec<HexLayout> {
        let mut out = Vec::new();
        for plane in [GridPlane::Xz, GridPlane::Xy] {
            for orientation in [Orientation::Pointy, Orientation::Flat] {
                out.push(
                    HexLayout::pointy(1.0)
                        .with_plane(plane)
                        .with_orientation(orientation),
                );
            }
        }
        out
    }

    /// Winding is what back-face culling reads, and getting it backwards is invisible in the code
    /// and glaring on screen. Every triangle's geometric normal must be the one it stores.
    #[test]
    fn every_face_is_wound_to_match_its_normal() {
        let grid = grid();
        for layout in layouts() {
            let mut meshes = vec![cap_mesh(&layout)];
            meshes.extend(grid.coords().map(|c| wall_mesh(&layout, &grid, c)));
            for mesh in &meshes {
                for ([v0, v1, v2], normal) in triangles(mesh) {
                    let geometric = (v1 - v0).cross(v2 - v0).normalize();
                    assert!(
                        geometric.abs_diff_eq(normal, EPS),
                        "face wound {geometric:?}, normal {normal:?} ({layout:?})"
                    );
                }
            }
        }
    }

    /// Caps face up, and so does every wall: the surface is a terrain, so nothing overhangs.
    #[test]
    fn nothing_faces_downwards() {
        let grid = grid();
        for layout in layouts() {
            let up = layout.plane.normal();
            for (_, normal) in triangles(&cap_mesh(&layout)) {
                assert!(normal.abs_diff_eq(up, EPS), "a cap should be level");
            }
            for coord in grid.coords() {
                for (tri, normal) in triangles(&wall_mesh(&layout, &grid, coord)) {
                    assert!(
                        normal.dot(up) >= -EPS,
                        "wall at {coord:?} overhangs: {normal:?} for {tri:?}"
                    );
                }
            }
        }
    }

    /// The money test for the whole scheme: wherever two cells meet the lattice at the same point,
    /// they must put their fence at exactly the same height, or the surface splits open along that
    /// edge. Bitwise equality — the mean is summed in coordinate order precisely so that every cell
    /// meeting at a point gets an identical answer, not merely a close one.
    #[test]
    fn cells_agree_on_every_shared_lattice_point() {
        let grid = grid();
        for layout in layouts() {
            for coord in grid.coords() {
                let corners = layout.corners(coord);
                for (j, position) in corners.iter().enumerate() {
                    let mine = fence_height(&layout, &grid, coord, j);
                    let (a, b) = layout.corner_directions(j);
                    for direction in [a, b] {
                        let neighbour = coord.neighbour(direction);
                        if !grid.contains(neighbour) {
                            continue;
                        }
                        // The neighbour's own index for this same point of the lattice.
                        let theirs = layout
                            .corners(neighbour)
                            .iter()
                            .position(|c| c.abs_diff_eq(*position, EPS))
                            .expect("neighbours share the corner");
                        assert_eq!(
                            mine,
                            fence_height(&layout, &grid, neighbour, theirs),
                            "{coord:?} corner {j} disagrees with {neighbour:?} corner {theirs}"
                        );
                    }
                }
            }
        }
    }

    /// Each cell's mesh covers its own hexagon and no more, which is what makes hiding or adding
    /// one a clean hexagon's worth of surface.
    #[test]
    fn a_cell_stays_inside_its_own_hexagon() {
        let layout = HexLayout::pointy(1.0);
        let grid = grid();
        let up = layout.plane.normal();
        for coord in grid.coords() {
            let mut reach: f32 = 0.0;
            for (tri, _) in triangles(&wall_mesh(&layout, &grid, coord)) {
                for v in tri {
                    reach = reach.max((v - v.dot(up) * up).length());
                }
            }
            // The circumradius is 1 in the unit frame, and the outer ring lies exactly on it.
            assert!((reach - 1.0).abs() < EPS, "{coord:?} reaches {reach}");
        }
    }

    #[test]
    fn a_lone_cell_is_a_flat_plate() {
        // With no neighbours every mean is over one location, so the fence is level with the cap
        // and the wall is a flat brim — the same code path that gives the grid's edge its lip.
        let layout = HexLayout::pointy(1.0);
        let mut grid: TerrainGrid = Grid::new();
        grid.insert(Location::new(Axial::ZERO, Terrain { height: -0.4 }));

        let mesh = wall_mesh(&layout, &grid, Axial::ZERO);
        let brim = triangles(&mesh);
        assert_eq!(brim.len(), 12, "six quads");
        for (tri, _) in brim {
            for v in tri {
                assert!((v.y - -0.4).abs() < EPS, "{v:?} should be level with the cap");
            }
        }
        assert_eq!(triangles(&cap_mesh(&layout)).len(), 6);
    }
}
