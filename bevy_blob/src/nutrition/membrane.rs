//! Queries over the deformable blob membrane used by nutrition mechanics.

use super::geometry::{circle_aabb_penetration, circle_convex_penetration, point_inside_convex};
use super::*;

/// Finds the membrane edge nearest to a target without mutating the blob.
pub(super) fn membrane_anchor(blob: &Blob, target: Vec2) -> (usize, f32) {
    let count = blob.particles.len();
    (0..count)
        .map(|index| {
            let start = blob.particles[index].position;
            let edge = blob.particles[(index + 1) % count].position - start;
            let t = ((target - start).dot(edge) / edge.length_squared().max(0.001)).clamp(0.0, 1.0);
            (index, t, (start + edge * t).distance_squared(target))
        })
        .min_by(|first, second| first.2.total_cmp(&second.2))
        .map(|(index, t, _)| (index, t))
        .unwrap_or((0, 0.5))
}

/// A probe may only start when its path to the nutrient is free of static
/// level geometry. This prevents visual protrusions through a wall.
pub(super) fn phagocytosis_path_clear(
    blob_center: Vec2,
    blob_radius: f32,
    nutrient_center: Vec2,
    nutrient_radius: f32,
    level: &Level,
) -> bool {
    let direction = (nutrient_center - blob_center).normalize_or(Vec2::X);
    let start = blob_center + direction * (blob_radius + 2.0);
    let end = nutrient_center - direction * (nutrient_radius + 2.0);
    if start.distance_squared(end) <= 1.0 {
        return true;
    }
    (1..10).all(|sample| {
        let point = start.lerp(end, sample as f32 / 10.0);
        !level.platforms.iter().any(|platform| {
            let delta = point - platform.center;
            delta.x.abs() < platform.half_size.x && delta.y.abs() < platform.half_size.y
        }) && !level
            .fixtures
            .iter()
            .any(|vertices| point_inside_convex(point, vertices))
    })
}

/// Shortens an external protrusion to the farthest path that remains outside
/// platforms, fixtures, and sibling blob membranes.
#[allow(clippy::too_many_arguments)]
pub(super) fn constrain_protrusion_load(
    blob: &Blob,
    host_id: u64,
    blobs: &BlobWorld,
    desired: Vec2,
    load_radius: f32,
    strength: f32,
    variation: f32,
    anchor_edge: usize,
    anchor_t: f32,
    level: &Level,
) -> Vec2 {
    if strength <= 0.01
        || !protrusion_intersects_environment(
            blob,
            host_id,
            blobs,
            desired,
            load_radius,
            strength,
            variation,
            anchor_edge,
            anchor_t,
            level,
        )
    {
        return desired;
    }
    let base = blob.particles[(anchor_edge + 1) % blob.particles.len()].position;
    let mut clear = 0.0;
    let mut blocked = 1.0;
    for _ in 0..10 {
        let candidate = (clear + blocked) * 0.5;
        let position = base.lerp(desired, candidate);
        if protrusion_intersects_environment(
            blob,
            host_id,
            blobs,
            position,
            load_radius,
            strength,
            variation,
            anchor_edge,
            anchor_t,
            level,
        ) {
            blocked = candidate;
        } else {
            clear = candidate;
        }
    }
    base.lerp(desired, clear)
}

#[allow(clippy::too_many_arguments)]
fn protrusion_intersects_environment(
    blob: &Blob,
    host_id: u64,
    blobs: &BlobWorld,
    load_position: Vec2,
    load_radius: f32,
    strength: f32,
    variation: f32,
    anchor_edge: usize,
    anchor_t: f32,
    level: &Level,
) -> bool {
    const SAMPLES: usize = 22;
    let count = blob.particles.len();
    let edge = anchor_edge % count;
    let start = blob.particles[edge].position;
    let base = blob.particles[(edge + 1) % count].position;
    let end = blob.particles[(edge + 2) % count].position;
    let tip = base.lerp(load_position, strength.clamp(0.0, 1.0));
    let length = base.distance(tip);
    if length < 0.5 {
        return false;
    }
    let load_direction = (load_position - blob.center()).normalize_or(Vec2::X);
    let normal_axis = load_direction.perp();
    let secondary = (variation * 7.137).fract();
    let asymmetry = (anchor_t.clamp(0.0, 1.0) - 0.5) * 0.08;
    let start_attachment = start.lerp(base, 0.18 + asymmetry);
    let end_attachment = base.lerp(end, 0.82 + asymmetry);
    let tangent = (end_attachment - start_attachment).normalize_or(Vec2::X);
    let mut root_normal = tangent.perp();
    if root_normal.dot(base - blob.center()) < 0.0 {
        root_normal = -root_normal;
    }
    let control_a = base
        + root_normal * length * (0.30 + variation * 0.08)
        + tangent * length * (variation - 0.5) * 0.05;
    let control_b = base.lerp(tip, 0.72) + normal_axis * length * (secondary - 0.5) * 0.18;
    let maximum_width = start_attachment.distance(end_attachment) * 0.5;
    let width = (load_radius * (0.55 + strength * 0.45) * (0.88 + variation * 0.24))
        .min(start.distance(end) * 0.48)
        .max(0.5);
    (4..=SAMPLES).any(|sample| {
        let along = sample as f32 / SAMPLES as f32;
        let inverse: f32 = 1.0 - along;
        let centerline = base * inverse.powi(3)
            + control_a * 3.0 * inverse.powi(2) * along
            + control_b * 3.0 * inverse * along.powi(2)
            + tip * along.powi(3);
        let collision_radius = (width * (1.0 - along * 0.58).max(0.18))
            .min(maximum_width * (1.0 - along * 0.58).max(0.18))
            .max(1.2);
        level.platforms.iter().any(|platform| {
            circle_aabb_penetration(
                centerline,
                collision_radius,
                platform.center,
                platform.half_size,
            )
            .is_some()
        }) || level.fixtures.iter().any(|vertices| {
            circle_convex_penetration(centerline, collision_radius, vertices).is_some()
        }) || blobs.active.iter().any(|other| {
            other.id != host_id
                && circle_intersects_blob_membrane(centerline, collision_radius, &other.body)
        })
    })
}

pub(super) fn circle_intersects_blob_membrane(center: Vec2, radius: f32, blob: &Blob) -> bool {
    point_inside_blob_membrane(center, blob)
        || blob
            .particles
            .iter()
            .zip(blob.particles.iter().cycle().skip(1))
            .any(|(first, second)| {
                let edge = second.position - first.position;
                let t = ((center - first.position).dot(edge) / edge.length_squared().max(0.001))
                    .clamp(0.0, 1.0);
                center.distance_squared(first.position + edge * t) < radius * radius
            })
}

pub(super) fn membrane_lower_boundary(blob: &Blob, world_x: f32) -> f32 {
    let mut lower = f32::INFINITY;
    for (first, second) in blob
        .particles
        .iter()
        .zip(blob.particles.iter().cycle().skip(1))
    {
        let min_x = first.position.x.min(second.position.x);
        let max_x = first.position.x.max(second.position.x);
        if world_x < min_x || world_x > max_x {
            continue;
        }
        let dx = second.position.x - first.position.x;
        let t = if dx.abs() < 0.001 {
            0.5
        } else {
            ((world_x - first.position.x) / dx).clamp(0.0, 1.0)
        };
        lower = lower.min(first.position.y + (second.position.y - first.position.y) * t);
    }
    if lower.is_finite() {
        lower
    } else {
        blob.particles
            .iter()
            .map(|particle| particle.position.y)
            .fold(blob.center().y - blob.rest_radius, f32::min)
    }
}

fn point_inside_blob_membrane(point: Vec2, blob: &Blob) -> bool {
    let mut inside = false;
    for (first, second) in blob
        .particles
        .iter()
        .zip(blob.particles.iter().cycle().skip(1))
    {
        let a = first.position;
        let b = second.position;
        let crosses_y = (a.y > point.y) != (b.y > point.y);
        if crosses_y {
            let intersection_x = a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if point.x < intersection_x {
                inside = !inside;
            }
        }
    }
    inside
}

pub(super) fn circle_outside_blob_membrane(center: Vec2, radius: f32, blob: &Blob) -> bool {
    if point_inside_blob_membrane(center, blob) {
        return false;
    }
    let nearest_edge = blob
        .particles
        .iter()
        .zip(blob.particles.iter().cycle().skip(1))
        .map(|(first, second)| {
            let edge = second.position - first.position;
            let t = ((center - first.position).dot(edge) / edge.length_squared().max(0.001))
                .clamp(0.0, 1.0);
            center.distance(first.position + edge * t)
        })
        .fold(f32::INFINITY, f32::min);
    nearest_edge >= radius * 0.92
}

/// Exact circle-versus-membrane correction used by expelled nutrients.
///
/// The old rest-radius approximation created a large invisible collision halo,
/// particularly around squashed or stretched blobs.
pub(crate) fn circle_blob_penetration(
    center: Vec2,
    radius: f32,
    blob: &Blob,
) -> Option<(f32, Vec2)> {
    let mut nearest_point = blob.center();
    let mut nearest_distance_squared = f32::INFINITY;
    for (first, second) in blob
        .particles
        .iter()
        .zip(blob.particles.iter().cycle().skip(1))
    {
        let edge = second.position - first.position;
        let along = ((center - first.position).dot(edge) / edge.length_squared().max(0.001))
            .clamp(0.0, 1.0);
        let point = first.position + edge * along;
        let distance_squared = center.distance_squared(point);
        if distance_squared < nearest_distance_squared {
            nearest_distance_squared = distance_squared;
            nearest_point = point;
        }
    }

    let distance = nearest_distance_squared.sqrt();
    if point_inside_blob_membrane(center, blob) {
        let normal =
            (nearest_point - center).normalize_or((center - blob.center()).normalize_or(Vec2::Y));
        Some((radius + distance, normal))
    } else if distance < radius {
        let normal =
            (center - nearest_point).normalize_or((center - blob.center()).normalize_or(Vec2::Y));
        Some((radius - distance, normal))
    } else {
        None
    }
}
