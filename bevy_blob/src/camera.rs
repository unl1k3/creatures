use super::*;

#[derive(Component)]
pub(super) struct GameCamera;

pub(super) fn follow_camera(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    blobs: Res<BlobWorld>,
    scenario: Res<TestScenario>,
    mut debug_overlay: ResMut<LevelDebugOverlay>,
    mut camera: Single<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    let Some(target) = selected_camera_target(&blobs) else {
        return;
    };
    if debug_overlay.visible {
        let (transform, projection) = &mut *camera;
        update_debug_camera(time.delta_secs(), &keyboard, target, transform, projection);
        return;
    }
    if debug_overlay.camera_detached {
        if keyboard.just_pressed(KeyCode::KeyP) {
            camera.0.translation.x = target.x;
            camera.0.translation.y = target.y;
            debug_overlay.camera_detached = false;
        } else {
            return;
        }
    }
    let response = (5.0 * time.delta_secs()).min(1.0);
    let framing_target = if scenario.0 > 1 {
        Vec2::new(target.x * 0.82, target.y + 75.0)
    } else {
        target
    };
    camera.0.translation.x += (framing_target.x - camera.0.translation.x) * response;
    camera.0.translation.y += (framing_target.y - camera.0.translation.y) * response;
    let desired_scale = match scenario.0 {
        2..=4 | 6 => 1.22,
        5 => 1.32,
        _ => 1.0,
    };
    let mut pixel_scale = None;
    if let Projection::Orthographic(projection) = &mut *camera.1 {
        projection.scale += (desired_scale - projection.scale) * response;
        pixel_scale = Some(projection.scale);
    }
    if let Some(pixel_scale) = pixel_scale {
        snap_to_texture_pixel_grid(&mut camera.0.translation, pixel_scale);
    }
}

/// Align the camera to the rendered pixel grid. Thin ink lines are stable only
/// when their sampling position does not drift through fractions of a pixel.
fn snap_to_texture_pixel_grid(translation: &mut Vec3, orthographic_scale: f32) {
    let pixel = orthographic_scale.max(f32::EPSILON);
    translation.x = (translation.x / pixel).round() * pixel;
    translation.y = (translation.y / pixel).round() * pixel;
}

fn update_debug_camera(
    dt: f32,
    keyboard: &ButtonInput<KeyCode>,
    selected_target: Vec2,
    transform: &mut Transform,
    projection: &mut Projection,
) {
    let horizontal = keyboard.pressed(KeyCode::KeyL) as i8 - keyboard.pressed(KeyCode::KeyJ) as i8;
    let vertical = keyboard.pressed(KeyCode::KeyI) as i8 - keyboard.pressed(KeyCode::KeyK) as i8;
    let mut direction = Vec2::new(horizontal as f32, vertical as f32);
    if direction != Vec2::ZERO {
        direction = direction.normalize();
    }

    let Projection::Orthographic(orthographic) = projection else {
        return;
    };
    let pan_speed = 520.0 * orthographic.scale;
    transform.translation += (direction * pan_speed * dt).extend(0.0);

    let zoom = keyboard.pressed(KeyCode::KeyU) as i8 - keyboard.pressed(KeyCode::KeyO) as i8;
    if zoom != 0 {
        orthographic.scale =
            (orthographic.scale * (zoom as f32 * 1.35 * dt).exp()).clamp(0.45, 3.5);
    }
    if keyboard.just_pressed(KeyCode::KeyP) {
        transform.translation.x = selected_target.x;
        transform.translation.y = selected_target.y;
        orthographic.scale = 1.0;
    }
}

pub(super) fn selected_camera_target(blobs: &BlobWorld) -> Option<Vec2> {
    blobs
        .active
        .get(blobs.selected)
        .map(|blob| blob.body.center())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_camera_can_pan_zoom_and_return_to_selected_blob() {
        let mut keyboard = ButtonInput::default();
        keyboard.press(KeyCode::KeyL);
        keyboard.press(KeyCode::KeyI);
        keyboard.press(KeyCode::KeyU);
        let mut transform = Transform::default();
        let mut projection = Projection::Orthographic(OrthographicProjection::default_2d());

        update_debug_camera(
            1.0,
            &keyboard,
            Vec2::new(40.0, 80.0),
            &mut transform,
            &mut projection,
        );
        assert!(transform.translation.x > 0.0 && transform.translation.y > 0.0);
        assert!(matches!(&projection, Projection::Orthographic(value) if value.scale > 1.0));

        keyboard.release(KeyCode::KeyL);
        keyboard.release(KeyCode::KeyI);
        keyboard.release(KeyCode::KeyU);
        keyboard.press(KeyCode::KeyP);
        update_debug_camera(
            0.0,
            &keyboard,
            Vec2::new(40.0, 80.0),
            &mut transform,
            &mut projection,
        );
        assert_eq!(transform.translation.truncate(), Vec2::new(40.0, 80.0));
        assert!(matches!(&projection, Projection::Orthographic(value) if value.scale == 1.0));
    }
}
