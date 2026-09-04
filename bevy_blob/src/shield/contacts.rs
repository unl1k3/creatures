//! Gameplay contacts created by deployed pseudo-spines.

use super::geometry::{
    clip_spine_tip_with_contact, sample_closed_contour, shield_spine_count, spine_layout,
    spine_size_multiplier,
};
use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SpineAnchor {
    pub(crate) direction: f32,
    pub(crate) wall_top: f32,
}

/// Returns the highest spine pressing against a vertical surface. Rendering
/// remains independent: a visible spine never becomes a collider by itself.
pub(crate) fn spider_climb_anchor_direction(
    blob_id: u64,
    blob: &Blob,
    extension: f32,
    platforms: &[Platform],
    fixtures: &[Vec<Vec2>],
) -> Option<SpineAnchor> {
    if extension <= 0.05 {
        return None;
    }

    let contour = blob
        .particles
        .iter()
        .map(|particle| particle.position)
        .collect::<Vec<_>>();
    let perimeter = contour
        .iter()
        .copied()
        .zip(contour.iter().copied().cycle().skip(1))
        .take(contour.len())
        .map(|(start, end)| start.distance(end))
        .sum::<f32>();
    let center = blob.center();
    let clearance = (0.8 * blob.size_scale()).max(0.35);

    spine_layout(blob_id, shield_spine_count(blob.rest_radius))
        .into_iter()
        .filter_map(|spec| {
            let base =
                sample_closed_contour(&contour, spec.contour_fraction * perimeter, perimeter);
            // The small extra reach stabilizes contact while the membrane flexes.
            let reach = blob.rest_radius
                * (spec.length_factor + 0.12)
                * extension
                * spine_size_multiplier(blob.rest_radius);
            let desired_tip = base + (base - center).normalize_or(Vec2::Y) * reach;
            let (tip, platform_contacted) =
                clip_spine_tip_with_contact(base, desired_tip, platforms, clearance);
            let direction = (tip.x - center.x).signum();
            if direction == 0.0 {
                return None;
            }
            let platform_anchor = vertical_wall_top(tip, center, direction, platforms)
                .filter(|_| platform_contacted)
                .map(|wall_top| (tip, wall_top, direction));
            let fixture_anchor =
                first_fixture_contact(base, desired_tip, fixtures).and_then(|(contact, point)| {
                    let direction = (point.x - center.x).signum();
                    (contact > 0.0 && direction != 0.0).then_some((point, point.y, direction))
                });
            platform_anchor
                .into_iter()
                .chain(fixture_anchor)
                .max_by(|first, second| first.0.y.total_cmp(&second.0.y))
                .map(|(tip, wall_top, direction)| (base, tip, wall_top, direction))
        })
        .max_by(|first, second| first.1.y.total_cmp(&second.1.y))
        .map(|(_, _, wall_top, direction)| SpineAnchor {
            direction,
            wall_top,
        })
}

fn first_fixture_contact(start: Vec2, end: Vec2, fixtures: &[Vec<Vec2>]) -> Option<(f32, Vec2)> {
    fixtures
        .iter()
        .flat_map(|fixture| {
            fixture
                .iter()
                .copied()
                .zip(fixture.iter().copied().cycle().skip(1))
                .take(fixture.len())
        })
        .filter_map(|(edge_start, edge_end)| segment_intersection(start, end, edge_start, edge_end))
        .min_by(|first, second| first.0.total_cmp(&second.0))
}

fn segment_intersection(
    start: Vec2,
    end: Vec2,
    edge_start: Vec2,
    edge_end: Vec2,
) -> Option<(f32, Vec2)> {
    let ray = end - start;
    let edge = edge_end - edge_start;
    let denominator = ray.perp_dot(edge);
    if denominator.abs() <= 0.000_01 {
        return None;
    }
    let offset = edge_start - start;
    let ray_fraction = offset.perp_dot(edge) / denominator;
    let edge_fraction = offset.perp_dot(ray) / denominator;
    ((0.0..=1.0).contains(&ray_fraction) && (0.0..=1.0).contains(&edge_fraction))
        .then_some((ray_fraction, start.lerp(end, ray_fraction)))
}

/// Rejects a spine that reached a horizontal face instead of a climbable wall.
fn vertical_wall_top(
    tip: Vec2,
    blob_center: Vec2,
    direction: f32,
    platforms: &[Platform],
) -> Option<f32> {
    const FACE_TOLERANCE: f32 = 2.0;
    platforms.iter().find_map(|platform| {
        let min = platform.center - platform.half_size;
        let max = platform.center + platform.half_size;
        let touches_requested_face = if direction > 0.0 {
            (tip.x - min.x).abs() <= FACE_TOLERANCE
        } else {
            (tip.x - max.x).abs() <= FACE_TOLERANCE
        };
        (touches_requested_face
            && tip.y >= min.y - FACE_TOLERANCE
            && tip.y <= max.y + FACE_TOLERANCE
            && blob_center.y >= min.y
            && blob_center.y <= max.y)
            .then_some(max.y)
    })
}
