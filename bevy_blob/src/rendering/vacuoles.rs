use super::palette::blob_family_rgb;
use super::*;

pub(super) fn update_blob_vacuole_mesh(
    mesh: &mut Mesh,
    active_blob: &ActiveBlob,
    elapsed: f32,
    alive: bool,
    lights: &[LightDefinition],
) {
    let blob = &active_blob.body;
    // Keep topology allocated after death. Replacing this dynamic mesh with
    // empty buffers can race Bevy's render extraction and free its slab before
    // the last queued copy completes.
    let visibility = alive as u8 as f32;
    let motion_time = if alive { elapsed } else { 0.0 };

    const SEGMENTS: usize = 10;
    const INTERNAL_ROTATION_FOLLOW: f32 = 0.22;
    let count = ((blob.rest_radius / 10.0).round() as usize).clamp(3, 7);
    let center = blob.center();
    let average_motion = blob
        .particles
        .iter()
        .map(|particle| particle.position - particle.previous)
        .sum::<Vec2>()
        / blob.particles.len() as f32;
    let inertial_offset = (-average_motion * 3.2).clamp_length_max(blob.rest_radius * 0.16);
    // Follow the material orientation rather than linear movement. Vacuoles
    // suspended in fluid should rotate gently when the body rolls, while a
    // pure translation should only produce the inertial lag above.
    let material_rotation = blob
        .particles
        .iter()
        .enumerate()
        .map(|(index, particle)| {
            let material_angle = index as f32 / blob.particles.len() as f32 * std::f32::consts::TAU;
            let radial =
                (particle.position - center).normalize_or(Vec2::from_angle(material_angle));
            Vec2::from_angle(-material_angle).rotate(radial)
        })
        .sum::<Vec2>()
        .normalize_or(Vec2::X);
    let polygon = blob
        .particles
        .iter()
        .map(|particle| particle.position)
        .collect::<Vec<_>>();
    let mut positions = Vec::with_capacity(count * (SEGMENTS + 1));
    let mut colors = Vec::with_capacity(count * (SEGMENTS + 1));
    let mut indices = Vec::with_capacity(count * SEGMENTS * 3);

    for index in 0..count {
        let angle_seed = random_unit(active_blob.id, index, 0) * std::f32::consts::TAU;
        let radial_seed = 0.16 + random_unit(active_blob.id, index, 1) * 0.38;
        let phase =
            angle_seed + motion_time * (0.34 + random_unit(active_blob.id, index, 2) * 0.30);
        let resting_base = Vec2::from_angle(angle_seed) * blob.rest_radius * radial_seed;
        let rotated_base = material_rotation.rotate(resting_base);
        let base = resting_base.lerp(rotated_base, INTERNAL_ROTATION_FOLLOW);
        let drift = Vec2::new(
            phase.sin() * blob.rest_radius * 0.055,
            (phase * 0.73).cos() * blob.rest_radius * 0.075,
        );
        let radius =
            (blob.rest_radius * (0.075 + random_unit(active_blob.id, index, 3) * 0.055)).max(1.5);
        let vacuole_center = fit_circle_inside_polygon(
            center,
            center + base + drift + inertial_offset,
            radius * 1.35,
            &polygon,
        );
        let first_vertex = positions.len() as u32;
        positions.push([vacuole_center.x, vacuole_center.y, 0.0]);
        let light = blob_vertex_light(vacuole_center, Vec2::Y, lights, true);
        let tint = vacuole_tint(active_blob.parent_id, index);
        let visible_light = [
            0.40 + light[0] * 0.60,
            0.44 + light[1] * 0.56,
            0.48 + light[2] * 0.52,
        ];
        colors.push([
            visible_light[0] * tint.x * 0.72,
            visible_light[1] * tint.y * 0.76,
            visible_light[2] * tint.z * 0.80,
            0.72 * visibility,
        ]);

        let stretch = 0.82 + random_unit(active_blob.id, index, 4) * 0.34;
        let tilt = angle_seed * 1.7;
        for segment in 0..SEGMENTS {
            let angle = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let organic = 1.0 + 0.08 * (angle * 3.0 + phase).sin();
            let local = Vec2::from_angle(tilt).rotate(Vec2::new(
                angle.cos() * radius * stretch,
                angle.sin() * radius / stretch,
            )) * organic;
            positions.push([vacuole_center.x + local.x, vacuole_center.y + local.y, 0.0]);
            colors.push([
                visible_light[0] * tint.x,
                visible_light[1] * tint.y,
                visible_light[2] * tint.z,
                visibility,
            ]);
        }
        for segment in 0..SEGMENTS {
            indices.extend_from_slice(&[
                first_vertex,
                first_vertex + 1 + segment as u32,
                first_vertex + 1 + ((segment + 1) % SEGMENTS) as u32,
            ]);
        }
    }

    append_grime_mottles(
        &mut positions,
        &mut colors,
        &mut indices,
        active_blob,
        alive,
        lights,
        center,
        material_rotation,
        &polygon,
    );
    append_rotation_edge_mark(
        &mut positions,
        &mut colors,
        &mut indices,
        active_blob,
        alive,
        lights,
    );

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}

/// A single, darker patch rides one material particle close to the membrane.
/// Unlike the interior grime it is deliberately prominent: it gives the
/// player an intuitive reference for rolling and water-induced rotation.
fn append_rotation_edge_mark(
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    active_blob: &ActiveBlob,
    alive: bool,
    lights: &[LightDefinition],
) {
    const SEGMENTS: usize = 8;
    if active_blob.body.particles.is_empty() {
        return;
    }
    let blob = &active_blob.body;
    let visibility = alive as u8 as f32;
    let center = blob.center();
    // The chosen material index remains stable for this creature's lifetime;
    // it therefore follows the same rotation as the soft-body membrane.
    let particle_index = active_blob.id as usize % blob.particles.len();
    let boundary = blob.particles[particle_index].position;
    let outward = (boundary - center).normalize_or(Vec2::Y);
    let tangent = outward.perp();
    let radial_radius = (blob.rest_radius * 0.075).max(2.0);
    let tangent_radius = radial_radius * 1.55;
    // Pull the patch slightly inward so it seats into, rather than floats
    // outside, the translucent membrane.
    let mark_center = boundary - outward * radial_radius * 0.62;
    let first_vertex = positions.len() as u32;
    let light = blob_vertex_light(boundary, outward, lights, false);
    let mark = crate::palette::BLOB_ROTATION_MARK;
    let shade = [
        (mark[0] * (0.76 + light[0] * 0.34)).min(1.0),
        (mark[1] * (0.76 + light[1] * 0.34)).min(1.0),
        (mark[2] * (0.76 + light[2] * 0.34)).min(1.0),
        mark[3] * visibility,
    ];
    positions.push([mark_center.x, mark_center.y, 0.0]);
    colors.push([shade[0], shade[1], shade[2], 0.76 * visibility]);
    for segment in 0..SEGMENTS {
        let angle = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let irregular = 0.90
            + random_unit(active_blob.id, particle_index, segment as u64 + 41) * 0.18
            + (angle * 2.0 + active_blob.id as f32 * 0.17).sin() * 0.04;
        let point = mark_center
            + tangent * angle.cos() * tangent_radius * irregular
            + outward * angle.sin() * radial_radius * irregular;
        positions.push([point.x, point.y, 0.0]);
        colors.push(shade);
    }
    for segment in 0..SEGMENTS {
        indices.extend_from_slice(&[
            first_vertex,
            first_vertex + 1 + segment as u32,
            first_vertex + 1 + ((segment + 1) % SEGMENTS) as u32,
        ]);
    }
}

/// Adds a few irregular dirt patches under the vacuoles. Their anchors use
/// the membrane material orientation, so they rotate with the blob rather
/// than sliding across it during ordinary translation.
fn append_grime_mottles(
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    active_blob: &ActiveBlob,
    alive: bool,
    lights: &[LightDefinition],
    center: Vec2,
    material_rotation: Vec2,
    polygon: &[Vec2],
) {
    const SEGMENTS: usize = 7;
    let blob = &active_blob.body;
    let count = ((blob.rest_radius / 22.0).round() as usize).clamp(2, 4);
    let visibility = alive as u8 as f32;
    for index in 0..count {
        let seed = index + 19;
        let angle = random_unit(active_blob.id, seed, 0) * std::f32::consts::TAU;
        let distance = blob.rest_radius * (0.16 + random_unit(active_blob.id, seed, 1) * 0.36);
        let radius =
            (blob.rest_radius * (0.085 + random_unit(active_blob.id, seed, 2) * 0.075)).max(1.25);
        let desired = center + material_rotation.rotate(Vec2::from_angle(angle) * distance);
        let mottle_center = fit_circle_inside_polygon(center, desired, radius * 1.55, polygon);
        let first_vertex = positions.len() as u32;
        let light = blob_vertex_light(mottle_center, Vec2::Y, lights, true);
        let grime = crate::palette::BLOB_GRIME;
        let shade = [
            grime[0] * (0.58 + light[0] * 0.42),
            grime[1] * (0.58 + light[1] * 0.42),
            grime[2] * (0.58 + light[2] * 0.42),
            grime[3] * visibility,
        ];
        positions.push([mottle_center.x, mottle_center.y, 0.0]);
        colors.push([shade[0], shade[1], shade[2], shade[3] * 0.70]);
        let tilt = angle * 1.41;
        for segment in 0..SEGMENTS {
            let arc = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let irregular = 0.74
                + random_unit(active_blob.id, seed, segment as u64 + 3) * 0.38
                + (arc * 3.0 + angle).sin() * 0.09;
            let local = Vec2::from_angle(tilt).rotate(Vec2::new(
                arc.cos() * radius * 1.38,
                arc.sin() * radius * 0.76,
            )) * irregular;
            positions.push([mottle_center.x + local.x, mottle_center.y + local.y, 0.0]);
            colors.push(shade);
        }
        for segment in 0..SEGMENTS {
            indices.extend_from_slice(&[
                first_vertex,
                first_vertex + 1 + segment as u32,
                first_vertex + 1 + ((segment + 1) % SEGMENTS) as u32,
            ]);
        }
    }
}

pub(super) fn vacuole_tint(parent_id: Option<u64>, index: usize) -> Vec3 {
    let (red, green, blue) = blob_family_rgb(parent_id);
    let base = Vec3::new(red, green, blue);
    let white = Vec3::ONE;
    match index % 3 {
        0 => (white - base).lerp(white, 0.34),
        1 => Vec3::new(base.z, base.x, base.y).lerp(white, 0.30),
        _ => base.lerp(white, 0.58),
    }
}

fn random_unit(blob_id: u64, index: usize, channel: u64) -> f32 {
    let mut value = blob_id
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((index as u64 + 1).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(channel.wrapping_mul(0x94D0_49BB_1331_11EB));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value as u32) as f32 / u32::MAX as f32
}

fn fit_circle_inside_polygon(center: Vec2, desired: Vec2, radius: f32, polygon: &[Vec2]) -> Vec2 {
    let mut candidate = desired;
    for _ in 0..12 {
        let clearance = polygon
            .iter()
            .copied()
            .zip(polygon.iter().copied().cycle().skip(1))
            .take(polygon.len())
            .map(|(start, end)| point_segment_distance(candidate, start, end))
            .fold(f32::INFINITY, f32::min);
        if point_in_polygon(candidate, polygon) && clearance >= radius {
            break;
        }
        candidate = center.lerp(candidate, 0.78);
    }
    candidate
}

fn point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    polygon
        .iter()
        .copied()
        .zip(polygon.iter().copied().cycle().skip(1))
        .take(polygon.len())
        .fold(false, |inside, (first, second)| {
            let crosses = (first.y > point.y) != (second.y > point.y)
                && point.x
                    < (second.x - first.x) * (point.y - first.y) / (second.y - first.y) + first.x;
            inside ^ crosses
        })
}

fn point_segment_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let edge = end - start;
    let fraction = (point - start).dot(edge) / edge.length_squared().max(f32::EPSILON);
    point.distance(start + edge * fraction.clamp(0.0, 1.0))
}
