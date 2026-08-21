use super::*;

pub(super) fn blob_vital_color(
    parent_id: Option<u64>,
    selected: bool,
    vitality: Vitality,
) -> Color {
    let base = blob_fill_color(parent_id, selected);
    let fade = 0.52 + vitality.energy * 0.48;
    let linear = base.to_srgba();
    Color::srgba(
        linear.red * fade,
        linear.green * fade,
        linear.blue * fade,
        linear.alpha,
    )
}

pub(super) fn blob_outline_color(
    parent_id: Option<u64>,
    selected: bool,
    vitality: Vitality,
) -> Color {
    if !vitality.is_alive() {
        return Color::srgba(0.24, 0.28, 0.30, 0.88);
    }
    let (red, green, blue) = blob_family_rgb(parent_id);
    if selected {
        Color::srgba(
            (red * 1.18 + 0.16).min(1.0),
            (green * 1.18 + 0.16).min(1.0),
            (blue * 1.18 + 0.16).min(1.0),
            0.98,
        )
    } else {
        Color::srgba(red * 0.38, green * 0.38, blue * 0.38, 0.78)
    }
}

pub(super) fn blob_family_rgb(parent_id: Option<u64>) -> (f32, f32, f32) {
    const FAMILY_COLORS: [(f32, f32, f32); 6] = [
        (0.30, 0.82, 0.72),
        (0.42, 0.68, 1.00),
        (0.88, 0.48, 0.82),
        (1.00, 0.58, 0.34),
        (0.62, 0.82, 0.34),
        (0.65, 0.52, 0.96),
    ];
    let index = parent_id
        .map(|id| (id as usize).wrapping_mul(5).wrapping_add(1) % FAMILY_COLORS.len())
        .unwrap_or(0);
    FAMILY_COLORS[index]
}

#[cfg(test)]
pub(crate) fn blob_family_color(parent_id: Option<u64>) -> Color {
    let (red, green, blue) = blob_family_rgb(parent_id);
    Color::srgba(red, green, blue, 0.9)
}

pub(crate) fn blob_fill_color(parent_id: Option<u64>, selected: bool) -> Color {
    let (red, green, blue) = blob_family_rgb(parent_id);
    if selected {
        Color::srgba(
            (red * 1.12 + 0.10).min(1.0),
            (green * 1.12 + 0.10).min(1.0),
            (blue * 1.12 + 0.10).min(1.0),
            0.96,
        )
    } else {
        Color::srgba(red * 0.72, green * 0.72, blue * 0.72, 0.62)
    }
}

/// Inexpensive 2D diffuse lighting shared by all blob rendering layers.
pub(super) fn blob_vertex_light(
    position: Vec2,
    outward_normal: Vec2,
    lights: &[LightDefinition],
    center_vertex: bool,
) -> [f32; 4] {
    let mut rgb = Vec3::new(0.38, 0.44, 0.48);
    for light in lights.iter().filter(|light| light.enabled) {
        let toward_light = light.position - position;
        let distance = toward_light.length();
        if distance >= light.radius {
            continue;
        }
        let radial = 1.0 - distance / light.radius;
        let facing = if center_vertex {
            0.42
        } else {
            0.12 + 0.88
                * outward_normal
                    .dot(toward_light.normalize_or_zero())
                    .max(0.0)
        };
        rgb += Vec3::from_array(light.color) * radial * facing * light.intensity;
    }
    [rgb.x.min(1.0), rgb.y.min(1.0), rgb.z.min(1.0), 1.0]
}
