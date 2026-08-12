//! Draws the grid as a continuous surface, one cell at a time.
//!
//! A location's territory is its **full** hexagon. Its cap is that hexagon shrunk by [`INSET`] and
//! lifted to the location's height; the ring between the two is its wall, and every cell emits the
//! same thirty triangles with no rule deciding who owns what.
//!
//! The gaps an inset hexagon leaves are of two kinds, and so is the wall. Along each edge sits a
//! **bridge**, half of the ramp down to the neighbour's cap, level along its length at the mean of
//! the two heights. At each corner sits a **wedge**, one third of the triangle joining the three
//! caps that meet at that point of the lattice.
//!
//! Both rules are symmetric, so two cells compute the same heights for the geometry they share and
//! the surface is continuous. Keeping the bridge at the *pairwise* mean is what makes it level
//! between two equal neighbours: putting its far edge on the lattice vertices instead dragged it up
//! towards whatever tall third cell happened to touch a corner, and warped it enough that its two
//! triangles shaded differently. Nothing anywhere refers to the grid plane, so a location below it
//! is simply a dip.
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

/// The ring joining one location's cap to its neighbours', filling exactly the part of the
/// location's hexagon its cap does not cover.
///
/// Two kinds of piece, because the gaps an inset hexagon leaves are of two kinds. Along each edge a
/// **bridge**, half of the ramp between this cap and the neighbour's, level along its length at the
/// mean of the two heights. At each corner a **wedge**, one third of the triangle joining the three
/// caps that meet at that point of the lattice, cut at its centroid.
///
/// Both are planar by construction — a bridge spans two parallel level edges, and a wedge is a piece
/// of the plane through three points — so a wall is flat-shaded without a crease running across it.
///
/// Built in the unit frame with dimensionless heights, so the cell's `Transform` supplies both the
/// hex size and the height scale and nothing here has to be rebuilt when either changes.
fn wall_mesh(layout: &HexLayout, grid: &TerrainGrid, coord: Axial) -> Mesh {
    let unit = layout.unit();
    let corners = unit.corner_offsets();
    let up = unit.plane.normal();
    let height = grid.get(coord).map_or(0.0, |l| l.data.height);

    let cap: [Vec3; 6] = core::array::from_fn(|j| corners[j] * (1.0 - INSET) + up * height);
    // Edge `j` runs from corner `j` to corner `j + 1`.
    let edge: [f32; 6] = core::array::from_fn(|j| edge_height(layout, grid, coord, j));
    let corner: [f32; 6] = core::array::from_fn(|j| corner_height(layout, grid, coord, j));

    // Where a bridge's far edge meets the hexagon's edge, near corner `j` of edge `j..k`: the
    // midpoint of this cap's corner and the neighbour cap's, which lands on the shared edge short
    // of the lattice vertex by the same inset.
    let bridge_end = |j: usize, k: usize, elevation: f32| {
        corners[j] * (1.0 - INSET / 2.0) + corners[k] * (INSET / 2.0) + up * elevation
    };

    let mut faces = Faces::with_capacity(24);
    for j in 0..6 {
        let (k, previous) = ((j + 1) % 6, (j + 5) % 6);

        // The bridge: level along the edge, so two equal neighbours are joined by a level ramp no
        // matter how tall anything else touching their corners is.
        let near = bridge_end(j, k, edge[j]);
        let far = bridge_end(k, j, edge[j]);
        faces.push_flat([cap[j], cap[k], far]);
        faces.push_flat([cap[j], far, near]);

        // The wedge, between the two bridges either side of corner `j` and reaching the lattice
        // vertex itself.
        let vertex = corners[j] + up * corner[j];
        faces.push_flat([cap[j], near, vertex]);
        faces.push_flat([cap[j], vertex, bridge_end(j, previous, edge[previous])]);
    }
    faces.build()
}

/// The height of a bridge along one edge: the mean of this location and the one across it.
fn edge_height(layout: &HexLayout, grid: &TerrainGrid, coord: Axial, edge: usize) -> f32 {
    let direction = layout.corner_directions(edge).0;
    mean_height(grid, &[coord, coord.neighbour(direction)])
}

/// The height at one corner of a location: the mean over the locations present at that point of the
/// lattice, which is at most this one and the two neighbours sharing the corner.
fn corner_height(layout: &HexLayout, grid: &TerrainGrid, coord: Axial, corner: usize) -> f32 {
    let (a, b) = layout.corner_directions(corner);
    mean_height(grid, &[coord, coord.neighbour(a), coord.neighbour(b)])
}

/// The mean height over whichever of `coords` the grid actually holds.
///
/// Averaging over what is **present**, rather than standing in a value for what is absent, is what
/// keeps the surface closed. Every cell meeting at a point computes this same mean, so their walls
/// land on exactly the same place; substituting each cell's own height for a missing neighbour would
/// have them disagree and split the seam open. At the edge of the grid the mean is over fewer cells,
/// which is why a boundary hex ends in a level lip and a lone one is a flat plate.
fn mean_height(grid: &TerrainGrid, coords: &[Axial]) -> f32 {
    let mut present: Vec<(Axial, f32)> = coords
        .iter()
        .filter_map(|c| grid.get(*c).map(|l| (*c, l.data.height)))
        .collect();

    // Summed in coordinate order, not in the order they were found. Floating-point addition is not
    // associative, so without this the three cells meeting at a corner would each get a slightly
    // different answer and crack the surface at exactly the vertices that are hardest to look at.
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

    /// The money test for the whole scheme: wherever two cells meet, they must put their shared
    /// geometry at exactly the same height, or the surface splits open. Bitwise equality — the mean
    /// is summed in coordinate order precisely so that every cell meeting at a point gets an
    /// identical answer, not merely a close one.
    #[test]
    fn cells_agree_on_every_shared_edge_and_corner() {
        let grid = grid();
        for layout in layouts() {
            for coord in grid.coords() {
                let corners = layout.corners(coord);
                for (j, position) in corners.iter().enumerate() {
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
                            corner_height(&layout, &grid, coord, j),
                            corner_height(&layout, &grid, neighbour, theirs),
                            "{coord:?} corner {j} disagrees with {neighbour:?} corner {theirs}"
                        );
                        // And the bridge they share: `a` is the edge starting at this corner.
                        if direction == a {
                            let back = (direction + 3) % 6;
                            let their_edge = (0..6)
                                .find(|&e| layout.corner_directions(e).0 == back)
                                .expect("the edge back this way");
                            assert_eq!(
                                edge_height(&layout, &grid, coord, j),
                                edge_height(&layout, &grid, neighbour, their_edge),
                                "{coord:?} edge {j} disagrees with {neighbour:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// The artefact that prompted this construction: a bridge between two locations of equal height
    /// must be level, however tall the cells touching its ends are. Putting the wall's far edge at
    /// the corner mean instead dragged it up towards whichever tall neighbour shared a corner, and
    /// warped the quad enough that its two triangles shaded differently.
    #[test]
    fn a_bridge_between_equal_neighbours_is_level() {
        let layout = HexLayout::pointy(1.0);
        let up = layout.plane.normal();
        let (left, right) = (Axial::ZERO, Axial::new(1, 0));
        let mut grid: TerrainGrid = Grid::new();
        grid.insert(Location::new(left, Terrain { height: 0.0 }));
        grid.insert(Location::new(right, Terrain { height: 0.0 }));
        // A tall cell on one of the corners the two share.
        for direction in 0..6 {
            let third = left.neighbour(direction);
            if third != right && third.distance(right) == 1 {
                grid.insert(Location::new(third, Terrain { height: 2.0 }));
            }
        }

        let edge = (0..6)
            .find(|&e| layout.corner_directions(e).0 == 0)
            .expect("the edge facing +q");
        assert_eq!(edge_height(&layout, &grid, left, edge), 0.0);

        // And the flat-shaded wall says the same: no triangle of the bridge tilts.
        for (tri, normal) in triangles(&wall_mesh(&layout, &grid, left)) {
            if tri.iter().all(|v| v.dot(up).abs() < EPS) {
                assert!(normal.abs_diff_eq(up, EPS), "a level piece should face straight up");
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
        assert_eq!(brim.len(), 24, "six bridges and six wedges");
        for (tri, _) in brim {
            for v in tri {
                assert!((v.y - -0.4).abs() < EPS, "{v:?} should be level with the cap");
            }
        }
        assert_eq!(triangles(&cap_mesh(&layout)).len(), 6);
    }
}
