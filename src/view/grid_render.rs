//! Draws the grid: one filled hexagon per location, outlined, with the selection picked out.
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

/// The one hexagon mesh every cell shares.
///
/// Held so an orientation change can rewrite it in place: corner angles differ between pointy and
/// flat, which a `Transform` cannot express. Scale changes still need no rebuild.
#[derive(Resource)]
pub struct HexMesh(pub Handle<Mesh>);

/// Both outline groups need a negative depth bias, because the lines are exactly coplanar with the
/// faces they trace and would otherwise z-fight with them.
///
/// This is easy to miss: at an oblique angle depth varies along each line and enough of it wins to
/// look correct, but from directly overhead an interior edge has a face on both sides at identical
/// depth and disappears entirely. Only edges bordering empty space survive, so the grid renders as a
/// silhouette with no internal structure.
pub fn configure_gizmo_widths(mut store: ResMut<GizmoConfigStore>) {
    let (grid_lines, _) = store.config_mut::<GridLines>();
    grid_lines.line.width = EDGE_WIDTH;
    grid_lines.depth_bias = -0.1;

    let (highlight, _) = store.config_mut::<Highlight>();
    highlight.line.width = ACTIVE_EDGE_WIDTH;
    highlight.depth_bias = -0.2;
}

/// Spawns a face per location, all sharing one unit-sized mesh and material.
pub fn spawn_grid(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    grid: Res<GridModel>,
    layout: Res<HexLayout>,
) {
    let mesh = meshes.add(unit_hex_mesh(&layout));
    commands.insert_resource(HexMesh(mesh.clone()));
    let material = materials.add(StandardMaterial {
        base_color: FILL,
        perceptual_roughness: 0.9,
        // The grid is a single flat sheet; showing it from underneath beats culling it away.
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    for coord in grid.coords() {
        commands.spawn((
            HexCell { coord },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            cell_transform(&layout, coord),
        ));
    }
}

fn cell_transform(layout: &HexLayout, coord: Axial) -> Transform {
    Transform::from_translation(layout.hex_to_world(coord)).with_scale(layout.mesh_scale())
}

/// Keeps the faces in step when the layout changes — its scale, or its orientation.
pub fn sync_cells(
    layout: Res<HexLayout>,
    hex_mesh: Option<Res<HexMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cells: Query<(&HexCell, &mut Transform)>,
) {
    if !layout.is_changed() {
        return;
    }

    // Positions move with orientation as well as scale, so both are recomputed here.
    for (cell, mut transform) in &mut cells {
        *transform = cell_transform(&layout, cell.coord);
    }

    // Rewriting the shared asset updates all 37 cells at once.
    if let Some(hex_mesh) = hex_mesh
        && let Some(mut mesh) = meshes.get_mut(&hex_mesh.0)
    {
        *mesh = unit_hex_mesh(&layout);
    }
}

pub fn draw_outlines(
    mut grid_lines: Gizmos<GridLines>,
    mut highlight: Gizmos<Highlight>,
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    selected: Res<Selected>,
) {
    for coord in grid.coords() {
        let corners = layout.corners(coord);
        // `linestrip` does not close the loop, so repeat the first corner.
        let loop_ = corners.into_iter().chain(std::iter::once(corners[0]));
        if selected.0 == Some(coord) {
            highlight.linestrip(loop_, ACTIVE_EDGE);
        } else {
            grid_lines.linestrip(loop_, EDGE);
        }
    }
}

/// A hexagon of circumradius 1, as a fan of six triangles about its centre.
///
/// Built from [`HexLayout::corner_offsets`] at unit scale — the same function the outlines use — so
/// the faces and the lines cannot disagree. Scaling to the layout's size is a `Transform`, so
/// changing scale never rebuilds this.
fn unit_hex_mesh(layout: &HexLayout) -> Mesh {
    let corners = layout.unit().corner_offsets();
    let normal = layout.plane.normal();

    let mut positions = Vec::with_capacity(18);
    for i in 0..6 {
        positions.push(Vec3::ZERO);
        positions.push(corners[i]);
        positions.push(corners[(i + 1) % 6]);
    }

    // Flat, untextured tiles: UVs exist only to satisfy the standard material's pipeline.
    let uvs = vec![[0.5, 0.5]; positions.len()];
    let normals = vec![normal; positions.len()];

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
}
