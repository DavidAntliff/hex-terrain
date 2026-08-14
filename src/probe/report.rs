//! What the scene actually contained, as JSON.
//!
//! A screenshot shows a symptom and never a cause: "the water is missing" and "the water plates
//! carry no vertices" are the same image. This is the other half — the numbers behind the frame,
//! written beside it so a scripted check can tell those two apart without another run.
//!
//! **The types here exist only to be serialised.** Nothing in [`crate::hex`] or [`crate::view`]
//! derives `Serialize`; the model stays a plain dimensionless model and this module owns both the
//! shapes and the conversion into them. That is also why it is a separate file from the plugin that
//! drives it.

use bevy::prelude::*;
use serde::Serialize;

use crate::camera::Orbit;
use crate::view::compass::ShowCompass;
use crate::view::grid_render::{HexCell, HexSkirt, HexWall, SeaLevel, ShowGridLines, WaterSurface};
use crate::view::labels::LabelMode;
use crate::view::selection::Selected;
use crate::view::{GridModel, HexLayout};

/// One capture's worth of state. Serialised whole, one document per shot.
#[derive(Serialize)]
pub struct Report {
    pub run: Run,
    pub window: WindowInfo,
    pub camera: Camera,
    pub layout: Layout,
    pub model: Model,
    pub render: Render,
    pub diagnostics: Diagnostics,
}

/// The framebuffer actually rendered, which is not necessarily the one asked for.
///
/// `HEX_TERRAIN_WINDOW` is a request, and a tiling window manager overrides it — it hands a tiled
/// window whatever geometry the layout dictates. Two screenshots are only comparable if these
/// numbers match, so they are reported rather than assumed: a diff between runs of different sizes
/// is measuring the window manager, not the scene.
#[derive(Serialize)]
pub struct WindowInfo {
    /// Physical pixels — the size of the PNG.
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}

/// Which shot this is, and what produced it. The report is the index: filenames carry only a
/// running counter, and this says what that counter meant.
#[derive(Serialize)]
pub struct Run {
    pub scene: String,
    /// The pose as it was written on the command line, not as it was resolved.
    pub pose: String,
    /// Which capture within this pose, for `HEX_TERRAIN_INTERVAL`. Zero when there is only one.
    pub tick: u32,
    pub shot: usize,
    pub frame: u32,
    /// The PNG this report describes, or `null` when only a report was asked for.
    pub image: Option<String>,
}

/// Where the camera was — in the spherical terms it is steered in, and in the world terms that
/// decide what is actually on screen.
///
/// The spherical terms are relative to `target`, which is no longer always the origin: it is the
/// point the last turn or pan was about. Without it, `yaw_deg`/`pitch_deg`/`radius` do not say
/// where the camera is.
#[derive(Serialize)]
pub struct Camera {
    pub yaw_deg: f32,
    pub pitch_deg: f32,
    pub radius: f32,
    pub target: [f32; 3],
    pub translation: [f32; 3],
    /// Vertical, matching Bevy's own convention.
    pub fov_deg: Option<f32>,
    pub aspect: Option<f32>,
}

/// The projection knobs — the only place world units exist.
#[derive(Serialize)]
pub struct Layout {
    pub orientation: &'static str,
    pub hex_scale: [f32; 2],
    pub height_scale: f32,
    /// The cap inset as a fraction of the circumradius — the `--inset` percentage divided by 100,
    /// or wherever the panel's slider was left.
    pub inset: f32,
    pub labels: &'static str,
    pub compass: bool,
    pub grid_lines: bool,
    pub selected: Option<[i32; 2]>,
}

/// The dimensionless model. Everything here is independent of scale and of the renderer.
#[derive(Serialize)]
pub struct Model {
    pub locations: usize,
    pub height_min: f32,
    pub height_max: f32,
    /// The slider's level, which is not the same thing as the levels in the model: a scene can
    /// author its own, and those stand until the slider is moved.
    pub sea_level: f32,
    pub water_locations: usize,
    /// Distinct water levels, ascending. A lower bound on the number of bodies, not the count:
    /// two bodies that happen to sit at the same level are one entry here.
    // ponytail: `grid_render::water_plates` knows the real partition but computes it per location
    // and does not expose it. Promote it if body identity ever has to be reported.
    pub water_levels: Vec<f32>,
}

/// What was handed to the GPU. The half a screenshot cannot show.
#[derive(Serialize)]
pub struct Render {
    pub cells: MeshStats,
    pub walls: MeshStats,
    pub water: MeshStats,
}

/// One kind of mesh in aggregate. `entities` separates "nothing was spawned" from "something was
/// spawned and it is empty" — different bugs that look identical in a PNG.
#[derive(Serialize)]
pub struct MeshStats {
    pub entities: usize,
    pub vertices: usize,
    pub triangles: usize,
}

#[derive(Serialize)]
pub struct Diagnostics {
    pub fps: Option<f64>,
    pub frame_time_ms: Option<f64>,
}

/// Everything the report needs from the world, gathered into one system parameter so the caller
/// stays a state machine rather than a forty-argument function.
#[derive(bevy::ecs::system::SystemParam)]
pub struct ReportSources<'w, 's> {
    pub camera: Query<
        'w,
        's,
        (
            &'static Orbit,
            &'static Transform,
            Option<&'static Projection>,
        ),
    >,
    pub layout: Res<'w, HexLayout>,
    pub grid: Res<'w, GridModel>,
    pub sea: Res<'w, SeaLevel>,
    pub labels: Res<'w, LabelMode>,
    pub compass: Res<'w, ShowCompass>,
    pub grid_lines: Res<'w, ShowGridLines>,
    pub selected: Res<'w, Selected>,
    pub meshes: Res<'w, Assets<Mesh>>,
    /// A cell's cap carries no marker of its own — it is the meshed child that is none of the wall,
    /// the skirt or a water plate — so it is found through the parent rather than by a component.
    /// Every new kind of child has to be excluded here or it is counted as a cap.
    pub cell_children: Query<'w, 's, &'static Children, With<HexCell>>,
    pub caps: Query<'w, 's, &'static Mesh3d, NotACap>,
    pub walls: Query<'w, 's, &'static Mesh3d, With<HexWall>>,
    pub water: Query<'w, 's, &'static Mesh3d, With<WaterSurface>>,
    pub diagnostics: Option<Res<'w, bevy::diagnostic::DiagnosticsStore>>,
    pub window: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
}

/// Everything a cell's meshed child can be *other* than its cap, which carries no marker of its own.
type NotACap = (Without<HexWall>, Without<HexSkirt>, Without<WaterSurface>);

impl Report {
    pub fn collect(run: Run, sources: &ReportSources) -> Self {
        Self {
            run,
            window: window(sources),
            camera: camera(sources),
            layout: layout(sources),
            model: model(sources),
            render: render(sources),
            diagnostics: diagnostics(sources),
        }
    }

    /// Pretty for a file — a report is read by people as often as by scripts, and a pretty one
    /// also diffs line by line between two runs.
    pub fn write_pretty(&self, writer: impl std::io::Write) -> std::io::Result<()> {
        serde_json::to_writer_pretty(writer, self).map_err(std::io::Error::other)
    }

    /// Compact for stdout: one document per line is JSON Lines, so a batch of shots stays
    /// parseable as it streams past whatever else the app logs.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }
}

fn window(sources: &ReportSources) -> WindowInfo {
    match sources.window.single() {
        Ok(window) => WindowInfo {
            width: window.resolution.physical_width(),
            height: window.resolution.physical_height(),
            scale_factor: window.resolution.scale_factor(),
        },
        Err(_) => WindowInfo {
            width: 0,
            height: 0,
            scale_factor: 0.0,
        },
    }
}

fn camera(sources: &ReportSources) -> Camera {
    let Ok((orbit, transform, projection)) = sources.camera.single() else {
        return Camera {
            yaw_deg: f32::NAN,
            pitch_deg: f32::NAN,
            radius: f32::NAN,
            target: [f32::NAN; 3],
            translation: [f32::NAN; 3],
            fov_deg: None,
            aspect: None,
        };
    };
    // Only the perspective camera is described here; nothing else is used in this app.
    let perspective = match projection {
        Some(Projection::Perspective(p)) => Some(p),
        _ => None,
    };
    Camera {
        yaw_deg: orbit.yaw.to_degrees(),
        pitch_deg: orbit.pitch.to_degrees(),
        radius: orbit.radius,
        target: orbit.target.to_array(),
        translation: transform.translation.to_array(),
        fov_deg: perspective.map(|p| p.fov.to_degrees()),
        aspect: perspective.map(|p| p.aspect_ratio),
    }
}

fn layout(sources: &ReportSources) -> Layout {
    let layout = &sources.layout;
    Layout {
        orientation: layout.orientation.name(),
        hex_scale: layout.size.to_array(),
        height_scale: layout.height_scale,
        inset: layout.inset,
        labels: sources.labels.name(),
        compass: sources.compass.0,
        grid_lines: sources.grid_lines.0,
        selected: sources.selected.0.map(|c| [c.q, c.r]),
    }
}

fn model(sources: &ReportSources) -> Model {
    let heights = || sources.grid.iter().map(|l| l.data.height);
    let mut levels: Vec<f32> = sources.grid.iter().filter_map(|l| l.data.water).collect();
    let water_locations = levels.len();
    levels.sort_by(f32::total_cmp);
    levels.dedup();

    Model {
        locations: sources.grid.len(),
        height_min: heights().fold(f32::INFINITY, f32::min),
        height_max: heights().fold(f32::NEG_INFINITY, f32::max),
        sea_level: sources.sea.0,
        water_locations,
        water_levels: levels,
    }
}

fn render(sources: &ReportSources) -> Render {
    let caps = sources
        .cell_children
        .iter()
        .flatten()
        .filter_map(|child| sources.caps.get(*child).ok());

    Render {
        cells: stats(caps, &sources.meshes),
        walls: stats(sources.walls.iter(), &sources.meshes),
        water: stats(sources.water.iter(), &sources.meshes),
    }
}

/// Vertex and triangle totals over a set of mesh handles.
///
/// Triangles come from the index buffer where there is one and from the vertex count otherwise,
/// because an indexed mesh's vertex count says nothing about how many triangles it draws.
fn stats<'a>(handles: impl Iterator<Item = &'a Mesh3d>, meshes: &Assets<Mesh>) -> MeshStats {
    let mut stats = MeshStats {
        entities: 0,
        vertices: 0,
        triangles: 0,
    };
    for handle in handles {
        stats.entities += 1;
        let Some(mesh) = meshes.get(&handle.0) else {
            continue;
        };
        stats.vertices += mesh.count_vertices();
        stats.triangles += match mesh.indices() {
            Some(indices) => indices.len() / 3,
            None => mesh.count_vertices() / 3,
        };
    }
    stats
}

fn diagnostics(sources: &ReportSources) -> Diagnostics {
    use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
    let smoothed = |path| {
        sources
            .diagnostics
            .as_ref()
            .and_then(|store| store.get(path))
            .and_then(|d| d.smoothed())
    };
    Diagnostics {
        fps: smoothed(&FrameTimeDiagnosticsPlugin::FPS),
        frame_time_ms: smoothed(&FrameTimeDiagnosticsPlugin::FRAME_TIME),
    }
}
