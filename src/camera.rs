//! Camera controls: fly with the right button held, middle-drag to turn and pan about the point
//! under the cursor.
//!
//! The `Transform` is the camera's state, and [`Orbit`] is read back out of it by [`rebase`] after
//! every interaction. That is what lets flying and orbiting compose with no mode to switch between:
//! fly somewhere, then turn about whatever is under the cursor from there. It also keeps [`place`],
//! the scripted poses and the scene report agreeing with where the camera actually is.
//!
//! Also the one place a camera position can be named from outside the app — see [`parse_pose`],
//! which is what lets a script aim the camera without editing [`Orbit::default`].

use bevy::{
    camera_controller::free_camera::FreeCameraState,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

use crate::view::{GridModel, HexLayout, selection::pick_point};

const LOOK_SENSITIVITY: f32 = 0.005;
const ZOOM_SENSITIVITY: f32 = 0.1;
pub const MIN_RADIUS: f32 = 3.0;
pub const MAX_RADIUS: f32 = 200.0;

/// Browsers report pixel deltas roughly this much larger than a desktop mouse's line deltas.
/// Without the correction, zoom is unusable in a browser while feeling fine natively.
const PIXELS_PER_LINE: f32 = 50.0;

/// Looking straight down. Reachable because [`place`] does not use `looking_at`.
pub const TOP_DOWN_PITCH: f32 = std::f32::consts::FRAC_PI_2;

/// Camera position in spherical coordinates about [`Orbit::target`].
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub radius: f32,
    pub target: Vec3,
}

impl Default for Orbit {
    /// Far enough out to frame a side-4 grid at unit scale.
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.9,
            radius: 26.0,
            target: Vec3::ZERO,
        }
    }
}

/// The point the camera turns and pans about: whatever was under the cursor when the drag began.
///
/// Latched at the press rather than re-picked each frame. A pivot that follows the cursor as the
/// view swings slides out from under the camera, and the drag becomes a slow crawl across the
/// scene instead of a rotation.
#[derive(Resource, Default, Debug)]
pub struct Pivot(pub Vec3);

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

/// A pose written as a preset name, as `yaw,pitch,radius` about the origin, or as
/// `free:x,y,z@tx,ty,tz` — an eye point and what it looks at, in world units.
///
/// Degrees rather than radians for the orbit form, because a caller writes these by hand; the
/// conversion happens here, so [`place`] and every existing caller keep working in radians.
/// Out-of-range values are clamped rather than rejected, which is what [`orbit`] and `reset_view`
/// already do to values arrived at by dragging — a pitch of 120° means "as far over as the camera
/// goes", not "your input is void". The free form has nothing to clamp: any point is a valid place
/// to stand, which is the point of it.
///
/// The free form resolves through [`rebase`] to an ordinary [`Orbit`], so it is not a second kind
/// of pose. It needs no new transform code and inherits `place`'s definition at the poles, which is
/// exactly the case a hand-written eye point tends to ask for.
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

    // Lowercased for the prefix alone; the numbers either side of it do not care about case.
    if let Some(rest) = s.to_ascii_lowercase().strip_prefix("free:") {
        let (eye, at) = rest.split_once('@')?;
        return Some(Pose::At(rebase(triple(eye)?, triple(at)?)));
    }

    let numbers = triple(s)?;
    Some(Pose::At(orbit_from_degrees(
        numbers.x, numbers.y, numbers.z,
    )))
}

/// Three comma-separated floats and nothing else. A fourth field is a mistake worth reporting,
/// not a value to ignore.
fn triple(s: &str) -> Option<Vec3> {
    let mut parts = s.split(',');
    let mut next = || parts.next()?.trim().parse::<f32>().ok();
    let vector = Vec3::new(next()?, next()?, next()?);
    parts.next().is_none().then_some(vector)
}

/// Every pose name [`parse_pose`] accepts, for the message shown when it rejects one.
pub fn pose_names() -> impl Iterator<Item = &'static str> {
    PRESETS.iter().map(|(name, ..)| *name).chain(["fit"])
}

fn orbit_from_degrees(yaw: f32, pitch: f32, radius: f32) -> Orbit {
    Orbit {
        yaw: yaw.to_radians(),
        pitch: pitch.to_radians().clamp(-TOP_DOWN_PITCH, TOP_DOWN_PITCH),
        radius: radius.clamp(MIN_RADIUS, MAX_RADIUS),
        target: Vec3::ZERO,
    }
}

/// The transform an [`Orbit`] describes. Shared by setup, the scripted poses and `reset_view` so
/// the first frame is already correct.
///
/// The rotation is built directly rather than through `looking_at`, which has no valid up vector
/// when looking straight down — the view direction is then parallel to `+Y`. This form is well
/// defined at the poles, so a pitch of exactly ±π/2 is allowed, and at that pitch north is up.
pub fn place(o: &Orbit) -> Transform {
    let rotation = Quat::from_euler(EulerRot::YXZ, o.yaw, -o.pitch, 0.0);
    Transform::from_translation(o.target + rotation * Vec3::Z * o.radius).with_rotation(rotation)
}

/// The [`Orbit`] describing a camera at `position` looking at `target` — the exact inverse of
/// [`place`]'s translation, which is what stops the two drifting apart.
///
/// Nothing is clamped. A lossy inverse would leave the reported pose disagreeing with the camera,
/// and after a flight the camera can genuinely be further out than [`MAX_RADIUS`]; the limits
/// belong on the input that moves it, not on the description of where it ended up.
pub fn rebase(position: Vec3, target: Vec3) -> Orbit {
    let offset = position - target;
    let radius = offset.length();
    Orbit {
        radius,
        target,
        // Clamped because `place` is defined at the poles: at a pitch of exactly ±π/2 the division
        // can overshoot 1.0 by an ulp, and `asin` of that is NaN.
        pitch: (offset.y / radius.max(f32::MIN_POSITIVE))
            .clamp(-1.0, 1.0)
            .asin(),
        // Undefined at the poles, where every yaw puts the camera in the same place. `atan2(0, 0)`
        // is zero, which is as good an answer as any.
        yaw: offset.x.atan2(offset.z),
    }
}

/// Turn, pan and zoom about the pivot: middle-drag turns, Shift+middle-drag pans, and the wheel
/// zooms towards the cursor.
///
/// Writes the `Transform` only when something actually moved. That is what lets a scripted pose
/// survive the frame it was set in, and what keeps this out of `FreeCamera`'s way.
// A system's parameters are the list of what it reads and writes, not an argument list a caller
// has to get right, so the usual reason to keep them few does not apply.
#[allow(clippy::too_many_arguments)]
pub fn orbit(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Single<&Window>,
    layout: Res<HexLayout>,
    grid: Res<GridModel>,
    mut pivot: ResMut<Pivot>,
    camera: Single<(&Camera, &Projection, &mut Orbit, &mut Transform)>,
) {
    // The right button hands the camera to `FreeCamera`; nothing here moves it while it is down.
    if buttons.pressed(MouseButton::Right) {
        return;
    }
    let (camera, projection, mut orbit, mut transform) = camera.into_inner();

    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let middle = buttons.pressed(MouseButton::Middle);
    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / PIXELS_PER_LINE,
    };

    // A drag takes its pivot once, at the press. A zoom takes a fresh one every time, which is what
    // makes the wheel converge on whatever the cursor is over rather than on the view centre.
    if buttons.just_pressed(MouseButton::Middle) || notches != 0.0 {
        pivot.0 = cursor_target(camera, &transform, &window, &layout, &grid, pivot.0);
    }

    let turning = middle && !shift;
    let panning = middle && shift;
    let delta = motion.delta;

    if turning && delta != Vec2::ZERO {
        turn_about(&mut transform, pivot.0, delta * LOOK_SENSITIVITY);
    } else if panning && delta != Vec2::ZERO {
        let Projection::Perspective(perspective) = projection else {
            // Only the perspective camera is driven here; nothing else is used in this app.
            return;
        };
        // Exact rather than a tuned constant: at the pivot's distance the viewport is this many
        // world units tall, so the ground keeps up with the cursor at any zoom instead of merely
        // moving the same way.
        let visible = 2.0 * transform.translation.distance(pivot.0) * (perspective.fov * 0.5).tan();
        let step = (delta.y * *transform.up() - delta.x * *transform.right())
            * (visible / window.height());
        transform.translation += step;
        // The pivot travels with the camera. Left behind, it would be off screen by the end of the
        // pan, and the next turn would swing about a point nobody can see.
        pivot.0 += step;
    } else if notches != 0.0 {
        let offset = transform.translation - pivot.0;
        let distance = offset.length();
        if distance > f32::EPSILON {
            let wanted =
                (distance * (1.0 - notches * ZOOM_SENSITIVITY)).clamp(MIN_RADIUS, MAX_RADIUS);
            transform.translation = pivot.0 + offset * (wanted / distance);
        }
    } else {
        return;
    }

    *orbit = rebase(transform.translation, pivot.0);
}

/// Turn the camera about `pivot`, moving its position and its orientation together.
///
/// Deliberately not [`rebase`] followed by [`place`]: that points the camera *at* the pivot, so the
/// view snaps whenever the pivot is off centre — which, with the pivot taken from under the cursor,
/// is most of the time. Rotating both leaves whatever you were looking at where it was.
///
/// Yaw is about world `+Y` and pitch about the camera's own right axis, which is what makes a
/// vertical drag pitch rather than roll however far round the view has been swung.
fn turn_about(transform: &mut Transform, pivot: Vec3, angles: Vec2) {
    let yaw = Quat::from_rotation_y(-angles.x);
    let turn = yaw * Quat::from_axis_angle(*transform.right(), angles.y);
    // Refuse a step that would put the camera's up vector below the horizon — the moment the view
    // tips over — keeping the yaw, so a diagonal drag still swings the view round. Testing the
    // *step* rather than how steep the view already is matters: a check on the view direction
    // cannot tell just-short-of-vertical from just-past-it, since both look equally steep.
    let turn = if (turn * *transform.up()).y <= 0.0 {
        yaw
    } else {
        turn
    };
    transform.translation = pivot + turn * (transform.translation - pivot);
    transform.rotation = turn * transform.rotation;
}

/// The world point the cursor is over, or `fallback` when there is no cursor to read.
fn cursor_target(
    camera: &Camera,
    transform: &Transform,
    window: &Window,
    layout: &HexLayout,
    grid: &GridModel,
    fallback: Vec3,
) -> Vec3 {
    let Some(cursor) = window.cursor_position() else {
        return fallback;
    };
    // Built from the local transform rather than the entity's `GlobalTransform`, which is a frame
    // behind our own writes mid-drag. The camera has no parent, so the two are otherwise the same.
    let global = GlobalTransform::from(*transform);
    match camera.viewport_to_world(&global, cursor) {
        Ok(ray) => pick_point(ray, layout, grid, fallback),
        Err(_) => fallback,
    }
}

/// Give `FreeCamera` the camera only while the right button is down.
///
/// Gated rather than left running for two reasons. It adjusts fly speed on **every** scroll event
/// whether or not the mouse is grabbed, so ungated it would silently ramp the speed while the wheel
/// was zooming; and its movement keys would answer WASD when the camera is not being flown.
/// Disabled, it consumes nothing and releases the cursor grab itself.
///
/// Yaw and pitch are re-seeded on the press because the controller latches them from the transform
/// exactly once, on its first run. After a turn or a scripted pose its own copy is stale, and the
/// view would jump back to it the moment the mouse moved.
///
/// **Registration is load-bearing in both directions**: `PreUpdate`, so this lands ahead of the
/// controller's own `RunFixedMainLoop` systems, and `.after(InputSystems)`, so it reads this frame's
/// buttons. Get the second wrong and the controller is enabled a frame late, which is exactly one
/// frame too late for the `just_pressed` its cursor grab hangs on — and the failure is quiet, since
/// everything driven by `pressed` still works and only mouse-look silently never engages.
pub fn fly_on_right_button(
    buttons: Res<ButtonInput<MouseButton>>,
    camera: Single<(&Transform, &mut FreeCameraState)>,
) {
    let (transform, mut state) = camera.into_inner();

    let flying = buttons.pressed(MouseButton::Right);
    if state.enabled != flying {
        state.enabled = flying;
    }

    if buttons.just_pressed(MouseButton::Right) {
        let (yaw, pitch, _roll) = transform.rotation.to_euler(EulerRot::YXZ);
        state.yaw = yaw;
        state.pitch = pitch;
    } else if buttons.just_released(MouseButton::Right) {
        // Otherwise whatever speed was left when the button came up is re-applied on the next
        // press, as a lurch before friction takes it back down.
        state.velocity = Vec3::ZERO;
    }
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
            assert!(
                parse_pose(name).is_some(),
                "{name} is advertised but rejected"
            );
        }
        assert_eq!(parse_pose("fit"), Some(Pose::Fit));
        // Case and surrounding space come from a shell, where both are easy to introduce.
        assert_eq!(parse_pose("  TOP "), parse_pose("top"));
    }

    #[test]
    fn numbers_are_read_as_degrees() {
        let orbit = at("90,45,20");
        assert!(
            (orbit.yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-6,
            "{orbit:?}"
        );
        assert!(
            (orbit.pitch - std::f32::consts::FRAC_PI_4).abs() < 1e-6,
            "{orbit:?}"
        );
        assert_eq!(orbit.radius, 20.0);
        assert_eq!(
            orbit.target,
            Vec3::ZERO,
            "an orbit pose is about the origin"
        );
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
        assert!(
            (iso.yaw - default.yaw).abs() < 1e-4,
            "{iso:?} vs {default:?}"
        );
        assert!(
            (iso.pitch - default.pitch).abs() < 1e-4,
            "{iso:?} vs {default:?}"
        );
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
        for bad in [
            "",
            "nope",
            "0,45",
            "0,45,20,7",
            "a,b,c",
            "0;45;20",
            ",",
            // The free form, mis-written every way it can be.
            "free:0,10,0",
            "free:0,10@0,0,0",
            "free:0,10,0@0,0",
            "free:a,b,c@0,0,0",
            "free:0,10,0@0,0,0,0",
            "free:@",
            "free:",
        ] {
            assert_eq!(parse_pose(bad), None, "{bad:?} should not parse");
        }
    }

    /// A free pose puts the camera exactly where it says and points it at what it names — including
    /// straight down, which is the case `place` exists to survive and the one a hand-written eye
    /// point most often asks for.
    #[test]
    fn a_free_pose_is_an_eye_point_and_what_it_looks_at() {
        let overhead = place(&at("free:0,10,0@0,0,0"));
        assert!(overhead.translation.distance(Vec3::new(0.0, 10.0, 0.0)) < 1e-4);
        assert!(overhead.forward().dot(Vec3::NEG_Y) > 0.9999, "{overhead:?}");

        let (eye, target) = (Vec3::new(12.0, 6.0, -12.0), Vec3::new(-2.0, 1.0, 3.0));
        let oblique = place(&at("FREE:12,6,-12@-2,1,3"));
        assert!(oblique.translation.distance(eye) < 1e-3, "{oblique:?}");
        let to_target = (target - oblique.translation).normalize();
        assert!(oblique.forward().dot(to_target) > 0.9999, "{oblique:?}");
    }

    /// The invariant the whole no-mode design rests on: `Orbit` can be recovered from a transform
    /// and put back without moving the camera. Break it and the scene report, the scripted poses
    /// and the interactive camera stop agreeing about where the camera is.
    #[test]
    fn rebase_inverts_place() {
        for target in [Vec3::ZERO, Vec3::new(-3.5, 1.25, 7.0)] {
            for (yaw, pitch, radius) in [
                (0.0, 0.0, 5.0),
                (2.1, -0.7, 18.0),
                (-1.3, 1.2, 40.0),
                // Beyond MAX_RADIUS: reachable by flying, and `rebase` must not clamp it away.
                (0.4, 0.3, 500.0),
                (0.7, TOP_DOWN_PITCH, 12.0),
                (-2.9, -TOP_DOWN_PITCH, 12.0),
            ] {
                let orbit = Orbit {
                    yaw,
                    pitch,
                    radius,
                    target,
                };
                let there = place(&orbit).translation;
                let back = rebase(there, target);

                assert!(
                    place(&back).translation.distance(there) < 1e-3,
                    "{orbit:?} came back as {back:?}"
                );
                assert!((back.radius - radius).abs() < 1e-3, "{orbit:?} -> {back:?}");
                assert!((back.pitch - pitch).abs() < 1e-4, "{orbit:?} -> {back:?}");
                // Yaw is undefined at the poles, where every yaw is the same place.
                if pitch.abs() < TOP_DOWN_PITCH - 1e-3 {
                    assert!((back.yaw - yaw).abs() < 1e-4, "{orbit:?} -> {back:?}");
                }
            }
        }
    }

    /// Turning about an off-centre pivot must not re-aim the camera at it — that snap is the whole
    /// reason `turn_about` exists instead of a `rebase`/`place` round trip.
    #[test]
    fn turning_about_an_off_centre_pivot_does_not_re_aim_the_camera() {
        let start = place(&Orbit::default());
        let pivot = Vec3::new(6.0, 0.0, -4.0);

        let mut turned = start;
        turn_about(&mut turned, pivot, Vec2::new(0.4, 0.0));

        // The pivot stayed put and the camera swung around it.
        assert!(
            (turned.translation.distance(pivot) - start.translation.distance(pivot)).abs() < 1e-3
        );
        // The heading turned by the angle asked for, not by however much it would take to face the
        // pivot. Measured about the vertical, because that is the axis a horizontal drag turns
        // about — the angle between the two forward vectors is smaller whenever the view is pitched.
        let heading = |t: &Transform| {
            let forward = *t.forward();
            forward.x.atan2(forward.z)
        };
        // Wrapped into ±π: the default view faces due `-Z`, which is exactly where `atan2` turns
        // over, so the raw difference comes out a full turn off.
        use std::f32::consts::{PI, TAU};
        let swung = (heading(&turned) - heading(&start) + PI).rem_euclid(TAU) - PI;
        assert!(
            (swung + 0.4).abs() < 1e-3,
            "swung {swung} for a 0.4 rad drag"
        );

        // And it is still not looking at the pivot, which is the snap this exists to avoid.
        let to_pivot = (pivot - turned.translation).normalize();
        assert!(
            turned.forward().dot(to_pivot) < 0.99,
            "the view snapped onto the pivot"
        );
    }

    /// A drag that would take the view over the top keeps its yaw and loses only its pitch, rather
    /// than tipping the horizon upside down — and it still gets all the way to vertical, rather
    /// than the guard freezing it short.
    #[test]
    fn the_view_cannot_be_turned_past_vertical() {
        let mut transform = place(&Orbit::default());
        for _ in 0..20 {
            turn_about(&mut transform, Vec3::ZERO, Vec2::new(0.05, -0.1));
            assert!(
                transform.up().y > 0.0,
                "the horizon tipped over: {transform:?}"
            );
        }
        assert!(
            transform.forward().y < -0.99,
            "stopped short: {transform:?}"
        );

        // The same going the other way, into looking up from below.
        for _ in 0..40 {
            turn_about(&mut transform, Vec3::ZERO, Vec2::new(0.0, 0.1));
            assert!(
                transform.up().y > 0.0,
                "the horizon tipped over: {transform:?}"
            );
        }
        assert!(transform.forward().y > 0.99, "stopped short: {transform:?}");
    }
}
