//! Minimal 3D scene to iterate in: a cube at the origin, an all-sky starfield, and an
//! orbit camera (right-drag to rotate, scroll to zoom).

use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit},
    light::Skybox,
    prelude::*,
    render::render_resource::{TextureViewDescriptor, TextureViewDimension},
};

/// Vertical 1x6 strip, wgpu face order: +X -X +Y -Y +Z -Z. See `tools/make_skybox.py`.
const CUBEMAP: &str = "textures/starmap_cubemap.png";

const LOOK_SENSITIVITY: f32 = 0.005;
const ZOOM_SENSITIVITY: f32 = 0.1;

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
        .add_systems(Startup, setup)
        .add_systems(Update, (patch_cubemap, orbit, exit_on_escape))
        .run();
}

/// Camera position in spherical coordinates about the origin.
// ponytail: target is always the origin. Add a `target: Vec3` when panning is needed.
#[derive(Component)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    radius: f32,
}

fn place(o: &Orbit) -> Transform {
    let dir = Quat::from_euler(EulerRot::YXZ, o.yaw, -o.pitch, 0.0) * Vec3::Z;
    Transform::from_translation(dir * o.radius).looking_at(Vec3::ZERO, Vec3::Y)
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::default())),
        MeshMaterial3d(materials.add(Color::srgb_u8(124, 144, 255))),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb_u8(180, 190, 220),
        brightness: 200.0,
        ..default()
    });

    let orbit = Orbit {
        yaw: 0.6,
        pitch: 0.4,
        radius: 6.0,
    };
    commands.spawn((
        Camera3d::default(),
        place(&orbit),
        // ponytail: the skybox is decoration only, it does not light the cube. For that add
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
}

fn orbit(
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
    orbit.radius = (orbit.radius * (1.0 - notches * ZOOM_SENSITIVITY)).clamp(1.5, 100.0);

    *transform = place(&orbit);
}
