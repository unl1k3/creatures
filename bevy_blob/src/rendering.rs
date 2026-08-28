use super::*;
use crate::environment::{ForegroundArtwork, LevelArtwork};
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

pub(super) fn toggle_ink_style(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut ink_style: ResMut<InkStylePreview>,
    mut clear_color: ResMut<ClearColor>,
    mut artwork: Query<&mut Visibility, With<LevelArtwork>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyM) {
        return;
    }
    ink_style.enabled = !ink_style.enabled;
    clear_color.0 = if ink_style.enabled {
        game_palette::color(game_palette::IVORY)
    } else {
        game_palette::color(game_palette::NIGHT)
    };
    for mut visibility in &mut artwork {
        *visibility = if ink_style.enabled {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

/// G toggles only decorative foreground artwork, useful while testing
/// collisions without obscuring the playable scene.
pub(super) fn toggle_foreground(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut foreground: Query<&mut Visibility, Or<(With<ForegroundArtwork>, With<InkForeground>)>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyG) {
        return;
    }
    for mut visibility in &mut foreground {
        *visibility = match *visibility {
            Visibility::Hidden => Visibility::Inherited,
            _ => Visibility::Hidden,
        };
    }
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

    // The illustration supplies depth and atmosphere only. Playable silhouettes
    // are generated below from the same geometry used by collision detection.
    if supports_ink_background(scenario.0) {
        commands.spawn((
            InkPreviewShape {
                scenario: scenario.0,
            },
            Sprite {
                image: asset_server.load("levels/sewer_01/art/ink/background.png"),
                custom_size: Some(level.size()),
                ..default()
            },
            Transform::from_translation(level.center().extend(-20.0)),
        ));
        commands.spawn((
            InkPreviewShape {
                scenario: scenario.0,
            },
            InkForeground,
            Sprite {
                image: asset_server.load("levels/sewer_01/art/ink/foreground.png"),
                custom_size: Some(level.size()),
                ..default()
            },
            // Foreground pipes and debris are an occlusion layer: gameplay
            // structures remain visible through their central opening, while
            // overlapping portions correctly pass behind this artwork.
            Transform::from_translation(level.center().extend(2.5)),
        ));
    }

    let ink = game_palette::color(game_palette::INK);
    // Platform geometry is shared by every level, so the paper-and-ink tile
    // is shared too: laboratory and regression scenarios stay visually
    // comparable with the playable sewer scene.
    let brick_texture = asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                // Ink lines are intentionally high contrast. Nearest sampling
                // keeps a line black rather than blending it to a flickering
                // grey while the camera follows the blob.
                mag_filter: ImageFilterMode::Nearest,
                min_filter: ImageFilterMode::Nearest,
                ..default()
            });
        })
        .load("levels/sewer_01/art/ink/platform-bricks.png");
    for platform in &level.platforms {
        spawn_ink_platform(
            &mut commands,
            &mut meshes,
            &mut materials,
            scenario.0,
            platform,
            ink,
            &brick_texture,
        );
    }
    let fixture_material = materials.add(ColorMaterial {
        texture: Some(brick_texture.clone()),
        ..default()
    });
    for fixture in &level.fixtures {
        if fixture.len() < 3 {
            continue;
        }
        // The physics uses these authored vertices. Grow only the visible
        // contour by the blob's contact skin so the membrane meets the drawn
        // surface without changing Avian's stable collision geometry.
        let visual_fixture = offset_convex_polygon(fixture, PLATFORM_VISUAL_CONTACT_OFFSET);
        let positions = visual_fixture
            .iter()
            .map(|point| [point.x, point.y, 0.0])
            .collect::<Vec<_>>();
        // Fixture vertices are already in world space. Their UV coordinates
        // therefore share the same global origin as every rectangular slab.
        let texture_coordinates = visual_fixture
            .iter()
            .map(|point| {
                [
                    point.x / BRICK_TILE_WORLD_WIDTH,
                    point.y / BRICK_TILE_WORLD_HEIGHT,
                ]
            })
            .collect::<Vec<_>>();
        let mut indices = Vec::with_capacity((fixture.len() - 2) * 3);
        for index in 1..fixture.len() - 1 {
            indices.extend_from_slice(&[0, index as u32, index as u32 + 1]);
        }
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, texture_coordinates);
        mesh.insert_indices(Indices::U32(indices));
        commands.spawn((
            InkPreviewShape {
                scenario: scenario.0,
            },
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(fixture_material.clone()),
            Transform::from_xyz(0.0, 0.0, FIXTURE_TEXTURE_DEPTH),
        ));
    }
}

fn offset_convex_polygon(vertices: &[Vec2], distance: f32) -> Vec<Vec2> {
    if vertices.len() < 3 || distance <= 0.0 {
        return vertices.to_vec();
    }
    let signed_area = vertices
        .iter()
        .copied()
        .zip(vertices.iter().copied().cycle().skip(1))
        .take(vertices.len())
        .map(|(first, second)| first.perp_dot(second))
        .sum::<f32>();
    let orientation = signed_area.signum();
    if orientation == 0.0 {
        return vertices.to_vec();
    }

    (0..vertices.len())
        .map(|index| {
            let previous = vertices[(index + vertices.len() - 1) % vertices.len()];
            let current = vertices[index];
            let next = vertices[(index + 1) % vertices.len()];
            let previous_outward = -(current - previous).perp().normalize_or_zero() * orientation;
            let next_outward = -(next - current).perp().normalize_or_zero() * orientation;
            let bisector = (previous_outward + next_outward).normalize_or(previous_outward);
            let alignment = bisector.dot(previous_outward).abs().max(0.35);
            current + bisector * (distance / alignment).min(distance * 2.5)
        })
        .collect()
}

fn spawn_ink_platform(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    scenario: u8,
    platform: &Platform,
    ink: Color,
    brick_texture: &Handle<Image>,
) {
    // Level 1's ordinary horizontal platforms are 28 world units tall. The
    // artwork's 40-pixel tile is calibrated to their visible inner height so
    // it displays exactly its authored two rows there. All other rectangles
    // reuse this fixed scale instead of stretching the drawing.
    // The contact solver retains a small gap around the physical rectangle.
    // Grow only the artwork by that documented skin to hide the numerical
    // clearance while keeping the Avian collider untouched.
    let visual_center = platform.center;
    let visual_half_size = platform.half_size + Vec2::splat(PLATFORM_VISUAL_CONTACT_OFFSET);
    let size = visual_half_size * 2.0;
    commands.spawn((
        InkPreviewShape { scenario },
        Sprite::from_color(ink, size),
        // The foreground artwork is an intentional occlusion layer (z=2.5),
        // so structures are drawn beneath its pipes and corner debris.
        Transform::from_translation(visual_center.extend(2.20)),
    ));

    let inner_size = (size - Vec2::splat(3.2)).max(Vec2::splat(2.0));
    // Repeat the texture in UV space over a mesh that is exactly the size of
    // the rectangle. Mesh bounds clip incomplete edge bricks rather than
    // compressing them, and a thicker structure naturally exposes more rows.
    let brick_world_size = Vec2::new(BRICK_TILE_WORLD_WIDTH, BRICK_TILE_WORLD_HEIGHT);
    let texture_scale = inner_size / brick_world_size;
    let texture_origin = (visual_center - inner_size * 0.5) / brick_world_size;
    let texture_material = materials.add(ColorMaterial {
        texture: Some(brick_texture.clone()),
        // Anchoring UV zero in world space keeps the pattern continuous where
        // two independently-authored structures meet or overlap.
        uv_transform: Affine2::from_scale_angle_translation(texture_scale, 0.0, texture_origin),
        ..default()
    });
    commands.spawn((
        InkPreviewShape { scenario },
        Mesh2d(meshes.add(Rectangle::new(inner_size.x, inner_size.y))),
        MeshMaterial2d(texture_material),
        Transform::from_translation(visual_center.extend(PLATFORM_TEXTURE_DEPTH)),
    ));
}

// Scenario 0 is the startup instance of level 1; pressing 1 explicitly assigns 1.
fn supports_ink_background(scenario: u8) -> bool {
    matches!(scenario, 0 | 1)
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
mod membrane;
mod palette;
mod vacuoles;
pub(super) use ambient::{
    setup_ambient_drop_assets, simulate_ambient_drops, simulate_wastewater,
    simulate_wastewater_bubbles, simulate_wastewater_impacts,
};
#[cfg(test)]
pub(super) use body::create_blob_mesh;
use body::{create_blob_mesh_with_load, update_blob_mesh_with_load};
use membrane::update_blob_outline_mesh;
#[cfg(test)]
pub(super) use palette::blob_family_color;
#[cfg(test)]
pub(super) use palette::blob_fill_color;
use palette::{blob_outline_color, blob_vertex_light, blob_vital_color};
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

pub(super) fn sync_blob_meshes(
    mut commands: Commands,
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    time: Res<Time>,
    vitality_world: Res<VitalityWorld>,
    nutrition: Res<NutritionWorld>,
    shields: Res<ShieldWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut rendered: Query<
        (
            Entity,
            &mut BlobMesh,
            &Mesh2d,
            &MeshMaterial2d<ColorMaterial>,
        ),
        (With<BlobMesh>, Without<BlobOutlineMesh>),
    >,
    mut outlines: Query<
        (
            Entity,
            &mut BlobOutlineMesh,
            &Mesh2d,
            &MeshMaterial2d<ColorMaterial>,
        ),
        (With<BlobOutlineMesh>, Without<BlobMesh>),
    >,
    mut vacuoles: Query<
        (Entity, &BlobVacuoleMesh, &Mesh2d),
        (
            With<BlobVacuoleMesh>,
            Without<BlobMesh>,
            Without<BlobOutlineMesh>,
        ),
    >,
) {
    let active_ids = blobs
        .active
        .iter()
        .map(|blob| blob.id)
        .collect::<HashSet<_>>();
    let mut rendered_ids = HashSet::new();

    for (entity, mut marker, mesh_handle, material_handle) in &mut rendered {
        let Some(active_blob) = blobs.active.iter().find(|blob| blob.id == marker.blob_id) else {
            commands.entity(entity).despawn();
            continue;
        };
        rendered_ids.insert(marker.blob_id);

        if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
            update_blob_mesh_with_load(
                &mut mesh,
                &active_blob.body,
                nutrition.internal_load(active_blob.id),
                &level.lights,
            );
        }
        let selected = blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.id == active_blob.id);
        let vitality = vitality_world.get(active_blob.id);
        let energy_band = (vitality.energy * 20.0).round() as u8;
        if marker.parent_id != active_blob.parent_id
            || marker.selected != selected
            || marker.life_state != vitality.state
            || marker.energy_band != energy_band
        {
            marker.parent_id = active_blob.parent_id;
            marker.selected = selected;
            marker.life_state = vitality.state;
            marker.energy_band = energy_band;
            if let Some(mut material) = materials.get_mut(&material_handle.0) {
                material.color = blob_vital_color(active_blob.parent_id, selected, vitality);
            }
        }
    }

    for active_blob in blobs
        .active
        .iter()
        .filter(|blob| active_ids.contains(&blob.id) && !rendered_ids.contains(&blob.id))
    {
        let selected = blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.id == active_blob.id);
        let mesh = meshes.add(create_blob_mesh_with_load(
            &active_blob.body,
            nutrition.internal_load(active_blob.id),
            &level.lights,
        ));
        let vitality = vitality_world.get(active_blob.id);
        let material = materials.add(ColorMaterial::from(blob_vital_color(
            active_blob.parent_id,
            selected,
            vitality,
        )));
        commands.spawn((
            BlobMesh {
                blob_id: active_blob.id,
                parent_id: active_blob.parent_id,
                selected,
                life_state: vitality.state,
                energy_band: (vitality.energy * 20.0).round() as u8,
            },
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_xyz(0.0, 0.0, -0.1),
        ));
    }

    let mut outlined_ids = HashSet::new();
    for (entity, mut marker, mesh_handle, _material_handle) in &mut outlines {
        let Some(active_blob) = blobs.active.iter().find(|blob| blob.id == marker.blob_id) else {
            commands.entity(entity).despawn();
            continue;
        };
        outlined_ids.insert(marker.blob_id);
        let selected = blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.id == active_blob.id);
        let vitality = vitality_world.get(active_blob.id);
        if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
            update_blob_outline_mesh(
                &mut mesh,
                &active_blob.body,
                nutrition.internal_load(active_blob.id),
                selected,
                active_blob.parent_id,
                vitality,
                &level.lights,
                active_blob.id,
                shields.extension(active_blob.id),
                shields.energy(active_blob.id),
                &level.platforms,
            );
        }
        if marker.selected != selected || marker.life_state != vitality.state {
            marker.selected = selected;
            marker.life_state = vitality.state;
        }
    }
    for active_blob in blobs
        .active
        .iter()
        .filter(|blob| !outlined_ids.contains(&blob.id))
    {
        let selected = blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.id == active_blob.id);
        let vitality = vitality_world.get(active_blob.id);
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        update_blob_outline_mesh(
            &mut mesh,
            &active_blob.body,
            nutrition.internal_load(active_blob.id),
            selected,
            active_blob.parent_id,
            vitality,
            &level.lights,
            active_blob.id,
            shields.extension(active_blob.id),
            shields.energy(active_blob.id),
            &level.platforms,
        );
        commands.spawn((
            BlobOutlineMesh {
                blob_id: active_blob.id,
                selected,
                life_state: vitality.state,
            },
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(ColorMaterial::from(Color::WHITE))),
            Transform::from_xyz(0.0, 0.0, -0.08),
        ));
    }

    let elapsed = time.elapsed_secs();
    let mut vacuole_ids = HashSet::new();
    for (entity, marker, mesh_handle) in &mut vacuoles {
        let Some(active_blob) = blobs.active.iter().find(|blob| blob.id == marker.blob_id) else {
            commands.entity(entity).despawn();
            continue;
        };
        vacuole_ids.insert(marker.blob_id);
        if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
            update_blob_vacuole_mesh(
                &mut mesh,
                active_blob,
                elapsed,
                vitality_world.get(active_blob.id).is_alive(),
                &level.lights,
            );
        }
    }
    for active_blob in blobs
        .active
        .iter()
        .filter(|blob| !vacuole_ids.contains(&blob.id))
    {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        update_blob_vacuole_mesh(
            &mut mesh,
            active_blob,
            elapsed,
            vitality_world.get(active_blob.id).is_alive(),
            &level.lights,
        );
        commands.spawn((
            BlobVacuoleMesh {
                blob_id: active_blob.id,
            },
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(ColorMaterial::from(game_palette::color(
                game_palette::TRANSLUCENT_WHITE,
            )))),
            Transform::from_xyz(0.0, 0.0, -0.06),
        ));
    }
}

pub(super) fn charge_indicator_radius(blob: &Blob) -> f32 {
    let center = blob.center();
    let outermost = blob
        .particles
        .iter()
        .map(|particle| particle.position.distance(center))
        .fold(blob.rest_radius, f32::max);
    outermost + (5.0 * blob.size_scale()).max(2.5)
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

#[derive(Clone, Copy)]
struct RenderedMembranePoint {
    position: Vec2,
    temporary: bool,
    appendage: bool,
    attachment: bool,
}

/// Shared per-frame contour consumed by the body, membrane and shield renderers.
struct RenderedBlobContour {
    points: Vec<RenderedMembranePoint>,
    inward_normals: Vec<Vec2>,
}

impl RenderedBlobContour {
    fn new(blob: &Blob, load: Option<(Vec2, f32, f32, f32, usize, f32)>) -> Self {
        let points = rendered_membrane_points(blob, load);
        let center = blob.center();
        let count = points.len();
        let inward_normals = (0..count)
            .map(|index| {
                let previous = points[(index + count - 1) % count].position;
                let current = points[index].position;
                let next = points[(index + 1) % count].position;
                let first = (current - previous).perp().normalize_or_zero();
                let second = (next - current).perp().normalize_or_zero();
                (first + second).normalize_or((center - current).normalize_or(Vec2::Y))
            })
            .collect();
        Self {
            points,
            inward_normals,
        }
    }

    fn positions(&self) -> Vec<Vec2> {
        self.points.iter().map(|point| point.position).collect()
    }
}

fn rendered_membrane_points(
    blob: &Blob,
    load: Option<(Vec2, f32, f32, f32, usize, f32)>,
) -> Vec<RenderedMembranePoint> {
    let Some((load_position, load_radius, strength, variation, anchor_edge, anchor_t)) =
        load.filter(|(_, _, value, _, _, _)| *value > 0.01)
    else {
        return blob
            .particles
            .iter()
            .map(|particle| RenderedMembranePoint {
                position: particle.position,
                temporary: false,
                appendage: false,
                attachment: false,
            })
            .collect();
    };
    let count = blob.particles.len();
    let nearest_edge = anchor_edge % count;
    let center = blob.center();
    let load_direction = (load_position - center).normalize_or(Vec2::X);
    let mut points = Vec::with_capacity(count + 31);
    for index in 0..count {
        let second_anchor_edge = (nearest_edge + 1) % count;
        if index == second_anchor_edge {
            continue;
        }
        let start = blob.particles[index].position;
        let end_index = if index == nearest_edge {
            (index + 2) % count
        } else {
            (index + 1) % count
        };
        let end = blob.particles[end_index].position;
        points.push(RenderedMembranePoint {
            position: start,
            temporary: false,
            appendage: false,
            attachment: false,
        });
        if index != nearest_edge {
            continue;
        }
        let base = blob.particles[(nearest_edge + 1) % count].position;
        let tip = base.lerp(load_position, strength.clamp(0.0, 1.0));
        let length = base.distance(tip);
        let normal_axis = load_direction.perp();
        let secondary = (variation * 7.137).fract();
        let asymmetry = (anchor_t.clamp(0.0, 1.0) - 0.5) * 0.08;
        let start_attachment = start.lerp(base, 0.18 + asymmetry);
        let end_attachment = base.lerp(end, 0.82 + asymmetry);
        let attachment_tangent = (end_attachment - start_attachment).normalize_or(Vec2::X);
        let mut root_normal = attachment_tangent.perp();
        if root_normal.dot(base - center) < 0.0 {
            root_normal = -root_normal;
        }
        let control_a = base
            + root_normal * length * (0.30 + variation * 0.08)
            + attachment_tangent * length * (variation - 0.5) * 0.05;
        let control_b = base.lerp(tip, 0.72) + normal_axis * length * (secondary - 0.5) * 0.18;
        let width = (load_radius * (0.55 + strength * 0.45) * (0.88 + variation * 0.24))
            .min(start.distance(end) * 0.48)
            .max(0.5);
        let base_normal = root_normal.perp();
        // Follow the membrane's existing winding: the first side of the tube
        // must leave from `start`, otherwise the two base triangles cross.
        let winding_side = if (start - base).dot(base_normal) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let attachment_half_width = start_attachment.distance(end_attachment) * 0.5;
        points.push(RenderedMembranePoint {
            position: start_attachment,
            temporary: true,
            appendage: false,
            attachment: true,
        });
        const MINIMAL_PROFILE: &[f32] = &[0.24];
        const MEDIUM_PROFILE: &[f32] = &[0.20, 0.46, 0.70, 0.88, 0.975];
        const FULL_PROFILE: &[f32] = &[
            0.06, 0.14, 0.23, 0.32, 0.42, 0.52, 0.62, 0.71, 0.79, 0.86, 0.92, 0.97, 0.995,
        ];
        let profile = if strength < 0.16 {
            MINIMAL_PROFILE
        } else if strength < 0.38 {
            MEDIUM_PROFILE
        } else {
            FULL_PROFILE
        };
        let mut outline = Vec::with_capacity(profile.len() * 2 + 1);
        outline.extend(profile.iter().copied().map(|along| (along, 1.0)));
        outline.push((1.0, 0.0));
        outline.extend(profile.iter().rev().copied().map(|along| (along, -1.0)));
        for (along, side) in outline {
            let raw_side = side;
            let side = raw_side * winding_side;
            let inverse: f32 = 1.0 - along;
            let centerline = base * inverse.powi(3)
                + control_a * 3.0 * inverse.powi(2) * along
                + control_b * 3.0 * inverse * along.powi(2)
                + tip * along.powi(3);
            let tangent = ((control_a - base) * 3.0 * inverse.powi(2)
                + (control_b - control_a) * 6.0 * inverse * along
                + (tip - control_b) * 3.0 * along.powi(2))
            .normalize_or(load_direction);
            let normal = tangent.perp();
            let organic_wave = 1.0
                + (along * std::f32::consts::PI * (2.0 + variation * 1.4)
                    + variation * std::f32::consts::TAU)
                    .sin()
                    * (along * std::f32::consts::PI).sin()
                    * 0.075;
            let root_flare = 1.0 + (1.0 - along).powi(2) * (0.38 + variation * 0.12);
            let taper = (1.0_f32 - along * (0.76 + secondary * 0.10)).max(0.14);
            let rounded_tip = if along > 0.9 {
                ((1.0 - along) / 0.1).sqrt()
            } else {
                1.0
            };
            let profile_width = (width * root_flare * taper * organic_wave)
                .min(attachment_half_width * (1.0 - along * 0.58).max(0.18));
            let curved_position = centerline + normal * side * profile_width * rounded_tip;
            let attachment = if raw_side >= 0.0 {
                start_attachment
            } else {
                end_attachment
            };
            let root_blend = smoothstep01(along / 0.20);
            points.push(RenderedMembranePoint {
                position: attachment.lerp(curved_position, root_blend),
                temporary: true,
                appendage: true,
                attachment: false,
            });
        }
        points.push(RenderedMembranePoint {
            position: end_attachment,
            temporary: true,
            appendage: false,
            attachment: true,
        });
    }
    points
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
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
