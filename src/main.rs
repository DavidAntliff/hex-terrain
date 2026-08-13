//! A hex grid to iterate terrain work in: a side-4 hexagon of locations under a daylight sky, with
//! an orbit camera, click selection and coordinate labels.

use bevy::{
    camera::Exposure,
    light::{
        light_consts::lux, CascadeShadowConfig, CascadeShadowConfigBuilder, EnvironmentMapLight,
        Skybox,
    },
    prelude::*,
    // Not in the prelude, unlike `Window` itself.
    window::{WindowResizeConstraints, WindowResolution},
};

use hex_terrain::camera::{self, place, Orbit};
use hex_terrain::hex::{scenes, TerrainGrid};
use hex_terrain::probe::{ProbePlugin, WINDOW};
use hex_terrain::sky::Sky;
use hex_terrain::view::{GridModel, HexLayout, HexViewPlugin};

/// Direction towards the sun, shared by the light and the sky so the highlight the water throws
/// and the sun it is supposedly reflecting cannot drift apart. About 55° up: a midday sun at
/// middle latitudes.
const SUN_DIR: Vec3 = Vec3::new(4.0, 8.0, 4.0);

/// Hexagon circumradius in world units. The in-plane scaling knob for the whole grid.
const HEX_SCALE: f32 = 1.0;

/// World units per unit of a location's height. The elevation knob, to `HEX_SCALE`'s in-plane one.
const HEIGHT_SCALE: f32 = 1.5;

fn main() {
    // Before the app, so an unknown scene name costs nothing but the message.
    let grid = named_scene();

    let pinned = pinned_resolution();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "hex-terrain".into(),
                fit_canvas_to_parent: true,
                resize_constraints: size_hints(&pinned),
                resolution: pinned.unwrap_or_default(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((HexViewPlugin, ProbePlugin))
        .insert_resource(HexLayout::pointy(HEX_SCALE).with_height_scale(HEIGHT_SCALE))
        .insert_resource(GridModel(grid))
        .add_systems(Startup, setup)
        .add_systems(Update, (camera::orbit, exit_on_escape))
        .run();
}

/// The scene named as the first argument, or [`scenes::DEFAULT`].
///
/// Hand-rolled rather than parsed: one positional name is the whole interface, and `bevy` is the only
/// dependency. On web there is no argv, so the default is all that is reachable there.
fn named_scene() -> TerrainGrid {
    let arg = std::env::args().nth(1);
    let name = arg.as_deref().unwrap_or(scenes::DEFAULT);
    scenes::build(name).unwrap_or_else(|| {
        let names: Vec<&str> = scenes::names().collect();
        eprintln!("unknown scene {name:?}; one of: {}", names.join(", "));
        std::process::exit(2);
    })
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
    use hex_terrain::hex::{Axial, Cube, Doubled, Orientation};

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
