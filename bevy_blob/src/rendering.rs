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

mod blob_scene;
mod ink;
mod ink_backdrop;
mod ink_geometry;
mod ink_scene;
mod route;
pub(super) use blob_scene::sync_blob_meshes;
use ink::ink_atmosphere_tint;
pub(crate) use ink::{
    sync_counterbalance_visuals, sync_ink_atmosphere, toggle_foreground, toggle_ink_style,
};
use ink_backdrop::{spawn_ink_backdrop, supports_ink_background};
use ink_geometry::spawn_ink_level_geometry;
pub(super) use ink_scene::sync_ink_preview;
pub(super) use route::sync_route_markers;

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
mod world;
pub(super) use ambient::{
    setup_ambient_drop_assets, simulate_ambient_drops, simulate_wastewater,
    simulate_wastewater_bubbles, simulate_wastewater_impacts, trigger_drop_shower,
};
#[cfg(test)]
pub(super) use body::create_blob_mesh;
use body::{create_blob_mesh_with_load, update_blob_mesh_with_load};
pub(super) use contour::charge_indicator_radius;
use contour::{RenderedBlobContour, RenderedMembranePoint, rendered_membrane_points};
use membrane::{MembraneRenderContext, update_blob_outline_mesh};
#[cfg(test)]
pub(super) use palette::blob_family_color;
#[cfg(test)]
pub(super) use palette::blob_fill_color;
pub(crate) use palette::light_dynamic_rgba;
use palette::{blob_outline_color, blob_vertex_light, blob_vital_color, scenery_vertex_light};
use vacuoles::update_blob_vacuole_mesh;
#[cfg(test)]
use vacuoles::vacuole_tint;
pub(super) use world::draw_world;

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
            MembraneRenderContext {
                blob: &blob,
                load: None,
                selected: true,
                parent_id: None,
                vitality: Vitality::default(),
                lights: &[],
                blob_id: 0,
                shield_extension: 0.0,
                shield_energy: 1.0,
                platforms: &[],
            },
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
