//! Deterministic membrane geometry for pseudo-spines.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SpineSpec {
    pub(super) contour_fraction: f32,
    pub(super) length_factor: f32,
    pub(super) width_factor: f32,
}

pub(crate) fn shield_spine_fans(
    blob_id: u64,
    blob: &Blob,
    extension: f32,
    platforms: &[Platform],
    contour: &[Vec2],
) -> Vec<(Vec<Vec2>, Vec2)> {
    if extension <= 0.01 {
        return Vec::new();
    }
    let center = blob.center();
    let perimeter = contour
        .iter()
        .copied()
        .zip(contour.iter().copied().cycle().skip(1))
        .take(contour.len())
        .map(|(start, end)| start.distance(end))
        .sum::<f32>();
    spine_layout(blob_id, shield_spine_count(blob.rest_radius))
        .into_iter()
        .map(|spec| {
            let size_multiplier = spine_size_multiplier(blob.rest_radius);
            let center_distance = spec.contour_fraction * perimeter;
            let base_center = sample_closed_contour(contour, center_distance, perimeter);
            let length = blob.rest_radius * spec.length_factor * extension * size_multiplier;
            let desired_tip = base_center + (base_center - center).normalize_or(Vec2::Y) * length;
            let tip = clip_spine_tip(
                base_center,
                desired_tip,
                platforms,
                (0.8 * blob.size_scale()).max(0.35),
            );
            let half_width = blob.rest_edge
                * (0.40 + extension * 0.10)
                * spec.width_factor
                * size_multiplier.powf(0.68);
            (
                contour_arc(
                    contour,
                    center_distance - half_width,
                    center_distance + half_width,
                    perimeter,
                ),
                tip,
            )
        })
        .collect()
}

pub(super) fn spine_size_multiplier(radius: f32) -> f32 {
    1.12 * (INITIAL_RADIUS / radius.max(MIN_SHIELD_RADIUS))
        .sqrt()
        .clamp(1.0, 1.75)
}

pub(super) fn sample_closed_contour(contour: &[Vec2], distance: f32, perimeter: f32) -> Vec2 {
    let target = distance.rem_euclid(perimeter);
    let mut traversed = 0.0;
    for (start, end) in contour
        .iter()
        .copied()
        .zip(contour.iter().copied().cycle().skip(1))
        .take(contour.len())
    {
        let edge_length = start.distance(end);
        if traversed + edge_length >= target {
            return start.lerp(end, (target - traversed) / edge_length.max(f32::EPSILON));
        }
        traversed += edge_length;
    }
    contour[0]
}

pub(super) fn contour_arc(contour: &[Vec2], start: f32, end: f32, perimeter: f32) -> Vec<Vec2> {
    let mut points = vec![sample_closed_contour(contour, start, perimeter)];
    let mut traversed = 0.0;
    let mut interior = Vec::new();
    for index in 0..contour.len() {
        if index > 0 {
            traversed += contour[index - 1].distance(contour[index]);
        }
        let mut unwrapped = traversed;
        while unwrapped <= start {
            unwrapped += perimeter;
        }
        if unwrapped < end {
            interior.push((unwrapped, contour[index]));
        }
    }
    interior.sort_by(|first, second| first.0.total_cmp(&second.0));
    points.extend(interior.into_iter().map(|(_, point)| point));
    points.push(sample_closed_contour(contour, end, perimeter));
    points.dedup_by(|first, second| first.distance_squared(*second) < 0.000_001);
    points
}

pub(super) fn shield_spine_count(radius: f32) -> usize {
    let relative = (radius / INITIAL_RADIUS).clamp(0.3, 1.4);
    (6.0 + relative * 10.0).round() as usize
}

pub(super) fn spine_layout(blob_id: u64, count: usize) -> Vec<SpineSpec> {
    let phase = spine_random(blob_id, count as u64, 7) / count as f32;
    (0..count)
        .map(|index| SpineSpec {
            contour_fraction: (phase
                + (index as f32 + 0.16 + spine_random(blob_id, index as u64, 0) * 0.68)
                    / count as f32)
                .fract(),
            length_factor: 0.18 + spine_random(blob_id, index as u64, 1) * 0.24,
            width_factor: 0.72 + spine_random(blob_id, index as u64, 2) * 0.56,
        })
        .collect()
}

fn spine_random(blob_id: u64, index: u64, channel: u64) -> f32 {
    let mut value = blob_id
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(index.wrapping_mul(0xbf58_476d_1ce4_e5b9))
        .wrapping_add(channel.wrapping_mul(0x94d0_49bb_1331_11eb));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value as u32) as f32 / u32::MAX as f32
}

pub(super) fn clip_spine_tip(
    base: Vec2,
    desired_tip: Vec2,
    platforms: &[Platform],
    clearance: f32,
) -> Vec2 {
    clip_spine_tip_with_contact(base, desired_tip, platforms, clearance).0
}

pub(super) fn clip_spine_tip_with_contact(
    base: Vec2,
    desired_tip: Vec2,
    platforms: &[Platform],
    clearance: f32,
) -> (Vec2, bool) {
    let direction = desired_tip - base;
    let length = direction.length();
    if length <= 0.0001 {
        return (base, false);
    }
    let first_contact = platforms
        .iter()
        .filter_map(|platform| segment_aabb_entry(base, desired_tip, platform))
        .reduce(f32::min);
    let Some(first_contact) = first_contact else {
        return (desired_tip, false);
    };
    let clearance_fraction = clearance / length;
    (
        base + direction * (first_contact - clearance_fraction).clamp(0.0, 1.0),
        true,
    )
}

fn segment_aabb_entry(start: Vec2, end: Vec2, platform: &Platform) -> Option<f32> {
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
                return None;
            }
            continue;
        }
        let first = (min_axis - origin) / delta;
        let second = (max_axis - origin) / delta;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
        if near > far {
            return None;
        }
    }
    (far >= 0.0 && near <= 1.0).then_some(near.clamp(0.0, 1.0))
}
