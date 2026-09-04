//! Debug route checkpoint labels.

use super::*;
use std::collections::HashSet;

#[derive(Component)]
pub(crate) struct RouteMarker {
    scenario: u8,
    index: usize,
}

pub(crate) fn sync_route_markers(
    mut commands: Commands,
    scenario: Res<TestScenario>,
    progress: Res<RouteProgress>,
    level: Res<Level>,
    debug_overlay: Res<LevelDebugOverlay>,
    markers: Query<(Entity, &RouteMarker)>,
) {
    if !debug_overlay.visible {
        for (entity, _) in &markers {
            commands.entity(entity).despawn();
        }
        return;
    }
    let mut existing = HashSet::new();
    for (entity, marker) in &markers {
        if marker.scenario != scenario.0
            || marker.index < progress.next
            || marker.index >= level.route.len()
        {
            commands.entity(entity).despawn();
        } else {
            existing.insert(marker.index);
        }
    }
    for index in progress.next..level.route.len() {
        if existing.contains(&index) {
            continue;
        }
        commands.spawn((
            RouteMarker {
                scenario: scenario.0,
                index,
            },
            Text2d::new(index.to_string()),
            TextFont {
                font_size: FontSize::Px((16.0 + index as f32 * 1.8).min(30.0)),
                ..default()
            },
            TextColor(game_palette::color(game_palette::ROUTE_LABEL)),
            Anchor::CENTER,
            Transform::from_translation(level.route[index].extend(0.35)),
        ));
    }
}
