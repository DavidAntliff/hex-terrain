//! Top-right readout for the selected hex, and the button that cycles label modes.

use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button};

use super::labels::LabelMode;
use super::layout::HexLayout;
use super::selection::Selected;

const PANEL_BG: Color = Color::srgba(0.04, 0.05, 0.08, 0.82);
const BUTTON_BG: Color = Color::srgb(0.18, 0.26, 0.38);
const TEXT: Color = Color::srgb(0.88, 0.91, 0.96);
const DIM: Color = Color::srgb(0.60, 0.65, 0.74);

/// The readout text node.
#[derive(Component)]
pub struct CoordReadout;

/// The button's own label, which names the *current* mode.
#[derive(Component)]
pub struct ModeButtonLabel;

/// Marks the button so the observer can tell it from any other activatable widget.
#[derive(Component)]
pub struct LabelModeButton;

pub fn spawn_debug_ui(mut commands: Commands, mode: Res<LabelMode>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                right: Val::Px(12.0),
                min_width: Val::Px(210.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
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
            panel
                .spawn((
                    LabelModeButton,
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(BUTTON_BG),
                ))
                .with_children(|button| {
                    button.spawn((
                        ModeButtonLabel,
                        Text::new(button_caption(*mode)),
                        TextFont::from_font_size(12.0),
                        TextColor(TEXT),
                    ));
                });
        });
}

const EMPTY_READOUT: &str = "click a hexagon";

fn button_caption(mode: LabelMode) -> String {
    format!("labels: {}", mode.name())
}

/// Cycles the label mode when the button is activated.
///
/// `Activate` fires for keyboard activation too, so the button works when focused.
pub fn on_button_activate(
    activate: On<Activate>,
    buttons: Query<(), With<LabelModeButton>>,
    mut mode: ResMut<LabelMode>,
) {
    if buttons.contains(activate.entity) {
        *mode = mode.next();
    }
}

pub fn update_button_caption(
    mode: Res<LabelMode>,
    mut caption: Single<&mut Text, With<ModeButtonLabel>>,
) {
    if mode.is_changed() {
        ***caption = button_caption(*mode);
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
            let doubled = coord.to_doubled();
            let world = layout.hex_to_world(coord);
            format!(
                "axial    q {}  r {}\n\
                 cube     q {}  r {}  s {}\n\
                 doubled  col {}  row {}\n\
                 world    x {:.2}  y {:.2}  z {:.2}",
                coord.q,
                coord.r,
                cube.q,
                cube.r,
                cube.s,
                doubled.col,
                doubled.row,
                world.x,
                world.y,
                world.z,
            )
        }
    };
}
