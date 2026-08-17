use super::*;
use crate::blob::Particle;
use avian2d::prelude::{
    Collider, CollisionLayers, PhysicsLayer, RigidBody, SpatialQuery, SpatialQueryFilter,
};
use std::collections::{HashMap, HashSet};

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
}

#[derive(Component, Debug)]
pub(super) struct AvianMigratedSurface;

#[derive(Resource)]
pub(super) struct Level {
    pub(super) platforms: Vec<Platform>,
    pub(super) fixtures: Vec<Vec<Vec2>>,
    pub(super) spawn_position: Vec2,
    pub(super) route: Vec<Vec2>,
}

#[derive(Resource, Default)]
pub(super) struct TestScenario(pub(super) u8);

#[derive(Resource, Default)]
pub(super) struct RouteProgress {
    pub(super) next: usize,
}

#[derive(Resource, Default)]
pub(super) struct AvianContactDiagnostics {
    pub(super) particles: usize,
    pub(super) avian_contacts: usize,
    pub(super) legacy_contacts: usize,
    pub(super) agreement: f32,
    pub(super) selected_surfaces: usize,
    pub(super) selected_particles: usize,
    pub(super) selected_ground_contacts: usize,
    pub(super) selected_max_depth: f32,
    pub(super) selected_contact_span: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AvianMembraneContact {
    pub(super) particle_index: usize,
    pub(super) collider: Entity,
    pub(super) point: Vec2,
    pub(super) normal: Vec2,
    pub(super) depth: f32,
}

#[derive(Resource, Default)]
pub(super) struct AvianContactManifolds {
    pub(super) by_blob: HashMap<u64, Vec<AvianMembraneContact>>,
}

impl Level {
    fn prototype() -> Self {
        Self {
            platforms: vec![
                platform(0.0, -370.0, 660.0, 38.0),
                platform(-250.0, -150.0, 260.0, 28.0),
                platform(210.0, 65.0, 300.0, 28.0),
                platform(-180.0, 290.0, 260.0, 28.0),
                platform(210.0, 510.0, 280.0, 28.0),
                platform(-170.0, 735.0, 300.0, 28.0),
            ],
            fixtures: Vec::new(),
            spawn_position: BLOB_START,
            route: Vec::new(),
        }
    }

    fn test_scenario(index: u8) -> (Self, Vec2) {
        match index {
            2 => (
                Self {
                    platforms: vec![
                        platform(0.0, -370.0, 760.0, 38.0),
                        platform(-245.0, -265.0, 70.0, 170.0),
                        platform(-105.0, -315.0, 105.0, 70.0),
                        platform(10.0, -270.0, 105.0, 160.0),
                        platform(170.0, -225.0, 105.0, 250.0),
                        platform(295.0, 55.0, 120.0, 28.0),
                    ],
                    fixtures: Vec::new(),
                    spawn_position: Vec2::new(-320.0, -285.0),
                    route: vec![
                        Vec2::new(-320.0, -285.0),
                        Vec2::new(-245.0, -140.0),
                        Vec2::new(-105.0, -240.0),
                        Vec2::new(10.0, -150.0),
                        Vec2::new(170.0, -60.0),
                        Vec2::new(295.0, 110.0),
                    ],
                },
                Vec2::new(-320.0, -285.0),
            ),
            3 => (
                Self {
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(350.0, 0.0, 105.0, 24.0),
                        platform(470.0, 145.0, 80.0, 24.0),
                    ],
                    fixtures: {
                        let mut fixtures = vec![vec![
                            Vec2::new(-340.0, -370.0),
                            Vec2::new(80.0, -370.0),
                            Vec2::new(80.0, -280.0),
                        ]];
                        fixtures.push(semicircle_fixture(Vec2::new(220.0, -250.0), 105.0, 28.0));
                        fixtures.extend(wave_fixtures(-330.0, 330.0, 285.0, 220.0, 9));
                        fixtures
                    },
                    spawn_position: Vec2::new(-300.0, -285.0),
                    route: vec![
                        Vec2::new(-300.0, -285.0),
                        Vec2::new(-150.0, -270.0),
                        Vec2::new(20.0, -220.0),
                        Vec2::new(220.0, -105.0),
                        Vec2::new(350.0, 55.0),
                        Vec2::new(470.0, 200.0),
                        Vec2::new(320.0, 330.0),
                        Vec2::new(120.0, 330.0),
                        Vec2::new(-80.0, 330.0),
                        Vec2::new(-260.0, 320.0),
                    ],
                },
                Vec2::new(-300.0, -285.0),
            ),
            4 => (
                Self {
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(-210.0, -285.0, 28.0, 190.0),
                        platform(10.0, -285.0, 28.0, 190.0),
                        platform(-100.0, -365.0, 248.0, 28.0),
                        platform(235.0, -250.0, 250.0, 28.0),
                    ],
                    fixtures: Vec::new(),
                    spawn_position: Vec2::new(-100.0, -245.0),
                    route: vec![
                        Vec2::new(-100.0, -245.0),
                        Vec2::new(-25.0, -145.0),
                        Vec2::new(80.0, -310.0),
                        Vec2::new(220.0, -310.0),
                        Vec2::new(355.0, -310.0),
                    ],
                },
                Vec2::new(-100.0, -245.0),
            ),
            5 => (
                Self {
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(-185.0, -245.0, 125.0, 24.0),
                        platform(20.0, -105.0, 105.0, 24.0),
                        platform(245.0, 45.0, 105.0, 24.0),
                        platform(20.0, 185.0, 115.0, 24.0),
                        platform(-220.0, 335.0, 95.0, 24.0),
                        platform(-40.0, 475.0, 110.0, 24.0),
                        platform(130.0, 600.0, 120.0, 24.0),
                        platform(245.0, 470.0, 26.0, 260.0),
                        platform(365.0, 470.0, 26.0, 260.0),
                    ],
                    fixtures: Vec::new(),
                    spawn_position: Vec2::new(-300.0, -285.0),
                    route: vec![
                        Vec2::new(-300.0, -285.0),
                        Vec2::new(-185.0, -190.0),
                        Vec2::new(20.0, -50.0),
                        Vec2::new(245.0, 100.0),
                        Vec2::new(20.0, 240.0),
                        Vec2::new(-220.0, 390.0),
                        Vec2::new(-40.0, 530.0),
                        Vec2::new(130.0, 655.0),
                        Vec2::new(305.0, 650.0),
                    ],
                },
                Vec2::new(-300.0, -285.0),
            ),
            6 => (
                Self {
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(270.0, -40.0, 105.0, 24.0),
                        platform(155.0, 115.0, 130.0, 24.0),
                        platform(-45.0, 115.0, 130.0, 24.0),
                    ],
                    fixtures: v_valley_fixtures(Vec2::new(0.0, -180.0), 300.0, 120.0),
                    spawn_position: Vec2::new(0.0, -125.0),
                    route: vec![
                        Vec2::new(0.0, -125.0),
                        Vec2::new(145.0, -15.0),
                        Vec2::new(270.0, 15.0),
                        Vec2::new(155.0, 170.0),
                        Vec2::new(-45.0, 170.0),
                    ],
                },
                Vec2::new(0.0, -125.0),
            ),
            _ => (Self::prototype(), BLOB_START),
        }
    }
}

fn semicircle_fixture(center: Vec2, radius: f32, depth: f32) -> Vec<Vec2> {
    let mut vertices = vec![center + Vec2::new(-radius, -depth)];
    for step in 0..=16 {
        let x = -radius + radius * 2.0 * step as f32 / 16.0;
        let y = (radius * radius - x * x).max(0.0).sqrt();
        vertices.push(center + Vec2::new(x, y));
    }
    vertices.push(center + Vec2::new(radius, -depth));
    vertices
}

fn wave_fixtures(
    minimum_x: f32,
    maximum_x: f32,
    baseline: f32,
    bottom: f32,
    segments: usize,
) -> Vec<Vec<Vec2>> {
    (0..segments)
        .map(|segment| {
            let fraction_a = segment as f32 / segments as f32;
            let fraction_b = (segment + 1) as f32 / segments as f32;
            let x_a = minimum_x + (maximum_x - minimum_x) * fraction_a;
            let x_b = minimum_x + (maximum_x - minimum_x) * fraction_b;
            let y_a = baseline + (fraction_a * std::f32::consts::TAU * 1.5).sin() * 48.0;
            let y_b = baseline + (fraction_b * std::f32::consts::TAU * 1.5).sin() * 48.0;
            vec![
                Vec2::new(x_a, bottom),
                Vec2::new(x_b, bottom),
                Vec2::new(x_b, y_b),
                Vec2::new(x_a, y_a),
            ]
        })
        .collect()
}

fn v_valley_fixtures(center: Vec2, width: f32, depth: f32) -> Vec<Vec<Vec2>> {
    let half = width * 0.5;
    vec![
        vec![
            center + Vec2::new(-half, -depth),
            center + Vec2::new(0.0, -depth),
            center,
            center + Vec2::new(-half, depth),
        ],
        vec![
            center + Vec2::new(0.0, -depth),
            center + Vec2::new(half, -depth),
            center + Vec2::new(half, depth),
            center,
        ],
    ]
}

pub(super) fn setup_environment(mut commands: Commands) {
    let level = Level::prototype();
    spawn_level_colliders(&mut commands, &level);
    commands.insert_resource(level);
    commands.insert_resource(TestScenario::default());
    commands.insert_resource(RouteProgress { next: 1 });
    commands.insert_resource(AvianContactDiagnostics::default());
    commands.insert_resource(AvianContactManifolds::default());
}

fn spawn_level_colliders(commands: &mut Commands, level: &Level) {
    for (platform_index, platform) in level.platforms.iter().copied().enumerate() {
        let mut entity = commands.spawn((
            EnvironmentCollider {
                platform_index: Some(platform_index),
            },
            RigidBody::Static,
            Collider::rectangle(platform.half_size.x * 2.0, platform.half_size.y * 2.0),
            CollisionLayers::new(
                [GameLayer::Environment],
                [
                    GameLayer::LivingBlob,
                    GameLayer::Corpse,
                    GameLayer::Projectile,
                ],
            ),
            Transform::from_xyz(platform.center.x, platform.center.y, 0.0),
        ));
        if platform_index <= 3 {
            entity.insert(AvianMigratedSurface);
        }
    }
    for vertices in &level.fixtures {
        if let Some(collider) = Collider::convex_hull(vertices.clone()) {
            commands.spawn((
                EnvironmentCollider {
                    platform_index: None,
                },
                AvianMigratedSurface,
                RigidBody::Static,
                collider,
                CollisionLayers::new(
                    [GameLayer::Environment],
                    [
                        GameLayer::LivingBlob,
                        GameLayer::Corpse,
                        GameLayer::Projectile,
                    ],
                ),
            ));
        }
    }
}

pub(super) fn switch_test_scenario(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    colliders: Query<Entity, With<EnvironmentCollider>>,
    mut scenario: ResMut<TestScenario>,
    mut route_progress: ResMut<RouteProgress>,
    mut level: ResMut<Level>,
    mut blobs: ResMut<BlobWorld>,
    mut vitality: ResMut<VitalityWorld>,
    mut nutrition: ResMut<NutritionWorld>,
) {
    let requested = (1..=6).find(|index| {
        keyboard.just_pressed(match index {
            1 => KeyCode::F1,
            2 => KeyCode::F2,
            3 => KeyCode::F3,
            4 => KeyCode::F4,
            5 => KeyCode::F5,
            _ => KeyCode::F6,
        })
    });
    let Some(requested) = requested else {
        return;
    };
    for entity in &colliders {
        commands.entity(entity).despawn();
    }
    let (new_level, spawn) = Level::test_scenario(requested);
    spawn_level_colliders(&mut commands, &new_level);
    *level = new_level;
    scenario.0 = requested;
    route_progress.next = 1;
    reset_world_at(&mut blobs, spawn);
    vitality.reset();
    nutrition.reset_near(spawn);
}

pub(super) fn advance_route_progress(
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

pub(super) fn resolve_avian_environment(
    time: Res<Time<Fixed>>,
    spatial_query: SpatialQuery,
    migrated_surfaces: Query<(), With<AvianMigratedSurface>>,
    environment_colliders: Query<&EnvironmentCollider>,
    level: Res<Level>,
    mut blobs: ResMut<BlobWorld>,
) {
    let filter = SpatialQueryFilter::from_mask(GameLayer::Environment);
    let dt = time.delta_secs();
    for active_blob in &mut blobs.active {
        let blob_center = active_blob.body.center();
        let skin = 5.0 * active_blob.body.size_scale();
        let ignore_impact_trauma = active_blob.body.ignores_impact_trauma();
        let mut grounded = false;
        let mut support_normal_sum = Vec2::ZERO;
        let mut support_count = 0;
        let mut impacts = Vec::new();
        for particle in &mut active_blob.body.particles {
            let movement = particle.position - particle.previous;
            let movement_length = movement.length();
            let current_projection = spatial_query.project_point_predicate(
                particle.position,
                false,
                &filter,
                &|entity| migrated_surfaces.contains(entity),
            );
            let exited_surface = spatial_query
                .project_point_predicate(particle.previous, false, &filter, &|entity| {
                    migrated_surfaces.contains(entity)
                })
                .is_some_and(|projection| projection.is_inside)
                && current_projection
                    .as_ref()
                    .is_none_or(|projection| !projection.is_inside);
            if !exited_surface
                && let Ok(direction) = Dir2::new(movement)
                && let Some(hit) = spatial_query.cast_ray_predicate(
                    particle.previous,
                    direction,
                    movement_length,
                    true,
                    &filter,
                    &|entity| migrated_surfaces.contains(entity),
                )
            {
                let surface_point = particle.previous + *direction * hit.distance;
                let contact = resolve_swept_particle(particle, surface_point, hit.normal, skin);
                grounded |= contact.normal.y > 0.55;
                if contact.normal.y > 0.55 {
                    support_normal_sum += contact.normal;
                    support_count += 1;
                }
                if !ignore_impact_trauma {
                    impacts.push(contact.impact_displacement / dt.max(0.000_001));
                }
                continue;
            }
            let Some(projection) = current_projection else {
                continue;
            };
            let (surface_point, forced_normal) = if projection.is_inside {
                let Ok(marker) = environment_colliders.get(projection.entity) else {
                    continue;
                };
                if let Some(platform_index) = marker.platform_index {
                    let platform = level.platforms[platform_index];
                    let (point, normal) =
                        stable_inside_surface(particle.position, blob_center, platform);
                    (point, Some(normal))
                } else {
                    let normal = (projection.point - particle.position)
                        .normalize_or((particle.position - blob_center).normalize_or(Vec2::Y));
                    (projection.point, Some(normal))
                }
            } else {
                (projection.point, None)
            };
            let Some(contact) = resolve_particle_projection_with_normal(
                particle,
                surface_point,
                projection.is_inside,
                forced_normal,
                skin,
            ) else {
                continue;
            };
            grounded |= contact.normal.y > 0.55;
            if contact.normal.y > 0.55 {
                support_normal_sum += contact.normal;
                support_count += 1;
            }
            if !ignore_impact_trauma {
                impacts.push(contact.impact_displacement / dt.max(0.000_001));
            }
        }
        active_blob.body.grounded |= grounded;
        if support_count > 0 {
            active_blob
                .body
                .record_support_normal(support_normal_sum / support_count as f32);
        }
        active_blob.body.last_impact_speed = active_blob
            .body
            .last_impact_speed
            .max(contact_patch_impact(&mut impacts));
    }
}

fn contact_patch_impact(impacts: &mut [f32]) -> f32 {
    impacts.sort_by(|first, second| second.total_cmp(first));
    match impacts {
        [] => 0.0,
        [single] => *single * 0.68,
        [first, second] => (*first * 0.72 + *second * 0.28) * 0.84,
        [first, second, third, ..] => *first * 0.62 + *second * 0.25 + *third * 0.13,
    }
}

fn resolve_swept_particle(
    particle: &mut Particle,
    surface_point: Vec2,
    normal: Vec2,
    skin: f32,
) -> ProjectionContact {
    let normal = normal.normalize_or(Vec2::Y);
    let velocity = particle.position - particle.previous;
    let impact_displacement = (-velocity.dot(normal)).max(0.0);
    particle.position = surface_point + normal * skin;
    let normal_speed = velocity.dot(normal);
    let corrected_velocity = if normal_speed < 0.0 {
        velocity - normal * normal_speed
    } else {
        velocity
    };
    particle.previous = particle.position - corrected_velocity;
    ProjectionContact {
        normal,
        impact_displacement,
    }
}

#[derive(Clone, Copy, Debug)]
struct ProjectionContact {
    normal: Vec2,
    impact_displacement: f32,
}

#[cfg(test)]
fn resolve_particle_projection(
    particle: &mut Particle,
    surface_point: Vec2,
    is_inside: bool,
    skin: f32,
) -> Option<ProjectionContact> {
    resolve_particle_projection_with_normal(particle, surface_point, is_inside, None, skin)
}

fn resolve_particle_projection_with_normal(
    particle: &mut Particle,
    surface_point: Vec2,
    is_inside: bool,
    forced_normal: Option<Vec2>,
    skin: f32,
) -> Option<ProjectionContact> {
    let separation = if is_inside {
        surface_point - particle.position
    } else {
        particle.position - surface_point
    };
    let distance = separation.length();
    if !is_inside && distance > skin {
        return None;
    }
    let normal = forced_normal.unwrap_or_else(|| separation.normalize_or(Vec2::Y));
    let velocity = particle.position - particle.previous;
    let impact_displacement = (-velocity.dot(normal)).max(0.0);
    particle.position = surface_point + normal * skin;
    let normal_speed = velocity.dot(normal);
    let corrected_velocity = if normal_speed < 0.0 {
        velocity - normal * normal_speed
    } else {
        velocity
    };
    particle.previous = particle.position - corrected_velocity;
    Some(ProjectionContact {
        normal,
        impact_displacement,
    })
}

fn stable_inside_surface(point: Vec2, blob_center: Vec2, platform: Platform) -> (Vec2, Vec2) {
    let minimum = platform.center - platform.half_size;
    let maximum = platform.center + platform.half_size;
    let relative = blob_center - platform.center;
    let normalized_x = relative.x.abs() / platform.half_size.x.max(1.0);
    let normalized_y = relative.y.abs() / platform.half_size.y.max(1.0);
    if normalized_x > normalized_y {
        if relative.x < 0.0 {
            (
                Vec2::new(minimum.x, point.y.clamp(minimum.y, maximum.y)),
                Vec2::NEG_X,
            )
        } else {
            (
                Vec2::new(maximum.x, point.y.clamp(minimum.y, maximum.y)),
                Vec2::X,
            )
        }
    } else if relative.y < 0.0 {
        (
            Vec2::new(point.x.clamp(minimum.x, maximum.x), minimum.y),
            Vec2::NEG_Y,
        )
    } else {
        (
            Vec2::new(point.x.clamp(minimum.x, maximum.x), maximum.y),
            Vec2::Y,
        )
    }
}

/// Observes membrane/environment contacts through Avian without applying a
/// second collision response. This shadow mode provides evidence before the
/// legacy platform solver is replaced.
pub(super) fn sample_avian_contacts(
    spatial_query: SpatialQuery,
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    mut diagnostics: ResMut<AvianContactDiagnostics>,
    mut manifolds: ResMut<AvianContactManifolds>,
) {
    let filter = SpatialQueryFilter::from_mask(GameLayer::Environment);
    let mut particles = 0;
    let mut avian_contacts = 0;
    let mut legacy_contacts = 0;
    let mut agreements = 0;
    manifolds.by_blob.clear();

    for active_blob in &blobs.active {
        let probe_radius = 6.0 * active_blob.body.size_scale();
        let contacts = manifolds.by_blob.entry(active_blob.id).or_default();
        for (particle_index, particle) in active_blob.body.particles.iter().enumerate() {
            particles += 1;
            let projection = spatial_query.project_point(particle.position, false, &filter);
            let avian_contact = projection.as_ref().is_some_and(|projection| {
                projection.is_inside || projection.point.distance(particle.position) <= probe_radius
            });
            if let Some(projection) = projection.filter(|_| avian_contact) {
                let separation = if projection.is_inside {
                    projection.point - particle.position
                } else {
                    particle.position - projection.point
                };
                let distance = separation.length();
                contacts.push(AvianMembraneContact {
                    particle_index,
                    collider: projection.entity,
                    point: projection.point,
                    normal: separation.normalize_or(Vec2::Y),
                    depth: if projection.is_inside {
                        probe_radius + distance
                    } else {
                        (probe_radius - distance).max(0.0)
                    },
                });
            }
            let legacy_contact = level
                .platforms
                .iter()
                .any(|platform| point_near_platform(particle.position, probe_radius, platform));
            avian_contacts += avian_contact as usize;
            legacy_contacts += legacy_contact as usize;
            agreements += (avian_contact == legacy_contact) as usize;
        }
    }

    diagnostics.particles = particles;
    diagnostics.avian_contacts = avian_contacts;
    diagnostics.legacy_contacts = legacy_contacts;
    diagnostics.agreement = if particles == 0 {
        1.0
    } else {
        agreements as f32 / particles as f32
    };
    let selected_contacts = blobs
        .active
        .get(blobs.selected)
        .and_then(|blob| manifolds.by_blob.get(&blob.id))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    diagnostics.selected_surfaces = selected_contacts
        .iter()
        .map(|contact| contact.collider)
        .collect::<HashSet<_>>()
        .len();
    diagnostics.selected_particles = selected_contacts
        .iter()
        .map(|contact| contact.particle_index)
        .collect::<HashSet<_>>()
        .len();
    diagnostics.selected_ground_contacts = selected_contacts
        .iter()
        .filter(|contact| contact.normal.y > 0.55)
        .count();
    diagnostics.selected_max_depth = selected_contacts
        .iter()
        .map(|contact| contact.depth)
        .fold(0.0, f32::max);
    let minimum_x = selected_contacts
        .iter()
        .map(|contact| contact.point.x)
        .fold(f32::INFINITY, f32::min);
    let maximum_x = selected_contacts
        .iter()
        .map(|contact| contact.point.x)
        .fold(f32::NEG_INFINITY, f32::max);
    diagnostics.selected_contact_span = if selected_contacts.is_empty() {
        0.0
    } else {
        maximum_x - minimum_x
    };
}

fn point_near_platform(point: Vec2, radius: f32, platform: &Platform) -> bool {
    let minimum = platform.center - platform.half_size;
    let maximum = platform.center + platform.half_size;
    let closest = point.clamp(minimum, maximum);
    point.distance_squared(closest) <= radius * radius
}

fn platform(x: f32, y: f32, width: f32, height: f32) -> Platform {
    Platform {
        center: Vec2::new(x, y),
        half_size: Vec2::new(width, height) * 0.5,
    }
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
    fn every_test_route_uses_conservative_jump_gaps() {
        for scenario in 2..=6 {
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

        let expected = Level::prototype().platforms;
        let mut query = app
            .world_mut()
            .query::<(&EnvironmentCollider, &RigidBody, &Transform)>();
        let colliders = query.iter(app.world()).collect::<Vec<_>>();
        assert_eq!(colliders.len(), expected.len());
        for (_, body, transform) in colliders {
            assert_eq!(*body, RigidBody::Static);
            assert!(
                expected
                    .iter()
                    .any(|platform| transform.translation.truncate() == platform.center)
            );
        }
        let pilot_count = app
            .world_mut()
            .query_filtered::<Entity, With<AvianMigratedSurface>>()
            .iter(app.world())
            .count();
        assert_eq!(pilot_count, 4);
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
