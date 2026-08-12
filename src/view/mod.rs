//! Everything that turns the grid model into something on screen.
//!
//! The model ([`crate::hex`]) is dimensionless. This layer owns the projection into world space
//! ([`layout::HexLayout`]) and everything Bevy-facing: meshes, outlines, labels, picking and the
//! debug UI.

pub mod compass;
pub mod debug_ui;
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
            .add_observer(debug_ui::on_button_activate)
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
                    grid_render::sync_cell_transforms,
                    labels::sync_label_anchors,
                    labels::update_label_text,
                    debug_ui::update_button_caption,
                    debug_ui::update_readout,
                    // Runs after the anchors are up to date so labels never lag a frame behind.
                    world_label::project_world_labels
                        .after(labels::sync_label_anchors),
                    grid_render::draw_outlines,
                    compass::draw_compass,
                ),
            );
    }
}
