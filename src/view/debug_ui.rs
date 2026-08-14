//! Top-right readout for the selected hex, and the controls that drive the view.
//!
//! Six controls: a button cycling the label mode (including off), a button toggling the hexagon
//! orientation, checkboxes for the compass and for hiding the terrain's skirt, sliders for the sea
//! level and the cap inset, and a button that frames the whole scene from overhead. The
//! state-carrying buttons cycle rather than offering radio lists, because this is a debug panel and
//! a cycle is one entity and one observer arm.

use bevy::prelude::*;
use bevy::ui::Checked;
use bevy::ui_widgets::{
    checkbox_self_update, slider_self_update, Activate, Button, Checkbox, Slider, SliderRange,
    SliderThumb, SliderValue, ValueChange,
};

use super::compass::ShowCompass;
use super::framing::ResetViewRequested;
use super::grid_render::{HideSkirt, SeaLevel};
use super::labels::LabelMode;
use super::layout::HexLayout;
use super::selection::Selected;

const PANEL_BG: Color = Color::srgba(0.04, 0.05, 0.08, 0.82);
const CONTROL_BG: Color = Color::srgb(0.18, 0.26, 0.38);
const TRACK_BG: Color = Color::srgb(0.10, 0.14, 0.20);
const THUMB: Color = Color::srgb(0.45, 0.70, 0.90);
const TEXT: Color = Color::srgb(0.88, 0.91, 0.96);
const DIM: Color = Color::srgb(0.60, 0.65, 0.74);

/// The slider's travel, in the same units as a height. The terrain spans about `-1..=1`, so the
/// ends of the track drain the grid completely and drown it completely.
const SEA_LEVEL_RANGE: std::ops::RangeInclusive<f32> = -1.0..=1.0;

/// The inset slider's travel, **in percent** — the units the caption reads in, so the widget's own
/// value needs no scaling. [`HexLayout::inset`] is the fraction, so the two are converted at this
/// edge and nowhere else. Half the circumradius is where a cap has shrunk to a quarter of its area
/// and the walls are all there is left to see; past that there is nothing to look at.
const INSET_RANGE: std::ops::RangeInclusive<f32> = 0.0..=50.0;

/// The thumb's width as a percentage of the track, kept out of the travel so it cannot overhang
/// either end.
const THUMB_WIDTH: f32 = 9.0;

const EMPTY_READOUT: &str = "click a hexagon";
/// Fixed, unlike the other captions — this button reports no state.
const RESET_CAPTION: &str = "reset view";
const COMPASS_CAPTION: &str = "compass";
const SKIRT_CAPTION: &str = "hide skirt";

/// The readout text node.
#[derive(Component)]
pub struct CoordReadout;

/// Marks a control, and says what it does. One component keeps the observers to a single arm each.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    LabelMode,
    Orientation,
    Compass,
    HideSkirt,
    SeaLevel,
    Inset,
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
    hide_skirt: Res<HideSkirt>,
    sea: Res<SeaLevel>,
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
            spawn_checkbox(panel, Control::Compass, show_compass.0, COMPASS_CAPTION);
            spawn_checkbox(panel, Control::HideSkirt, hide_skirt.0, SKIRT_CAPTION);
            spawn_slider(
                panel,
                Control::SeaLevel,
                sea.0,
                SEA_LEVEL_RANGE,
                sea_level_caption(sea.0),
            );
            let inset_percent = layout.inset * 100.0;
            spawn_slider(
                panel,
                Control::Inset,
                inset_percent,
                INSET_RANGE,
                inset_caption(inset_percent),
            );
            spawn_button(panel, Control::ResetView, RESET_CAPTION.to_string());
        });
}

/// A captioned slider: the caption above, the track below, and a thumb inside the track that
/// [`position_slider_thumbs`] moves. The widget is headless — it reads the drag and reports a
/// value; every pixel of it is ours.
fn spawn_slider(
    panel: &mut ChildSpawnerCommands,
    control: Control,
    value: f32,
    range: std::ops::RangeInclusive<f32>,
    caption: String,
) {
    panel
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|group| {
            group.spawn((
                ControlCaption(control),
                Text::new(caption),
                TextFont::from_font_size(12.0),
                TextColor(TEXT),
            ));
            group
                .spawn((
                    control,
                    Slider::default(),
                    SliderValue(value),
                    SliderRange::from_range(range),
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(14.0),
                        ..default()
                    },
                    BackgroundColor(TRACK_BG),
                ))
                .observe(slider_self_update)
                .with_children(|track| {
                    track.spawn((
                        SliderThumb,
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(THUMB_WIDTH),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(THUMB),
                    ));
                });
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

fn sea_level_caption(level: f32) -> String {
    format!("sea level: {level:+.2}")
}

/// Takes a percentage, not the fraction [`HexLayout::inset`] holds — the slider's own value.
fn inset_caption(percent: f32) -> String {
    format!("inset: {percent:.0}%")
}

/// Puts each thumb where its value says. The widget deliberately leaves this to the caller, since
/// it cannot know how the thumb is drawn; the travel is reduced by the thumb's own width so it
/// stops flush with either end of the track rather than hanging over it.
pub fn position_slider_thumbs(
    sliders: Query<(&SliderValue, &SliderRange, &Children), Changed<SliderValue>>,
    mut thumbs: Query<&mut Node, With<SliderThumb>>,
) {
    for (value, range, children) in &sliders {
        let fraction = range.thumb_position(value.0);
        for child in children {
            if let Ok(mut node) = thumbs.get_mut(*child) {
                node.left = Val::Percent(fraction * (100.0 - THUMB_WIDTH));
            }
        }
    }
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
    mut hide_skirt: ResMut<HideSkirt>,
) {
    match controls.get(change.source) {
        Ok(Control::Compass) => show_compass.0 = change.value,
        Ok(Control::HideSkirt) => hide_skirt.0 = change.value,
        _ => {}
    }
}

/// Sliders: adopt the reported value. Separate from the checkbox observer because the payload type
/// is what selects the event.
///
/// Each arm compares before it writes. A drag reports a value every frame it is held, most of them
/// the same one, and a write here marks the resource changed — which for the inset rebuilds every
/// cap, wall and skirt in the grid.
pub fn on_slider_changed(
    change: On<ValueChange<f32>>,
    controls: Query<&Control>,
    mut sea: ResMut<SeaLevel>,
    mut layout: ResMut<HexLayout>,
) {
    match controls.get(change.source) {
        Ok(Control::SeaLevel) if sea.0 != change.value => sea.0 = change.value,
        // The slider reads in percent; the layout holds the fraction the meshes are built from.
        Ok(Control::Inset) => {
            let inset = change.value / 100.0;
            if layout.inset != inset {
                layout.inset = inset;
            }
        }
        _ => {}
    }
}

pub fn update_captions(
    mode: Res<LabelMode>,
    layout: Res<HexLayout>,
    show_compass: Res<ShowCompass>,
    hide_skirt: Res<HideSkirt>,
    sea: Res<SeaLevel>,
    mut captions: Query<(&ControlCaption, &mut Text)>,
) {
    if !mode.is_changed()
        && !layout.is_changed()
        && !show_compass.is_changed()
        && !hide_skirt.is_changed()
        && !sea.is_changed()
    {
        return;
    }
    for (caption, mut text) in &mut captions {
        **text = match caption.0 {
            Control::LabelMode => label_caption(*mode),
            Control::Orientation => orientation_caption(layout.orientation),
            Control::Compass => checkbox_caption(show_compass.0, COMPASS_CAPTION),
            Control::HideSkirt => checkbox_caption(hide_skirt.0, SKIRT_CAPTION),
            Control::SeaLevel => sea_level_caption(sea.0),
            Control::Inset => inset_caption(layout.inset * 100.0),
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
