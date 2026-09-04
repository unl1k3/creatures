//! Procedural wastewater surface geometry, lighting and reflections.

use super::*;

const WASTEWATER_SEGMENTS: usize = 64;
const WASTEWATER_ROWS: usize = 4;
// The rear layer gives the basin its coloured volume. The front layer only
// filters immersed objects, so it must stay substantially more transparent.
const WASTEWATER_REAR_ALPHA_SCALE: [f32; WASTEWATER_ROWS] = [0.26, 0.34, 0.29, 0.32];
const WASTEWATER_FRONT_ALPHA: [f32; WASTEWATER_ROWS] = [0.20, 0.08, 0.12, 0.17];

pub(super) fn create_wastewater_mesh(
    definition: WastewaterAreaDefinition,
    area_index: usize,
    elapsed: f32,
    occlusion_layer: bool,
    lights: &[crate::level_format::LightDefinition],
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    let positions = wastewater_positions(definition, area_index, elapsed, None);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        wastewater_colors(definition, occlusion_layer, &positions, lights, elapsed),
    );

    let row_width = WASTEWATER_SEGMENTS + 1;
    let mut indices = Vec::with_capacity(WASTEWATER_SEGMENTS * (WASTEWATER_ROWS - 1) * 6);
    for row in 0..WASTEWATER_ROWS - 1 {
        for column in 0..WASTEWATER_SEGMENTS {
            let top_left = (row * row_width + column) as u32;
            let top_right = top_left + 1;
            let bottom_left = top_left + row_width as u32;
            let bottom_right = bottom_left + 1;
            indices.extend_from_slice(&[
                top_left,
                bottom_left,
                top_right,
                top_right,
                bottom_left,
                bottom_right,
            ]);
        }
    }
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

pub(super) fn update_wastewater_positions(
    mesh: &mut Mesh,
    definition: WastewaterAreaDefinition,
    area_index: usize,
    elapsed: f32,
    effects: &WastewaterEffects,
    lights: &[crate::level_format::LightDefinition],
    occlusion_layer: bool,
) {
    let positions = wastewater_positions(definition, area_index, elapsed, Some(effects));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
    // Lighting follows the wavy surface as it moves, so shallow bright bands
    // do not remain frozen in world space after an impact ripple.
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        wastewater_colors(definition, occlusion_layer, &positions, lights, elapsed),
    );
}

pub(crate) fn create_bubble_mesh() -> Mesh {
    const SIDES: usize = 24;
    let mut positions = Vec::with_capacity(SIDES + 1);
    let mut colors = Vec::with_capacity(SIDES + 1);
    positions.push([0.0, 0.0, 0.0]);
    colors.push(palette::BUBBLE_CENTER);
    for index in 0..SIDES {
        let angle = index as f32 / SIDES as f32 * std::f32::consts::TAU;
        positions.push([angle.cos(), angle.sin(), 0.0]);
        colors.push(palette::BUBBLE_EDGE);
    }
    let mut indices = Vec::with_capacity(SIDES * 3);
    for index in 0..SIDES {
        indices.extend_from_slice(&[0, index as u32 + 1, (index as u32 + 1) % SIDES as u32 + 1]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn wastewater_positions(
    definition: WastewaterAreaDefinition,
    area_index: usize,
    elapsed: f32,
    effects: Option<&WastewaterEffects>,
) -> Vec<[f32; 3]> {
    let half_size = definition.size * 0.5;
    let mut positions = Vec::with_capacity((WASTEWATER_SEGMENTS + 1) * WASTEWATER_ROWS);
    for row in 0..WASTEWATER_ROWS {
        for column in 0..=WASTEWATER_SEGMENTS {
            let fraction = column as f32 / WASTEWATER_SEGMENTS as f32;
            let x = -half_size.x + definition.size.x * fraction;
            let world_x = definition.position.x + x;
            let surface = definition.wave_offset(x, elapsed)
                + effects.map_or(0.0, |effects| effects.surface_offset(area_index, world_x));
            let y = match row {
                // A thin, uneven scum rim breaks the hard rectangular edge
                // and visually seats the animated water in the basin.
                0 => half_size.y + surface + wastewater_shore_rim(x, elapsed),
                1 => half_size.y + surface,
                2 => half_size.y - 16.0 + surface * 0.32,
                _ => -half_size.y,
            };
            positions.push([x, y, 0.0]);
        }
    }
    positions
}

fn wastewater_shore_rim(local_x: f32, elapsed: f32) -> f32 {
    let broad = (local_x * 0.021 + elapsed * 0.34).sin() * 0.72;
    let fine = (local_x * 0.071 - elapsed * 0.52).sin() * 0.34;
    2.6 + broad + fine
}

fn wastewater_colors(
    definition: WastewaterAreaDefinition,
    occlusion_layer: bool,
    positions: &[[f32; 3]],
    lights: &[crate::level_format::LightDefinition],
    elapsed: f32,
) -> Vec<[f32; 4]> {
    let [red, green, blue, alpha] = definition.color;
    let alphas = if occlusion_layer {
        // Objects remain visible below the surface, increasingly filtered by
        // murky water with depth instead of being cut away completely. The
        // old values made the foreground layer almost opaque by itself.
        WASTEWATER_FRONT_ALPHA
    } else {
        WASTEWATER_REAR_ALPHA_SCALE.map(|scale| alpha * scale)
    };
    let shades = [
        // Dark, low-saturation rim: it reads as foam, grease and debris
        // instead of a clean vector line over the water.
        [
            (red * 0.42).min(1.0),
            (green * 0.50).min(1.0),
            (blue * 0.28).min(1.0),
            alphas[0],
        ],
        [
            (red * 1.28).min(1.0),
            (green * 1.22).min(1.0),
            (blue * 0.82).min(1.0),
            alphas[1],
        ],
        [red, green, blue, alphas[2]],
        [red * 0.45, green * 0.48, blue * 0.38, alphas[3]],
    ];
    positions
        .iter()
        .enumerate()
        .map(|(index, position)| {
            let shade = shades[index / (WASTEWATER_SEGMENTS + 1)];
            let row = index / (WASTEWATER_SEGMENTS + 1);
            let world_position = definition.position + Vec2::new(position[0], position[1]);
            let mut color = light_dynamic_rgba(shade, world_position, lights);
            // Reflections belong only to the two moving surface rows. Their
            // broken horizontal bands drift independently under each lantern
            // rather than painting a uniform bright stripe across the basin.
            if row <= 1 {
                for (light_index, light) in
                    lights.iter().enumerate().filter(|(_, light)| light.enabled)
                {
                    let lateral = (1.0
                        - (world_position.x - light.position.x).abs() / light.radius)
                        .clamp(0.0, 1.0);
                    let vertical = (1.0
                        - (world_position.y - light.position.y).abs() / (light.radius * 1.35))
                        .clamp(0.0, 1.0);
                    let phase = world_position.x * (0.075 + light_index as f32 * 0.004)
                        - elapsed * (2.1 + light_index as f32 * 0.11)
                        + light_index as f32 * 1.37;
                    let shimmer = (phase.sin() * 0.5 + 0.5).powi(5)
                        * lateral.powi(2)
                        * vertical
                        * light.intensity
                        * 0.18;
                    color[0] = (color[0] + light.color[0] * shimmer).min(1.0);
                    color[1] = (color[1] + light.color[1] * shimmer).min(1.0);
                    color[2] = (color[2] + light.color[2] * shimmer).min(1.0);
                }
            }
            color
        })
        .collect()
}
