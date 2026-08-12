//! Orbit camera: right-drag to rotate about the origin, scroll to zoom.

use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

const LOOK_SENSITIVITY: f32 = 0.005;
const ZOOM_SENSITIVITY: f32 = 0.1;
const MIN_RADIUS: f32 = 3.0;
const MAX_RADIUS: f32 = 200.0;

/// Camera position in spherical coordinates about the origin.
// ponytail: target is always the origin. Add a `target: Vec3` when panning is needed.
#[derive(Component)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub radius: f32,
}

impl Default for Orbit {
    /// Far enough out to frame a side-4 grid at unit scale.
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.9,
            radius: 26.0,
        }
    }
}

/// The transform an [`Orbit`] describes. Shared by setup and the orbit system so the first frame
/// is already correct.
pub fn place(o: &Orbit) -> Transform {
    let dir = Quat::from_euler(EulerRot::YXZ, o.yaw, -o.pitch, 0.0) * Vec3::Z;
    Transform::from_translation(dir * o.radius).looking_at(Vec3::ZERO, Vec3::Y)
}

pub fn orbit(
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    camera: Single<(&mut Orbit, &mut Transform)>,
) {
    let (mut orbit, mut transform) = camera.into_inner();

    if buttons.pressed(MouseButton::Right) {
        orbit.yaw -= motion.delta.x * LOOK_SENSITIVITY;
        // Stop just short of the poles, where `looking_at` degenerates.
        orbit.pitch = (orbit.pitch - motion.delta.y * LOOK_SENSITIVITY).clamp(-1.55, 1.55);
    }

    // Browsers report pixel deltas roughly 50x larger than a desktop mouse's line deltas.
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 50.0,
    };
    orbit.radius =
        (orbit.radius * (1.0 - notches * ZOOM_SENSITIVITY)).clamp(MIN_RADIUS, MAX_RADIUS);

    *transform = place(&orbit);
}
