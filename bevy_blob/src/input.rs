use super::*;

pub(super) fn exit_on_escape(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut exit: MessageWriter<AppExit>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

pub(super) fn cycle_selection(keyboard: Res<ButtonInput<KeyCode>>, mut blobs: ResMut<BlobWorld>) {
    if keyboard.just_pressed(KeyCode::Tab) && blobs.active.len() > 1 {
        blobs.selected = next_selection(blobs.selected, blobs.active.len());
    }
}

pub(super) fn handle_blob_actions(
    keyboard: Res<ButtonInput<KeyCode>>,
    fixed_time: Res<Time<Fixed>>,
    mut blobs: ResMut<BlobWorld>,
    mut split_rng: ResMut<SplitRng>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        if !start_selected_rejoin(&mut blobs) && blobs.active.len() == 1 {
            reset_world(&mut blobs);
        }
        return;
    }

    if keyboard.just_pressed(KeyCode::KeyX)
        && blobs.active.len() < MAX_ACTIVE_BLOBS
        && blobs.rejoin_parent.is_none()
        && blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.body.can_split())
    {
        split_selected(&mut blobs, &mut split_rng, fixed_time.delta_secs());
    }
}

pub(super) fn next_selection(current: usize, blob_count: usize) -> usize {
    if blob_count == 0 {
        0
    } else {
        (current + 1) % blob_count
    }
}
