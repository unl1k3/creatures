//! Runtime lifecycle and gameplay systems for the active level.

use super::*;

/// Spawns the embedded production level and initializes its runtime state.
pub(crate) fn setup_environment(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<ColorMaterial>>>,
) {
    let level = Level::prototype();
    spawn_level_colliders(&mut commands, &level);
    spawn_level_artwork(&mut commands, asset_server.as_deref(), &level);
    if let (Some(mut meshes), Some(mut materials)) = (meshes, materials) {
        spawn_level_chains(&mut commands, &level, &mut meshes, &mut materials);
    }
    commands.insert_resource(level);
    commands.insert_resource(TestScenario::default());
    commands.insert_resource(LevelDebugOverlay::default());
    commands.insert_resource(RouteProgress { next: 1 });
    commands.insert_resource(WastewaterEffects::default());
    commands.insert_resource(AvianContactDiagnostics::default());
    commands.insert_resource(AvianContactManifolds::default());
}

/// Rebuilds the world from one of the development-only regression levels.
#[cfg(feature = "dev-tools")]
pub(crate) fn switch_test_scenario(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    colliders: Query<Entity, With<EnvironmentCollider>>,
    artwork: Query<Entity, With<LevelArtwork>>,
    chains: Query<Entity, With<LevelChain>>,
    asset_server: Option<Res<AssetServer>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<ColorMaterial>>>,
    mut scenario: ResMut<TestScenario>,
    mut route_progress: ResMut<RouteProgress>,
    mut level: ResMut<Level>,
    mut blobs: ResMut<BlobWorld>,
    mut vitality: ResMut<VitalityWorld>,
    mut nutrition: ResMut<NutritionWorld>,
    nutrient_bodies: Query<Entity, With<NutrientPhysics>>,
) {
    // Modified number keys remain available for unrelated shortcuts.
    if keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        return;
    }
    let requested = [
        (1, KeyCode::Digit1),
        (2, KeyCode::Digit2),
        (3, KeyCode::Digit3),
        (4, KeyCode::Digit4),
        (5, KeyCode::Digit5),
        (6, KeyCode::Digit6),
        (7, KeyCode::Digit7),
        (8, KeyCode::Digit8),
        (9, KeyCode::Digit9),
    ]
    .into_iter()
    .find_map(|(index, key)| keyboard.just_pressed(key).then_some(index));
    let Some(requested) = requested else {
        return;
    };

    for entity in &colliders {
        commands.entity(entity).despawn();
    }
    for entity in &artwork {
        commands.entity(entity).despawn();
    }
    for entity in &chains {
        commands.entity(entity).despawn();
    }
    for entity in &nutrient_bodies {
        commands.entity(entity).despawn();
    }

    let (new_level, spawn) = Level::test_scenario(requested);
    spawn_level_artwork(&mut commands, asset_server.as_deref(), &new_level);
    if let (Some(mut meshes), Some(mut materials)) = (meshes, materials) {
        spawn_level_chains(&mut commands, &new_level, &mut meshes, &mut materials);
    }
    spawn_level_colliders(&mut commands, &new_level);
    *level = new_level;
    scenario.0 = requested;
    route_progress.next = 1;
    reset_world_at(&mut blobs, spawn);
    vitality.reset();
    nutrition.reset_from_definitions(&level.nutrients);
    spawn_nutrient_bodies(&mut commands, &level.nutrients);
}

/// Applies continuous damage to blobs intersecting authored hazard volumes.
pub(crate) fn simulate_level_hazards(
    time: Res<Time<Fixed>>,
    level: Res<Level>,
    blobs: Res<BlobWorld>,
    mut vitality: ResMut<VitalityWorld>,
) {
    let dt = time.delta_secs();
    for active_blob in &blobs.active {
        for hazard in &level.hazards {
            let half_size = hazard.size * 0.5;
            if active_blob.body.particles.iter().any(|particle| {
                let offset = (particle.position - hazard.position).abs();
                offset.x <= half_size.x && offset.y <= half_size.y
            }) {
                vitality.damage(active_blob.id, hazard.damage_per_second * dt);
            }
        }
    }
}

/// Advances authored route checkpoints for the currently selected blob.
pub(crate) fn advance_route_progress(
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    mut progress: ResMut<RouteProgress>,
) {
    let Some(blob) = blobs.active.get(blobs.selected) else {
        return;
    };
    while let Some(checkpoint) = level.route.get(progress.next) {
        let reach = (blob.body.rest_radius * 1.45).max(52.0);
        if blob.body.center().distance(*checkpoint) > reach {
            break;
        }
        progress.next += 1;
    }
}
