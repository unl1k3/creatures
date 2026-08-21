use super::*;

pub(super) fn update_blob_outline_mesh(
    mesh: &mut Mesh,
    blob: &Blob,
    load: Option<(Vec2, f32, f32, f32, usize, f32)>,
    selected: bool,
    parent_id: Option<u64>,
    vitality: Vitality,
    lights: &[LightDefinition],
    blob_id: u64,
    shield_extension: f32,
    shield_energy: f32,
    platforms: &[Platform],
) {
    let contour = RenderedBlobContour::new(blob, load);
    let count = contour.points.len();
    let thickness =
        (if selected { 7.0 } else { 5.2 } * blob.size_scale() * (1.0 + shield_extension * 0.52))
            .max(2.4);
    let transition_depth = thickness * 0.42;
    let mut positions = Vec::with_capacity(count * 3);
    let mut colors = Vec::with_capacity(count * 3);
    let base = blob_outline_color(parent_id, selected, vitality).to_srgba();
    let base_rgb = Vec3::new(base.red, base.green, base.blue)
        .lerp(Vec3::new(0.34, 0.88, 1.0), shield_extension * 0.62);

    for point in &contour.points {
        positions.push([point.position.x, point.position.y, 0.0]);
    }
    for (point, inward) in contour.points.iter().zip(&contour.inward_normals) {
        let transition = point.position + *inward * transition_depth;
        positions.push([transition.x, transition.y, 0.0]);
    }
    for (point, inward) in contour.points.iter().zip(&contour.inward_normals) {
        let inner = point.position + *inward * thickness;
        positions.push([inner.x, inner.y, 0.0]);
    }
    for (point, inward) in contour.points.iter().zip(&contour.inward_normals) {
        let illumination = blob_vertex_light(point.position, -*inward, lights, false);
        let energy = 0.72 + vitality.energy * 0.28;
        colors.push([
            (base_rgb.x * (0.62 + illumination[0] * 0.70) * energy).min(1.0),
            (base_rgb.y * (0.62 + illumination[1] * 0.70) * energy).min(1.0),
            (base_rgb.z * (0.62 + illumination[2] * 0.70) * energy).min(1.0),
            if selected { 0.98 } else { 0.90 },
        ]);
    }
    colors.extend((0..count).map(|_| {
        [
            base_rgb.x * 0.76,
            base_rgb.y * 0.76,
            base_rgb.z * 0.76,
            0.88,
        ]
    }));
    colors.extend((0..count).map(|_| {
        [
            base_rgb.x * 0.34,
            base_rgb.y * 0.34,
            base_rgb.z * 0.34,
            0.68,
        ]
    }));

    let mut indices = Vec::with_capacity(count * 12);
    for index in 0..count {
        let next = (index + 1) % count;
        indices.extend_from_slice(&[
            index as u32,
            next as u32,
            (count + next) as u32,
            index as u32,
            (count + next) as u32,
            (count + index) as u32,
            (count + index) as u32,
            (count + next) as u32,
            (count * 2 + next) as u32,
            (count + index) as u32,
            (count * 2 + next) as u32,
            (count * 2 + index) as u32,
        ]);
    }
    let positions_only = contour.positions();
    for (base_arc, tip) in
        shield_spine_fans(blob_id, blob, shield_extension, platforms, &positions_only)
    {
        let brightness = 0.58 + shield_energy * 0.42;
        for edge in base_arc.windows(2) {
            let first = positions.len() as u32;
            positions.extend_from_slice(&[
                [edge[0].x, edge[0].y, 0.0],
                [tip.x, tip.y, 0.0],
                [edge[1].x, edge[1].y, 0.0],
            ]);
            colors.extend_from_slice(&[
                [0.22, 0.72, 0.82, 0.82],
                [0.52 * brightness, 0.96 * brightness, brightness, 0.96],
                [0.22, 0.72, 0.82, 0.82],
            ]);
            indices.extend_from_slice(&[first, first + 1, first + 2]);
        }
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}
