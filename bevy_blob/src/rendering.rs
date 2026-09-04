use super::*;
use crate::environment::{ForegroundArtwork, LevelArtwork, ParallaxLayer};
use crate::level_format::LightDefinition;
use crate::palette as game_palette;
use crate::shield::shield_spine_fans;
use bevy::image::{
    ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor,
};
use bevy::math::Affine2;
use bevy::sprite::Anchor;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};
use std::collections::HashSet;

mod blob_scene;
mod ink;
mod ink_scene;
pub(super) use blob_scene::sync_blob_meshes;
use ink::ink_atmosphere_tint;
pub(crate) use ink::{
    sync_counterbalance_visuals, sync_ink_atmosphere, toggle_foreground, toggle_ink_style,
};
use ink_scene::{spawn_ink_backdrop, spawn_ink_level_geometry, supports_ink_background};

const PLATFORM_VISUAL_CONTACT_OFFSET: f32 = 5.0 * DEFAULT_CREATURE_SCALE;
const BRICK_TILE_PIXEL_WIDTH: f32 = 256.0;
const BRICK_TILE_PIXEL_HEIGHT: f32 = 40.0;
const REFERENCE_PLATFORM_HEIGHT: f32 = 28.0;
const BRICK_TILE_WORLD_SCALE: f32 =
    (REFERENCE_PLATFORM_HEIGHT + 2.0 * PLATFORM_VISUAL_CONTACT_OFFSET - 3.2)
        / BRICK_TILE_PIXEL_HEIGHT;
const BRICK_TILE_WORLD_WIDTH: f32 = BRICK_TILE_PIXEL_WIDTH * BRICK_TILE_WORLD_SCALE;
const BRICK_TILE_WORLD_HEIGHT: f32 = BRICK_TILE_PIXEL_HEIGHT * BRICK_TILE_WORLD_SCALE;
const PLATFORM_TEXTURE_DEPTH: f32 = 2.25;
// Overlapping authored slabs are intentional in several levels. A stable
// depth step prevents their coplanar texture meshes from flickering.
const PLATFORM_DEPTH_STEP: f32 = 0.001;
// Rounded caps and authored polygon fixtures can meet the visual skin of a
// rectangle. Draw them just above the slab texture to avoid coplanar overlap.
const FIXTURE_TEXTURE_DEPTH: f32 = PLATFORM_TEXTURE_DEPTH + 0.01;

#[derive(Resource)]
pub(super) struct InkStylePreview {
    pub(super) enabled: bool,
}

impl Default for InkStylePreview {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Component)]
pub(super) struct InkPreviewShape {
    scenario: u8,
}

#[derive(Component)]
pub(super) struct InkForeground;

/// Identifies the authored illustration layers whose tint changes with the
/// camera height. Gameplay meshes keep their own local lamp lighting.
#[derive(Component)]
pub(super) struct InkAtmosphereLayer {
    foreground: bool,
}

/// Purely decorative mist. It shares the slow background parallax but never
/// creates a collider or changes visibility of game entities.
#[derive(Component)]
struct InkFogBand;

/// Tags both visual layers of a platform that is moved by a counterbalance.
#[derive(Component)]
pub(super) struct CounterbalanceVisual {
    platform_index: usize,
}

pub(super) fn sync_ink_preview(
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

#[cfg(test)]
mod ink_preview_tests {
    use super::{InkStylePreview, supports_ink_background};

    #[test]
    fn ink_preview_is_the_default_rendering_mode() {
        assert!(InkStylePreview::default().enabled);
    }

    #[test]
    fn background_is_available_before_and_after_pressing_f1() {
        assert!(supports_ink_background(0));
        assert!(supports_ink_background(1));
        assert!(!supports_ink_background(2));
    }
}

mod ambient;
mod body;
mod contour;
mod membrane;
mod palette;
mod vacuoles;
pub(super) use ambient::{
    setup_ambient_drop_assets, simulate_ambient_drops, simulate_wastewater,
    simulate_wastewater_bubbles, simulate_wastewater_impacts, trigger_drop_shower,
};
#[cfg(test)]
pub(super) use body::create_blob_mesh;
use body::{create_blob_mesh_with_load, update_blob_mesh_with_load};
pub(super) use contour::charge_indicator_radius;
use contour::{RenderedBlobContour, RenderedMembranePoint, rendered_membrane_points};
use membrane::update_blob_outline_mesh;
#[cfg(test)]
pub(super) use palette::blob_family_color;
#[cfg(test)]
pub(super) use palette::blob_fill_color;
pub(crate) use palette::light_dynamic_rgba;
use palette::{blob_outline_color, blob_vertex_light, blob_vital_color, scenery_vertex_light};
use vacuoles::update_blob_vacuole_mesh;
#[cfg(test)]
use vacuoles::vacuole_tint;

#[derive(Component)]
pub(super) struct BlobMesh {
    blob_id: u64,
    parent_id: Option<u64>,
    selected: bool,
    life_state: LifeState,
    energy_band: u8,
}

#[derive(Component)]
pub(super) struct BlobOutlineMesh {
    blob_id: u64,
    selected: bool,
    life_state: LifeState,
}

#[derive(Component)]
pub(super) struct BlobVacuoleMesh {
    blob_id: u64,
}

#[derive(Component)]
pub(super) struct RouteMarker {
    scenario: u8,
    index: usize,
}

pub(super) fn sync_route_markers(
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

#[cfg(test)]
mod membrane_detail_tests {
    use super::*;

    #[test]
    fn internal_load_temporarily_adds_local_membrane_points() {
        let blob = Blob::new(Vec2::ZERO, 40.0);
        let normal_count = rendered_membrane_points(&blob, None).len();
        let detailed =
            rendered_membrane_points(&blob, Some((Vec2::new(20.0, 0.0), 12.0, 1.0, 0.37, 0, 0.5)));
        assert!(detailed.len() > normal_count);
        assert!(detailed.iter().any(|point| point.temporary));
        assert_eq!(detailed.iter().filter(|point| point.attachment).count(), 2);
        assert!(detailed.iter().any(|point| point.appendage));
        assert_eq!(rendered_membrane_points(&blob, None).len(), normal_count);
    }

    #[test]
    fn nascent_protrusion_is_fully_triangulated() {
        let blob = Blob::new(Vec2::ZERO, 40.0);
        for strength in [0.011, 0.025, 0.05, 0.10] {
            let load = Some((Vec2::new(72.0, 8.0), 5.0, strength, 0.61, 0, 0.5));
            let membrane = rendered_membrane_points(&blob, load);
            let mesh = create_blob_mesh_with_load(&blob, load, &[]);
            assert_eq!(
                mesh.indices().expect("mesh indices").len(),
                membrane.len() * 3,
                "incomplete triangulation at strength {strength}"
            );
        }
    }

    #[test]
    fn outline_is_a_closed_triangle_ring() {
        let blob = Blob::new(Vec2::ZERO, 40.0);
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        update_blob_outline_mesh(
            &mut mesh,
            &blob,
            None,
            true,
            None,
            Vitality::default(),
            &[],
            0,
            0.0,
            1.0,
            &[],
        );

        assert_eq!(
            mesh.indices().expect("outline indices").len(),
            blob.particles.len() * 12
        );
        assert_eq!(mesh.count_vertices(), blob.particles.len() * 3);
    }

    #[test]
    fn nearby_light_brightens_the_facing_membrane() {
        let light = LightDefinition {
            position: Vec2::new(20.0, 0.0),
            color: [1.0, 0.3, 0.1],
            radius: 100.0,
            intensity: 1.0,
            enabled: true,
        };
        let facing = blob_vertex_light(Vec2::ZERO, Vec2::X, &[light], false);
        let opposite = blob_vertex_light(Vec2::ZERO, Vec2::NEG_X, &[light], false);
        let outside = blob_vertex_light(Vec2::new(-100.0, 0.0), Vec2::X, &[light], false);

        assert!(facing[0] > opposite[0]);
        assert!(opposite[0] > outside[0]);
        assert!(facing[0] - facing[2] > opposite[0] - opposite[2]);
    }

    #[test]
    fn disabled_light_does_not_affect_the_blob() {
        let light = LightDefinition {
            position: Vec2::new(10.0, 0.0),
            color: [1.0, 1.0, 1.0],
            radius: 100.0,
            intensity: 2.0,
            enabled: false,
        };

        assert_eq!(
            blob_vertex_light(Vec2::ZERO, Vec2::X, &[light], false),
            blob_vertex_light(Vec2::ZERO, Vec2::X, &[], false)
        );
    }

    #[test]
    fn vacuole_palette_varies_with_index_and_blob_family() {
        assert_ne!(vacuole_tint(Some(2), 0), vacuole_tint(Some(2), 1));
        assert_ne!(vacuole_tint(Some(2), 1), vacuole_tint(Some(2), 2));
        assert_ne!(vacuole_tint(Some(2), 0), vacuole_tint(Some(3), 0));
    }

    #[test]
    fn dead_vacuoles_keep_their_mesh_allocation() {
        let active_blob = ActiveBlob {
            id: 7,
            parent_id: None,
            body: Blob::new(Vec2::ZERO, 30.0),
        };
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        update_blob_vacuole_mesh(&mut mesh, &active_blob, 1.0, true, &[]);
        let live_vertices = mesh.count_vertices();
        let live_indices = mesh.indices().expect("live vacuole indices").len();

        update_blob_vacuole_mesh(&mut mesh, &active_blob, 2.0, false, &[]);

        assert!(live_vertices > 0);
        assert_eq!(mesh.count_vertices(), live_vertices);
        assert_eq!(
            mesh.indices().expect("dead vacuole indices").len(),
            live_indices
        );
    }
}

/// Draws a thin, slightly twisted ink rope instead of a single debug line.
fn draw_ink_rope(gizmos: &mut Gizmos, start: Vec2, end: Vec2, ink: Color) {
    let span = end - start;
    let length = span.length();
    if length <= f32::EPSILON {
        return;
    }
    let direction = span / length;
    let normal = Vec2::new(-direction.y, direction.x);
    let edge = normal * 1.35;
    gizmos.line_2d(start + edge, end + edge, ink);
    gizmos.line_2d(start - edge, end - edge, ink);

    // Short alternating ties keep the cable organic while remaining readable
    // at the small in-game scale.
    let ties = (length / 18.0).floor() as usize;
    for index in 1..ties {
        let center = start + direction * (index as f32 * 18.0);
        let slant = if index % 2 == 0 {
            direction
        } else {
            -direction
        };
        gizmos.line_2d(
            center - edge - slant * 1.8,
            center + edge + slant * 1.8,
            ink,
        );
    }
}

pub(super) fn draw_world(
    mut gizmos: Gizmos,
    blobs: Res<BlobWorld>,
    vitality_world: Res<VitalityWorld>,
    level: Res<Level>,
    debug_overlay: Res<LevelDebugOverlay>,
    route_progress: Res<RouteProgress>,
    nutrition: Res<NutritionWorld>,
    ink_style: Res<InkStylePreview>,
) {
    // Counterbalances are rendered as ink mechanisms, not as debug volumes.
    for balance in &level.counterbalances {
        if let (Some(plate), Some(gate)) = (
            level.platforms.get(balance.plate_platform),
            level.platforms.get(balance.gate_platform),
        ) {
            // Fixed pulleys sit above the two moving ends. The cable sections
            // then make the equal and opposite travel of plate and gate clear.
            // Higher than the gate's fully open top edge, so the door never
            // visually crosses a pulley during its upward stroke.
            let pulley_height = 145.0;
            let left_pulley = Vec2::new(plate.center.x, pulley_height);
            let right_pulley = Vec2::new(gate.center.x, pulley_height);
            let plate_anchor = plate.center + Vec2::Y * plate.half_size.y;
            let gate_anchor = gate.center + Vec2::Y * gate.half_size.y;
            let ink_at = |position| {
                game_palette::color(light_dynamic_rgba(
                    game_palette::INK,
                    position,
                    &level.lights,
                ))
            };
            draw_ink_rope(
                &mut gizmos,
                plate_anchor,
                left_pulley,
                ink_at((plate_anchor + left_pulley) * 0.5),
            );
            draw_ink_rope(
                &mut gizmos,
                left_pulley,
                right_pulley,
                ink_at((left_pulley + right_pulley) * 0.5),
            );
            draw_ink_rope(
                &mut gizmos,
                right_pulley,
                gate_anchor,
                ink_at((right_pulley + gate_anchor) * 0.5),
            );
            for pulley in [left_pulley, right_pulley] {
                let cable = ink_at(pulley);
                // A small hanging bracket and two imperfect-looking rings
                // read as hand-drawn hardware over the ivory level artwork.
                gizmos.line_2d(pulley + Vec2::Y * 10.0, pulley + Vec2::Y * 23.0, cable);
                gizmos.line_2d(
                    pulley + Vec2::new(-8.0, 23.0),
                    pulley + Vec2::new(8.0, 23.0),
                    cable,
                );
                gizmos.circle_2d(pulley, 11.0, cable);
                gizmos.circle_2d(pulley, 4.0, cable);
                gizmos.line_2d(
                    pulley + Vec2::new(-7.0, -7.0),
                    pulley + Vec2::new(7.0, 7.0),
                    cable,
                );
                gizmos.line_2d(
                    pulley + Vec2::new(-7.0, 7.0),
                    pulley + Vec2::new(7.0, -7.0),
                    cable,
                );
            }
        }
    }
    // Laboratories without artwork retain their unobtrusive collision view.
    if !ink_style.enabled && !level.has_artwork() && !debug_overlay.visible {
        for platform in &level.platforms {
            gizmos.rect_2d(
                platform.center,
                platform.half_size * 2.0,
                game_palette::color(game_palette::LAB_PLATFORM),
            );
        }
        for fixture in &level.fixtures {
            gizmos.lineloop_2d(
                fixture.iter().copied(),
                game_palette::color(game_palette::LAB_FIXTURE),
            );
        }
    }
    if debug_overlay.visible {
        // Draw three close contours to remain readable over detailed artwork.
        for platform in &level.platforms {
            for expansion in [-3.0, 0.0, 3.0] {
                gizmos.rect_2d(
                    platform.center,
                    platform.half_size * 2.0 + Vec2::splat(expansion),
                    game_palette::color(game_palette::DEBUG_PLATFORM),
                );
            }
        }
        for fixture in &level.fixtures {
            for (first, second) in fixture
                .iter()
                .copied()
                .zip(fixture.iter().copied().cycle().skip(1))
                .take(fixture.len())
            {
                let normal = (second - first).perp().normalize_or_zero();
                for offset in [-2.0, 0.0, 2.0] {
                    gizmos.line_2d(
                        first + normal * offset,
                        second + normal * offset,
                        game_palette::color(game_palette::DEBUG_PLATFORM),
                    );
                }
            }
        }
        for expansion in [-3.0, 0.0, 3.0] {
            gizmos.rect_2d(
                level.center(),
                level.size() + Vec2::splat(expansion),
                game_palette::color(game_palette::DEBUG_BOUNDS),
            );
        }
        let marker_size = 14.0;
        for offset in [-2.0, 0.0, 2.0] {
            gizmos.line_2d(
                level.spawn_position + Vec2::new(-marker_size, offset),
                level.spawn_position + Vec2::new(marker_size, offset),
                game_palette::color(game_palette::DEBUG_SPAWN),
            );
            gizmos.line_2d(
                level.spawn_position + Vec2::new(offset, -marker_size),
                level.spawn_position + Vec2::new(offset, marker_size),
                game_palette::color(game_palette::DEBUG_SPAWN),
            );
        }
        for light in level.lights.iter().filter(|light| light.enabled) {
            let color = Color::srgba(
                light.color[0],
                light.color[1],
                light.color[2],
                (0.35 + light.intensity * 0.18).clamp(0.35, 0.9),
            );
            gizmos.circle_2d(light.position, light.radius, color);
            gizmos.circle_2d(light.position, 5.0, color);
        }
        for point in &level.expulsion_points {
            let length = (point.strength * 0.12).clamp(20.0, 80.0);
            let end = point.position + point.direction * length;
            gizmos.arrow_2d(
                point.position,
                end,
                game_palette::color(game_palette::DEBUG_EXPULSION),
            );
        }
        for hazard in &level.hazards {
            for expansion in [-2.0, 0.0, 2.0] {
                gizmos.rect_2d(
                    hazard.position,
                    hazard.size + Vec2::splat(expansion),
                    game_palette::color(game_palette::DEBUG_HAZARD),
                );
            }
        }
    }
    if debug_overlay.visible {
        for (index, checkpoint) in level.route.iter().enumerate().skip(route_progress.next) {
            let radius = (7.0 + index as f32 * 1.5).min(20.0);
            gizmos.circle_2d(
                *checkpoint,
                radius,
                game_palette::color(game_palette::DEBUG_ROUTE),
            );
        }
    }
    if !debug_overlay.visible {
        for hazard in &level.hazards {
            let left = hazard.position.x - hazard.size.x * 0.5;
            // Hazard volumes grow upward from their supporting surface.
            let surface_y = hazard.position.y - hazard.size.y * 0.5;
            let surface = (0..=12).map(|step| {
                let fraction = step as f32 / 12.0;
                Vec2::new(
                    left + hazard.size.x * fraction,
                    surface_y + (fraction * std::f32::consts::TAU * 2.0).sin() * 2.4,
                )
            });
            gizmos.linestrip_2d(surface, game_palette::color(game_palette::HAZARD_SURFACE));
            for offset in [0.2, 0.5, 0.78] {
                gizmos.circle_2d(
                    Vec2::new(left + hazard.size.x * offset, surface_y - 5.0),
                    2.5,
                    game_palette::color(game_palette::HAZARD_BUBBLE),
                );
            }
        }
    }

    for active_blob in &blobs.active {
        let blob = &active_blob.body;
        let vitality = vitality_world.get(active_blob.id);
        let center = blob.center();
        let size_scale = blob.size_scale();
        if debug_overlay.visible {
            let membrane = rendered_membrane_points(blob, nutrition.internal_load(active_blob.id));
            for point in membrane.iter().filter(|point| point.temporary) {
                let radius = if point.attachment { 2.0 } else { 1.35 };
                let point_color = if point.attachment {
                    game_palette::color(game_palette::DEBUG_ATTACHMENT)
                } else {
                    game_palette::color(game_palette::DEBUG_PARTICLE)
                };
                gizmos.circle_2d(point.position, (radius * size_scale).max(0.72), point_color);
            }
            for particle in &blob.particles {
                gizmos.line_2d(
                    center,
                    particle.position,
                    game_palette::color(game_palette::DEBUG_SPRING),
                );
            }
            if vitality.is_alive() {
                gizmos.circle_2d(
                    center,
                    9.0 * size_scale,
                    game_palette::color(game_palette::DEBUG_CENTER),
                );
            }
        }

        if vitality.is_alive() && blob.charge > 0.0 {
            let radius = charge_indicator_radius(blob);
            let line_spacing = (1.8 * size_scale).max(0.9);
            gizmos.circle_2d(
                center,
                radius,
                game_palette::color(game_palette::CHARGE_GLOW),
            );
            for offset in [-line_spacing, 0.0, line_spacing] {
                gizmos.arc_2d(
                    center,
                    std::f32::consts::TAU * blob.charge,
                    radius + offset,
                    game_palette::color(game_palette::CHARGE_ARC),
                );
            }
        }
    }
}
