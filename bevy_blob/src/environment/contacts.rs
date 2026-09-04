//! Narrow geometry queries used while validating membrane contacts.
//!
//! More projection helpers can move here incrementally without changing the
//! Avian bridge in `environment.rs`.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct ProjectionContact {
    pub(super) normal: Vec2,
    pub(super) impact_displacement: f32,
}

pub(super) fn point_near_platform(point: Vec2, radius: f32, platform: &Platform) -> bool {
    let minimum = platform.center - platform.half_size;
    let maximum = platform.center + platform.half_size;
    point.distance_squared(point.clamp(minimum, maximum)) <= radius * radius
}

pub(super) fn contact_point_is_shared(point: Vec2, owner: usize, fixtures: &[Vec<Vec2>]) -> bool {
    const CONTACT_TOLERANCE: f32 = 0.75;
    let Some(polygon) = fixtures.get(owner) else {
        return false;
    };
    polygon
        .iter()
        .copied()
        .zip(polygon.iter().copied().cycle().skip(1))
        .take(polygon.len())
        .any(|(start, end)| {
            point_segment_distance(point, start, end) <= CONTACT_TOLERANCE
                && fixtures.iter().enumerate().any(|(index, candidate)| {
                    index != owner
                        && candidate.iter().any(|candidate_start| {
                            candidate_start.distance_squared(start) <= 0.0001
                        })
                        && candidate
                            .iter()
                            .any(|candidate_end| candidate_end.distance_squared(end) <= 0.0001)
                })
        })
}

pub(super) fn impact_from_patch(impacts: &mut [f32]) -> f32 {
    impacts.sort_by(|first, second| second.total_cmp(first));
    match impacts {
        [] => 0.0,
        [single] => *single * 0.68,
        [first, second] => (*first * 0.72 + *second * 0.28) * 0.84,
        [first, second, third, ..] => *first * 0.62 + *second * 0.25 + *third * 0.13,
    }
}

pub(super) fn resolve_swept(
    particle: &mut Particle,
    surface_point: Vec2,
    normal: Vec2,
    skin: f32,
) -> ProjectionContact {
    resolve_projection(particle, surface_point, normal.normalize_or(Vec2::Y), skin)
}

pub(super) fn project_particle(
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
    if !is_inside && separation.length() > skin {
        return None;
    }
    Some(resolve_projection(
        particle,
        surface_point,
        forced_normal.unwrap_or_else(|| separation.normalize_or(Vec2::Y)),
        skin,
    ))
}

#[cfg(test)]
pub(super) fn project_particle_for_test(
    particle: &mut Particle,
    surface_point: Vec2,
    is_inside: bool,
    skin: f32,
) -> Option<ProjectionContact> {
    project_particle(particle, surface_point, is_inside, None, skin)
}

pub(super) fn stable_inside(point: Vec2, blob_center: Vec2, platform: Platform) -> (Vec2, Vec2) {
    let minimum = platform.center - platform.half_size;
    let maximum = platform.center + platform.half_size;
    let relative = blob_center - platform.center;
    let horizontal = relative.x.abs() / platform.half_size.x.max(1.0);
    let vertical = relative.y.abs() / platform.half_size.y.max(1.0);
    if horizontal > vertical {
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

fn resolve_projection(
    particle: &mut Particle,
    surface_point: Vec2,
    normal: Vec2,
    skin: f32,
) -> ProjectionContact {
    let velocity = particle.position - particle.previous;
    let impact_displacement = (-velocity.dot(normal)).max(0.0);
    particle.position = surface_point + normal * skin;
    let corrected_velocity = if velocity.dot(normal) < 0.0 {
        velocity - normal * velocity.dot(normal)
    } else {
        velocity
    };
    particle.previous = particle.position - corrected_velocity;
    ProjectionContact {
        normal,
        impact_displacement,
    }
}

fn point_segment_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let edge = end - start;
    let fraction =
        ((point - start).dot(edge) / edge.length_squared().max(f32::EPSILON)).clamp(0.0, 1.0);
    point.distance(start + edge * fraction)
}
