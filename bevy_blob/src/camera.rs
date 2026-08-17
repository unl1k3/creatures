use super::*;

#[derive(Component)]
pub(super) struct GameCamera;

pub(super) fn follow_camera(
    time: Res<Time>,
    blobs: Res<BlobWorld>,
    scenario: Res<TestScenario>,
    mut camera: Single<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    let Some(target) = selected_camera_target(&blobs) else {
        return;
    };
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
    if let Projection::Orthographic(projection) = &mut *camera.1 {
        projection.scale += (desired_scale - projection.scale) * response;
    }
}

pub(super) fn selected_camera_target(blobs: &BlobWorld) -> Option<Vec2> {
    blobs
        .active
        .get(blobs.selected)
        .map(|blob| blob.body.center())
}
