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
    // Keep the membrane proportional to the creature. The old fixed minimum
    // dominated small fragments and made their three layers look like a thick
    // painted outline.
    let thickness_ratio = if selected { 0.043 } else { 0.036 };
    let thickness =
        (blob.rest_radius * thickness_ratio * (1.0 + shield_extension * 0.58)).clamp(1.25, 3.4);
    // Cover the sub-pixel seam produced when two translucent, antialiased
    // contours terminate on the exact same physical coordinate. This affects
    // only the membrane artwork; collision geometry remains unchanged.
    let outer_expansion = (blob.rest_radius * 0.009).clamp(0.35, 0.75);
    let mut positions = Vec::with_capacity(count * 3);
    let mut colors = Vec::with_capacity(count * 3);
    let base = blob_outline_color(parent_id, selected, vitality).to_srgba();
    let base_rgb = Vec3::new(base.red, base.green, base.blue)
        .lerp(Vec3::new(0.34, 0.88, 1.0), shield_extension * 0.62);

    for (point, inward) in contour.points.iter().zip(&contour.inward_normals) {
        let outer = point.position - *inward * outer_expansion;
        positions.push([outer.x, outer.y, 0.0]);
    }
    for (index, (point, inward)) in contour
        .points
        .iter()
        .zip(&contour.inward_normals)
        .enumerate()
    {
        let organic = 1.0 + (index as f32 * 1.73 + blob_id as f32 * 0.31).sin() * 0.045;
        let transition = point.position + *inward * thickness * organic * 0.34;
        positions.push([transition.x, transition.y, 0.0]);
    }
    for (index, (point, inward)) in contour
        .points
        .iter()
        .zip(&contour.inward_normals)
        .enumerate()
    {
        let organic = 1.0 + (index as f32 * 1.73 + blob_id as f32 * 0.31).sin() * 0.045;
        let inner = point.position + *inward * thickness * organic;
        positions.push([inner.x, inner.y, 0.0]);
    }
    for (point, inward) in contour.points.iter().zip(&contour.inward_normals) {
        let illumination = blob_vertex_light(point.position, -*inward, lights, false);
        let energy = 0.72 + vitality.energy * 0.28;
        colors.push([
            (base_rgb.x * (0.62 + illumination[0] * 0.70) * energy).min(1.0),
            (base_rgb.y * (0.62 + illumination[1] * 0.70) * energy).min(1.0),
            (base_rgb.z * (0.62 + illumination[2] * 0.70) * energy).min(1.0),
            if selected { 0.98 } else { 0.94 },
        ]);
    }
    colors.extend((0..count).map(|_| {
        [
            base_rgb.x * 0.76,
            base_rgb.y * 0.76,
            base_rgb.z * 0.76,
            0.58,
        ]
    }));
    colors.extend((0..count).map(|_| {
        [
            base_rgb.x * 0.34,
            base_rgb.y * 0.34,
            base_rgb.z * 0.34,
            0.26,
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
