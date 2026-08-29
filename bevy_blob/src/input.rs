use super::*;

pub(super) fn exit_on_escape(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut exit: MessageWriter<AppExit>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

/// Pauses virtual time, which also stops fixed physics while preserving the
/// UI update loop so the same key can resume the game.
pub(super) fn toggle_pause(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    if keyboard.just_pressed(KeyCode::KeyP) {
        if virtual_time.is_paused() {
            virtual_time.unpause();
        } else {
            virtual_time.pause();
        }
    }
}

pub(super) fn cycle_selection(
    keyboard: Res<ButtonInput<KeyCode>>,
    vitality: Res<VitalityWorld>,
    mut blobs: ResMut<BlobWorld>,
) {
    if keyboard.just_pressed(KeyCode::Tab) && blobs.active.len() > 1 {
        for _ in 0..blobs.active.len() {
            blobs.selected = next_selection(blobs.selected, blobs.active.len());
            if vitality.is_alive(blobs.active[blobs.selected].id) {
                break;
            }
        }
    }
}

pub(super) fn handle_blob_actions(
    keyboard: Res<ButtonInput<KeyCode>>,
    fixed_time: Res<Time<Fixed>>,
    level: Res<Level>,
    mut blobs: ResMut<BlobWorld>,
    mut split_rng: ResMut<SplitRng>,
    mut acid: ResMut<AcidWorld>,
    mut shields: ResMut<ShieldWorld>,
    mut vitality: ResMut<VitalityWorld>,
    mut route_progress: ResMut<RouteProgress>,
    mut nutrition: ResMut<NutritionWorld>,
    mut commands: Commands,
    nutrient_bodies: Query<Entity, With<NutrientPhysics>>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        reset_world_at(&mut blobs, level.spawn_position);
        acid.reset();
        shields.reset();
        vitality.reset();
        route_progress.next = 1;
        for entity in &nutrient_bodies {
            commands.entity(entity).despawn();
        }
        nutrition.reset_from_definitions(&level.nutrients);
        spawn_nutrient_bodies(&mut commands, &level.nutrients);
        return;
    }

    let selected_alive = blobs
        .active
        .get(blobs.selected)
        .is_some_and(|blob| vitality.is_alive(blob.id));
    let selected_digesting = blobs
        .active
        .get(blobs.selected)
        .is_some_and(|blob| nutrition.is_digesting(blob.id));

    let living_sibling_pair = blobs
        .active
        .get(blobs.selected)
        .and_then(|blob| blob.parent_id)
        .is_some_and(|parent| {
            blobs
                .active
                .iter()
                .filter(|blob| blob.parent_id == Some(parent))
                .all(|blob| vitality.is_alive(blob.id))
        });
    if keyboard.just_pressed(KeyCode::KeyE)
        && selected_alive
        && !selected_digesting
        && living_sibling_pair
    {
        start_selected_rejoin(&mut blobs);
        return;
    }

    if keyboard.just_pressed(KeyCode::KeyX)
        && blobs.active.len() < MAX_ACTIVE_BLOBS
        && blobs.rejoin_parent.is_none()
        && selected_alive
        && !selected_digesting
        && blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.body.can_split())
    {
        let parent_id = blobs.active[blobs.selected].id;
        split_selected(&mut blobs, &mut split_rng, fixed_time.delta_secs());
        if let (Some(first), Some(second)) = (
            blobs.active.get(blobs.selected),
            blobs.active.get(blobs.selected + 1),
        ) {
            vitality.split(parent_id, [first.id, second.id]);
        }
    }
}

pub(super) fn next_selection(current: usize, blob_count: usize) -> usize {
    if blob_count == 0 {
        0
    } else {
        (current + 1) % blob_count
    }
}
