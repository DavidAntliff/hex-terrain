//! Top-right readout for the selected hex, and the controls that drive the view.
//!
//! Four controls: a button cycling the label mode (including off), a button toggling the hexagon
//! orientation, a checkbox for the compass, and a button that frames the whole scene from
//! overhead. The state-carrying buttons cycle rather than offering radio lists, because this is a
//! debug panel and a cycle is one entity and one observer arm.

use bevy::prelude::*;
use bevy::ui::Checked;
use bevy::ui_widgets::{checkbox_self_update, Activate, Button, Checkbox, ValueChange};

use super::compass::ShowCompass;
use super::framing::ResetViewRequested;
use super::labels::LabelMode;
use super::layout::HexLayout;
use super::selection::Selected;

const PANEL_BG: Color = Color::srgba(0.04, 0.05, 0.08, 0.82);
const CONTROL_BG: Color = Color::srgb(0.18, 0.26, 0.38);
const TEXT: Color = Color::srgb(0.88, 0.91, 0.96);
const DIM: Color = Color::srgb(0.60, 0.65, 0.74);

const EMPTY_READOUT: &str = "click a hexagon";
/// Fixed, unlike the other captions — this button reports no state.
const RESET_CAPTION: &str = "reset view";

/// The readout text node.
#[derive(Component)]
pub struct CoordReadout;

/// Marks a control, and says what it does. One component keeps the observers to a single arm each.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    LabelMode,
    Orientation,
    Compass,
    ResetView,
}

/// The caption belonging to a control, so it can be kept in step with the state.
#[derive(Component)]
pub struct ControlCaption(pub Control);

pub fn spawn_debug_ui(
    mut commands: Commands,
    mode: Res<LabelMode>,
    layout: Res<HexLayout>,
    show_compass: Res<ShowCompass>,
) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                right: Val::Px(12.0),
                min_width: Val::Px(230.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(PANEL_BG),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new("selection"),
                TextFont::from_font_size(11.0),
                TextColor(DIM),
            ));
            panel.spawn((
                CoordReadout,
                Text::new(EMPTY_READOUT),
                TextFont::from_font_size(13.0),
                TextColor(TEXT),
            ));

            spawn_button(panel, Control::LabelMode, label_caption(*mode));
            spawn_button(
                panel,
                Control::Orientation,
                orientation_caption(layout.orientation),
            );
            spawn_checkbox(panel, Control::Compass, show_compass.0, "compass");
            spawn_button(panel, Control::ResetView, RESET_CAPTION.to_string());
        });
}

fn spawn_button(panel: &mut ChildSpawnerCommands, control: Control, caption: String) {
    panel
        .spawn((
            control,
            Button,
            Node {
                padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(CONTROL_BG),
        ))
        .with_children(|button| {
            button.spawn((
                ControlCaption(control),
                Text::new(caption),
                TextFont::from_font_size(12.0),
                TextColor(TEXT),
            ));
        });
}

fn spawn_checkbox(
    panel: &mut ChildSpawnerCommands,
    control: Control,
    checked: bool,
    name: &str,
) {
    let mut entity = panel.spawn((
        control,
        Checkbox,
        Node {
            padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
            ..default()
        },
        BackgroundColor(CONTROL_BG),
    ));
    if checked {
        entity.insert(Checked);
    }
    // The widget is headless: it maintains `Checked` via this observer and reports changes, but
    // drawing the box is ours. A text caption is enough here.
    entity
        .observe(checkbox_self_update)
        .with_children(|checkbox| {
            checkbox.spawn((
                ControlCaption(control),
                Text::new(checkbox_caption(checked, name)),
                TextFont::from_font_size(12.0),
                TextColor(TEXT),
            ));
        });
}

fn label_caption(mode: LabelMode) -> String {
    format!("labels: {}", mode.name())
}

fn orientation_caption(orientation: crate::hex::Orientation) -> String {
    format!("hexes: {}", orientation.name())
}

fn checkbox_caption(checked: bool, name: &str) -> String {
    format!("[{}] {name}", if checked { "x" } else { " " })
}

/// Buttons: cycle the state they own.
pub fn on_button_activate(
    activate: On<Activate>,
    controls: Query<&Control>,
    mut mode: ResMut<LabelMode>,
    mut layout: ResMut<HexLayout>,
    mut reset_view: ResMut<ResetViewRequested>,
) {
    match controls.get(activate.entity) {
        Ok(Control::LabelMode) => *mode = mode.next(),
        // Positions, corner angles and the doubled variant all follow from this one field.
        Ok(Control::Orientation) => layout.orientation = layout.orientation.other(),
        // Deferred to `framing::reset_view`, which needs the camera and its projection.
        Ok(Control::ResetView) => reset_view.0 = true,
        _ => {}
    }
}

/// Checkboxes: adopt the reported value.
pub fn on_checkbox_changed(
    change: On<ValueChange<bool>>,
    controls: Query<&Control>,
    mut show_compass: ResMut<ShowCompass>,
) {
    if let Ok(Control::Compass) = controls.get(change.source) {
        show_compass.0 = change.value;
    }
}

pub fn update_captions(
    mode: Res<LabelMode>,
    layout: Res<HexLayout>,
    show_compass: Res<ShowCompass>,
    mut captions: Query<(&ControlCaption, &mut Text)>,
) {
    if !mode.is_changed() && !layout.is_changed() && !show_compass.is_changed() {
        return;
    }
    for (caption, mut text) in &mut captions {
        **text = match caption.0 {
            Control::LabelMode => label_caption(*mode),
            Control::Orientation => orientation_caption(layout.orientation),
            Control::Compass => checkbox_caption(show_compass.0, "compass"),
            Control::ResetView => RESET_CAPTION.to_string(),
        };
    }
}

pub fn update_readout(
    selected: Res<Selected>,
    layout: Res<HexLayout>,
    mut readout: Single<&mut Text, With<CoordReadout>>,
) {
    if !selected.is_changed() && !layout.is_changed() {
        return;
    }

    ***readout = match selected.0 {
        None => EMPTY_READOUT.to_string(),
        Some(coord) => {
            let cube = coord.to_cube();
            let doubled = coord.to_doubled(layout.orientation);
            let world = layout.hex_to_world(coord);
            format!(
                "axial    q {}  r {}\n\
                 cube     q {}  r {}  s {}\n\
                 {}\n         col {}  row {}\n\
                 world    x {:.2}  y {:.2}  z {:.2}",
                coord.q,
                coord.r,
                cube.q,
                cube.r,
                cube.s,
                // Naming the variant makes it obvious that this row depends on the orientation.
                layout.orientation.doubled_variant(),
                doubled.col,
                doubled.row,
                world.x,
                world.y,
                world.z,
            )
        }
    };
}
