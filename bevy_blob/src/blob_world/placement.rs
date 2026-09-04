//! Safe placement and line-of-sight queries against authored geometry.

use super::*;

pub(crate) fn place_blob_clear(
    blob: &mut Blob,
    platforms: &[Platform],
    fixtures: &[Vec<Vec2>],
) -> bool {
    let initial_center = blob.center();
    let clearance_radius = blob.rest_radius + 3.0 * blob.size_scale();
    for _ in 0..16 {
        let center = blob.center();
        let correction = platforms
            .iter()
            .find_map(|platform| merge_circle_aabb_penetration(center, clearance_radius, platform))
            .or_else(|| {
                fixtures.iter().find_map(|vertices| {
                    merge_circle_convex_penetration(center, clearance_radius, vertices)
                })
            });
        let Some((depth, normal)) = correction else {
            return blob.center().distance(initial_center) <= blob.rest_radius * 1.1;
        };
        blob.translate(normal * (depth + 0.5));
    }
    false
}

fn merge_circle_aabb_penetration(
    center: Vec2,
    radius: f32,
    platform: &Platform,
) -> Option<(f32, Vec2)> {
    let local = center - platform.center;
    let closest = local.clamp(-platform.half_size, platform.half_size);
    let delta = local - closest;
    let distance = delta.length();
    if distance > 0.001 {
        return (distance < radius).then(|| (radius - distance, delta / distance));
    }
    let x_clearance = platform.half_size.x - local.x.abs();
    let y_clearance = platform.half_size.y - local.y.abs();
    if x_clearance < y_clearance {
        let side = if local.x >= 0.0 { 1.0 } else { -1.0 };
        Some((radius + x_clearance, Vec2::new(side, 0.0)))
    } else {
        let side = if local.y >= 0.0 { 1.0 } else { -1.0 };
        Some((radius + y_clearance, Vec2::new(0.0, side)))
    }
}

fn merge_circle_convex_penetration(
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
    let mut inside = true;
    let mut nearest = (f32::INFINITY, Vec2::Y, Vec2::Y);
    for (first, second) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
        let edge = *second - *first;
        inside &= edge.perp_dot(center - *first) * orientation >= 0.0;
        let t = ((center - *first).dot(edge) / edge.length_squared().max(0.001)).clamp(0.0, 1.0);
        let delta = center - (*first + edge * t);
        if delta.length() < nearest.0 {
            let outward = -edge.perp() * orientation / edge.length().max(0.001);
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

pub(crate) fn path_is_clear(start: Vec2, end: Vec2, platforms: &[Platform]) -> bool {
    !platforms
        .iter()
        .any(|platform| segment_intersects_aabb(start, end, platform))
}

fn segment_intersects_aabb(start: Vec2, end: Vec2, platform: &Platform) -> bool {
    let minimum = platform.center - platform.half_size;
    let maximum = platform.center + platform.half_size;
    let direction = end - start;
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;

    for (origin, delta, min_axis, max_axis) in [
        (start.x, direction.x, minimum.x, maximum.x),
        (start.y, direction.y, minimum.y, maximum.y),
    ] {
        if delta.abs() < 0.0001 {
            if origin < min_axis || origin > max_axis {
                return false;
            }
            continue;
        }
        let first = (min_axis - origin) / delta;
        let second = (max_axis - origin) / delta;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
        if near > far {
            return false;
        }
    }
    far >= 0.0 && near <= 1.0
}
