//! Scripted screenshots, for verifying the scene without a human at the keyboard.
//!
//! Capturing the window through the X server is unreliable — a window on an inactive workspace is
//! unmapped and yields a blank image — so the app captures its own framebuffer instead.
//!
//! Set `HEX_TERRAIN_SCREENSHOT=<path>` to save a PNG once the scene has settled, then exit:
//!
//! ```sh
//! HEX_TERRAIN_SCREENSHOT=/tmp/grid.png cargo run
//! ```

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

/// Frames to render before capturing, so assets are loaded and the first frame's transients are
/// gone. The skybox in particular takes a few frames to appear.
const SETTLE_FRAMES: u32 = 120;

/// Frames to wait after requesting the capture. The PNG is written from the render world
/// asynchronously, so exiting immediately would truncate it.
const SAVE_FRAMES: u32 = 60;

const ENV_VAR: &str = "HEX_TERRAIN_SCREENSHOT";

#[derive(Resource)]
struct PendingScreenshot {
    path: String,
    frames_left: u32,
    requested: bool,
}

/// Enables `HEX_TERRAIN_SCREENSHOT`. Does nothing when the variable is unset, so it costs a normal
/// run one environment lookup at startup.
pub struct ScreenshotOnDemandPlugin;

impl Plugin for ScreenshotOnDemandPlugin {
    fn build(&self, app: &mut App) {
        let Ok(path) = std::env::var(ENV_VAR) else {
            return;
        };
        info!("saving a screenshot to {path} after {SETTLE_FRAMES} frames, then exiting");
        app.insert_resource(PendingScreenshot {
            path,
            frames_left: SETTLE_FRAMES,
            requested: false,
        })
        .add_systems(Update, capture_when_settled);
    }
}

fn capture_when_settled(
    mut commands: Commands,
    pending: Option<ResMut<PendingScreenshot>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(mut pending) = pending else {
        return;
    };

    if pending.frames_left > 0 {
        pending.frames_left -= 1;
        return;
    }

    if !pending.requested {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(pending.path.clone()));
        pending.requested = true;
        pending.frames_left = SAVE_FRAMES;
        return;
    }

    exit.write(AppExit::Success);
}
