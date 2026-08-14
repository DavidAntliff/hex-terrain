//! A hex grid to iterate terrain work in: a side-4 hexagon of locations under a daylight sky, with
//! editor-style camera controls, click selection and coordinate labels.

use clap::{Parser, builder::PossibleValuesParser};

use bevy::{
    asset::AssetMetaCheck,
    camera::Exposure,
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin},
    light::{
        CascadeShadowConfig, CascadeShadowConfigBuilder, EnvironmentMapLight, Skybox,
        light_consts::lux,
    },
    prelude::*,
    // Not in the prelude, unlike `Window` itself.
    window::{WindowResizeConstraints, WindowResolution},
};

use hex_terrain::camera::{self, Orbit, place};
use hex_terrain::hex::scenes;
use hex_terrain::probe::{ProbePlugin, WINDOW};
use hex_terrain::sky::Sky;
use hex_terrain::view::layout::DEFAULT_INSET;
use hex_terrain::view::{GridModel, HexLayout, HexViewPlugin};

/// Direction towards the sun, shared by the light and the sky so the highlight the water throws
/// and the sun it is supposedly reflecting cannot drift apart. About 55° up: a midday sun at
/// middle latitudes.
const SUN_DIR: Vec3 = Vec3::new(4.0, 8.0, 4.0);

/// Hexagon circumradius in world units. The in-plane scaling knob for the whole grid.
const HEX_SCALE: f32 = 1.0;

/// World units per unit of a location's height. The elevation knob, to `HEX_SCALE`'s in-plane one.
const HEIGHT_SCALE: f32 = 1.5;

/// What the shell can be told from the command line: which scene to load, and the one rendering
/// knob worth tuning without a rebuild.
///
/// Parsed **before** `App::new()`, so a bad argument costs the message and not a GPU
/// initialisation. On web there is no argv, so every field takes its default there and the panel is
/// the only way to reach any of this — see `spec/scene.md`.
// `long_about = None` keeps the doc comment above out of `--help`: it is written for a reader of
// the source, not for someone at a prompt.
#[derive(Parser, Debug)]
#[command(about = "A hex grid to iterate terrain work in.", long_about = None)]
struct Cli {
    /// Which named scene to load.
    #[arg(
        long,
        default_value = scenes::DEFAULT,
        value_parser = PossibleValuesParser::new(scenes::names().collect::<Vec<_>>()),
    )]
    scene: String,

    /// How far each cap is shrunk towards its centre, as a percentage of the hexagon's
    /// circumradius. Also a slider in the panel.
    #[arg(long, value_name = "PERCENT", value_parser = inset_percent)]
    inset: Option<f32>,
}

/// A percentage on the command line, the fraction the meshes are built from in the layout.
///
/// The ceiling matches the panel slider's, so the two knobs reach the same places. Rejecting rather
/// than clamping: a number outside the range is a mistake worth hearing about, not a value to
/// silently substitute.
fn inset_percent(arg: &str) -> Result<f32, String> {
    let percent: f32 = arg.parse().map_err(|_| format!("not a number: {arg:?}"))?;
    if !(0.0..=50.0).contains(&percent) {
        return Err(format!("{percent} is outside 0..=50"));
    }
    Ok(percent / 100.0)
}

fn main() {
    let cli = Cli::parse();
    // `clap` has already rejected any name not in the table.
    let grid = scenes::build(&cli.scene).expect("clap validated the scene name");
    let layout = HexLayout::pointy(HEX_SCALE)
        .with_height_scale(HEIGHT_SCALE)
        .with_inset(cli.inset.unwrap_or(DEFAULT_INSET));

    let pinned = pinned_resolution();

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "hex-terrain".into(),
                        fit_canvas_to_parent: true,
                        resize_constraints: size_hints(&pinned),
                        resolution: pinned.unwrap_or_default(),
                        ..default()
                    }),
                    ..default()
                })
                // No asset here has a `.meta` file, and on the web asking for one is
                // actively harmful: a static host that answers a missing path with its
                // index page and a 200 has Bevy read that as meta, fail to deserialize
                // it, and abandon the asset — the shader is then never fetched at all
                // and every mesh using it silently goes undrawn. Skipping the lookup
                // costs nothing on native, where the absent file is reported missing.
                // See spec/wiki/build-performance.md.
                .set(AssetPlugin {
                    meta_check: AssetMetaCheck::Never,
                    ..default()
                }),
        )
        // `FreeCameraPlugin` is what flies the camera while the right button is held; the rest of
        // the controls are `camera::orbit`'s. See spec/camera-controls.md.
        .add_plugins((
            HexViewPlugin,
            ProbePlugin::for_scene(cli.scene),
            FreeCameraPlugin,
        ))
        .insert_resource(layout)
        .insert_resource(GridModel(grid))
        .init_resource::<camera::Pivot>()
        .add_systems(Startup, setup)
        // Between Bevy's input systems and `FreeCameraPlugin`'s, which run in `RunFixedMainLoop`.
        // Both halves of that are load-bearing: after `InputSystems` so it reads this frame's
        // buttons rather than last frame's, and in `PreUpdate` so the controller sees the result
        // on the frame the button goes down. See spec/camera-controls.md.
        .add_systems(
            PreUpdate,
            camera::fly_on_right_button.after(bevy::input::InputSystems),
        )
        .add_systems(Update, (camera::orbit, exit_on_escape))
        .run();
}

/// `HEX_TERRAIN_WINDOW=<W>x<H>`, or `None` for whatever the window manager decides.
///
/// Pinning the size is what makes two runs comparable at all: an image diff between screenshots of
/// different sizes is measuring the window manager, not the scene. Lives here rather than in
/// `probe` because the window is `main`'s to configure, and it is useful on its own — a run that
/// only wants a repeatable frame size sets nothing else.
///
/// **The numbers are logical pixels.** The PNG is that times the display's scale factor, so a 2×
/// screen turns `1280x720` into a 2560×1440 image. This is not adjustable from here: `bevy_winit`
/// multiplies the requested physical size by the backend scale factor when the window is created,
/// and does so whether or not `scale_factor_override` is set. The override is still worth setting —
/// it fixes the logical-to-physical ratio the resize constraints below are interpreted in — but it
/// does not stop the multiplication. The report records the size actually rendered.
fn pinned_resolution() -> Option<WindowResolution> {
    let spec = std::env::var(WINDOW).ok()?;
    let parsed = spec
        .trim()
        .split_once(['x', 'X'])
        .and_then(|(w, h)| Some((w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?)));
    match parsed {
        Some((w, h)) if w > 0 && h > 0 => {
            Some(WindowResolution::new(w, h).with_scale_factor_override(1.0))
        }
        _ => {
            eprintln!("bad {WINDOW} {spec:?}; expected <width>x<height>, e.g. 1280x720");
            std::process::exit(2);
        }
    }
}

/// Fixed minimum and maximum size, which is how a size request survives a window manager.
///
/// A tiling WM gives a tiled window whatever geometry its layout dictates and ignores the size the
/// application asked for. Equal min and max hints are the standard way to say the window is not
/// resizable, and i3 in particular auto-floats a window whose minimum and maximum sizes are equal —
/// which is exactly what takes it out of the tiling layout and lets the requested size stand.
///
/// Constraints are in logical pixels; [`pinned_resolution`] pins the scale factor at 1.0, so they
/// are the same numbers.
fn size_hints(pinned: &Option<WindowResolution>) -> WindowResizeConstraints {
    let Some(resolution) = pinned else {
        return default();
    };
    let (w, h) = (
        resolution.physical_width() as f32,
        resolution.physical_height() as f32,
    );
    WindowResizeConstraints {
        min_width: w,
        max_width: w,
        min_height: h,
        max_height: h,
    }
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.spawn((
        DirectionalLight {
            // A real midday sun, to go with a sky in real photometric units and a camera exposed
            // for daylight. Every level in the scene is now a physical quantity rather than a
            // number tuned against the one next to it.
            illuminance: lux::DIRECT_SUNLIGHT,
            // 0.19 spells it `shadow_maps_enabled`, not `shadows_enabled`.
            shadow_maps_enabled: true,
            ..default()
        },
        // The default is four cascades reaching 150 world units, which spends almost all of the
        // shadow map on empty space around a grid a few units across. One cascade is also what
        // WebGL2 is limited to, so native and web get the same picture.
        CascadeShadowConfig::from(CascadeShadowConfigBuilder {
            num_cascades: 1,
            maximum_distance: 60.0,
            ..default()
        }),
        Transform::from_translation(SUN_DIR).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let sky = Sky {
        sun: SUN_DIR.normalize(),
        ..default()
    };
    let (zenith, horizon, ground) = sky.gradient_colours();

    // No ambient fill. The environment map below now lights what the sun does not, using the
    // colours actually overhead instead of one flat tint — a flat fill on top of it would only
    // wash that shading back out.
    commands.insert_resource(GlobalAmbientLight {
        brightness: 0.0,
        ..default()
    });

    let orbit = Orbit::default();
    commands.spawn((
        Camera3d::default(),
        place(&orbit),
        // Exposed for daylight rather than Bevy's default, which is set several stops darker for
        // an interior. With the sun, sky and camera all on physical scales, `brightness` and
        // `intensity` below are plain 1.0: the sky is handed over in the units they already want.
        Exposure::SUNLIGHT,
        Skybox {
            image: Some(images.add(sky.cubemap())),
            brightness: 1.0,
            ..default()
        },
        // What the water reflects, and what fills the shadows. Three colours off the same model the
        // skybox is drawn from, so the reflection agrees with the sky behind it by construction —
        // and a hemispherical gradient needs no prefiltered KTX2 map, so there is no asset pipeline
        // behind any of this.
        EnvironmentMapLight {
            intensity: 1.0,
            ..EnvironmentMapLight::hemispherical_gradient(&mut images, zenith, horizon, ground)
        },
        orbit,
        // Speeds are world units per second, and the grid is only about seven units across, so
        // the controller's defaults of 5 and 15 cross the whole scene in under a second. The
        // wheel scales both while flying, so these are a starting point rather than a limit.
        FreeCamera {
            walk_speed: 3.0,
            run_speed: 9.0,
            ..default()
        },
    ));
}

/// Writing `AppExit` is the graceful path: Bevy finishes the frame, runs `Drop` on the world
/// and closes the window itself. On web there is no window to close, so this does nothing.
fn exit_on_escape(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use hex_terrain::hex::{Axial, Cube, Doubled, Orientation};

    /// `clap`'s own consistency check over the derived command — conflicting flags, a default that
    /// its own value parser would reject, and so on.
    #[test]
    fn the_command_is_well_formed() {
        Cli::command().debug_assert();
    }

    /// Every test here goes through `try_parse_from`. `Cli::parse` reads the real `argv`, which
    /// under `cargo test` is the test-name filter, so calling it would fail the harness on
    /// `cargo test some_filter`.
    #[test]
    fn no_arguments_is_the_default_scene_at_the_default_inset() {
        let cli = Cli::try_parse_from(["hex-terrain"]).expect("no arguments is valid");
        assert_eq!(cli.scene, scenes::DEFAULT);
        assert_eq!(cli.inset, None);
    }

    #[test]
    fn a_percentage_on_the_command_line_arrives_as_a_fraction() {
        let cli = Cli::try_parse_from(["hex-terrain", "--scene", "two-lakes", "--inset", "12"])
            .expect("a real scene and an in-range inset");
        assert_eq!(cli.scene, "two-lakes");
        assert_eq!(cli.inset, Some(0.12));
    }

    /// Both rejections happen before the window opens, which is the point of parsing first.
    #[test]
    fn a_bad_scene_or_a_bad_inset_is_refused() {
        assert!(Cli::try_parse_from(["hex-terrain", "--scene", "nope"]).is_err());
        assert!(Cli::try_parse_from(["hex-terrain", "--inset", "80"]).is_err());
        assert!(Cli::try_parse_from(["hex-terrain", "--inset", "-1"]).is_err());
        assert!(Cli::try_parse_from(["hex-terrain", "--inset", "wide"]).is_err());
    }

    // Escape cannot be sent by hand in a headless check, so verify the wiring instead: the
    // right key code, the system registered, and an actual AppExit reaching the app.
    #[test]
    fn escape_requests_a_clean_exit() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, exit_on_escape);

        app.update();
        assert!(app.should_exit().is_none(), "exited with no key pressed");

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert_eq!(app.should_exit(), Some(AppExit::Success));
    }

    /// The grid the app builds with no scene named is the one the spec describes.
    ///
    /// Built through `scenes` rather than through `named_scene`, which reads `argv` — under
    /// `cargo test` the first argument is the test-name filter, not a scene.
    #[test]
    fn the_default_scene_grid_is_a_hexagon_of_side_four() {
        let grid = scenes::build(scenes::DEFAULT).expect("the default names a scene");
        assert_eq!(grid.len(), 37);
        assert!(grid.contains(Axial::ZERO));
    }

    /// The centre hex is the origin of every coordinate system, including world space, in either
    /// orientation.
    #[test]
    fn centre_hex_is_the_origin_of_all_systems() {
        let centre = Axial::ZERO;
        assert_eq!(centre.to_cube(), Cube::ZERO);
        for orientation in [Orientation::Pointy, Orientation::Flat] {
            let layout = HexLayout::pointy(HEX_SCALE).with_orientation(orientation);
            assert_eq!(centre.to_doubled(orientation), Doubled::new(0, 0));
            assert_eq!(layout.hex_to_world(centre), Vec3::ZERO);
        }
    }
}
