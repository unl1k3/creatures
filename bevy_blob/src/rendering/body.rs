use super::*;

#[cfg(test)]
pub(crate) fn create_blob_mesh(blob: &Blob) -> Mesh {
    create_blob_mesh_with_load(blob, None, &[])
}

pub(super) fn create_blob_mesh_with_load(
    blob: &Blob,
    load: Option<(Vec2, f32, f32, f32, usize, f32)>,
    lights: &[LightDefinition],
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    update_blob_mesh_with_load(&mut mesh, blob, load, lights);
    mesh
}

pub(super) fn update_blob_mesh_with_load(
    mesh: &mut Mesh,
    blob: &Blob,
    load: Option<(Vec2, f32, f32, f32, usize, f32)>,
    lights: &[LightDefinition],
) {
    let center = blob.center();
    let membrane = rendered_membrane_points(blob, load);
    let mut positions = Vec::with_capacity(membrane.len() + 1);
    let mut uvs = Vec::with_capacity(membrane.len() + 1);
    let mut colors = Vec::with_capacity(membrane.len() + 1);
    positions.push([center.x, center.y, 0.0]);
    uvs.push([0.5, 0.5]);
    let base_colours_only = super::palette::uses_base_colors(lights);
    let volume_light = volume_light_direction(center, lights);
    let center_light = blob_vertex_light(center, Vec2::Y, lights, true);
    // The centre remains the thickest part of the gelatin.  Brightening it
    // slightly while the rim falls away turns the existing 2D fan into a
    // stable faux-volume without adding a shader or physics-only geometry.
    colors.push(if base_colours_only {
        center_light
    } else {
        scale_rgb(center_light, 1.08)
    });
    for point in &membrane {
        positions.push([point.position.x, point.position.y, 0.0]);
        let local = (point.position - center) / (blob.rest_radius * 2.0);
        uvs.push([0.5 + local.x, 0.5 + local.y]);
        let illumination = blob_vertex_light(
            point.position,
            (point.position - center).normalize_or(Vec2::Y),
            lights,
            false,
        );
        let radial = (point.position - center).normalize_or(Vec2::Y);
        // The virtual light is shared by every membrane point, so the bright
        // lobe follows the level lanterns rather than jumping with particle
        // indices as the soft body changes shape.
        let facing = radial.dot(volume_light);
        let rim_shade = 0.68 + ((facing + 1.0) * 0.5) * 0.24;
        colors.push(if base_colours_only {
            illumination
        } else {
            scale_rgb(illumination, rim_shade)
        });
    }

    let mut indices = Vec::with_capacity(membrane.len() * 3);
    let original = membrane
        .iter()
        .enumerate()
        .filter(|(_, point)| !point.appendage)
        .map(|(index, _)| index as u32 + 1)
        .collect::<Vec<_>>();
    for index in 0..original.len() {
        indices.extend_from_slice(&[0, original[index], original[(index + 1) % original.len()]]);
    }
    if let Some(first_appendage) = membrane.iter().position(|point| point.appendage) {
        let last_appendage = membrane
            .iter()
            .rposition(|point| point.appendage)
            .unwrap_or(first_appendage);
        let start = first_appendage.saturating_sub(1);
        let end = (last_appendage + 1) % membrane.len();
        let mut appendage = vec![start as u32 + 1];
        appendage.extend((first_appendage..=last_appendage).map(|index| index as u32 + 1));
        appendage.push(end as u32 + 1);
        triangulate_appendage(&appendage, &membrane, &mut indices);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}

/// Finds the weighted direction of nearby lamps. A fixed high-left fallback
/// keeps the body readable in levels without authored lights.
fn volume_light_direction(center: Vec2, lights: &[LightDefinition]) -> Vec2 {
    let contribution = lights
        .iter()
        .filter(|light| light.enabled && light.intensity > f32::EPSILON)
        .filter_map(|light| {
            let toward_light = light.position - center;
            let distance = toward_light.length();
            (distance < light.radius).then(|| {
                toward_light.normalize_or_zero() * (1.0 - distance / light.radius) * light.intensity
            })
        })
        .sum::<Vec2>();
    contribution.normalize_or(Vec2::new(-0.42, 0.91))
}

fn scale_rgb(mut color: [f32; 4], multiplier: f32) -> [f32; 4] {
    color[0] = (color[0] * multiplier).min(1.0);
    color[1] = (color[1] * multiplier).min(1.0);
    color[2] = (color[2] * multiplier).min(1.0);
    color
}

fn triangulate_appendage(
    polygon: &[u32],
    membrane: &[RenderedMembranePoint],
    indices: &mut Vec<u32>,
) {
    let point = |index: u32| membrane[index as usize - 1].position;
    let area = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(a, b)| point(*a).perp_dot(point(*b)))
        .sum::<f32>();
    let orientation = area.signum();
    let mut remaining = polygon.to_vec();
    while remaining.len() > 2 {
        let mut ear = None;
        for index in 0..remaining.len() {
            let previous = remaining[(index + remaining.len() - 1) % remaining.len()];
            let current = remaining[index];
            let next = remaining[(index + 1) % remaining.len()];
            let a = point(previous);
            let b = point(current);
            let c = point(next);
            if (b - a).perp_dot(c - b) * orientation <= 0.000_001 {
                continue;
            }
            let contains_point = remaining.iter().any(|candidate| {
                *candidate != previous
                    && *candidate != current
                    && *candidate != next
                    && point_in_triangle(point(*candidate), a, b, c)
            });
            if !contains_point {
                ear = Some((index, [previous, current, next]));
                break;
            }
        }
        let Some((index, triangle)) = ear else {
            for fan_index in 1..remaining.len().saturating_sub(1) {
                indices.extend_from_slice(&[
                    remaining[0],
                    remaining[fan_index],
                    remaining[fan_index + 1],
                ]);
            }
            return;
        };
        indices.extend_from_slice(&triangle);
        remaining.remove(index);
    }
}

fn point_in_triangle(point: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let first = (b - a).perp_dot(point - a);
    let second = (c - b).perp_dot(point - b);
    let third = (a - c).perp_dot(point - c);
    (first >= -0.000_01 && second >= -0.000_01 && third >= -0.000_01)
        || (first <= 0.000_01 && second <= 0.000_01 && third <= 0.000_01)
}
