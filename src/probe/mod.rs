//! Scripted observation, for verifying the scene without a human at the keyboard.
//!
//! Three things a script needs that the interactive app does not give it: a way to aim the camera,
//! a way to get more than one view out of a single launch, and a way to read the scene's state as
//! data rather than as pixels. All are off unless asked for, so a plain `cargo run` is untouched.
//!
//! Capturing the window through the X server is unreliable — a window on an inactive workspace is
//! unmapped and yields a blank image — so the app captures its own framebuffer instead.
//!
//! ```sh
//! HEX_TERRAIN_CAMERA='top;iso;low;fit' \
//! HEX_TERRAIN_SCREENSHOT=/tmp/s.png \
//! HEX_TERRAIN_REPORT=/tmp/s.json \
//! HEX_TERRAIN_WINDOW=1280x720 \
//!   cargo run -- two-lakes
//! ```
//!
//! | Variable | Value | Effect |
//! |---|---|---|
//! | `HEX_TERRAIN_CAMERA` | `;`-separated poses | Aims the camera; one capture per pose. |
//! | `HEX_TERRAIN_SCREENSHOT` | path | A PNG per capture. |
//! | `HEX_TERRAIN_REPORT` | path, or `-` | A JSON report per capture. |
//! | `HEX_TERRAIN_INTERVAL` | `<frames>x<count>` | Capture `count` times per pose, `frames` apart. |
//! | `HEX_TERRAIN_WINDOW` | `<W>x<H>` | Pins the window size, in logical pixels. Read by `main`. |
//!
//! With more than one capture, a zero-padded index goes in before the extension —
//! `/tmp/s-00.png`, `-01`, … A single capture keeps the path exactly as given. **The report is the
//! index**: each one names the pose and tick it came from, so nothing has to be encoded in the
//! filename.
//!
//! On web every variable is absent — `std::env::var` returns `Err` there — so the whole plugin
//! disables itself and costs a handful of lookups at startup.

pub mod report;

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

use crate::camera::{place, Orbit, Pose};
use crate::view::framing::ResetViewRequested;
use report::{Report, ReportSources, Run};

/// Frames to render before the first capture, so assets are loaded and the first frame's transients
/// are gone. The skybox in particular takes a few frames to appear.
const SETTLE_FRAMES: u32 = 120;

/// Frames to wait after requesting the last capture. The PNG is written from the render world
/// asynchronously, so exiting immediately would truncate it.
const SAVE_FRAMES: u32 = 60;

/// Frames to hold after moving the camera, before capturing from the new pose.
///
/// Much shorter than [`SETTLE_FRAMES`] because nothing is loading by then — this covers only what
/// depends on the view: the cascade shadow map re-rendering, and reflections settling.
const REPOSE_FRAMES: u32 = 8;

const CAMERA: &str = "HEX_TERRAIN_CAMERA";
const SCREENSHOT: &str = "HEX_TERRAIN_SCREENSHOT";
const REPORT: &str = "HEX_TERRAIN_REPORT";
const INTERVAL: &str = "HEX_TERRAIN_INTERVAL";
/// Read by `main`, which owns the `Window`; named here so every variable is in one table.
pub const WINDOW: &str = "HEX_TERRAIN_WINDOW";

/// One capture: where to look from, and which repeat within that pose.
struct Shot {
    pose: Option<Pose>,
    /// As written by the caller, for the report. `"default"` when no pose was asked for.
    name: String,
    tick: u32,
    /// Frames to wait before capturing, over and above settling.
    delay: u32,
}

#[derive(Resource)]
struct Schedule {
    shots: Vec<Shot>,
    /// Index of the next shot to take. Equal to `shots.len()` once they are all done.
    next: usize,
    image: Option<String>,
    report: Option<String>,
    scene: String,
    frames_left: u32,
    /// Set between requesting a capture and moving on, so the request happens once.
    captured: bool,
    frame: u32,
}

impl Schedule {
    /// Whether an index needs to go into the output paths at all. One shot writes exactly the path
    /// it was given, which keeps every existing single-shot script working unchanged.
    fn indexed(&self) -> bool {
        self.shots.len() > 1
    }
}

/// Enables the scripted-observation variables. Does nothing when none of them are set.
///
/// Carries the scene name because the report records it and the probe has no way to learn it
/// otherwise: `main` resolves the arguments, and re-reading `argv` here would only guess at what
/// they meant.
pub struct ProbePlugin {
    scene: String,
}

impl ProbePlugin {
    pub fn for_scene(scene: String) -> Self {
        Self { scene }
    }
}

impl Plugin for ProbePlugin {
    fn build(&self, app: &mut App) {
        let camera = std::env::var(CAMERA).ok();
        let image = std::env::var(SCREENSHOT).ok();
        let report = std::env::var(REPORT).ok();
        if camera.is_none() && image.is_none() && report.is_none() {
            return;
        }

        let poses = parse_poses(camera.as_deref());
        let (interval, count) = parse_interval(std::env::var(INTERVAL).ok().as_deref());
        let shots = build_shots(poses, interval, count);

        info!(
            "probe: {} shot(s) after {SETTLE_FRAMES} frames{}{}",
            shots.len(),
            image.as_deref().map(|p| format!(", images to {p}")).unwrap_or_default(),
            report.as_deref().map(|p| format!(", reports to {p}")).unwrap_or_default(),
        );

        if report.is_some() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }

        app.insert_resource(Schedule {
            shots,
            next: 0,
            image,
            report,
            scene: self.scene.clone(),
            frames_left: SETTLE_FRAMES,
            captured: false,
            frame: 0,
        })
        .add_systems(Update, run_schedule);
    }
}

/// The pose list, or a single `None` meaning "wherever the camera already is".
///
/// Follows `main::named_scene`'s precedent for a bad value: say what was valid and exit 2, rather
/// than opening a window that silently shows the wrong thing.
fn parse_poses(spec: Option<&str>) -> Vec<(Option<Pose>, String)> {
    let Some(spec) = spec else {
        return vec![(None, "default".into())];
    };
    spec.split(';')
        .filter(|s| !s.trim().is_empty())
        .map(|s| match crate::camera::parse_pose(s) {
            Some(pose) => (Some(pose), s.trim().to_string()),
            None => {
                let names: Vec<&str> = crate::camera::pose_names().collect();
                eprintln!(
                    "unknown camera pose {:?}; one of: {}, \
                     or yaw,pitch,radius in degrees, \
                     or free:x,y,z@tx,ty,tz in world units",
                    s.trim(),
                    names.join(", ")
                );
                std::process::exit(2);
            }
        })
        .collect()
}

/// `<frames>x<count>`, defaulting to one capture with no wait.
///
/// The realised gap is `frames + 2`, the two extra being the state machine's own advance and
/// capture steps. Sampling an animation does not care, so it is not corrected.
fn parse_interval(spec: Option<&str>) -> (u32, u32) {
    let Some(spec) = spec else {
        return (0, 1);
    };
    let parsed = spec.trim().split_once(['x', 'X']).and_then(|(f, c)| {
        Some((f.trim().parse().ok()?, c.trim().parse::<u32>().ok()?.max(1)))
    });
    parsed.unwrap_or_else(|| {
        eprintln!("bad {INTERVAL} {spec:?}; expected <frames>x<count>, e.g. 30x4");
        std::process::exit(2);
    })
}

/// The cross product of poses and ticks, flattened into the order they will be taken in.
fn build_shots(poses: Vec<(Option<Pose>, String)>, interval: u32, count: u32) -> Vec<Shot> {
    let mut shots = Vec::new();
    for (pose, name) in poses {
        for tick in 0..count {
            shots.push(Shot {
                pose,
                name: name.clone(),
                tick,
                // The first capture at a pose waits for the move to take effect; later ones wait
                // out the requested interval instead.
                delay: if tick == 0 { REPOSE_FRAMES } else { interval },
            });
        }
    }
    shots
}

/// Inserts `-NN` before the extension: `/tmp/a.png` → `/tmp/a-00.png`. A path with no extension
/// simply gains the suffix.
fn indexed_path(path: &str, index: usize) -> String {
    let suffix = format!("-{index:02}");
    match path.rfind('.') {
        // Only a real extension, not a dot in a directory name earlier in the path.
        Some(dot) if dot > path.rfind(['/', '\\']).map_or(0, |sep| sep + 1) => {
            format!("{}{suffix}{}", &path[..dot], &path[dot..])
        }
        _ => format!("{path}{suffix}"),
    }
}

/// Drives the whole sequence: settle, pose, wait, capture, advance, exit.
///
/// Aiming needs the camera mutably and the report needs it immutably, which is a conflict Bevy
/// rejects even though the two never happen on the same frame — a pose is always followed by a
/// wait. A `ParamSet` is what says "one at a time" to the scheduler.
fn run_schedule(
    mut commands: Commands,
    schedule: Option<ResMut<Schedule>>,
    mut reset_view: ResMut<ResetViewRequested>,
    mut params: ParamSet<(Query<(&mut Orbit, &mut Transform)>, ReportSources)>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut schedule) = schedule else {
        return;
    };
    schedule.frame += 1;

    if schedule.frames_left > 0 {
        schedule.frames_left -= 1;
        return;
    }

    // Every shot taken: wait out the asynchronous PNG write, then leave.
    let Some(shot) = schedule.shots.get(schedule.next) else {
        exit.write(AppExit::Success);
        return;
    };
    // Copied out so the schedule can be written below; a shot is three small fields and a name.
    let (pose, name, tick, delay) = (shot.pose, shot.name.clone(), shot.tick, shot.delay);

    if !schedule.captured {
        // Aiming and waiting happen in the frame the shot comes up, so a pose change is on screen
        // before anything is read from it.
        match pose {
            Some(Pose::At(orbit)) => {
                if let Ok((mut current, mut transform)) = params.p0().single_mut() {
                    *current = orbit;
                    *transform = place(&current);
                }
            }
            // Deferred to `framing::reset_view`, which needs the live projection and aspect ratio.
            Some(Pose::Fit) => reset_view.0 = true,
            None => {}
        }
        if delay > 0 {
            schedule.frames_left = delay;
            schedule.captured = true;
            return;
        }
        schedule.captured = true;
    }

    let index = schedule.next;
    let image = schedule.image.as_ref().map(|path| {
        if schedule.indexed() {
            indexed_path(path, index)
        } else {
            path.clone()
        }
    });

    if let Some(path) = &image {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
    }

    if let Some(target) = schedule.report.clone() {
        let run = Run {
            scene: schedule.scene.clone(),
            pose: name,
            tick,
            shot: index,
            frame: schedule.frame,
            image: image.clone(),
        };
        let indexed = schedule.indexed();
        let sources = params.p1();
        write_report(&target, indexed, index, Report::collect(run, &sources));
    }

    schedule.next += 1;
    schedule.captured = false;
    // The last shot's PNG is still being written from the render world; the exit branch above runs
    // once this has elapsed.
    if schedule.next == schedule.shots.len() {
        schedule.frames_left = SAVE_FRAMES;
    }
}

/// `-` means stdout, as one JSON Lines record. Anything else is a file, pretty-printed.
fn write_report(target: &str, indexed: bool, index: usize, report: Report) {
    if target == "-" {
        println!("{}", report.to_line());
        return;
    }
    let path = if indexed {
        indexed_path(target, index)
    } else {
        target.to_string()
    };
    // A report that cannot be written is worth a message but not a crash: the screenshot beside it
    // may still be the point of the run.
    match std::fs::File::create(&path) {
        Ok(file) => {
            if let Err(e) = report.write_pretty(std::io::BufWriter::new(file)) {
                error!("writing {path}: {e}");
            }
        }
        Err(e) => error!("creating {path}: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_index_goes_before_the_extension() {
        assert_eq!(indexed_path("/tmp/a.png", 0), "/tmp/a-00.png");
        assert_eq!(indexed_path("/tmp/a.png", 12), "/tmp/a-12.png");
        assert_eq!(indexed_path("a.tar.gz", 1), "a.tar-01.gz");
        // No extension, and a dot in a directory rather than in the filename: the suffix goes on
        // the end, where it cannot corrupt the path.
        assert_eq!(indexed_path("/tmp/shot", 3), "/tmp/shot-03");
        assert_eq!(indexed_path("/tmp/v1.2/shot", 3), "/tmp/v1.2/shot-03");
        // A dotfile is all extension and no stem; appending is the only sane reading.
        assert_eq!(indexed_path("/tmp/.png", 0), "/tmp/.png-00");
    }

    #[test]
    fn intervals_default_to_a_single_capture() {
        assert_eq!(parse_interval(None), (0, 1));
        assert_eq!(parse_interval(Some("30x4")), (30, 4));
        assert_eq!(parse_interval(Some(" 30 X 4 ")), (30, 4));
        // A count of zero would take no shots at all, which is never what was meant.
        assert_eq!(parse_interval(Some("30x0")), (30, 1));
    }

    #[test]
    fn shots_are_the_poses_crossed_with_the_ticks() {
        let poses = vec![
            (Some(Pose::Fit), "fit".to_string()),
            (None, "default".to_string()),
        ];
        let shots = build_shots(poses, 30, 3);
        assert_eq!(shots.len(), 6);
        assert_eq!(shots[0].tick, 0);
        assert_eq!(shots[2].tick, 2);
        assert_eq!(shots[3].name, "default", "poses vary slowest");
        // The first tick at a pose waits for the move, the rest wait out the interval.
        assert_eq!(shots[0].delay, REPOSE_FRAMES);
        assert_eq!(shots[1].delay, 30);
    }
}
