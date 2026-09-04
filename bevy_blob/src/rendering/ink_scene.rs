//! Authored ink-scene selection rules.
//!
//! The geometry builders will move here incrementally; keeping the selection
//! policy beside them prevents the presentation layer from depending on test
//! scenario details.

use super::*;

pub(crate) fn sync_ink_preview(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    ink_style: Res<InkStylePreview>,
    scenario: Res<TestScenario>,
    level: Res<Level>,
    existing: Query<(Entity, &InkPreviewShape)>,
    mut artwork: Query<&mut Visibility, With<LevelArtwork>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for mut visibility in &mut artwork {
        *visibility = if ink_style.enabled {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    let current = existing
        .iter()
        .all(|(_, marker)| marker.scenario == scenario.0);
    if !ink_style.enabled || !current {
        for (entity, _) in &existing {
            commands.entity(entity).despawn();
        }
    }
    if !ink_style.enabled || (current && !existing.is_empty()) {
        return;
    }

    spawn_ink_backdrop(
        &mut commands,
        &asset_server,
        &mut meshes,
        &mut materials,
        &level,
        scenario.0,
    );

    spawn_ink_level_geometry(
        &mut commands,
        &asset_server,
        &mut meshes,
        &mut materials,
        &level,
        scenario.0,
    );
}
