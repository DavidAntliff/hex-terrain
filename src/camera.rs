//! Orbit camera: right-drag to rotate about the origin, scroll to zoom.
//!
//! Also the one place a camera position can be named from outside the app — see [`parse_pose`],
//! which is what lets a script aim the camera without editing [`Orbit::default`].

use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

const LOOK_SENSITIVITY: f32 = 0.005;
const ZOOM_SENSITIVITY: f32 = 0.1;
pub const MIN_RADIUS: f32 = 3.0;
pub const MAX_RADIUS: f32 = 200.0;

/// Looking straight down. Reachable because [`place`] does not use `looking_at`.
pub const TOP_DOWN_PITCH: f32 = std::f32::consts::FRAC_PI_2;

/// Camera position in spherical coordinates about the origin.
// ponytail: target is always the origin. Add a `target: Vec3` when panning is needed.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
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

/// A camera position asked for by name or by number.
///
/// `Fit` is deferred rather than resolved here because framing the whole scene needs the live
/// projection and window aspect ratio — see [`crate::view::framing::reset_view`], which already
/// computes it for the debug panel's button.
// ponytail: poses are discrete. No interpolation between them: the scripted path reads still
// images, and a tween is only visible to a human watching. Add one if a flythrough is ever wanted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pose {
    Fit,
    At(Orbit),
}

/// The named poses, in the order [`parse_pose`] reports them when it rejects a name.
///
/// Angles in degrees, matching the syntax a caller writes; `radius` in world units. `iso` is
/// [`Orbit::default`] restated so the table is the whole set of names rather than one name plus a
/// special case.
const PRESETS: &[(&str, f32, f32, f32)] = &[
    // Straight down. Radius is the default's, so `top` differs from `fit` only in not being framed.
    ("top", 0.0, 90.0, 26.0),
    ("iso", 0.0, 51.566, 26.0),
    // Grazing, and the reason this exists: shorelines and the horizon haze are only visible from
    // near the plane, and reaching such a view previously meant editing the default in source.
    ("low", 0.0, 8.0, 30.0),
];

/// A pose written as a preset name, or as `yaw,pitch,radius` in degrees, degrees and world units.
///
/// Degrees rather than radians because a caller writes these by hand; the conversion happens here,
/// so [`place`] and every existing caller keep working in radians. Out-of-range values are clamped
/// rather than rejected, which is what [`orbit`] and `reset_view` already do to values arrived at
/// by dragging — a pitch of 120° means "as far over as the camera goes", not "your input is void".
///
/// Returns `None` only for a name that is not a preset, or numbers that do not parse.
pub fn parse_pose(s: &str) -> Option<Pose> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("fit") {
        return Some(Pose::Fit);
    }
    if let Some(&(_, yaw, pitch, radius)) = PRESETS
        .iter()
        .find(|(name, ..)| name.eq_ignore_ascii_case(s))
    {
        return Some(Pose::At(orbit_from_degrees(yaw, pitch, radius)));
    }

    let mut parts = s.split(',');
    let mut next = || parts.next()?.trim().parse::<f32>().ok();
    let (yaw, pitch, radius) = (next()?, next()?, next()?);
    // A fourth field is a mistake worth reporting, not a value to ignore.
    if parts.next().is_some() {
        return None;
    }
    Some(Pose::At(orbit_from_degrees(yaw, pitch, radius)))
}

/// Every pose name [`parse_pose`] accepts, for the message shown when it rejects one.
pub fn pose_names() -> impl Iterator<Item = &'static str> {
    PRESETS.iter().map(|(name, ..)| *name).chain(["fit"])
}

fn orbit_from_degrees(yaw: f32, pitch: f32, radius: f32) -> Orbit {
    Orbit {
        yaw: yaw.to_radians(),
        pitch: pitch
            .to_radians()
            .clamp(-TOP_DOWN_PITCH, TOP_DOWN_PITCH),
        radius: radius.clamp(MIN_RADIUS, MAX_RADIUS),
    }
}

/// The transform an [`Orbit`] describes. Shared by setup and the orbit system so the first frame
/// is already correct.
///
/// The rotation is built directly rather than through `looking_at`, which has no valid up vector
/// when looking straight down — the view direction is then parallel to `+Y`. This form is well
/// defined at the poles, so a pitch of exactly ±π/2 is allowed, and at that pitch north is up.
pub fn place(o: &Orbit) -> Transform {
    let rotation = Quat::from_euler(EulerRot::YXZ, o.yaw, -o.pitch, 0.0);
    Transform::from_translation(rotation * Vec3::Z * o.radius).with_rotation(rotation)
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
        orbit.pitch = (orbit.pitch - motion.delta.y * LOOK_SENSITIVITY)
            .clamp(-TOP_DOWN_PITCH, TOP_DOWN_PITCH);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Orbit {
        match parse_pose(s) {
            Some(Pose::At(orbit)) => orbit,
            other => panic!("{s:?} did not parse to a position: {other:?}"),
        }
    }

    #[test]
    fn every_advertised_name_parses() {
        for name in pose_names() {
            assert!(parse_pose(name).is_some(), "{name} is advertised but rejected");
        }
        assert_eq!(parse_pose("fit"), Some(Pose::Fit));
        // Case and surrounding space come from a shell, where both are easy to introduce.
        assert_eq!(parse_pose("  TOP "), parse_pose("top"));
    }

    #[test]
    fn numbers_are_read_as_degrees() {
        let orbit = at("90,45,20");
        assert!((orbit.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-6, "{orbit:?}");
        assert!((orbit.pitch - std::f32::consts::FRAC_PI_4).abs() < 1e-6, "{orbit:?}");
        assert_eq!(orbit.radius, 20.0);
        assert_eq!(at(" 0 , 45 , 20 "), at("0,45,20"), "spaces around fields");
    }

    /// `top` really does look straight down, which is the pitch [`place`] is built to survive.
    #[test]
    fn top_is_the_pole() {
        assert_eq!(at("top").pitch, TOP_DOWN_PITCH);
    }

    /// The table restates [`Orbit::default`] in degrees, so the two can drift apart. They must not:
    /// `iso` is documented as the view the app opens with.
    #[test]
    fn iso_is_the_default_view() {
        let (iso, default) = (at("iso"), Orbit::default());
        assert!((iso.yaw - default.yaw).abs() < 1e-4, "{iso:?} vs {default:?}");
        assert!((iso.pitch - default.pitch).abs() < 1e-4, "{iso:?} vs {default:?}");
        assert_eq!(iso.radius, default.radius);
    }

    #[test]
    fn out_of_range_values_are_clamped_not_rejected() {
        assert_eq!(at("0,120,20").pitch, TOP_DOWN_PITCH);
        assert_eq!(at("0,-120,20").pitch, -TOP_DOWN_PITCH);
        assert_eq!(at("0,45,9999").radius, MAX_RADIUS);
        assert_eq!(at("0,45,-5").radius, MIN_RADIUS);
        // Yaw is not clamped: it is an angle about a full circle, and it is used through `sin`/`cos`.
        assert!((at("720,45,20").yaw - (720f32).to_radians()).abs() < 1e-4);
    }

    #[test]
    fn garbage_is_rejected() {
        for bad in ["", "nope", "0,45", "0,45,20,7", "a,b,c", "0;45;20", ","] {
            assert_eq!(parse_pose(bad), None, "{bad:?} should not parse");
        }
    }
}
