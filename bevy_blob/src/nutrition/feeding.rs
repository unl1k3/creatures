//! Player-triggered nutrient acquisition.
//!
//! This module chooses an edible target and initializes either an exploratory
//! probe or the engulfing state. The fixed-step digestive animation remains in
//! the nutrition simulation, where it can be kept in lockstep with Avian.

use super::*;

pub(crate) fn start_phagocytosis(
    keyboard: Res<ButtonInput<KeyCode>>,
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    vitality: Res<VitalityWorld>,
    mut nutrition: ResMut<NutritionWorld>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    if !keyboard.just_pressed(KeyCode::KeyC) || super::movement_command(&keyboard) {
        return;
    }
    let Some(blob) = blobs.active.get(blobs.selected) else {
        return;
    };
    if !vitality.is_alive(blob.id) || nutrition.is_digesting(blob.id) {
        return;
    }

    sound_events.write(BlobSoundEvent::Probe);
    let center = blob.body.center();
    let nearest_direction = nutrition
        .nutrients
        .iter()
        .filter(|nutrient| nutrient.is_edible())
        .min_by(|a, b| {
            center
                .distance_squared(a.position)
                .total_cmp(&center.distance_squared(b.position))
        })
        .map(|nutrient| (nutrient.position - center).normalize_or(Vec2::X))
        .unwrap_or(Vec2::X);
    let candidate = nutrition
        .nutrients
        .iter()
        .enumerate()
        .filter(|(_, nutrient)| {
            nutrient.is_edible()
                && nutrient.radius <= blob.body.rest_radius * 0.48
                && super::phagocytosis_path_clear(
                    center,
                    blob.body.rest_radius,
                    nutrient.position,
                    nutrient.radius,
                    &level,
                )
        })
        .map(|(index, nutrient)| {
            let gap =
                (center.distance(nutrient.position) - blob.body.rest_radius - nutrient.radius)
                    .max(0.0);
            (index, gap)
        })
        .filter(|(_, gap)| *gap <= PHAGOCYTOSIS_REACH)
        .min_by(|first, second| first.1.total_cmp(&second.1));

    nutrition.variation_serial = nutrition.variation_serial.wrapping_add(1);
    let variation = protrusion_variation(blob.id, nutrition.variation_serial as u32);
    let (anchor_edge, anchor_t) =
        super::membrane_anchor(&blob.body, center + nearest_direction * 100.0);
    let Some((index, reach)) = candidate else {
        nutrition.probe = Some(ExploratoryProbe {
            blob_id: blob.id,
            age: 0.0,
            extension: 0.0,
            direction: nearest_direction,
            tip: center + nearest_direction * blob.body.rest_radius,
            variation,
            anchor_edge,
            anchor_t,
        });
        return;
    };

    nutrition.probe = None;
    let nutrient = &mut nutrition.nutrients[index];
    nutrient.state = NutrientState::Engulfing {
        blob_id: blob.id,
        elapsed: 0.0,
        origin: nutrient.position,
        reach,
        probe_tip: center
            + (nutrient.position - center).normalize_or(Vec2::X) * blob.body.rest_radius,
        contact_elapsed: None,
        variation,
        anchor_edge,
        anchor_t,
    };
}

fn protrusion_variation(blob_id: u64, salt: u32) -> f32 {
    let mut value = blob_id ^ (salt as u64).rotate_left(23) ^ 0xa076_1d64_78bd_642f;
    value ^= value >> 32;
    value = value.wrapping_mul(0xe703_7ed1_a0b4_28db);
    ((value >> 40) & 0xffff) as f32 / 65_535.0
}

/// Keeps expelled waste varied while preserving an upward launch direction.
pub(super) fn expulsion_launch(blob_id: u64, position: Vec2) -> (Vec2, f32) {
    let mut value = blob_id
        ^ (position.x.to_bits() as u64).rotate_left(17)
        ^ (position.y.to_bits() as u64).rotate_left(39)
        ^ 0x9e37_79b9_7f4a_7c15;
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    let horizontal = ((value & 0xffff) as f32 / 65_535.0 - 0.5) * 1.25;
    let speed_variation = ((value >> 16) & 0xffff) as f32 / 65_535.0;
    (
        Vec2::new(horizontal, 1.0).normalize(),
        235.0 + speed_variation * 95.0,
    )
}

/// Converts a fully digested nutrient into a smaller free residue.
pub(super) fn make_waste(nutrient: &mut Nutrient, velocity: Vec2) {
    nutrient.radius = nutrient.original_radius * 0.42;
    nutrient.state = NutrientState::Waste { velocity };
}
