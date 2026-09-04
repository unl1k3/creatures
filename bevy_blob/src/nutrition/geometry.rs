//! Geometric predicates shared by digestion, probes, and free nutrient safety.

use super::*;

pub(super) fn point_inside_convex(point: Vec2, vertices: &[Vec2]) -> bool {
    if vertices.len() < 3 {
        return false;
    }
    let mut sign = 0.0_f32;
    for (first, second) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
        let cross = (*second - *first).perp_dot(point - *first);
        if cross.abs() <= 0.001 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    sign != 0.0
}

pub(super) fn circle_aabb_penetration(
    center: Vec2,
    radius: f32,
    box_center: Vec2,
    half_size: Vec2,
) -> Option<(f32, Vec2)> {
    let local = center - box_center;
    let closest = local.clamp(-half_size, half_size);
    let delta = local - closest;
    let distance = delta.length();
    if distance > 0.001 {
        return (distance < radius).then(|| (radius - distance, delta / distance));
    }
    let x_clearance = half_size.x - local.x.abs();
    let y_clearance = half_size.y - local.y.abs();
    if x_clearance < y_clearance {
        let side = if local.x >= 0.0 { 1.0 } else { -1.0 };
        Some((radius + x_clearance, Vec2::new(side, 0.0)))
    } else {
        let side = if local.y >= 0.0 { 1.0 } else { -1.0 };
        Some((radius + y_clearance, Vec2::new(0.0, side)))
    }
}

pub(super) fn circle_convex_penetration(
    center: Vec2,
    radius: f32,
    vertices: &[Vec2],
) -> Option<(f32, Vec2)> {
    if vertices.len() < 3 {
        return None;
    }
    let orientation = vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .map(|(first, second)| first.perp_dot(*second))
        .sum::<f32>()
        .signum();
    if orientation == 0.0 {
        return None;
    }
    let mut nearest = (f32::INFINITY, Vec2::Y, Vec2::Y);
    let mut inside = true;
    for (first, second) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
        let edge = *second - *first;
        let length = edge.length().max(0.001);
        inside &= edge.perp_dot(center - *first) * orientation >= 0.0;
        let t = ((center - *first).dot(edge) / edge.length_squared().max(0.001)).clamp(0.0, 1.0);
        let delta = center - (*first + edge * t);
        if delta.length() < nearest.0 {
            let outward = -edge.perp() * orientation / length;
            nearest = (delta.length(), outward, delta.normalize_or(outward));
        }
    }
    if inside {
        Some((radius + nearest.0, nearest.1))
    } else if nearest.0 < radius {
        Some((radius - nearest.0, nearest.2))
    } else {
        None
    }
}
