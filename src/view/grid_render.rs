//! Draws the grid as a continuous surface, one cell at a time.
//!
//! A location's territory is its **full** hexagon. Its cap is that hexagon shrunk by
//! [`HexLayout::inset`] and lifted to the location's height; the ring between the two is its wall,
//! and every cell emits the same thirty triangles with no rule deciding who owns what.
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
//! Under both hangs the **skirt**: a closed hexagonal prism reaching from the boundary of the
//! location's hexagon down to a bottom of its own, so the surface is a solid seen from below rather
//! than a shell. Where the grid ends and a location is under water, the skirt also carries the cut
//! face of that water. It is a third child, and hiding every one of them leaves the bare shell.
//!
//! Faces are meshes; outlines are gizmos. Gizmos suit the outlines because they are immediate-mode,
//! so switching which hex is highlighted costs nothing, and line width is a per-config-group
//! setting — hence two groups rather than one.

use bevy::{
    asset::RenderAssetUsages,
    gizmos::config::GizmoConfigGroup,
    mesh::PrimitiveTopology,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    reflect::Reflect,
    // 0.19 split the render crates up: `ShaderRef` now lives in `bevy_shader`, not `bevy_render`.
    render::render_resource::{AsBindGroup, ShaderType},
    shader::ShaderRef,
};

use super::GridModel;
use super::layout::HexLayout;
use super::selection::Selected;
use crate::hex::{Axial, Terrain, TerrainGrid};

/// Thin outlines for every hex.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct GridLines;

/// Thick outline for the active hex.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct Highlight;

/// Whether every hex is outlined. Off unless asked for: the outlines say which hexes can be
/// clicked, which is a question about the grid rather than part of the scene. The selected hex
/// keeps its own outline either way.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowGridLines(pub bool);

const CAP_FILL: Color = Color::srgb(0.16, 0.19, 0.26);
const WALL_FILL: Color = Color::srgb(0.13, 0.16, 0.22);
const WATER_FILL: Color = Color::srgb(0.06, 0.20, 0.34);

/// Colour of water shallow enough to see the bottom through — the pale end of the shoaling ramp,
/// [`WATER_FILL`] being the deep end.
///
/// **Linear, not sRGB.** It reaches the shader as a bare `vec3` and is mixed there against a base
/// colour the material has already converted, so it has to arrive in the same space; the skirt's
/// cross-section writes it as a vertex colour, which is linear too.
const WATER_SHALLOW: LinearRgba = LinearRgba::rgb(0.10, 0.32, 0.36);

/// Depth, in the model's height units, by which water has reached [`WATER_FILL`]. Shared by the
/// surface's shoaling and by the cross-section's gradient, so the cut agrees with what it cuts.
const WATER_SHALLOW_DEPTH: f32 = 0.55;

/// How far the shallowest skirt reaches below the lowest ground in the grid, in dimensionless
/// height. A floor common to the whole grid rather than a depth below each cap: a cell's own
/// boundary dips towards its lower neighbours, so a depth measured from its cap could leave that
/// boundary hanging under its own bottom.
const SKIRT_BASE: f32 = 0.6;

/// The step between the five skirt lengths [`wobble`] chooses between.
///
/// **Twice this must stay under [`SKIRT_BASE`]**, or the deepest wobble upwards lifts a bottom above
/// the lowest ground and the prism turns inside out.
const SKIRT_STEP: f32 = 0.12;

const _: () = assert!(
    2.0 * SKIRT_STEP < SKIRT_BASE,
    "a wobble upwards could lift a bottom above the ground it hangs from"
);

/// How far a water surface is set **below** its stated level, in dimensionless height, to break an
/// exact depth tie against the ground.
///
/// Ground can sit *exactly* at its own water line — `undulating` puts a whole diagonal at a bitwise
/// zero — and a plate coplanar with a cap is an exact depth tie, which z-fights in fans radiating
/// from the cell centres. `StandardMaterial::depth_bias` does not help: despite its documentation
/// it only feeds the render-phase sort key, and the mesh pipeline hardcodes a zero rasterizer bias.
/// So the tie is broken in geometry, by a nudge far below anything visible.
///
/// **Downwards**, so that the ground wins and land level with the water reads as land. Nudging the
/// plate up instead loses the tie to the water over every such cap — and since a piece reaches from
/// a submerged edge inward to the cell's centre, that is half of each location along that diagonal
/// covered in zero-depth water, which is the palest there is: a bright line across dry ground.
const WATER_TIE_BREAK: f32 = 0.002;

/// The water material every plate shares.
///
/// An extension to the standard material, not a material of its own: Fresnel, the environment
/// reflection and the sun's specular all come from the stock lighting, and the shader supplies only
/// the two things it cannot know — a normal broken up by ripples, and the shoaling colour.
pub type WaterMaterial = ExtendedMaterial<StandardMaterial, WaterExtension>;

/// What the water shader needs beyond a standard material. Mirrors `WaterSettings` in
/// `assets/shaders/water.wgsl`; the two are bound together by nothing but agreement, so they move
/// in the same edit.
#[derive(Clone, Copy, Debug, ShaderType, Reflect)]
pub struct WaterSettings {
    /// Water shallow enough to see the bottom through.
    pub shallow: Vec3,
    /// Depth, in the model's height units, by which the colour has reached [`WATER_FILL`].
    pub shallow_depth: f32,
    /// Peak slope of the ripple field — a slope, not a height, because nothing is displaced.
    pub amplitude: f32,
    /// Length of the longest ripple, in world units. At `HEX_SCALE` of 1 this wants to be a small
    /// fraction of a hexagon: these are the cat's-paws a breeze puts on a lake, not sea waves.
    pub wavelength: f32,
    /// How fast the pattern travels, in world units per second.
    pub speed: f32,
    /// Distance at which ripples have faded to half strength.
    pub fade: f32,
}

/// The material extension. Bindings start at 100 because 0-99 belong to the base material.
#[derive(Asset, AsBindGroup, Clone, Debug, Reflect)]
pub struct WaterExtension {
    #[uniform(100)]
    pub settings: WaterSettings,
}

impl MaterialExtension for WaterExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/water.wgsl".into()
    }
}

const EDGE: Color = Color::srgb(0.35, 0.75, 0.85);
const ACTIVE_EDGE: Color = Color::srgb(1.0, 0.78, 0.25);

const EDGE_WIDTH: f32 = 1.5;
const ACTIVE_EDGE_WIDTH: f32 = 6.0;

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

/// The child holding a cell's skirt: the closed prism that hangs from its hexagon's boundary, and
/// the cross-section of any water standing over it at the grid's rim.
#[derive(Component)]
pub struct HexSkirt {
    coord: Axial,
}

/// Whether the skirt is hidden, leaving the bare shell the surface used to be. Driven by the debug
/// panel's checkbox, and off by default — the skirt is what closes the grid.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HideSkirt(pub bool);

/// The assets every location shares: the hexagon fan every cap is drawn from, and the material every
/// water surface is drawn with.
///
/// The mesh is held so an orientation change can rewrite it in place: corner angles differ between
/// pointy and flat, which a `Transform` cannot express. Scale and height-scale changes need no
/// rebuild. The material is held because water surfaces come and go as the level moves.
///
/// A water plate is **not** here, unlike a cap: each carries its own depths, so no two are alike.
#[derive(Resource)]
pub struct SharedAssets {
    cap: Handle<Mesh>,
    water_material: Handle<WaterMaterial>,
}

/// A location's water surface, so the set of them can be rebuilt when the level moves.
#[derive(Component)]
pub struct WaterSurface;

/// The level the whole grid is flooded to, in the same units as a height. Driven by the debug
/// panel's slider.
#[derive(Resource, Debug, PartialEq)]
pub struct SeaLevel(pub f32);

impl Default for SeaLevel {
    fn default() -> Self {
        Self(crate::hex::SEA_LEVEL)
    }
}

/// Writes the sea level into the model, which is what actually decides where water is.
///
/// The model carries a level per location so that separate bodies can differ; a single sea level is
/// simply the case where they all agree.
///
/// The `is_added` half of the guard is what leaves a scene's own water alone: inserting a resource
/// counts as changing it, so without it the first frame would flood over whatever levels the scene
/// authored, before any of it was ever on screen.
// ponytail: one sea for the whole grid, so a drag of the slider still overwrites every level in the
// model, authored or not. Superseded by a real flooding algorithm, not worth patching before it.
pub fn apply_sea_level(sea: Res<SeaLevel>, mut grid: ResMut<GridModel>) {
    if !sea.is_changed() || sea.is_added() {
        return;
    }
    crate::hex::flood(&mut grid, sea.0);
}

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
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    grid: Res<GridModel>,
    layout: Res<HexLayout>,
) {
    let shared = SharedAssets {
        cap: meshes.add(hex_fan_mesh(&layout, 1.0 - layout.inset)),
        water_material: water_materials.add(WaterMaterial {
            base: StandardMaterial {
                base_color: WATER_FILL,
                // Near a mirror: what a smooth water surface is. The ripples in the shader are
                // what spread the sun's reflection into a glitter rather than a point, so this
                // stays low and lets them do it.
                perceptual_roughness: 0.03,
                // Water's index of refraction, 1.33, is the reflectance the standard material's
                // 0.5 midpoint is calibrated against — close enough to leave alone. What was
                // missing was never the number, it was something to reflect.
                reflectance: 0.5,
                // Opaque, and load-bearing. The terrain occludes the plate through the depth
                // buffer, which is what cuts the shoreline with no clipping geometry; and
                // neighbouring plates overlap by design, which under blending would darken twice
                // and draw hexagons on the sea.
                ..default()
            },
            extension: WaterExtension {
                settings: WaterSettings {
                    shallow: Vec3::new(WATER_SHALLOW.red, WATER_SHALLOW.green, WATER_SHALLOW.blue),
                    shallow_depth: WATER_SHALLOW_DEPTH,
                    amplitude: 0.15,
                    wavelength: 0.32,
                    speed: 0.05,
                    fade: 14.0,
                },
            },
        }),
    };

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
    // White, because the skirt carries its colour per vertex — rock along the prism, and the
    // water's shoaling ramp across a cross-section. The standard material multiplies the two, so
    // anything but white would tint both.
    let skirt_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.95,
        ..default()
    });

    let up = layout.plane.normal();
    let lowest = lowest_height(&grid);
    for location in grid.iter() {
        let coord = location.coord;
        commands.spawn((
            HexCell { coord },
            cell_transform(&layout, coord),
            Visibility::default(),
            children![
                (
                    Mesh3d(shared.cap.clone()),
                    MeshMaterial3d(cap_material.clone()),
                    // The cap rides at the location's own height; the parent's scale turns that
                    // dimensionless height into world units along with everything else.
                    Transform::from_translation(up * location.data.height),
                ),
                (
                    HexWall { coord },
                    Mesh3d(meshes.add(wall_mesh(&layout, &grid, coord))),
                    MeshMaterial3d(wall_material.clone()),
                ),
                (
                    HexSkirt { coord },
                    Mesh3d(meshes.add(skirt_mesh(&layout, &grid, coord, lowest))),
                    MeshMaterial3d(skirt_material.clone()),
                ),
            ],
        ));
    }

    commands.insert_resource(shared);
}

/// Rebuilds every water surface whenever the model's water changes — which is what a move of the
/// sea level amounts to, since it decides both the level and which locations are wet at all.
// ponytail: throws them all away and builds them again — now a mesh each as well as an entity each,
// which during a slider drag is a few dozen of both a frame. Still nothing at this size, and it
// keeps one code path for a set that changes shape. Rebuild only what moved if the grid grows.
pub fn sync_water(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    grid: Res<GridModel>,
    layout: Res<HexLayout>,
    shared: Option<Res<SharedAssets>>,
    existing: Query<Entity, With<WaterSurface>>,
    cells: Query<(Entity, &HexCell)>,
) {
    let Some(shared) = shared else {
        return;
    };
    if !grid.is_changed() && !layout.is_changed() {
        return;
    }

    for surface in &existing {
        commands.entity(surface).despawn();
    }

    let up = layout.plane.normal();
    for (entity, cell) in &cells {
        for (level, pieces) in water_plates(&layout, &grid, cell.coord) {
            commands.spawn((
                WaterSurface,
                Mesh3d(meshes.add(water_fan_mesh(&layout, &grid, cell.coord, level, pieces))),
                MeshMaterial3d(shared.water_material.clone()),
                Transform::from_translation(up * (level - WATER_TIE_BREAK)),
                ChildOf(entity),
            ));
        }
    }
}

/// Which of a plate's twelve pieces it draws.
///
/// A location's hexagon divides into six sectors, one per edge, and each sector halves at the
/// midpoint of that edge: piece `2 * j` is the half of sector `j` nearer corner `j`, piece `2 * j + 1`
/// the half nearer corner `j + 1`. A half is the smallest area a water level can be granted, and it
/// is the granularity that matters — a half touches exactly **two** neighbours, the one across its
/// edge and the one sharing its corner, so it is never claimed by two bodies unless those two bodies
/// are themselves adjacent, which no two bodies at different levels can be.
type Pieces = [bool; 12];

/// The water surfaces a location has to draw, each as a level and the pieces it covers.
///
/// A plate is opaque, so the terrain occludes it wherever the ground stands higher and the shoreline
/// falls out of the depth buffer for free — no clipping and no shoreline geometry. What each plate
/// has to get right is therefore only its **extent**, and two rules fix it:
///
/// - **Reach.** A level covers a piece only if the body holding it touches that piece: the location's
///   own water covers all twelve, and a neighbour's covers the halves along their shared edge and the
///   halves reaching the two corners they share. Carrying a neighbour's level at all is what closes
///   the water's edge — a dry location's territory includes half of each bridge, at the mean of the
///   two heights, so it dips below the line near a flooded neighbour even though its own cap stands
///   clear. Confining that to the halves the neighbour touches is what stops a body reaching across
///   the location and out the far side, over another body's shore.
/// - **Submergence.** A piece is dropped when every height under it stands above the level, since the
///   plate is buried there and draws nothing anyway. Those heights are the location's own — its cap —
///   the bridge along its edge, and its one or two corners; between them the terrain is planar, so
///   there is nothing lower hiding in between.
///
/// One ring is still all that is ever needed: a location with no flooded neighbour has every
/// surrounding height above the water, so every bridge and wedge is above it too.
fn water_plates(layout: &HexLayout, grid: &TerrainGrid, coord: Axial) -> Vec<(f32, Pieces)> {
    let Some(location) = grid.get(coord) else {
        return Vec::new();
    };
    let height = location.data.height;
    let edge: [f32; 6] = core::array::from_fn(|j| edge_height(layout, grid, coord, j));
    let corner: [f32; 6] = core::array::from_fn(|j| corner_height(layout, grid, coord, j));
    let across = |direction: usize| {
        grid.get(coord.neighbour(direction))
            .and_then(|l| l.data.water)
    };

    let mut plates: Vec<(f32, Pieces)> = Vec::new();
    for j in 0..6 {
        // `corner_directions(j)` is the pair of neighbours meeting at corner `j`, the first of which
        // is the one across edge `j` — the edge running from corner `j` to corner `j + 1`.
        let (over_edge, before) = layout.corner_directions(j);
        let after = layout.corner_directions((j + 1) % 6).0;
        for (piece, floor, touching) in [
            (
                2 * j,
                height.min(edge[j]).min(corner[j]),
                [over_edge, before],
            ),
            (
                2 * j + 1,
                height.min(edge[j]).min(corner[(j + 1) % 6]),
                [over_edge, after],
            ),
        ] {
            let reaching = location
                .data
                .water
                .into_iter()
                .chain(touching.map(across).into_iter().flatten());
            for level in reaching {
                // Strictly below, matching `flood`: ground exactly at a level is dry, and a piece
                // whose lowest ground is exactly there has nothing under it to cover at all. It is
                // only half the story where a location sits level with the water — the pieces whose
                // *edge* is submerged are still drawn, and reach inward over that ground — which is
                // what [`WATER_TIE_BREAK`] settles.
                if floor < level {
                    cover(&mut plates, level, piece);
                }
            }
        }
    }
    plates
}

/// Records that a plate at `level` covers `piece`, starting that plate if this is its first piece.
///
/// Levels are compared exactly, which is what groups one body's pieces onto one plate: every
/// location takes the level from the same `Terrain::water`, so the bits are identical.
fn cover(plates: &mut Vec<(f32, Pieces)>, level: f32, piece: usize) {
    match plates.iter_mut().find(|(held, _)| *held == level) {
        Some((_, pieces)) => pieces[piece] = true,
        None => {
            let mut pieces = [false; 12];
            pieces[piece] = true;
            plates.push((level, pieces));
        }
    }
}

/// Places a cell and stretches its unit-scale meshes to the layout. Heights ride inside the meshes,
/// so this depends on the coordinate alone.
fn cell_transform(layout: &HexLayout, coord: Axial) -> Transform {
    Transform::from_translation(layout.hex_to_world(coord)).with_scale(layout.mesh_scale())
}

/// Keeps the cells in step when the layout changes — its scale, height scale, orientation or inset.
///
/// The meshes only really need rebuilding when the orientation or the inset changes; a scale change
/// is pure `Transform`.
// ponytail: rebuilds on any layout change rather than tracking which field moved. Thirty-seven
// cells of twelve triangles is nothing; revisit if the grid grows or the layout animates.
pub fn sync_cells(
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    shared: Option<Res<SharedAssets>>,
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

    // Rewriting the shared cap updates every location's at once. The water surfaces need no help
    // here: `sync_water` also runs on a layout change, and rebuilds them outright.
    if let Some(shared) = shared
        && let Some(mut mesh) = meshes.get_mut(&shared.cap)
    {
        *mesh = hex_fan_mesh(&layout, 1.0 - layout.inset);
    }

    for (wall, handle) in &walls {
        if let Some(mut mesh) = meshes.get_mut(&handle.0) {
            *mesh = wall_mesh(&layout, &grid, wall.coord);
        }
    }
}

/// Rebuilds every skirt when the model or the layout changes.
///
/// The model half is not idle: the sea level moves which locations are flooded, and a skirt at the
/// rim carries a cross-section of that water.
pub fn sync_skirts(
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    mut meshes: ResMut<Assets<Mesh>>,
    skirts: Query<(&HexSkirt, &Mesh3d)>,
) {
    if !grid.is_changed() && !layout.is_changed() {
        return;
    }
    let lowest = lowest_height(&grid);
    for (skirt, handle) in &skirts {
        if let Some(mut mesh) = meshes.get_mut(&handle.0) {
            *mesh = skirt_mesh(&layout, &grid, skirt.coord, lowest);
        }
    }
}

/// Shows or hides every skirt at once, leaving the bare shell behind when hidden.
pub fn sync_skirt_visibility(
    hide: Res<HideSkirt>,
    mut skirts: Query<&mut Visibility, With<HexSkirt>>,
) {
    if !hide.is_changed() {
        return;
    }
    for mut visibility in &mut skirts {
        *visibility = if hide.0 {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

pub fn draw_outlines(
    mut grid_lines: Gizmos<GridLines>,
    mut highlight: Gizmos<Highlight>,
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    selected: Res<Selected>,
    show_grid_lines: Res<ShowGridLines>,
) {
    for location in grid.iter() {
        let is_selected = selected.0 == Some(location.coord);
        if !is_selected && !show_grid_lines.0 {
            continue;
        }
        let corners = outline_corners(&layout, location.coord, location.data);
        // `linestrip` does not close the loop, so repeat the first corner.
        let loop_ = corners.into_iter().chain(std::iter::once(corners[0]));
        if is_selected {
            highlight.linestrip(loop_, ACTIVE_EDGE);
        } else {
            grid_lines.linestrip(loop_, EDGE);
        }
    }
}

/// World positions of the six corners an outline traces: the inset hexagon, floating just clear of
/// whatever the location presents — its cap, or the water over it.
///
/// A submerged location is traced on the water rather than on the sea bed, which is the only way
/// the line reads at all: drawn on the bed it is either swallowed by the water or, where the
/// gizmo's depth bias happens to beat the shallow depth difference, showing through it — and which
/// of those you get depends on how deep the water is, so the grid looks arbitrary.
///
/// The clearance is upward, and larger than [`WATER_TIE_BREAK`], so a line clears every surface it
/// could be competing with — its own cap, and any plate reaching across from a neighbour, which now
/// sits *below* its level rather than above it.
fn outline_corners(layout: &HexLayout, coord: Axial, terrain: Terrain) -> [Vec3; 6] {
    let centre = layout.surface_centre(coord, terrain.surface() + 2.0 * WATER_TIE_BREAK);
    layout
        .corner_offsets()
        .map(|o| centre + o * (1.0 - layout.inset))
}

/// A flat hexagon at the given fraction of the circumradius, as a fan of six triangles about its
/// centre: a location's cap when inset, its water surface at full width.
///
/// Built from [`HexLayout::corner_offsets`] at unit scale — the same function the outlines and the
/// walls use — so none of them can disagree.
fn hex_fan_mesh(layout: &HexLayout, radius: f32) -> Mesh {
    let corners = layout.unit().corner_offsets().map(|o| o * radius);
    let up = layout.plane.normal();

    let mut faces = Faces::with_capacity(6);
    for i in 0..6 {
        // `corner_offsets` runs clockwise seen from the `+normal` side, so an upward-facing fan
        // walks it backwards.
        faces.push([Vec3::ZERO, corners[(i + 1) % 6], corners[i]], up);
    }
    faces.build()
}

/// A location's water surface at one level: the `pieces` of the full-width hexagon that level covers,
/// with the **depth of the water at each vertex** carried in `uv.x`.
///
/// The depth is taken from the model rather than from the depth buffer, which WebGL2 cannot sample.
/// It costs nothing to know: the fan's vertices are the centre, the six lattice vertices, and the six
/// edge midpoints, which are exactly the points [`wall_mesh`] already places geometry at — a cap, a
/// corner and a bridge respectively — so [`corner_height`] and [`edge_height`] give the ground under
/// each one. Depth therefore reaches zero precisely where the terrain rises through the plate, the
/// shoreline the depth buffer cuts, and the shallows meet it with no seam. Between the vertices the
/// terrain is planar, so interpolating is exact rather than merely close.
///
/// The edge midpoint is load-bearing twice over: it halves the sectors, which is what lets a level be
/// granted the part of a location it actually reaches, and it puts a vertex over the bridge. A chord
/// straight from corner to corner interpolates between two corner means and misses the bridge running
/// under it, which is a different height — so the shallows along an edge used to be shaded from a
/// depth the water does not have there.
///
/// A dry location carrying a flooded neighbour's plate gets negative depths over its own ground.
/// That is not a special case: the plate is buried there, and the shader clamps at zero anyway.
fn water_fan_mesh(
    layout: &HexLayout,
    grid: &TerrainGrid,
    coord: Axial,
    level: f32,
    pieces: Pieces,
) -> Mesh {
    let unit = layout.unit();
    let corners = unit.corner_offsets();
    let up = unit.plane.normal();

    let middle = level - grid.get(coord).map_or(0.0, |l| l.data.height);
    let corner: [f32; 6] = core::array::from_fn(|j| level - corner_height(layout, grid, coord, j));
    let bridge: [f32; 6] = core::array::from_fn(|j| level - edge_height(layout, grid, coord, j));

    let mut faces = Faces::with_capacity(12);
    for j in 0..6 {
        let k = (j + 1) % 6;
        // On the chord between the two corners, so halving a sector does not change the outline.
        let mid = (corners[j] + corners[k]) * 0.5;
        // Backwards around the ring, as in `hex_fan_mesh`, so the fan faces up.
        if pieces[2 * j] {
            faces.push_depths(
                [Vec3::ZERO, mid, corners[j]],
                up,
                [middle, bridge[j], corner[j]],
            );
        }
        if pieces[2 * j + 1] {
            faces.push_depths(
                [Vec3::ZERO, corners[k], mid],
                up,
                [middle, corner[k], bridge[j]],
            );
        }
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

    let cap: [Vec3; 6] = core::array::from_fn(|j| corners[j] * (1.0 - layout.inset) + up * height);
    let profile: [Profile; 6] = core::array::from_fn(|j| edge_profile(layout, grid, coord, j));

    let mut faces = Faces::with_capacity(24);
    for j in 0..6 {
        let (k, previous) = ((j + 1) % 6, (j + 5) % 6);
        let [vertex, near, _, far, _] = profile[j];

        // The bridge: level along the edge, so two equal neighbours are joined by a level ramp no
        // matter how tall anything else touching their corners is.
        faces.push_flat([cap[j], cap[k], far]);
        faces.push_flat([cap[j], far, near]);

        // The wedge, between the two bridges either side of corner `j` and reaching the lattice
        // vertex itself — where the previous edge's profile ends.
        faces.push_flat([cap[j], near, vertex]);
        faces.push_flat([cap[j], vertex, profile[previous][3]]);
    }
    faces.build()
}

/// One edge's worth of a location's outer boundary: corner, bridge end, edge midpoint, bridge end,
/// corner.
type Profile = [Vec3; 5];

/// The five points at which a location's surface meets the boundary of its own hexagon along edge
/// `j`, running from corner `j` to corner `j + 1`. All five lie on that edge in plan; what varies is
/// their height.
///
/// This is the seam [`wall_mesh`] and [`skirt_mesh`] share — the wall's outer rim above it, the
/// skirt hanging from it below — so both take it from here rather than each rederiving it. The two
/// inner points are where a bridge's far edge meets the hexagon: the midpoint of this cap's corner
/// and the neighbour cap's, which lands on the shared edge short of the lattice vertex by the same
/// inset.
///
/// The **edge midpoint** between them is the wall's to ignore. It splits the edge where [`Pieces`]
/// splits it, which is what lets a water cross-section be granted the halves the body reaches and no
/// more.
fn edge_profile(layout: &HexLayout, grid: &TerrainGrid, coord: Axial, j: usize) -> Profile {
    let unit = layout.unit();
    let corners = unit.corner_offsets();
    let up = unit.plane.normal();
    let inset = layout.inset;
    let k = (j + 1) % 6;

    let bridge = up * edge_height(layout, grid, coord, j);
    [
        corners[j] + up * corner_height(layout, grid, coord, j),
        corners[j] * (1.0 - inset / 2.0) + corners[k] * (inset / 2.0) + bridge,
        (corners[j] + corners[k]) * 0.5 + bridge,
        corners[k] * (1.0 - inset / 2.0) + corners[j] * (inset / 2.0) + bridge,
        corners[k] + up * corner_height(layout, grid, coord, k),
    ]
}

/// A location's skirt: the closed hexagonal prism hanging from the boundary of its hexagon down to a
/// bottom of its own, plus a cross-section of the water standing over it where the grid ends.
///
/// Closed on **every** location, not only the ones at the rim. Interior sides are buried, but
/// emitting them costs a handful of triangles and buys the absence of a rule: no location has to
/// know whether it is on the boundary, and the step between two neighbours' bottoms — which the
/// wobble guarantees there will be — is closed by the deeper one's own side.
///
/// The water cross-section is the one thing that *is* confined to the rim, because it is the only
/// part that is ever seen. It is taken from [`water_plates`], the same source the surfaces
/// themselves come from, so the cut reaches exactly as far around the rim as the water does — which
/// includes a dry location whose corner dips under a flooded neighbour's level.
// ponytail: the buried interior sides are ~2/3 of the geometry here. Cull them against the
// neighbours' bottoms if the grid ever grows enough for the triangle count to matter.
fn skirt_mesh(layout: &HexLayout, grid: &TerrainGrid, coord: Axial, lowest: f32) -> Mesh {
    let unit = layout.unit();
    let corners = unit.corner_offsets();
    let up = unit.plane.normal();
    let bottom = skirt_bottom(lowest, coord);
    let rock = WALL_FILL.to_linear();
    // Moves a point of the profile to a given height, leaving it where it is in plan.
    let sink = |v: Vec3, height: f32| v - up * v.dot(up) + up * height;

    let plates = water_plates(layout, grid, coord);
    // Eight triangles an edge, plus the fan underneath. A cut face grows it further.
    let mut faces = Faces::with_capacity(6 * 8 + 6);
    for j in 0..6 {
        let profile = edge_profile(layout, grid, coord, j);

        // The prism's side, in the four strips the profile divides the edge into. Wound so the
        // outward face is the front one: the corners run clockwise seen from above, so a triangle
        // taken along the edge and then downwards faces away from the centre.
        for i in 0..4 {
            let (near, far) = (profile[i], profile[i + 1]);
            faces.push_shaded([near, far, sink(far, bottom)], [rock; 3]);
            faces.push_shaded([near, sink(far, bottom), sink(near, bottom)], [rock; 3]);
        }

        // Beyond here is the water above the ground, which only shows where the grid stops.
        if grid.contains(coord.neighbour(layout.corner_directions(j).0)) {
            continue;
        }
        for (level, pieces) in &plates {
            // The plate's own level, so the section's top edge lies in the surface rather than a
            // hair above it.
            let surface = level - WATER_TIE_BREAK;
            for i in 0..4 {
                // Strips 0 and 1 are the half of the sector nearer corner `j`, 2 and 3 the half
                // nearer corner `j + 1` — the two pieces the plate is granted separately.
                if !pieces[2 * j + i / 2] {
                    continue;
                }
                let (near, far) = (profile[i], profile[i + 1]);
                let (near_ground, far_ground) = (near.dot(up), far.dot(up));
                if near_ground >= surface && far_ground >= surface {
                    continue;
                }
                // An end whose ground already stands above the surface collapses to nothing rather
                // than turning the strip inside out.
                let (near_top, far_top) = (near_ground.max(surface), far_ground.max(surface));
                let shade = |height: f32| shoaled(surface - height);
                faces.push_shaded(
                    [sink(near, near_top), sink(far, far_top), far],
                    [shade(near_top), shade(far_top), shade(far_ground)],
                );
                faces.push_shaded(
                    [sink(near, near_top), far, near],
                    [shade(near_top), shade(far_ground), shade(near_ground)],
                );
            }
        }
    }

    // The underside, facing down: the same fan as a cap, wound the other way.
    let base = up * bottom;
    for i in 0..6 {
        faces.push_shaded(
            [base, corners[i] + base, corners[(i + 1) % 6] + base],
            [rock; 3],
        );
    }
    faces.build()
}

/// The colour of water at a given depth below its surface: [`WATER_SHALLOW`] at the top, reaching
/// [`WATER_FILL`] by [`WATER_SHALLOW_DEPTH`].
///
/// The same ramp `water.wgsl` applies across a plate, applied here down a vertical cut instead. On
/// the plate the argument is how deep the water is at that point; here it is how far down the column
/// the vertex sits. Both say the same thing — the first [`WATER_SHALLOW_DEPTH`] of water is the pale
/// part — so the cut and the surface agree where they meet.
fn shoaled(depth: f32) -> LinearRgba {
    let t = (depth / WATER_SHALLOW_DEPTH).clamp(0.0, 1.0);
    let deep = WATER_FILL.to_linear();
    let mix = |shallow: f32, deep: f32| shallow + (deep - shallow) * t;
    LinearRgba::rgb(
        mix(WATER_SHALLOW.red, deep.red),
        mix(WATER_SHALLOW.green, deep.green),
        mix(WATER_SHALLOW.blue, deep.blue),
    )
}

/// The lowest ground in the grid, which is the level every skirt hangs from. Infinite for an empty
/// grid, which has no cells to hang anything under.
fn lowest_height(grid: &TerrainGrid) -> f32 {
    grid.iter()
        .map(|l| l.data.height)
        .fold(f32::INFINITY, f32::min)
}

/// Where a location's prism ends: a floor common to the whole grid, stepped by that location's own
/// [`wobble`] so the underside is not one flat plate.
fn skirt_bottom(lowest: f32, coord: Axial) -> f32 {
    lowest - SKIRT_BASE + wobble(coord) as f32 * SKIRT_STEP
}

/// A deterministic `-2..=2` for a coordinate — what keeps the underside from being a smooth copy of
/// the terrain above it.
///
/// A hand-rolled integer mix rather than `DefaultHasher`, whose output is explicitly not stable
/// between toolchains, and rather than a crate, since `bevy` is the only dependency. Any avalanche
/// would do; this is the usual xor-shift-multiply finalizer, which spreads neighbouring coordinates
/// apart rather than banding them.
fn wobble(coord: Axial) -> i32 {
    let mut h =
        (coord.q as u32).wrapping_mul(0x9E37_79B9) ^ (coord.r as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    (h % 5) as i32 - 2
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

/// The UV of a face that has none. Untextured tiles carry a UV channel only because the standard
/// material's pipeline expects one.
const UNUSED_UV: [f32; 2] = [0.5, 0.5];

/// Accumulates flat-shaded triangles.
///
/// Every face stores the normal its winding implies — `(v1 - v0) × (v2 - v0)` — because that cross
/// product is what back-face culling reads. Getting it backwards is invisible in the source and
/// glaring on screen, so a test checks the two against each other.
struct Faces {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<[f32; 2]>,
    /// Empty unless the mesh is one that carries colours, which only the skirt is. A colour channel
    /// on the others would specialize their pipeline for a value they never read.
    colours: Vec<[f32; 4]>,
}

impl Faces {
    fn with_capacity(triangles: usize) -> Self {
        Self {
            positions: Vec::with_capacity(triangles * 3),
            normals: Vec::with_capacity(triangles * 3),
            uvs: Vec::with_capacity(triangles * 3),
            colours: Vec::new(),
        }
    }

    fn push(&mut self, tri: [Vec3; 3], normal: Vec3) {
        self.positions.extend(tri);
        self.normals.extend([normal; 3]);
        self.uvs.extend([UNUSED_UV; 3]);
    }

    /// A triangle whose vertices carry a water depth, which rides in `uv.x`.
    ///
    /// The UV channel rather than a vertex attribute of its own: these meshes already write a UV
    /// they never use, `VertexOutput` already carries it to the fragment shader, and an untextured
    /// standard material ignores it — so a real attribute would buy nothing but a vertex layout to
    /// specialize. It does mean **a water plate's UVs are not texture coordinates**, which is the
    /// kind of thing that has to be said out loud.
    fn push_depths(&mut self, tri: [Vec3; 3], normal: Vec3, depths: [f32; 3]) {
        self.positions.extend(tri);
        self.normals.extend([normal; 3]);
        self.uvs.extend(depths.map(|d| [d, 0.0]));
    }

    /// Pushes a triangle normalled from its own winding, for faces whose facing is not known up
    /// front — a wall's incline depends on the heights either side of it.
    fn push_flat(&mut self, tri: [Vec3; 3]) {
        let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
        self.push(tri, normal);
    }

    /// A triangle carrying a colour per vertex, normalled from its own winding.
    ///
    /// The colours are **linear**, since that is what a vertex colour is: the standard material
    /// multiplies its own base colour by this one, having already converted its own out of sRGB.
    fn push_shaded(&mut self, tri: [Vec3; 3], colours: [LinearRgba; 3]) {
        self.push_flat(tri);
        self.colours
            .extend(colours.map(|c| [c.red, c.green, c.blue, c.alpha]));
    }

    fn build(self) -> Mesh {
        debug_assert!(
            self.colours.is_empty() || self.colours.len() == self.positions.len(),
            "a mesh carries a colour for every vertex or for none"
        );
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        if self.colours.is_empty() {
            mesh
        } else {
            mesh.with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colours)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::{Grid, Location, Orientation, Terrain, undulating};
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
            let lowest = lowest_height(&grid);
            let mut meshes = vec![hex_fan_mesh(&layout, 1.0 - layout.inset)];
            meshes.extend(grid.coords().map(|c| wall_mesh(&layout, &grid, c)));
            meshes.extend(grid.coords().map(|c| skirt_mesh(&layout, &grid, c, lowest)));
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
            for (_, normal) in triangles(&hex_fan_mesh(&layout, 1.0 - layout.inset)) {
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
        grid.insert(Location::new(
            left,
            Terrain {
                height: 0.0,
                water: None,
            },
        ));
        grid.insert(Location::new(
            right,
            Terrain {
                height: 0.0,
                water: None,
            },
        ));
        // A tall cell on one of the corners the two share.
        for direction in 0..6 {
            let third = left.neighbour(direction);
            if third != right && third.distance(right) == 1 {
                grid.insert(Location::new(
                    third,
                    Terrain {
                        height: 2.0,
                        water: None,
                    },
                ));
            }
        }

        let edge = (0..6)
            .find(|&e| layout.corner_directions(e).0 == 0)
            .expect("the edge facing +q");
        assert_eq!(edge_height(&layout, &grid, left, edge), 0.0);

        // And the flat-shaded wall says the same: no triangle of the bridge tilts.
        for (tri, normal) in triangles(&wall_mesh(&layout, &grid, left)) {
            if tri.iter().all(|v| v.dot(up).abs() < EPS) {
                assert!(
                    normal.abs_diff_eq(up, EPS),
                    "a level piece should face straight up"
                );
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

    /// The inset knob's whole job: a larger one shrinks every cap, and does it without moving the
    /// hexagon the location owns — the spacing is untouched, so the wall's outer rim stays on the
    /// circumradius and the wall simply gets wider.
    #[test]
    fn a_larger_inset_shrinks_the_cap_and_leaves_the_territory_alone() {
        let grid = grid();
        let narrow = HexLayout::pointy(1.0).with_inset(0.05);
        let wide = narrow.with_inset(0.25);

        let reach = |mesh: &Mesh, layout: &HexLayout| {
            positions(mesh)
                .iter()
                .map(|v| layout.plane.to_plane(*v).length())
                .fold(0.0f32, f32::max)
        };

        let cap = |layout: &HexLayout| reach(&hex_fan_mesh(layout, 1.0 - layout.inset), layout);
        assert!(
            cap(&wide) < cap(&narrow),
            "{} is not smaller than {}",
            cap(&wide),
            cap(&narrow)
        );

        let rim = |layout: &HexLayout| reach(&wall_mesh(layout, &grid, Axial::ZERO), layout);
        assert!((rim(&wide) - 1.0).abs() < EPS);
        assert!((rim(&narrow) - 1.0).abs() < EPS);
    }

    /// A water surface has to reach one ring beyond the locations that are actually flooded, and
    /// no further. A dry location beside a flooded one has half of their shared bridge under the
    /// water line even though its own cap stands clear; without a plate there, the water's edge
    /// would end in mid-air over submerged ground. Two rings are never needed, because a location
    /// with no flooded neighbour has every surrounding height above the line, so every bridge and
    /// wedge is above it too.
    #[test]
    fn water_reaches_exactly_one_ring_past_the_flooded_locations() {
        let mut grid: TerrainGrid = Grid::new();
        let lake = Axial::ZERO;
        grid.insert(Location::new(
            lake,
            Terrain {
                height: -1.0,
                water: Some(0.0),
            },
        ));
        for coord in lake.neighbours() {
            grid.insert(Location::new(
                coord,
                Terrain {
                    height: 0.5,
                    water: None,
                },
            ));
        }
        // One further out, sharing no corner with the lake.
        let far = Axial::new(2, 0);
        grid.insert(Location::new(
            far,
            Terrain {
                height: 0.5,
                water: None,
            },
        ));

        let layout = HexLayout::pointy(1.0);

        // The lake's own cap is under water, so nothing of its hexagon is buried.
        assert_eq!(
            water_plates(&layout, &grid, lake),
            vec![(0.0, [true; 12])],
            "the lake itself"
        );

        for shore in lake.neighbours() {
            let plates = water_plates(&layout, &grid, shore);
            assert_eq!(plates.len(), 1, "{shore:?} borders one body: {plates:?}");
            let (level, pieces) = plates[0];
            assert_eq!(level, 0.0);
            // Both halves of their shared edge, over the bridge that dips to the mean of the two
            // heights — and nothing else. The halves reaching the two shared corners are within the
            // lake's *reach*, but each corner is the mean of `0.5, -1.0, 0.5`, which is exactly the
            // water line: dry, by the same strict rule `flood` uses.
            let drawn = pieces.iter().filter(|drawn| **drawn).count();
            assert_eq!(drawn, 2, "{shore:?} draws {drawn} pieces: {pieces:?}");

            // And that set is exactly the submerged one: every piece drawn dips below the water
            // line, and every piece that dips is drawn. No gap, and nothing over dry ground.
            for (piece, drawn) in pieces.iter().enumerate() {
                let (sector, corner) = (piece / 2, (piece / 2 + piece % 2) % 6);
                let floor = 0.5f32
                    .min(edge_height(&layout, &grid, shore, sector))
                    .min(corner_height(&layout, &grid, shore, corner));
                assert_eq!(
                    *drawn,
                    floor < 0.0,
                    "{shore:?} piece {piece}, floor {floor}"
                );
            }
        }

        assert!(
            water_plates(&layout, &grid, far).is_empty(),
            "dry ground stays dry"
        );
    }

    /// A water plate carries the depth of the water at each of its vertices, so the shader can
    /// shade the shallows without the depth buffer — which WebGL2 cannot sample.
    ///
    /// The number that matters is the one at the shoreline. A location's corner height is the mean
    /// over the locations meeting there, so at the edge of a lake it is exactly the height the
    /// terrain's own wedge reaches; the depth there is therefore **zero**, and the pale water meets
    /// the line the depth buffer cuts with nothing to line up by hand.
    #[test]
    fn a_water_plate_carries_the_depth_at_every_vertex() {
        let layout = HexLayout::pointy(1.0);
        let lake = Axial::ZERO;
        let mut grid: TerrainGrid = Grid::new();
        grid.insert(Location::new(
            lake,
            Terrain {
                height: -1.0,
                water: Some(0.0),
            },
        ));
        for coord in lake.neighbours() {
            grid.insert(Location::new(
                coord,
                Terrain {
                    height: 0.5,
                    water: None,
                },
            ));
        }

        let depths = |coord| {
            let mesh = water_fan_mesh(&layout, &grid, coord, 0.0, [true; 12]);
            let positions = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .expect("positions")
                .as_float3()
                .expect("float3")
                .to_vec();
            let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0).expect("uvs") {
                bevy::mesh::VertexAttributeValues::Float32x2(uv) => uv.clone(),
                other => panic!("uvs should be Float32x2, got {other:?}"),
            };
            assert_eq!(positions.len(), uvs.len());
            positions
                .into_iter()
                .map(Vec3::from_array)
                .zip(uvs.into_iter().map(|uv| uv[0]))
                .collect::<Vec<_>>()
        };

        // Three kinds of vertex, told apart by how far out they sit: the centre, the six lattice
        // corners on the circumradius, and the six edge midpoints on the inradius between them.
        for (position, depth) in depths(lake) {
            let radius = position.length();
            if radius < EPS {
                assert!(
                    (depth - 1.0).abs() < EPS,
                    "over the lake bed, 1.0 below the surface"
                );
            } else if radius > 0.9 {
                // Corner mean of -1.0, 0.5 and 0.5 is exactly the water line.
                assert!(
                    depth.abs() < EPS,
                    "the shore is where the water runs out: {depth}"
                );
            } else {
                // A midpoint sits over the bridge, at the mean of the lake bed and the one shore
                // across that edge — which is deeper than either corner beside it.
                assert!(
                    (depth - 0.25).abs() < EPS,
                    "over the bridge, a quarter deep: {depth}"
                );
            }
        }

        // A dry shoreline location carries the lake's plate too, buried under its own ground. Its
        // depths are negative, which is not a special case: the shader clamps, and the plate is
        // invisible there anyway.
        let shore = lake.neighbours()[0];
        let centre = depths(shore)
            .into_iter()
            .find(|(p, _)| *p == Vec3::ZERO)
            .expect("a centre vertex")
            .1;
        assert!(
            (centre - -0.5).abs() < EPS,
            "buried half a unit deep, got {centre}"
        );
    }

    /// The artefact `two-lakes` was built to show, and the rule that fixes it.
    ///
    /// The bridge's wall dips to the mean of the two heights either side, which is under the *higher*
    /// level on both flanks — so a plate covering the whole hexagon drew the higher water over the
    /// lower body's shore, ending in a wall of water at the hexagon boundary since the lower body's
    /// own locations never carry the higher level. Reach keeps the higher body to the halves it
    /// touches, and the two bodies end up on disjoint pieces of it.
    ///
    /// Both bodies do reach the bridge, and both are right to. The wall along an edge is the mean of
    /// two heights, but a corner is the mean of three, so where the bridge meets *two* cells of the
    /// lower body that corner falls to `mean(0.95, -0.6, -0.6) = -0.08` — under the lower level, and
    /// so genuinely its shore. What matters is that the higher body is nowhere near it.
    #[test]
    fn a_bridge_between_two_levels_keeps_each_body_to_its_own_side() {
        let layout = HexLayout::pointy(1.0);
        let grid = crate::hex::scenes::build("two-lakes").expect("a registered scene");

        // The centre of the bridge, and the two bodies either side of it.
        let bridge = Axial::ZERO;
        let level = |coord| grid.get(coord).expect("in the grid").data.water;
        let high = level(Axial::new(-1, 0)).expect("the high body");
        let low = level(Axial::new(1, 0)).expect("the low body");
        assert!(high > low, "two levels, or there is nothing to show");

        // The wall towards the lower body is under the higher level — which is what used to carry
        // the higher water across — and above the lower one.
        let towards = |coord: Axial| {
            let direction = (0..6)
                .find(|&d| bridge.neighbour(d) == coord)
                .expect("adjacent");
            (0..6)
                .find(|&e| layout.corner_directions(e).0 == direction)
                .expect("the edge that way")
        };
        let wall = edge_height(&layout, &grid, bridge, towards(Axial::new(1, 0)));
        assert!(wall < high && wall > low, "the wall is at {wall}");

        // Both bodies draw on the bridge, on pieces that do not overlap.
        let plates = water_plates(&layout, &grid, bridge);
        let plate = |wanted: f32| {
            plates
                .iter()
                .find(|(held, _)| *held == wanted)
                .unwrap_or_else(|| panic!("a plate at {wanted}: {plates:?}"))
                .1
        };
        let (above, below) = (plate(high), plate(low));
        for (piece, (up, down)) in above.iter().zip(below).enumerate() {
            assert!(!(*up && down), "piece {piece} is claimed by both bodies");
        }

        // The higher body is confined to the halves it touches, so no piece over a sector facing the
        // lower body carries it — the wall of water is gone. Its own shore is still drawn.
        let mut own = 0;
        for (piece, drawn) in above.iter().enumerate() {
            match level(bridge.neighbour(layout.corner_directions(piece / 2).0)) {
                Some(l) if l == low => assert!(!drawn, "piece {piece} faces the lower body"),
                Some(l) if l == high => own += *drawn as usize,
                _ => {}
            }
        }
        assert!(own > 0, "the higher body still draws its own shore");
    }

    /// No location may draw two water levels over the same piece of its hexagon.
    ///
    /// This is the artefact in its general form: where two plates overlap, the higher one wins the
    /// depth test and is drawn over the lower body's shore, ending in a wall of water at whatever
    /// boundary the lower body's own locations stop at. A half-sector touches only two neighbours,
    /// and two bodies at different levels cannot be neighbours — one would drain into the other — so
    /// on sensible data this never has to happen.
    #[test]
    fn no_location_draws_two_water_levels_over_the_same_piece() {
        let layout = HexLayout::pointy(1.0);
        for scene in crate::hex::scenes::names() {
            let grid = crate::hex::scenes::build(scene).expect("a registered scene");
            for coord in grid.coords() {
                let plates = water_plates(&layout, &grid, coord);
                for piece in 0..12 {
                    let claims: Vec<f32> = plates
                        .iter()
                        .filter(|(_, pieces)| pieces[piece])
                        .map(|(level, _)| *level)
                        .collect();
                    assert!(
                        claims.len() <= 1,
                        "{scene}: {coord:?} piece {piece} claimed by {claims:?}"
                    );
                }
            }
        }
    }

    /// Colours as `[f32; 4]` per vertex, or empty for a mesh that carries none.
    fn colours(mesh: &Mesh) -> Vec<[f32; 4]> {
        match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            None => Vec::new(),
            Some(bevy::mesh::VertexAttributeValues::Float32x4(c)) => c.clone(),
            Some(other) => panic!("colours should be Float32x4, got {other:?}"),
        }
    }

    fn positions(mesh: &Mesh) -> Vec<Vec3> {
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("positions")
            .as_float3()
            .expect("float3")
            .iter()
            .map(|v| Vec3::from_array(*v))
            .collect()
    }

    /// The skirt hangs from the wall's own outer rim, which is the seam that must not crack. Both
    /// take it from [`edge_profile`]; this is what fails if either grows its own copy.
    ///
    /// Bitwise, like the corner and bridge means it is built from — a vertex an ulp adrift is a
    /// crack in a solid that is supposed to be closed.
    #[test]
    fn the_skirt_hangs_from_the_walls_own_rim() {
        let grid = grid();
        for layout in layouts() {
            let lowest = lowest_height(&grid);
            for coord in grid.coords() {
                let wall = positions(&wall_mesh(&layout, &grid, coord));
                let skirt = positions(&skirt_mesh(&layout, &grid, coord, lowest));
                for j in 0..6 {
                    let profile = edge_profile(&layout, &grid, coord, j);
                    // The midpoint is the skirt's alone: the wall spans the bridge in one quad.
                    for (i, point) in profile.iter().enumerate() {
                        assert!(
                            skirt.contains(point),
                            "{coord:?} edge {j} point {i} is not on the skirt"
                        );
                        assert!(
                            i == 2 || wall.contains(point),
                            "{coord:?} edge {j} point {i} is not on the wall"
                        );
                    }
                }
            }
        }
    }

    /// Two locations either side of an edge must hang their skirts from the same line, or the solid
    /// splits open along it exactly as the surface would. Heights bitwise, positions to a tolerance
    /// — they are reached by adding different centres, as `cells_agree_on_every_shared_edge_and_corner`
    /// also allows.
    #[test]
    fn neighbours_hang_their_skirts_from_the_same_line() {
        let grid = grid();
        for layout in layouts() {
            let unit = layout.unit();
            let up = unit.plane.normal();
            for coord in grid.coords() {
                for j in 0..6 {
                    let direction = layout.corner_directions(j).0;
                    let neighbour = coord.neighbour(direction);
                    if !grid.contains(neighbour) {
                        continue;
                    }
                    let back = (direction + 3) % 6;
                    let facing = (0..6)
                        .find(|&e| layout.corner_directions(e).0 == back)
                        .expect("the edge back this way");

                    let ours = edge_profile(&layout, &grid, coord, j);
                    let theirs = edge_profile(&layout, &grid, neighbour, facing);
                    let (here, there) = (unit.hex_to_world(coord), unit.hex_to_world(neighbour));
                    // Their edge runs the other way round the hexagon, so it reverses.
                    for (mine, other) in ours.iter().zip(theirs.iter().rev()) {
                        assert_eq!(
                            mine.dot(up),
                            other.dot(up),
                            "{coord:?} edge {j} disagrees with {neighbour:?} on a height"
                        );
                        assert!(
                            (here + *mine).abs_diff_eq(there + *other, EPS),
                            "{coord:?} edge {j} disagrees with {neighbour:?} on a position"
                        );
                    }
                }
            }
        }
    }

    /// No prism may turn inside out: a bottom has to clear every piece of ground it hangs from, in
    /// every scene. This is the invariant `SKIRT_STEP * 2 < SKIRT_BASE` buys, and the reason the
    /// floor is common to the grid rather than measured down from each cap — a location's own
    /// boundary dips towards its lower neighbours, well below its own height.
    #[test]
    fn every_skirt_hangs_below_the_terrain() {
        let layout = HexLayout::pointy(1.0);
        let up = layout.plane.normal();
        for scene in crate::hex::scenes::names() {
            let grid = crate::hex::scenes::build(scene).expect("a registered scene");
            let lowest = lowest_height(&grid);
            for coord in grid.coords() {
                let bottom = skirt_bottom(lowest, coord);
                for j in 0..6 {
                    for point in edge_profile(&layout, &grid, coord, j) {
                        assert!(
                            bottom < point.dot(up),
                            "{scene}: {coord:?} hangs from {} but ends at {bottom}",
                            point.dot(up)
                        );
                    }
                }
            }
        }
    }

    /// Like the wall, a skirt covers its own hexagon and no more.
    #[test]
    fn a_skirt_stays_inside_its_own_hexagon() {
        let layout = HexLayout::pointy(1.0);
        let grid = grid();
        let lowest = lowest_height(&grid);
        let up = layout.plane.normal();
        for coord in grid.coords() {
            let mut reach: f32 = 0.0;
            for v in positions(&skirt_mesh(&layout, &grid, coord, lowest)) {
                reach = reach.max((v - v.dot(up) * up).length());
            }
            assert!((reach - 1.0).abs() < EPS, "{coord:?} reaches {reach}");
        }
    }

    /// The water's cut face is the one part of a skirt confined to the rim, because it is the one
    /// part that is ever seen. Inland the ground is all there is, however deep the sea over it.
    #[test]
    fn only_a_rim_skirt_cuts_through_the_water() {
        let layout = HexLayout::pointy(1.0);
        let mut grid: TerrainGrid = Grid::hexagon(2, |_| Terrain {
            height: -1.0,
            water: Some(0.0),
        });
        let lowest = lowest_height(&grid);
        let rock = WALL_FILL.to_linear();
        let is_rock = |c: [f32; 4]| c[0] == rock.red && c[1] == rock.green && c[2] == rock.blue;

        let inland = skirt_mesh(&layout, &grid, Axial::ZERO, lowest);
        assert!(
            colours(&inland).into_iter().all(is_rock),
            "an interior location has no water to cut"
        );

        // A corner of the hexagon: three of its six edges have no neighbour.
        let rim = Axial::new(2, 0);
        let cut = skirt_mesh(&layout, &grid, rim, lowest);
        let up = layout.plane.normal();
        let water: Vec<(Vec3, [f32; 4])> = positions(&cut)
            .into_iter()
            .zip(colours(&cut))
            .filter(|(_, c)| !is_rock(*c))
            .collect();
        assert!(!water.is_empty(), "a flooded rim location shows its water");

        let surface = -WATER_TIE_BREAK;
        for (position, colour) in &water {
            let height = position.dot(up);
            assert!(
                height <= surface + EPS,
                "water above its own surface: {height}"
            );
            let expected = shoaled(surface - height);
            assert!(
                (colour[0] - expected.red).abs() < EPS,
                "at {height} the cut should be {expected:?}, got {colour:?}"
            );
        }
        // Pale at the surface and full deep at the bed, which is a unit down — well past
        // `WATER_SHALLOW_DEPTH`.
        let deep = WATER_FILL.to_linear();
        assert!(water.iter().any(|(_, c)| c[0] == WATER_SHALLOW.red));
        assert!(water.iter().any(|(_, c)| (c[0] - deep.red).abs() < EPS));

        // And drain it: with no water anywhere the same location is all rock again.
        crate::hex::flood(&mut grid, -2.0);
        assert!(
            colours(&skirt_mesh(&layout, &grid, rim, lowest))
                .into_iter()
                .all(is_rock),
            "a drained location has nothing to cut"
        );
    }

    /// The wobble is what keeps the underside from being a smooth copy of the terrain. It has to be
    /// stable — the same grid must come out the same on every run and every toolchain — and it has
    /// to actually use its range.
    #[test]
    fn the_wobble_is_stable_and_spends_its_whole_range() {
        assert_eq!(wobble(Axial::ZERO), wobble(Axial::ZERO));
        assert_eq!(wobble(Axial::new(0, 0)), -2);
        assert_eq!(wobble(Axial::new(1, 0)), 1);
        assert_eq!(wobble(Axial::new(-3, 2)), 2);

        let seen: std::collections::BTreeSet<i32> = grid().coords().map(wobble).collect();
        assert_eq!(
            seen,
            (-2..=2).collect(),
            "a 37-cell grid should show every step"
        );
    }

    #[test]
    fn a_lone_cell_is_a_flat_plate() {
        // With no neighbours every mean is over one location, so the fence is level with the cap
        // and the wall is a flat brim — the same code path that gives the grid's edge its lip.
        let layout = HexLayout::pointy(1.0);
        let mut grid: TerrainGrid = Grid::new();
        grid.insert(Location::new(
            Axial::ZERO,
            Terrain {
                height: -0.4,
                water: None,
            },
        ));

        let mesh = wall_mesh(&layout, &grid, Axial::ZERO);
        let brim = triangles(&mesh);
        assert_eq!(brim.len(), 24, "six bridges and six wedges");
        for (tri, _) in brim {
            for v in tri {
                assert!(
                    (v.y - -0.4).abs() < EPS,
                    "{v:?} should be level with the cap"
                );
            }
        }
        assert_eq!(
            triangles(&hex_fan_mesh(&layout, 1.0 - layout.inset)).len(),
            6
        );
    }
}
