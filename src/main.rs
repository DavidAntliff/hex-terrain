//! A hex grid to iterate terrain work in: a side-4 hexagon of locations under a starfield, with an
//! orbit camera, click selection and coordinate labels.

use bevy::{
    light::{CascadeShadowConfig, CascadeShadowConfigBuilder, Skybox},
    prelude::*,
    render::render_resource::{TextureViewDescriptor, TextureViewDimension},
};

use hex_terrain::camera::{self, place, Orbit};
use hex_terrain::hex::{undulating, TerrainGrid};
use hex_terrain::screenshot::ScreenshotOnDemandPlugin;
use hex_terrain::view::{GridModel, HexLayout, HexViewPlugin, GRID_RADIUS};

/// Vertical 1x6 strip, wgpu face order: +X -X +Y -Y +Z -Z. See `tools/make_skybox.py`.
const CUBEMAP: &str = "textures/starmap_cubemap.png";

/// Hexagon circumradius in world units. The in-plane scaling knob for the whole grid.
const HEX_SCALE: f32 = 1.0;

/// World units per unit of a location's height. The elevation knob, to `HEX_SCALE`'s in-plane one.
const HEIGHT_SCALE: f32 = 1.5;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "hex-terrain".into(),
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((HexViewPlugin, ScreenshotOnDemandPlugin))
        .insert_resource(HexLayout::pointy(HEX_SCALE).with_height_scale(HEIGHT_SCALE))
        .insert_resource(GridModel(TerrainGrid::hexagon(GRID_RADIUS, undulating)))
        .add_systems(Startup, setup)
        .add_systems(Update, (patch_cubemap, camera::orbit, exit_on_escape))
        .run();
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
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
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Fill light, so shadowed walls and the insides of pits stay readable instead of going black.
    // The skybox contributes nothing, so without this there is only the one directional light.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb_u8(180, 190, 220),
        brightness: 900.0,
        ..default()
    });

    let orbit = Orbit::default();
    commands.spawn((
        Camera3d::default(),
        place(&orbit),
        // ponytail: the skybox is decoration only, it does not light the grid. For that add
        // EnvironmentMapLight, which needs a prefiltered KTX2 map (toktx/basisu).
        Skybox {
            image: Some(assets.load(CUBEMAP)),
            brightness: 1000.0,
            ..default()
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

/// A PNG carries no cubemap metadata, so reinterpret the loaded strip as 6 array layers.
fn patch_cubemap(
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut skybox: Single<&mut Skybox>,
) {
    let Some(handle) = skybox.image.clone() else {
        return;
    };
    if !assets.load_state(&handle).is_loaded() {
        return;
    }
    // Peek immutably first: `get_mut` flags the asset modified, which would re-upload the
    // texture every frame. An already-reinterpreted image has 6 layers, so this is idempotent.
    if images
        .get(&handle)
        .is_none_or(|image| image.texture_descriptor.array_layer_count() != 1)
    {
        return;
    }

    let mut image = images.get_mut(&handle).unwrap();
    let layers = image.height() / image.width();
    image
        .reinterpret_stacked_2d_as_array(layers)
        .expect("cubemap must be a vertical strip of square faces");
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    skybox.image = Some(handle); // nudge the render world into rebuilding its bind group
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

    /// The grid the app actually builds is the one the spec describes.
    #[test]
    fn the_scene_grid_is_a_hexagon_of_side_four() {
        let grid = TerrainGrid::hexagon(GRID_RADIUS, undulating);
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
