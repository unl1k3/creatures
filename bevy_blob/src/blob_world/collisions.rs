//! Soft-body contact resolution between active blobs.

use super::*;

#[cfg(test)]
pub(crate) fn resolve_blob_collisions(blobs: &mut [ActiveBlob]) {
    resolve_blob_collisions_impl(blobs, |_| (true, true));
}

pub(crate) fn resolve_blob_collisions_with_vitality(
    blobs: &mut [ActiveBlob],
    _vitality: &VitalityWorld,
) {
    resolve_blob_collisions_impl(blobs, |_| (true, true));
}

pub(crate) fn resolve_blob_collisions_impl(
    blobs: &mut [ActiveBlob],
    interaction: impl Fn(u64) -> (bool, bool),
) {
    let crowded = blobs.len() > 2;
    for first_index in 0..blobs.len() {
        let (before_second, from_second) = blobs.split_at_mut(first_index + 1);
        let first_active = &mut before_second[first_index];
        let (first_alive, first_collides) = interaction(first_active.id);
        let first = &mut first_active.body;
        for second in from_second {
            let (second_alive, second_collides) = interaction(second.id);
            if !first_collides || !second_collides {
                continue;
            }
            let second = &mut second.body;
            let pair_scale = (first.size_scale() + second.size_scale()) * 0.5;
            // Keep a generous predictive skin for stable continuous contact,
            // but do not expose that entire skin as a visible gap between the
            // rendered membranes.
            let prediction_clearance = BLOB_CONTACT_PREDICTION_CLEARANCE * pair_scale;
            let visual_clearance = BLOB_CONTACT_VISUAL_CLEARANCE * pair_scale;
            let Some((normal, contact_points, penetration)) =
                avian_blob_contacts(first, second, prediction_clearance)
            else {
                continue;
            };

            // A predictive manifold only says that contact is imminent. Do
            // not deform the membranes or cancel their closing velocity until
            // their visible contours have actually reached one another.
            if blob_surface_gap(first, second) > visual_clearance {
                continue;
            }

            let first_mass = first.mass();
            let second_mass = second.mass();
            let total_mass = first_mass + second_mass;
            let contact_load = if crowded {
                (penetration + 1.5 * pair_scale)
                    .min(first.rest_radius.min(second.rest_radius) * 0.12)
            } else {
                (penetration + 3.0 * pair_scale)
                    .min(first.rest_radius.min(second.rest_radius) * 0.18)
            };
            let point_count = contact_points.len().max(1) as f32;
            let actual_overlap = (penetration - prediction_clearance).max(0.0);
            for point in contact_points {
                let first_load = if first_alive {
                    contact_load / point_count
                } else {
                    actual_overlap * 0.30 / point_count
                };
                let second_load = if second_alive {
                    contact_load / point_count
                } else {
                    actual_overlap * 0.30 / point_count
                };
                if first_load > 0.001 {
                    first.apply_contact_patch(point, normal, first_load, !first_alive);
                }
                if second_load > 0.001 {
                    second.apply_contact_patch(point, -normal, second_load, !second_alive);
                }
            }

            let predicted_post_correction =
                avian_blob_contacts(first, second, prediction_clearance)
                    .map(|(_, _, penetration)| penetration)
                    .unwrap_or(0.0);
            let mut post_penetration =
                (predicted_post_correction - (prediction_clearance - visual_clearance)).max(0.0);
            if crowded {
                post_penetration = post_penetration.min(BLOB_CONTACT_MAX_CORRECTION * pair_scale);
            }
            match (first_alive, second_alive) {
                (true, false) => first.translate(-normal * post_penetration),
                (false, true) => second.translate(normal * post_penetration),
                _ => {
                    first.translate(-normal * post_penetration * second_mass / total_mass);
                    second.translate(normal * post_penetration * first_mass / total_mass);
                }
            }

            // Convex contact normals can rotate slightly after a soft patch is
            // deformed. Close the tiny residual along the new centre axis so
            // the visible contours never remain interpenetrating.
            let final_delta = second.center() - first.center();
            let final_normal = final_delta.normalize_or(normal);
            let residual = (visual_clearance - blob_surface_gap(first, second)).max(0.0);
            match (first_alive, second_alive) {
                (true, false) => first.translate(-final_normal * residual),
                (false, true) => second.translate(final_normal * residual),
                _ => {
                    first.translate(-final_normal * residual * second_mass / total_mass);
                    second.translate(final_normal * residual * first_mass / total_mass);
                }
            }

            // Blob-to-blob contact can support jump charging just like level
            // geometry. Only the upper body is grounded; side contacts do not
            // arm a jump.
            if normal.y > 0.55 && second_alive {
                second.grounded = true;
                second.record_support_normal(normal);
            } else if normal.y < -0.55 && first_alive {
                first.grounded = true;
                first.record_support_normal(-normal);
            }

            let mut relative_normal_speed = (second.velocity() - first.velocity()).dot(normal);
            if crowded {
                relative_normal_speed =
                    relative_normal_speed.max(-BLOB_CONTACT_MAX_TRANSFER_SPEED * pair_scale);
            }
            if relative_normal_speed < 0.0 {
                match (first_alive, second_alive) {
                    (true, true) => {
                        first.add_velocity(normal * relative_normal_speed * 0.5);
                        second.add_velocity(-normal * relative_normal_speed * 0.5);
                    }
                    (true, false) => {
                        first.add_velocity(normal * relative_normal_speed);
                        second.damp_velocity(0.03);
                    }
                    (false, true) => {
                        second.add_velocity(-normal * relative_normal_speed);
                        first.damp_velocity(0.03);
                    }
                    (false, false) => {
                        first.damp_velocity(0.03);
                        second.damp_velocity(0.03);
                    }
                }
            }
        }
    }
}

pub(crate) fn avian_blob_contacts(
    first: &Blob,
    second: &Blob,
    prediction_distance: f32,
) -> Option<(Vec2, Vec<Vec2>, f32)> {
    let first_center = first.center();
    let second_center = second.center();
    let first_collider = Collider::convex_hull(
        first
            .particles
            .iter()
            .map(|particle| particle.position - first_center)
            .collect(),
    )?;
    let second_collider = Collider::convex_hull(
        second
            .particles
            .iter()
            .map(|particle| particle.position - second_center)
            .collect(),
    )?;
    let mut manifolds = Vec::<ContactManifold>::new();
    contact_manifolds(
        &first_collider,
        first_center,
        0.0,
        &second_collider,
        second_center,
        0.0,
        prediction_distance,
        &mut manifolds,
    );
    let manifold = manifolds
        .iter()
        .filter(|manifold| !manifold.points.is_empty())
        .max_by(|first, second| {
            let first_depth = first
                .points
                .iter()
                .map(|point| point.penetration)
                .fold(f32::NEG_INFINITY, f32::max);
            let second_depth = second
                .points
                .iter()
                .map(|point| point.penetration)
                .fold(f32::NEG_INFINITY, f32::max);
            first_depth.total_cmp(&second_depth)
        })?;
    let points = manifold
        .points
        .iter()
        .map(|point| point.point)
        .collect::<Vec<_>>();
    let correction = manifold
        .points
        .iter()
        .map(|point| point.penetration + prediction_distance)
        .fold(0.0, f32::max)
        .max(0.0);
    Some((manifold.normal, points, correction))
}

pub(crate) fn support_extent(blob: &Blob, direction: Vec2) -> f32 {
    let center = blob.center();
    blob.particles
        .iter()
        .map(|particle| (particle.position - center).dot(direction))
        .fold(0.0, f32::max)
}

pub(crate) fn blob_surface_gap(first: &Blob, second: &Blob) -> f32 {
    let delta = second.center() - first.center();
    let distance = delta.length();
    let normal = if distance > 0.001 {
        delta / distance
    } else {
        Vec2::X
    };
    distance - support_extent(first, normal) - support_extent(second, -normal)
}
