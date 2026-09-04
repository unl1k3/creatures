use super::*;

/// Complete visual state needed to rebuild one blob membrane mesh.
pub(super) struct MembraneRenderContext<'a> {
    pub(super) blob: &'a Blob,
    pub(super) load: Option<(Vec2, f32, f32, f32, usize, f32)>,
    pub(super) selected: bool,
    pub(super) parent_id: Option<u64>,
    pub(super) vitality: Vitality,
    pub(super) lights: &'a [LightDefinition],
    pub(super) blob_id: u64,
    pub(super) shield_extension: f32,
    pub(super) shield_energy: f32,
    pub(super) platforms: &'a [Platform],
}

pub(super) fn update_blob_outline_mesh(mesh: &mut Mesh, context: MembraneRenderContext<'_>) {
    let MembraneRenderContext {
        blob,
        load,
        selected,
        parent_id,
        vitality,
        lights,
        blob_id,
        shield_extension,
        shield_energy,
        platforms,
    } = context;
    let contour = RenderedBlobContour::new(blob, load);
    let count = contour.points.len();
    // Keep the membrane proportional to the creature. The old fixed minimum
    // dominated small fragments and made their three layers look like a thick
    // painted outline.
    let thickness_ratio = if selected { 0.043 } else { 0.036 };
    let thickness =
        (blob.rest_radius * thickness_ratio * (1.0 + shield_extension * 0.58)).clamp(1.25, 3.4);
    let mut positions = Vec::with_capacity(count * 3);
    let mut colors = Vec::with_capacity(count * 3);
    let base = blob_outline_color(parent_id, selected, vitality).to_srgba();
    let base_rgb = Vec3::new(base.red, base.green, base.blue)
        .lerp(Vec3::new(0.34, 0.88, 1.0), shield_extension * 0.62);

    for point in &contour.points {
        positions.push([point.position.x, point.position.y, 0.0]);
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
    // The three rings are not merely progressively darker copies.  Each one
    // samples the same local light as the body, then uses a reduced response.
    // This keeps the translucent membrane attached to the lit side instead of
    // looking like a fixed dark sticker around the blob.
    let mut outer_colors = Vec::with_capacity(count);
    let mut transition_colors = Vec::with_capacity(count);
    let mut inner_colors = Vec::with_capacity(count);
    let base_colours_only = super::palette::uses_base_colors(lights);
    for (point, inward) in contour.points.iter().zip(&contour.inward_normals) {
        let illumination = blob_vertex_light(point.position, -*inward, lights, false);
        let energy = 0.72 + vitality.energy * 0.28;
        if base_colours_only {
            outer_colors.push([
                base_rgb.x,
                base_rgb.y,
                base_rgb.z,
                if selected { 0.94 } else { 0.84 },
            ]);
            transition_colors.push([
                base_rgb.x * 0.58,
                base_rgb.y * 0.58,
                base_rgb.z * 0.58,
                if selected { 0.58 } else { 0.48 },
            ]);
            inner_colors.push([
                base_rgb.x * 0.30,
                base_rgb.y * 0.30,
                base_rgb.z * 0.30,
                0.22,
            ]);
            continue;
        }
        outer_colors.push([
            (base_rgb.x * (0.42 + illumination[0] * 0.92) * energy).min(1.0),
            (base_rgb.y * (0.42 + illumination[1] * 0.92) * energy).min(1.0),
            (base_rgb.z * (0.42 + illumination[2] * 0.92) * energy).min(1.0),
            if selected { 0.94 } else { 0.84 },
        ]);
        transition_colors.push([
            (base_rgb.x * (0.24 + illumination[0] * 0.60) * energy).min(1.0),
            (base_rgb.y * (0.24 + illumination[1] * 0.60) * energy).min(1.0),
            (base_rgb.z * (0.24 + illumination[2] * 0.60) * energy).min(1.0),
            if selected { 0.58 } else { 0.48 },
        ]);
        inner_colors.push([
            (base_rgb.x * (0.10 + illumination[0] * 0.30) * energy).min(1.0),
            (base_rgb.y * (0.10 + illumination[1] * 0.30) * energy).min(1.0),
            (base_rgb.z * (0.10 + illumination[2] * 0.30) * energy).min(1.0),
            0.22,
        ]);
    }
    colors.extend(outer_colors);
    colors.extend(transition_colors);
    colors.extend(inner_colors);

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
                crate::palette::MEMBRANE_SHIELD_BASE,
                crate::palette::scale_rgb(crate::palette::MEMBRANE_SHIELD_TIP, brightness),
                crate::palette::MEMBRANE_SHIELD_BASE,
            ]);
            indices.extend_from_slice(&[first, first + 1, first + 2]);
        }
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}
