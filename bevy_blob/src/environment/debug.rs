//! Development-only visual overlay controls for the current level.

use super::*;

pub(crate) fn toggle_level_debug(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut overlay: ResMut<LevelDebugOverlay>,
) {
    let selecting_lighting = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if !selecting_lighting
        && (keyboard.just_pressed(KeyCode::Digit0) || keyboard.just_pressed(KeyCode::Backquote))
    {
        overlay.visible = !overlay.visible;
        if overlay.visible {
            overlay.camera_detached = true;
        }
    }
}
