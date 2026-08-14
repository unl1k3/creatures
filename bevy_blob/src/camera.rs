use super::*;

pub(super) fn follow_camera(
    time: Res<Time>,
    blobs: Res<BlobWorld>,
    mut camera: Single<&mut Transform, With<Camera2d>>,
) {
    let Some(target) = selected_camera_target(&blobs) else {
        return;
    };
    let response = (5.0 * time.delta_secs()).min(1.0);
    camera.translation.x += (target.x - camera.translation.x) * response;
    camera.translation.y += (target.y - camera.translation.y) * response;
}

pub(super) fn selected_camera_target(blobs: &BlobWorld) -> Option<Vec2> {
    blobs
        .active
        .get(blobs.selected)
        .map(|blob| blob.body.center())
}
