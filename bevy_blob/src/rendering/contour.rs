//! Shared visual contour derived from the simulated blob membrane.

use super::*;

pub(crate) fn charge_indicator_radius(blob: &Blob) -> f32 {
    let center = blob.center();
    let outermost = blob
        .particles
        .iter()
        .map(|particle| particle.position.distance(center))
        .fold(blob.rest_radius, f32::max);
    outermost + (5.0 * blob.size_scale()).max(2.5)
}

#[derive(Clone, Copy)]
pub(super) struct RenderedMembranePoint {
    pub(super) position: Vec2,
    pub(super) temporary: bool,
    pub(super) appendage: bool,
    pub(super) attachment: bool,
}

/// Shared per-frame contour consumed by the body, membrane and shield renderers.
pub(super) struct RenderedBlobContour {
    pub(super) points: Vec<RenderedMembranePoint>,
    pub(super) inward_normals: Vec<Vec2>,
}

impl RenderedBlobContour {
    pub(super) fn new(blob: &Blob, load: Option<(Vec2, f32, f32, f32, usize, f32)>) -> Self {
        let points = rendered_membrane_points(blob, load);
        let center = blob.center();
        let count = points.len();
        let inward_normals = (0..count)
            .map(|index| {
                let previous = points[(index + count - 1) % count].position;
                let current = points[index].position;
                let next = points[(index + 1) % count].position;
                let first = (current - previous).perp().normalize_or_zero();
                let second = (next - current).perp().normalize_or_zero();
                (first + second).normalize_or((center - current).normalize_or(Vec2::Y))
            })
            .collect();
        Self {
            points,
            inward_normals,
        }
    }

    pub(super) fn positions(&self) -> Vec<Vec2> {
        self.points.iter().map(|point| point.position).collect()
    }
}

pub(super) fn rendered_membrane_points(
    blob: &Blob,
    load: Option<(Vec2, f32, f32, f32, usize, f32)>,
) -> Vec<RenderedMembranePoint> {
    let Some((load_position, load_radius, strength, variation, anchor_edge, anchor_t)) =
        load.filter(|(_, _, value, _, _, _)| *value > 0.01)
    else {
        return blob
            .particles
            .iter()
            .map(|particle| RenderedMembranePoint {
                position: particle.position,
                temporary: false,
                appendage: false,
                attachment: false,
            })
            .collect();
    };
    let count = blob.particles.len();
    let nearest_edge = anchor_edge % count;
    let center = blob.center();
    let load_direction = (load_position - center).normalize_or(Vec2::X);
    let mut points = Vec::with_capacity(count + 31);
    for index in 0..count {
        let second_anchor_edge = (nearest_edge + 1) % count;
        if index == second_anchor_edge {
            continue;
        }
        let start = blob.particles[index].position;
        let end_index = if index == nearest_edge {
            (index + 2) % count
        } else {
            (index + 1) % count
        };
        let end = blob.particles[end_index].position;
        points.push(RenderedMembranePoint {
            position: start,
            temporary: false,
            appendage: false,
            attachment: false,
        });
        if index != nearest_edge {
            continue;
        }
        let base = blob.particles[(nearest_edge + 1) % count].position;
        let tip = base.lerp(load_position, strength.clamp(0.0, 1.0));
        let length = base.distance(tip);
        let normal_axis = load_direction.perp();
        let secondary = (variation * 7.137).fract();
        let asymmetry = (anchor_t.clamp(0.0, 1.0) - 0.5) * 0.08;
        let start_attachment = start.lerp(base, 0.18 + asymmetry);
        let end_attachment = base.lerp(end, 0.82 + asymmetry);
        let attachment_tangent = (end_attachment - start_attachment).normalize_or(Vec2::X);
        let mut root_normal = attachment_tangent.perp();
        if root_normal.dot(base - center) < 0.0 {
            root_normal = -root_normal;
        }
        let control_a = base
            + root_normal * length * (0.30 + variation * 0.08)
            + attachment_tangent * length * (variation - 0.5) * 0.05;
        let control_b = base.lerp(tip, 0.72) + normal_axis * length * (secondary - 0.5) * 0.18;
        let width = (load_radius * (0.55 + strength * 0.45) * (0.88 + variation * 0.24))
            .min(start.distance(end) * 0.48)
            .max(0.5);
        let base_normal = root_normal.perp();
        // Follow the membrane's existing winding: the first side of the tube
        // must leave from `start`, otherwise the two base triangles cross.
        let winding_side = if (start - base).dot(base_normal) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let attachment_half_width = start_attachment.distance(end_attachment) * 0.5;
        points.push(RenderedMembranePoint {
            position: start_attachment,
            temporary: true,
            appendage: false,
            attachment: true,
        });
        const MINIMAL_PROFILE: &[f32] = &[0.24];
        const MEDIUM_PROFILE: &[f32] = &[0.20, 0.46, 0.70, 0.88, 0.975];
        const FULL_PROFILE: &[f32] = &[
            0.06, 0.14, 0.23, 0.32, 0.42, 0.52, 0.62, 0.71, 0.79, 0.86, 0.92, 0.97, 0.995,
        ];
        let profile = if strength < 0.16 {
            MINIMAL_PROFILE
        } else if strength < 0.38 {
            MEDIUM_PROFILE
        } else {
            FULL_PROFILE
        };
        let mut outline = Vec::with_capacity(profile.len() * 2 + 1);
        outline.extend(profile.iter().copied().map(|along| (along, 1.0)));
        outline.push((1.0, 0.0));
        outline.extend(profile.iter().rev().copied().map(|along| (along, -1.0)));
        for (along, side) in outline {
            let raw_side = side;
            let side = raw_side * winding_side;
            let inverse: f32 = 1.0 - along;
            let centerline = base * inverse.powi(3)
                + control_a * 3.0 * inverse.powi(2) * along
                + control_b * 3.0 * inverse * along.powi(2)
                + tip * along.powi(3);
            let tangent = ((control_a - base) * 3.0 * inverse.powi(2)
                + (control_b - control_a) * 6.0 * inverse * along
                + (tip - control_b) * 3.0 * along.powi(2))
            .normalize_or(load_direction);
            let normal = tangent.perp();
            let organic_wave = 1.0
                + (along * std::f32::consts::PI * (2.0 + variation * 1.4)
                    + variation * std::f32::consts::TAU)
                    .sin()
                    * (along * std::f32::consts::PI).sin()
                    * 0.075;
            let root_flare = 1.0 + (1.0 - along).powi(2) * (0.38 + variation * 0.12);
            let taper = (1.0_f32 - along * (0.76 + secondary * 0.10)).max(0.14);
            let rounded_tip = if along > 0.9 {
                ((1.0 - along) / 0.1).sqrt()
            } else {
                1.0
            };
            let profile_width = (width * root_flare * taper * organic_wave)
                .min(attachment_half_width * (1.0 - along * 0.58).max(0.18));
            let curved_position = centerline + normal * side * profile_width * rounded_tip;
            let attachment = if raw_side >= 0.0 {
                start_attachment
            } else {
                end_attachment
            };
            let root_blend = smoothstep01(along / 0.20);
            points.push(RenderedMembranePoint {
                position: attachment.lerp(curved_position, root_blend),
                temporary: true,
                appendage: true,
                attachment: false,
            });
        }
        points.push(RenderedMembranePoint {
            position: end_attachment,
            temporary: true,
            appendage: false,
            attachment: true,
        });
    }
    points
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}
