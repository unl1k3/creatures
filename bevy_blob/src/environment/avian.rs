//! Shared Avian contact diagnostics and membrane contact records.

use super::*;
use std::collections::{HashMap, HashSet};

#[derive(Resource, Default)]
pub(crate) struct AvianContactDiagnostics {
    pub(crate) particles: usize,
    pub(crate) avian_contacts: usize,
    pub(crate) legacy_contacts: usize,
    pub(crate) agreement: f32,
    pub(crate) selected_surfaces: usize,
    pub(crate) selected_particles: usize,
    pub(crate) selected_ground_contacts: usize,
    pub(crate) selected_max_depth: f32,
    pub(crate) selected_contact_span: f32,
    pub(crate) fixture_corrections: usize,
    pub(crate) lateral_fixture_corrections: usize,
    pub(crate) shared_edge_corrections: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AvianMembraneContact {
    pub(super) particle_index: usize,
    pub(super) collider: Entity,
    pub(super) point: Vec2,
    pub(super) normal: Vec2,
    pub(super) depth: f32,
}

#[derive(Resource, Default)]
pub(crate) struct AvianContactManifolds {
    pub(super) by_blob: HashMap<u64, Vec<AvianMembraneContact>>,
}

/// Resolves membrane/environment penetration using Avian spatial queries.
pub(crate) fn resolve_avian_environment(
    time: Res<Time<Fixed>>,
    spatial_query: SpatialQuery,
    environment_colliders: Query<&EnvironmentCollider>,
    level: Res<Level>,
    mut blobs: ResMut<BlobWorld>,
    mut diagnostics: ResMut<AvianContactDiagnostics>,
) {
    let filter = SpatialQueryFilter::from_mask(GameLayer::Environment);
    let dt = time.delta_secs();
    diagnostics.fixture_corrections = 0;
    diagnostics.lateral_fixture_corrections = 0;
    let selected = blobs.selected;
    for (blob_index, active_blob) in blobs.active.iter_mut().enumerate() {
        let blob_center = active_blob.body.center();
        let skin = (5.0 * active_blob.body.size_scale()).max(crate::blob::MIN_COLLISION_SKIN);
        let probe_radius = (skin * 0.55).max(0.8);
        let probe = Collider::circle(probe_radius);
        let ignore_impact_trauma = active_blob.body.ignores_impact_trauma();
        let mut grounded = false;
        let mut support_normal_sum = Vec2::ZERO;
        let mut support_count = 0;
        let mut impacts = Vec::new();
        let mut had_external_projection = false;
        for particle in &mut active_blob.body.particles {
            let movement = particle.position - particle.previous;
            let movement_length = movement.length();
            let current_projection = spatial_query.project_point_predicate(
                particle.position,
                false,
                &filter,
                &|entity| environment_colliders.contains(entity),
            );
            if let Ok(direction) = Dir2::new(movement)
                && let Some(hit) = spatial_query.cast_shape_predicate(
                    &probe,
                    particle.previous,
                    0.0,
                    direction,
                    &ShapeCastConfig::from_max_distance(movement_length),
                    &filter,
                    &|entity| environment_colliders.contains(entity),
                )
            {
                let shared_edge = environment_colliders
                    .get(hit.entity)
                    .ok()
                    .and_then(|marker| marker.fixture_index)
                    .is_some_and(|fixture_index| {
                        contact_point_is_shared(hit.point1, fixture_index, &level.fixtures)
                    });
                if blob_index == selected
                    && let Ok(marker) = environment_colliders.get(hit.entity)
                    && marker.fixture_index.is_some()
                {
                    diagnostics.fixture_corrections += 1;
                    diagnostics.lateral_fixture_corrections +=
                        (hit.normal1.y.abs() < 0.55) as usize;
                    diagnostics.shared_edge_corrections += shared_edge as usize;
                }
                if shared_edge {
                    continue;
                }
                let contact = resolve_swept(
                    particle,
                    hit.point1,
                    hit.normal1,
                    probe_radius + skin * 0.45,
                );
                had_external_projection = true;
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
            let shared_edge = environment_colliders
                .get(projection.entity)
                .ok()
                .and_then(|marker| marker.fixture_index)
                .is_some_and(|fixture_index| {
                    contact_point_is_shared(projection.point, fixture_index, &level.fixtures)
                });
            if blob_index == selected
                && let Ok(marker) = environment_colliders.get(projection.entity)
                && marker.fixture_index.is_some()
            {
                let normal = (projection.point - particle.position).normalize_or(Vec2::Y);
                diagnostics.fixture_corrections += 1;
                diagnostics.lateral_fixture_corrections += (normal.y.abs() < 0.55) as usize;
                diagnostics.shared_edge_corrections += shared_edge as usize;
            }
            if shared_edge {
                continue;
            }
            let (surface_point, forced_normal) = if projection.is_inside {
                let Ok(marker) = environment_colliders.get(projection.entity) else {
                    continue;
                };
                if let Some(platform_index) = marker.platform_index {
                    let platform = level.platforms[platform_index];
                    let (point, normal) = stable_inside(particle.position, blob_center, platform);
                    (point, Some(normal))
                } else {
                    let normal = (projection.point - particle.position)
                        .normalize_or((particle.position - blob_center).normalize_or(Vec2::Y));
                    (projection.point, Some(normal))
                }
            } else {
                (projection.point, None)
            };
            let Some(contact) = project_particle(
                particle,
                surface_point,
                projection.is_inside,
                forced_normal,
                skin,
            ) else {
                continue;
            };
            had_external_projection = true;
            grounded |= contact.normal.y > 0.55;
            if contact.normal.y > 0.55 {
                support_normal_sum += contact.normal;
                support_count += 1;
            }
            if !ignore_impact_trauma {
                impacts.push(contact.impact_displacement / dt.max(0.000_001));
            }
        }
        if had_external_projection {
            active_blob.body.stabilize_after_external_projection();
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
            .max(impact_from_patch(&mut impacts));
    }
}

/// Samples Avian contacts without applying an additional collision response.
pub(crate) fn sample_avian_contacts(
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
