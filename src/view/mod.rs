//! Everything that turns the grid model into something on screen.
//!
//! The model ([`crate::hex`]) is dimensionless. This layer owns the projection into world space
//! ([`layout::HexLayout`]) and everything Bevy-facing: meshes, outlines, labels, picking and the
//! debug UI.

pub mod compass;
pub mod debug_ui;
pub mod framing;
pub mod grid_render;
pub mod labels;
pub mod layout;
pub mod selection;
pub mod world_label;

use bevy::prelude::*;

pub use layout::HexLayout;

use crate::hex::TerrainGrid;

/// Hexagon rings around the centre. Radius 3 is a hexagon of side 4: 37 locations.
pub const GRID_RADIUS: i32 = 3;

/// The grid model as a Bevy resource.
///
/// The bridge between the two layers, and the reason it exists: [`crate::hex::Grid`] deliberately
/// does not derive `Resource`, because that would put a Bevy dependency in the model. Systems get
/// at the model through this newtype's `Deref`.
#[derive(Resource, Deref, DerefMut)]
pub struct GridModel(pub TerrainGrid);

/// Draws and drives the hex grid view.
pub struct HexViewPlugin;

impl Plugin for HexViewPlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<grid_render::GridLines>()
            .init_gizmo_group::<grid_render::Highlight>()
            .init_gizmo_group::<compass::CompassLines>()
            .init_resource::<selection::Selected>()
            .init_resource::<labels::LabelMode>()
            .init_resource::<compass::ShowCompass>()
            .init_resource::<grid_render::SeaLevel>()
            .init_resource::<framing::ResetViewRequested>()
            .add_observer(debug_ui::on_button_activate)
            .add_observer(debug_ui::on_checkbox_changed)
            .add_observer(debug_ui::on_slider_changed)
            .add_systems(
                Startup,
                (
                    grid_render::configure_gizmo_widths,
                    compass::configure_gizmo_width,
                    grid_render::spawn_grid,
                    compass::spawn_compass,
                    labels::spawn_labels,
                    debug_ui::spawn_debug_ui,
                ),
            )
            .add_systems(
                Update,
                (
                    selection::select_on_click,
                    framing::reset_view,
                    // The slider writes a level; the model decides from it which locations are
                    // wet, and only then can the surfaces be rebuilt.
                    grid_render::apply_sea_level,
                    grid_render::sync_water.after(grid_render::apply_sea_level),
                    debug_ui::position_slider_thumbs,
                    // Everything that reacts to the layout changing — scale, or the orientation
                    // toggle, which moves every hex and rotates the axes.
                    grid_render::sync_cells,
                    labels::sync_label_anchors,
                    labels::sync_label_visibility,
                    labels::update_label_text,
                    compass::sync_compass_labels,
                    debug_ui::update_captions,
                    debug_ui::update_readout,
                    // After the anchors are up to date, so labels never lag a frame behind.
                    world_label::project_world_labels
                        .after(labels::sync_label_anchors)
                        .after(compass::sync_compass_labels),
                    grid_render::draw_outlines,
                    compass::draw_compass,
                ),
            );
    }
}
