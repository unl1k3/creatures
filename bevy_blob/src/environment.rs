use super::*;
use crate::BlobSoundEvent;
use crate::blob::Particle;
use crate::level_format::{
    ChainDefinition, CounterbalanceDefinition, ExpulsionPointDefinition, HazardDefinition,
    LightDefinition, NutrientDefinition, ParsedLevel, SafetyBoundsDefinition, VisualLayer,
    WastewaterAreaDefinition, parse_level,
};
#[cfg(feature = "dev-tools")]
use crate::nutrition::{NutrientPhysics, NutritionWorld, spawn_nutrient_bodies};
use avian2d::prelude::{
    Collider, CollisionLayers, PhysicsLayer, RigidBody, ShapeCastConfig, SpatialQuery,
    SpatialQueryFilter,
};

mod artwork;
mod avian;
mod chains;
mod colliders;
mod contacts;
mod counterbalance;
mod debug;
mod levels;
mod runtime;
mod wastewater;
#[cfg(feature = "dev-tools")]
use chains::LevelChain;

use artwork::spawn_level_artwork;
pub(crate) use artwork::update_parallax_layers;
#[cfg(test)]
use avian::AvianMembraneContact;
pub(super) use avian::{
    AvianContactDiagnostics, AvianContactManifolds, resolve_avian_environment,
    sample_avian_contacts,
};
pub(super) use chains::{
    draw_level_chains, resolve_blob_chain_contacts, spawn_level_chains, sync_chain_lighting,
};
use colliders::spawn_level_colliders;
use contacts::{
    contact_point_is_shared, impact_from_patch, point_near_platform, project_particle,
    resolve_swept, stable_inside,
};
#[cfg(test)]
use contacts::{
    contact_point_is_shared as point_on_shared_fixture_edge,
    impact_from_patch as contact_patch_impact,
    project_particle_for_test as resolve_particle_projection,
    resolve_swept as resolve_swept_particle, stable_inside as stable_inside_surface,
};
pub(super) use counterbalance::simulate_counterbalances;
use counterbalance::{CounterbalanceGate, CounterbalancePlate};
pub(super) use debug::toggle_level_debug;
#[cfg(test)]
use levels::platform;
#[cfg(feature = "dev-tools")]
pub(super) use runtime::switch_test_scenario;
pub(super) use runtime::{advance_route_progress, setup_environment, simulate_level_hazards};

#[derive(PhysicsLayer, Clone, Copy, Debug, Default)]
pub(super) enum GameLayer {
    #[default]
    Environment,
    LivingBlob,
    Corpse,
    Projectile,
}

#[derive(Component, Debug)]
pub(super) struct EnvironmentCollider {
    platform_index: Option<usize>,
    fixture_index: Option<usize>,
}

#[derive(Component, Debug)]
pub(super) struct AvianMigratedSurface;

#[derive(Component)]
pub(super) struct LevelArtwork;

#[derive(Component)]
pub(super) struct ForegroundArtwork;

/// Artwork offset from its authored world position to create parallax while
/// leaving colliders and world-aligned foreground art untouched.
#[derive(Component)]
pub(super) struct ParallaxLayer {
    origin: Vec3,
    factor: f32,
}

impl ParallaxLayer {
    pub(super) fn new(origin: Vec3, factor: f32) -> Self {
        Self { origin, factor }
    }
}

#[derive(Resource)]
pub(super) struct Level {
    _name: String,
    size: Vec2,
    center: Vec2,
    pub(super) safety_bounds: Option<SafetyBoundsDefinition>,
    pub(super) platforms: Vec<Platform>,
    pub(super) fixtures: Vec<Vec<Vec2>>,
    pub(super) spawn_position: Vec2,
    pub(super) route: Vec<Vec2>,
    visual_layers: Vec<VisualLayer>,
    pub(super) ice_platforms: Vec<usize>,
    pub(super) glue_platforms: Vec<usize>,
    decorations: Vec<VisualLayer>,
    pub(super) wastewater_areas: Vec<WastewaterAreaDefinition>,
    pub(super) nutrients: Vec<NutrientDefinition>,
    pub(super) lights: Vec<LightDefinition>,
    pub(super) expulsion_points: Vec<ExpulsionPointDefinition>,
    pub(super) hazards: Vec<HazardDefinition>,
    pub(super) chains: Vec<ChainDefinition>,
    pub(super) counterbalances: Vec<CounterbalanceDefinition>,
}

#[derive(Resource, Default)]
pub(super) struct TestScenario(pub(super) u8);

#[derive(Resource, Default)]
pub(super) struct LevelDebugOverlay {
    pub(super) visible: bool,
    pub(super) camera_detached: bool,
}

#[derive(Resource, Default)]
pub(super) struct RouteProgress {
    pub(super) next: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WastewaterImpact {
    pub(super) area_index: usize,
    pub(super) position: Vec2,
    pub(super) source_radius: f32,
    pub(super) variation: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WastewaterRipple {
    pub(super) area_index: usize,
    pub(super) center_x: f32,
    pub(super) age: f32,
    pub(super) duration: f32,
    pub(super) amplitude: f32,
}

#[derive(Resource, Default)]
pub(super) struct WastewaterEffects {
    pub(super) pending: Vec<WastewaterImpact>,
    pub(super) ripples: Vec<WastewaterRipple>,
    variation_serial: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_corner_contact_is_not_treated_as_full_body_impact() {
        assert_eq!(contact_patch_impact(&mut [1_000.0]), 680.0);
        assert!(contact_patch_impact(&mut [1_000.0, 900.0, 800.0]) > 900.0);
    }

    #[test]
    fn shared_fixture_edge_is_detected_as_internal() {
        let fixtures = vec![
            vec![
                Vec2::new(-10.0, -10.0),
                Vec2::new(0.0, -10.0),
                Vec2::new(0.0, 0.0),
                Vec2::new(-10.0, 0.0),
            ],
            vec![
                Vec2::new(0.0, -10.0),
                Vec2::new(10.0, -10.0),
                Vec2::new(10.0, 0.0),
                Vec2::new(0.0, 0.0),
            ],
        ];

        assert!(point_on_shared_fixture_edge(
            Vec2::new(0.0, -5.0),
            0,
            &fixtures
        ));
        assert!(!point_on_shared_fixture_edge(
            Vec2::new(-5.0, 0.0),
            0,
            &fixtures
        ));
    }

    #[test]
    fn prototype_loads_authored_objects_without_local_lanterns() {
        let level = Level::prototype();
        assert_eq!(level.nutrients.len(), 3);
        assert!(level.lights.is_empty());
        assert_eq!(level.expulsion_points.len(), 1);
        assert_eq!(level.hazards.len(), 1);
        assert_eq!(level.decorations.len(), 1);
    }

    #[test]
    fn every_test_route_uses_conservative_jump_gaps() {
        for scenario in 1..=6 {
            let (level, _) = Level::test_scenario(scenario);
            assert!(level.route.len() >= 2);
            for pair in level.route.windows(2) {
                let delta = pair[1] - pair[0];
                assert!(
                    delta.y <= 240.0,
                    "scenario {scenario} requires an excessive rise of {}",
                    delta.y
                );
                assert!(
                    delta.x.abs() <= 260.0,
                    "scenario {scenario} requires an excessive horizontal gap of {}",
                    delta.x.abs()
                );
                assert!(
                    route_segment_has_clear_arc(pair[0], pair[1], &level),
                    "scenario {scenario} has no clear blob-sized route from {:?} to {:?}",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    fn route_segment_has_clear_arc(start: Vec2, end: Vec2, level: &Level) -> bool {
        const CLEARANCE: f32 = 39.0;
        [30.0, 45.0, 65.0, 110.0, 160.0, 215.0, 260.0]
            .into_iter()
            .any(|arc_height| {
                (3..=14).all(|step| {
                    let t = step as f32 / 20.0;
                    let point = start.lerp(end, t) + Vec2::Y * arc_height * 4.0 * t * (1.0 - t);
                    !level
                        .platforms
                        .iter()
                        .any(|platform| point_inside_expanded_platform(point, *platform, CLEARANCE))
                        && !level.fixtures.iter().any(|vertices| {
                            point_inside_or_near_polygon(point, vertices, CLEARANCE)
                        })
                })
            })
    }

    fn point_inside_expanded_platform(point: Vec2, platform: Platform, clearance: f32) -> bool {
        let extent = platform.half_size + Vec2::splat(clearance);
        let delta = (point - platform.center).abs();
        delta.x < extent.x && delta.y < extent.y
    }

    fn point_inside_or_near_polygon(point: Vec2, vertices: &[Vec2], clearance: f32) -> bool {
        if vertices.len() < 3 {
            return false;
        }
        let inside = vertices
            .iter()
            .zip(vertices.iter().cycle().skip(1))
            .take(vertices.len())
            .fold(None, |sign: Option<f32>, (first, second)| {
                let cross = (*second - *first).perp_dot(point - *first);
                match sign {
                    None if cross.abs() > 0.001 => Some(cross.signum()),
                    Some(previous) if cross.signum() != previous && cross.abs() > 0.001 => {
                        Some(0.0)
                    }
                    value => value,
                }
            })
            .is_some_and(|sign| sign != 0.0);
        inside
            || vertices
                .iter()
                .zip(vertices.iter().cycle().skip(1))
                .take(vertices.len())
                .any(|(first, second)| {
                    let edge = *second - *first;
                    let t = ((point - *first).dot(edge) / edge.length_squared().max(0.001))
                        .clamp(0.0, 1.0);
                    point.distance(*first + edge * t) < clearance
                })
    }

    #[test]
    fn every_platform_gets_one_static_avian_collider_at_the_same_position() {
        let mut app = App::new();
        app.add_systems(Startup, setup_environment);
        app.update();

        let expected = Level::prototype();
        let mut query = app
            .world_mut()
            .query::<(&EnvironmentCollider, &RigidBody, &Transform)>();
        let colliders = query.iter(app.world()).collect::<Vec<_>>();
        let platform_colliders = colliders
            .into_iter()
            .filter(|(environment, _, _)| environment.platform_index.is_some())
            .collect::<Vec<_>>();
        assert_eq!(platform_colliders.len(), expected.platforms.len());
        for (environment, body, transform) in platform_colliders {
            assert_eq!(*body, RigidBody::Static);
            let platform_index = environment.platform_index.expect("platform collider");
            assert_eq!(
                transform.translation.truncate(),
                expected.platforms[platform_index].center
            );
        }
        let pilot_count = app
            .world_mut()
            .query_filtered::<Entity, With<AvianMigratedSurface>>()
            .iter(app.world())
            .count();
        assert_eq!(pilot_count, 6);
    }

    #[test]
    fn digit_zero_toggles_level_debug_overlay() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<LevelDebugOverlay>()
            .add_systems(Update, toggle_level_debug);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit0);

        app.update();

        let overlay = app.world().resource::<LevelDebugOverlay>();
        assert!(overlay.visible);
        assert!(overlay.camera_detached);
    }

    #[test]
    fn legacy_contact_probe_matches_rectangle_distance() {
        let platform = platform(10.0, 20.0, 100.0, 20.0);
        assert!(point_near_platform(Vec2::new(10.0, 34.0), 4.0, &platform));
        assert!(!point_near_platform(Vec2::new(10.0, 35.0), 4.0, &platform));
        assert!(point_near_platform(Vec2::new(60.0, 30.0), 0.0, &platform));
    }

    #[test]
    fn membrane_contact_keeps_particle_and_surface_geometry() {
        let contact = AvianMembraneContact {
            particle_index: 7,
            collider: Entity::PLACEHOLDER,
            point: Vec2::new(4.0, 8.0),
            normal: Vec2::Y,
            depth: 2.5,
        };
        assert_eq!(contact.particle_index, 7);
        assert_eq!(contact.point, Vec2::new(4.0, 8.0));
        assert_eq!(contact.normal, Vec2::Y);
        assert_eq!(contact.depth, 2.5);
    }

    #[test]
    fn pilot_projection_removes_inward_velocity_without_bounce() {
        let mut particle = Particle {
            position: Vec2::new(0.0, -2.0),
            previous: Vec2::new(0.0, 6.0),
        };
        let contact = resolve_particle_projection(&mut particle, Vec2::ZERO, true, 3.0).unwrap();
        assert_eq!(contact.normal, Vec2::Y);
        assert_eq!(particle.position, Vec2::new(0.0, 3.0));
        assert!(particle.position.y - particle.previous.y >= 0.0);
        assert_eq!(contact.impact_displacement, 8.0);
    }

    #[test]
    fn swept_contact_preserves_the_face_hit_from_below() {
        let mut particle = Particle {
            previous: Vec2::new(0.0, -20.0),
            position: Vec2::new(0.0, 20.0),
        };
        let contact = resolve_swept_particle(&mut particle, Vec2::ZERO, Vec2::NEG_Y, 3.0);
        assert_eq!(contact.normal, Vec2::NEG_Y);
        assert_eq!(particle.position, Vec2::new(0.0, -3.0));
        assert!(particle.position.y - particle.previous.y <= 0.0);
        assert_eq!(contact.impact_displacement, 40.0);
    }

    #[test]
    fn swept_contact_preserves_a_lateral_face() {
        let mut particle = Particle {
            previous: Vec2::new(-20.0, 0.0),
            position: Vec2::new(20.0, 0.0),
        };
        let contact = resolve_swept_particle(&mut particle, Vec2::ZERO, Vec2::NEG_X, 3.0);
        assert_eq!(contact.normal, Vec2::NEG_X);
        assert_eq!(particle.position, Vec2::new(-3.0, 0.0));
        assert!(particle.position.x - particle.previous.x <= 0.0);
    }

    #[test]
    fn embedded_point_uses_the_surface_facing_the_blob_center() {
        let platform = platform(0.0, 0.0, 100.0, 20.0);
        let (top_point, top_normal) =
            stable_inside_surface(Vec2::new(12.0, 0.0), Vec2::new(0.0, 40.0), platform);
        assert_eq!(top_point, Vec2::new(12.0, 10.0));
        assert_eq!(top_normal, Vec2::Y);

        let (bottom_point, bottom_normal) =
            stable_inside_surface(Vec2::new(-8.0, 0.0), Vec2::new(0.0, -40.0), platform);
        assert_eq!(bottom_point, Vec2::new(-8.0, -10.0));
        assert_eq!(bottom_normal, Vec2::NEG_Y);
    }
}
