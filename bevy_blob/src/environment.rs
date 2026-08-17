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
    platform_index: usize,
}

#[derive(Component, Debug)]
pub(super) struct AvianMigratedSurface;

#[derive(Resource)]
pub(super) struct Level {
    pub(super) platforms: Vec<Platform>,
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
        }
    }
}

pub(super) fn setup_environment(mut commands: Commands) {
    let level = Level::prototype();
    for (platform_index, platform) in level.platforms.iter().copied().enumerate() {
        let mut entity = commands.spawn((
            EnvironmentCollider { platform_index },
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
    commands.insert_resource(level);
    commands.insert_resource(AvianContactDiagnostics::default());
    commands.insert_resource(AvianContactManifolds::default());
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
        let mut grounded = false;
        let mut maximum_impact = 0.0_f32;
        for particle in &mut active_blob.body.particles {
            let movement = particle.position - particle.previous;
            let movement_length = movement.length();
            if let Ok(direction) = Dir2::new(movement)
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
                maximum_impact =
                    maximum_impact.max(contact.impact_displacement / dt.max(0.000_001));
                continue;
            }
            let Some(projection) = spatial_query.project_point_predicate(
                particle.position,
                false,
                &filter,
                &|entity| migrated_surfaces.contains(entity),
            ) else {
                continue;
            };
            let (surface_point, forced_normal) = if projection.is_inside {
                let Ok(marker) = environment_colliders.get(projection.entity) else {
                    continue;
                };
                let platform = level.platforms[marker.platform_index];
                let (point, normal) =
                    stable_inside_surface(particle.position, blob_center, platform);
                (point, Some(normal))
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
            maximum_impact = maximum_impact.max(contact.impact_displacement / dt.max(0.000_001));
        }
        active_blob.body.grounded |= grounded;
        active_blob.body.last_impact_speed = active_blob.body.last_impact_speed.max(maximum_impact);
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
