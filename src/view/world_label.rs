//! Text pinned to a point in the world.
//!
//! Bevy's UI is screen-space, so a world-anchored label is a UI node repositioned each frame from
//! the camera's projection. Shared by the hex coordinate labels and the compass, so there is one
//! place that knows how to hide a label that falls off-screen or behind the camera.

use bevy::prelude::*;

/// Marks a UI text node that should follow a world position.
#[derive(Component)]
pub struct WorldLabel {
    pub anchor: Vec3,
    /// Whether the owner wants this label shown at all. Distinct from being off-screen, which
    /// [`project_world_labels`] decides for itself — this is the on/off switch the UI drives.
    pub visible: bool,
}

impl WorldLabel {
    pub fn new(anchor: Vec3) -> Self {
        Self {
            anchor,
            visible: true,
        }
    }
}

/// The components every world label needs.
pub fn world_label(anchor: Vec3, text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        WorldLabel::new(anchor),
        Text::new(text),
        TextFont::from_font_size(size),
        TextColor(color),
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
    )
}

pub fn project_world_labels(
    camera: Single<(&Camera, &GlobalTransform)>,
    mut labels: Query<(&WorldLabel, &mut Node)>,
) {
    let (camera, camera_transform) = *camera;

    for (label, mut node) in &mut labels {
        if !label.visible {
            node.display = Display::None;
            continue;
        }
        match camera.world_to_viewport(camera_transform, label.anchor) {
            Ok(screen) => {
                node.display = Display::Block;
                // Nudge left and up so the text is roughly centred on the anchor. Exact centring
                // would need the measured text size, which is not worth a frame of lag here.
                node.left = Val::Px(screen.x - 18.0);
                node.top = Val::Px(screen.y - 8.0);
            }
            // Behind the camera or outside the viewport.
            Err(_) => node.display = Display::None,
        }
    }
}
