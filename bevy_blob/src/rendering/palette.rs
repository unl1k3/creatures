use super::*;

pub(super) fn blob_vital_color(
    parent_id: Option<u64>,
    selected: bool,
    vitality: Vitality,
) -> Color {
    if !vitality.is_alive() {
        return crate::palette::color(crate::palette::DEAD_BLOB);
    }
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
        return crate::palette::color(crate::palette::DEAD_BLOB);
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
    const FAMILY_COLORS: &[[f32; 3]] = &crate::palette::BLOB_FAMILIES;
    // The root blob owns the base colour. Every split uses the parent's stable
    // id to select a different sibling-family colour: both children match one
    // another, including the first pair, but remain distinct from their parent.
    let index = crate::palette::blob_family_index(parent_id);
    let [red, green, blue] = FAMILY_COLORS[index];
    (red, green, blue)
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

/// Inexpensive translucent 2D lighting shared by all blob rendering layers.
pub(super) fn blob_vertex_light(
    position: Vec2,
    outward_normal: Vec2,
    lights: &[LightDefinition],
    center_vertex: bool,
) -> [f32; 4] {
    // The sewer has no visible lamps. When there are no local lights, a
    // broad diffuse illumination filters in from shafts and grates instead
    // of leaving dynamic bodies nearly black.
    if !has_enabled_lights(lights) {
        return sewer_ambient_light(position);
    }
    if uses_base_colors(lights) {
        return [1.0, 1.0, 1.0, 1.0];
    }
    // A low cool ambient keeps the unlit side readable without flattening
    // the warm contribution of nearby lamps.
    // Almost-black neutral ambience: the sewer is readable, but the lanterns
    // must remain the visible source of warmth instead of washing the scene.
    let mut rgb = Vec3::new(0.050, 0.062, 0.065);
    for light in lights.iter().filter(|light| light.enabled) {
        let toward_light = light.position - position;
        let distance = toward_light.length();
        if distance >= light.radius {
            continue;
        }
        let attenuation = lantern_attenuation(distance, light.radius);
        let light_direction = toward_light.normalize_or_zero();
        let normal_light = outward_normal.dot(light_direction);
        let response = if center_vertex {
            0.30
        } else {
            // Wrapped diffuse light suggests subsurface scattering. A weaker
            // back-facing term lets warm light bleed through the gelatin.
            let wrapped = ((normal_light + 0.32) / 1.32).clamp(0.0, 1.0);
            let transmission = (-normal_light).max(0.0).powi(2) * 0.16;
            0.10 + wrapped * 0.66 + transmission
        };
        let light_color = Vec3::from_array(light.color);
        rgb += light_color * attenuation * response * light.intensity;

        if !center_vertex {
            // A broad, coloured wet sheen is more stable on a deforming,
            // vertex-lit membrane than a small white specular spot. The old
            // sharp highlight jumped between membrane points and looked like
            // a detached reflection, especially through clear wastewater.
            let sheen = normal_light.max(0.0).powi(5) * attenuation * light.intensity * 0.075;
            rgb += light_color.lerp(Vec3::ONE, 0.15) * sheen;
        }
    }
    // Reinhard-style compression preserves hue when several lamps overlap,
    // unlike a hard clamp that turns the whole surface white.
    let mapped = rgb / (Vec3::ONE + rgb) * 1.34;
    [mapped.x.min(1.0), mapped.y.min(1.0), mapped.z.min(1.0), 1.0]
}

/// Static scenery uses the same authored lamps as the blob, without a fake
/// surface normal. The light is sampled per mesh vertex so the paper-and-ink
/// platforms can fall into shadow between the lantern pools.
pub(super) fn scenery_vertex_light(position: Vec2, lights: &[LightDefinition]) -> [f32; 4] {
    if !has_enabled_lights(lights) {
        return sewer_ambient_light(position);
    }
    if uses_base_colors(lights) {
        return [1.0, 1.0, 1.0, 1.0];
    }
    let mut rgb = Vec3::new(0.045, 0.055, 0.058);
    for light in lights.iter().filter(|light| light.enabled) {
        let distance = light.position.distance(position);
        if distance >= light.radius {
            continue;
        }
        let attenuation = lantern_attenuation(distance, light.radius);
        rgb += Vec3::from_array(light.color) * attenuation * light.intensity * 0.78;
    }
    let mapped = rgb / (Vec3::ONE + rgb) * 1.42;
    [mapped.x.min(1.0), mapped.y.min(1.0), mapped.z.min(1.0), 1.0]
}

/// Global, source-less illumination for the ink sewer. The playable shaft is
/// open enough above to receive cool daylight through grates, while the basin
/// remains damp and dim. It intentionally has no circular falloff or visible
/// origin, so it cannot read as a misplaced lantern or a painted light beam.
fn sewer_ambient_light(position: Vec2) -> [f32; 4] {
    const BOTTOM: f32 = -568.0;
    const HEIGHT: f32 = 3000.0;

    let height = ((position.y - BOTTOM) / HEIGHT).clamp(0.0, 1.0);
    let filtered_height = height * height * (3.0 - 2.0 * height);
    let lower = Vec3::new(0.43, 0.48, 0.43);
    let upper = Vec3::new(0.67, 0.72, 0.73);
    let colour = lower.lerp(upper, filtered_height);
    [colour.x, colour.y, colour.z, 1.0]
}

fn has_enabled_lights(lights: &[LightDefinition]) -> bool {
    lights.iter().any(|light| light.enabled)
}

/// Profile 0 retains authored palette values instead of using the dark sewer
/// ambience. It is represented by enabled lamps with zero runtime intensity,
/// so disabled JSON lights still behave as ordinary disabled fixtures.
pub(crate) fn uses_base_colors(lights: &[LightDefinition]) -> bool {
    let mut has_enabled_light = false;
    for light in lights.iter().filter(|light| light.enabled) {
        has_enabled_light = true;
        if light.intensity > f32::EPSILON {
            return false;
        }
    }
    has_enabled_light
}

/// A compact, physically inspired falloff shared by blobs and scenery. The
/// previous smoothstep curve stayed bright through most of the radius, making
/// every lantern read as a large uniform coloured circle.
fn lantern_attenuation(distance: f32, radius: f32) -> f32 {
    let radial = (1.0 - distance / radius.max(f32::EPSILON)).clamp(0.0, 1.0);
    radial.powf(1.45) * (0.78 + radial * 0.22)
}

/// Applies scene lighting to an authored dynamic colour without changing its
/// alpha. A modest floor preserves gameplay readability while the nearby lamp
/// colour still determines the visible hue and intensity.
pub(crate) fn light_dynamic_rgba(
    base: [f32; 4],
    position: Vec2,
    lights: &[LightDefinition],
) -> [f32; 4] {
    if uses_base_colors(lights) {
        return base;
    }
    let light = scenery_vertex_light(position, lights);
    [
        base[0] * (0.22 + light[0] * 1.02),
        base[1] * (0.22 + light[1] * 1.02),
        base[2] * (0.22 + light[2] * 1.02),
        base[3],
    ]
}
