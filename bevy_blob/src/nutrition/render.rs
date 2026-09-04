//! Mesh construction and palette rules for nutrients and expelled waste.

use super::*;
use crate::rendering::light_dynamic_rgba;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};

pub(crate) fn draw_nutrition(
    time: Res<Time>,
    level: Res<Level>,
    nutrition: Res<NutritionWorld>,
    mut render_assets: ResMut<NutrientRenderAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(mut mesh) = meshes.get_mut(&render_assets.mesh) else {
        return;
    };
    render_assets.slots = render_assets.slots.max(nutrition.nutrients.len()).max(1);
    update_nutrient_mesh(
        &mut mesh,
        &nutrition.nutrients,
        render_assets.slots,
        time.elapsed_secs(),
        &level.lights,
    );
}

pub(super) fn update_nutrient_mesh(
    mesh: &mut Mesh,
    nutrients: &[Nutrient],
    slots: usize,
    elapsed: f32,
    lights: &[crate::level_format::LightDefinition],
) {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    for nutrient in nutrients {
        append_nutrient_mesh(nutrient, elapsed, &mut positions, &mut colors, &mut indices);
    }
    for _ in nutrients.len()..slots {
        append_hidden_nutrient_mesh(&mut positions, &mut colors, &mut indices);
    }
    for (position, color) in positions.iter().zip(&mut colors) {
        *color = light_dynamic_rgba(*color, Vec2::new(position[0], position[1]), lights);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}

pub(super) fn append_hidden_nutrient_mesh(
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    let first_position = positions.len();
    let hidden = Nutrient {
        position: Vec2::ZERO,
        radius: 0.0,
        original_radius: 0.0,
        health: 0.0,
        state: NutrientState::Waste {
            velocity: Vec2::ZERO,
        },
        was_submerged: false,
    };
    append_nutrient_mesh(&hidden, 0.0, positions, colors, indices);
    for color in &mut colors[first_position..] {
        color[3] = 0.0;
    }
}

pub(crate) fn empty_nutrient_mesh() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

pub(super) fn append_nutrient_mesh(
    nutrient: &Nutrient,
    elapsed: f32,
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    const SIDES: usize = 20;
    let seed = nutrient.original_radius * 0.173
        + nutrient.position.x * 0.011
        + nutrient.position.y * 0.007;
    let (body, center, edge, energy, activity) = nutrient_palette(nutrient);
    let pulse_cycle = elapsed / 1.85 + seed * 0.13;
    let pulse_phase = pulse_cycle.fract();
    let pulse = (pulse_phase * std::f32::consts::PI).sin().powi(2) * activity;
    let lobe_angle = (pulse_cycle.floor() * 2.17 + seed * 1.31).rem_euclid(std::f32::consts::TAU);
    // The procedural mesh has no entity transform of its own. Derive a
    // stable rolling phase from the Avian-driven world position so the visual
    // capsule turns as it travels instead of merely translating.
    let rolling_phase = match nutrient.state {
        // Internal nutrients are anchored to the blob: translating with it
        // must not make their visible pattern look like it is rolling.
        NutrientState::Engulfing { .. } | NutrientState::Digesting { .. } => 0.0,
        NutrientState::Available { .. }
        | NutrientState::Expelling { .. }
        | NutrientState::Waste { .. } => -nutrient.position.x * 0.052 + nutrient.position.y * 0.009,
    };
    let pulsed_body = mix_rgba(body, energy, pulse * 0.15);
    let pulsed_center = mix_rgba(center, energy, pulse * 0.23);
    let first = positions.len() as u32;
    positions.push([nutrient.position.x, nutrient.position.y, 0.0]);
    colors.push(pulsed_center);

    let mut outline = Vec::with_capacity(SIDES);
    for index in 0..SIDES {
        let angle = index as f32 / SIDES as f32 * std::f32::consts::TAU;
        let irregularity =
            1.0 + (angle * 3.0 + seed).sin() * 0.065 + (angle * 5.0 - seed * 1.7).sin() * 0.032;
        let lobe = (angle - lobe_angle).cos().max(0.0).powi(2);
        let local = Vec2::new(angle.cos() * 0.94, angle.sin() * 0.88)
            * nutrient.radius
            * irregularity
            * (1.0 + pulse * lobe * 0.20);
        outline.push(nutrient.position + Vec2::from_angle(rolling_phase).rotate(local));
    }
    for point in &outline {
        let inner = nutrient.position + (*point - nutrient.position) * 0.78;
        positions.push([inner.x, inner.y, 0.0]);
        colors.push(pulsed_body);
    }
    for point in &outline {
        positions.push([point.x, point.y, 0.0]);
        colors.push(edge);
    }
    for index in 0..SIDES {
        let next = (index + 1) % SIDES;
        let inner = first + 1 + index as u32;
        let inner_next = first + 1 + next as u32;
        let outer = first + 1 + SIDES as u32 + index as u32;
        let outer_next = first + 1 + SIDES as u32 + next as u32;
        indices.extend_from_slice(&[
            first, inner, inner_next, inner, outer, outer_next, inner, outer_next, inner_next,
        ]);
    }

    for nodule in 0..4 {
        let angle = seed * (0.71 + nodule as f32 * 0.13) + nodule as f32 * 1.67;
        let distance = nutrient.radius * (0.22 + nodule as f32 * 0.055);
        let nodule_center = nutrient.position
            + Vec2::from_angle(rolling_phase).rotate(Vec2::new(
                angle.cos() * distance,
                angle.sin() * distance * 0.72,
            ));
        let radius = nutrient.radius
            * (0.075 + nodule as f32 * 0.012)
            * (1.0 + (elapsed * 2.2 + angle).sin() * 0.10 * activity + pulse * 0.24);
        append_nodule(
            nodule_center,
            radius,
            mix_rgba(energy, palette::NUTRIENT_HIGHLIGHT, pulse * 0.28),
            positions,
            colors,
            indices,
        );
    }
}

pub(super) fn mix_rgba(first: [f32; 4], second: [f32; 4], amount: f32) -> [f32; 4] {
    let amount = amount.clamp(0.0, 1.0);
    [
        first[0] + (second[0] - first[0]) * amount,
        first[1] + (second[1] - first[1]) * amount,
        first[2] + (second[2] - first[2]) * amount,
        first[3] + (second[3] - first[3]) * amount,
    ]
}

pub(super) fn append_nodule(
    center: Vec2,
    radius: f32,
    color: [f32; 4],
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    const SIDES: usize = 10;
    let first = positions.len() as u32;
    positions.push([center.x, center.y, 0.0]);
    colors.push(color);
    for index in 0..SIDES {
        let angle = index as f32 / SIDES as f32 * std::f32::consts::TAU;
        let point = center + Vec2::from_angle(angle) * radius;
        positions.push([point.x, point.y, 0.0]);
        colors.push([color[0] * 0.66, color[1] * 0.66, color[2] * 0.66, color[3]]);
    }
    for index in 0..SIDES {
        indices.extend_from_slice(&[
            first,
            first + index as u32 + 1,
            first + (index as u32 + 1) % SIDES as u32 + 1,
        ]);
    }
}

pub(super) fn nutrient_palette(
    nutrient: &Nutrient,
) -> ([f32; 4], [f32; 4], [f32; 4], [f32; 4], f32) {
    match nutrient.state {
        NutrientState::Available { .. } => {
            let health = nutrient.health.clamp(0.0, 1.0);
            let decay = 1.0 - health;
            (
                mix_rgba(palette::DEAD_NUTRIENT_BODY, palette::NUTRIENT_BODY, health),
                mix_rgba(palette::DEAD_NUTRIENT_CORE, palette::NUTRIENT_CORE, health),
                mix_rgba(palette::DEAD_NUTRIENT_EDGE, palette::NUTRIENT_EDGE, health),
                mix_rgba(
                    palette::DEAD_NUTRIENT_ENERGY,
                    palette::NUTRIENT_ENERGY,
                    health,
                ),
                health * (1.0 - decay * 0.35),
            )
        }
        NutrientState::Engulfing {
            contact_elapsed, ..
        } => {
            let depth = contact_elapsed
                .map(|elapsed| smoothstep(elapsed / ENGULF_DURATION))
                .unwrap_or(0.0);
            (
                with_opacity(palette::NUTRIENT_ENGULFED_BODY, 1.0 - depth * 0.48),
                with_opacity(palette::NUTRIENT_ENGULFED_CORE, 1.0 - depth * 0.48),
                with_opacity(palette::NUTRIENT_ENGULFED_EDGE, 1.0 - depth * 0.38),
                with_opacity(palette::NUTRIENT_ENGULFED_ENERGY, 1.0 - depth * 0.25),
                0.65,
            )
        }
        NutrientState::Digesting { elapsed, .. } => {
            let progress = (elapsed / DIGESTION_DURATION).clamp(0.0, 1.0);
            (
                with_opacity(
                    mix_rgba(palette::NUTRIENT_BODY, palette::DIGESTED_BODY, progress),
                    0.48,
                ),
                with_opacity(
                    mix_rgba(palette::NUTRIENT_CORE, palette::DIGESTED_CORE, progress),
                    0.52,
                ),
                with_opacity(palette::DIGESTED_EDGE, 0.58),
                with_opacity(
                    mix_rgba(palette::NUTRIENT_ENERGY, palette::DIGESTED_ENERGY, progress),
                    0.72,
                ),
                1.0 - progress,
            )
        }
        NutrientState::Expelling { elapsed, .. } => {
            let visibility = 0.52 + smoothstep(elapsed / EXPULSION_DURATION) * 0.48;
            (
                with_opacity(palette::WASTE_BODY, visibility),
                with_opacity(palette::WASTE_CORE, visibility),
                with_opacity(palette::WASTE_EDGE, visibility),
                with_opacity(palette::WASTE_ENERGY, visibility),
                0.0,
            )
        }
        NutrientState::Waste { .. } => (
            palette::WASTE_BODY,
            palette::WASTE_CORE,
            palette::WASTE_EDGE,
            palette::WASTE_ENERGY,
            0.0,
        ),
    }
}

pub(super) fn with_opacity(mut color: [f32; 4], opacity: f32) -> [f32; 4] {
    color[3] *= opacity.clamp(0.0, 1.0);
    color
}
