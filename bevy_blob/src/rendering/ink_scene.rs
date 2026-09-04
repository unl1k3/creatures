//! Authored ink-scene selection rules.
//!
//! The geometry builders will move here incrementally; keeping the selection
//! policy beside them prevents the presentation layer from depending on test
//! scenario details.

use super::*;
use bevy::ecs::system::SystemParam;

/// Resources used to rebuild the optional ink rendering of a level.
#[derive(SystemParam)]
pub(crate) struct InkPreviewResources<'w> {
    asset_server: Res<'w, AssetServer>,
    ink_style: Res<'w, InkStylePreview>,
    scenario: Res<'w, TestScenario>,
    level: Res<'w, Level>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<ColorMaterial>>,
}

/// Scene entities whose visibility or lifetime belongs to the ink preview.
#[derive(SystemParam)]
pub(crate) struct InkPreviewEntities<'w, 's> {
    existing: Query<'w, 's, (Entity, &'static InkPreviewShape)>,
    artwork: Query<'w, 's, &'static mut Visibility, With<LevelArtwork>>,
}

pub(crate) fn sync_ink_preview(
    mut commands: Commands,
    resources: InkPreviewResources,
    mut entities: InkPreviewEntities,
) {
    let InkPreviewResources {
        asset_server,
        ink_style,
        scenario,
        level,
        mut meshes,
        mut materials,
    } = resources;
    for mut visibility in &mut entities.artwork {
        *visibility = if ink_style.enabled {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    let current = entities
        .existing
        .iter()
        .all(|(_, marker)| marker.scenario == scenario.0);
    if !ink_style.enabled || !current {
        for (entity, _) in &entities.existing {
            commands.entity(entity).despawn();
        }
    }
    if !ink_style.enabled || (current && !entities.existing.is_empty()) {
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
