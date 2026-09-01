//! Canonical visual palette for the whole game.
//!
//! Rendering code refers to semantic roles from this module instead of
//! embedding RGB values. Level-authored colours remain in JSON because they
//! are data, not hard-coded presentation rules.

use bevy::prelude::Color;

pub(crate) const IVORY: [f32; 4] = [0.89, 0.86, 0.77, 1.0];
pub(crate) const NIGHT: [f32; 4] = [0.025, 0.035, 0.075, 1.0];
pub(crate) const INK: [f32; 4] = [0.035, 0.045, 0.055, 1.0];
/// Cool translucent mist used in the higher, poorly lit sewer shafts.
pub(crate) const SEWER_FOG: [f32; 4] = [0.20, 0.34, 0.36, 0.12];
/// Faded infrastructure silhouettes behind the playable room.
pub(crate) const DISTANT_INFRASTRUCTURE: [f32; 4] = [0.045, 0.075, 0.085, 0.28];

pub(crate) const BLOB_FAMILIES: [[f32; 3]; 6] = [
    // Root cyan, then deliberately distant hues for descendant families.
    [0.10, 0.82, 0.78],
    [1.00, 0.30, 0.18],
    [0.68, 0.30, 1.00],
    [1.00, 0.72, 0.08],
    [0.28, 0.88, 0.24],
    [0.18, 0.42, 1.00],
];

/// Stable gameplay family index. Entry zero belongs to the original blob;
/// each split parent selects one of the descendant entries for both children.
pub(crate) fn blob_family_index(parent_id: Option<u64>) -> usize {
    match parent_id {
        None => 0,
        Some(id) => {
            let descendant_colors = BLOB_FAMILIES.len() - 1;
            (id as usize).wrapping_mul(3) % descendant_colors + 1
        }
    }
}
/// Inert, desaturated colour shared by a dead blob's body and membrane.
pub(crate) const DEAD_BLOB: [f32; 4] = [0.72, 0.74, 0.75, 0.90];
/// Organic grime carried on a living blob's outer gel layer.
pub(crate) const BLOB_GRIME: [f32; 4] = [0.36, 0.20, 0.06, 0.88];
/// Defensive spines use coral so an active shield is readable against the
/// cyan body and toxic-green sewer palette.
pub(crate) const MEMBRANE_SHIELD_BASE: [f32; 4] = [0.62, 0.10, 0.10, 0.90];
pub(crate) const MEMBRANE_SHIELD_TIP: [f32; 4] = [1.00, 0.32, 0.22, 0.98];

pub(crate) const NUTRIENT_BODY: [f32; 4] = [0.82, 0.22, 0.16, 0.98];
pub(crate) const NUTRIENT_CORE: [f32; 4] = [1.0, 0.48, 0.28, 0.98];
pub(crate) const NUTRIENT_EDGE: [f32; 4] = [0.065, 0.040, 0.035, 1.0];
pub(crate) const NUTRIENT_ENERGY: [f32; 4] = [1.0, 0.80, 0.16, 1.0];
pub(crate) const NUTRIENT_HIGHLIGHT: [f32; 4] = [1.0, 0.86, 0.30, 1.0];
// A nutrient killed by wastewater stays present as inert organic matter.
pub(crate) const DEAD_NUTRIENT_BODY: [f32; 4] = [0.22, 0.28, 0.12, 0.98];
pub(crate) const DEAD_NUTRIENT_CORE: [f32; 4] = [0.31, 0.37, 0.16, 0.98];
pub(crate) const DEAD_NUTRIENT_EDGE: [f32; 4] = [0.045, 0.052, 0.030, 1.0];
pub(crate) const DEAD_NUTRIENT_ENERGY: [f32; 4] = [0.34, 0.42, 0.15, 0.78];
pub(crate) const NUTRIENT_ENGULFED_BODY: [f32; 4] = [0.70, 0.16, 0.12, 0.98];
pub(crate) const NUTRIENT_ENGULFED_CORE: [f32; 4] = [0.92, 0.35, 0.19, 0.98];
pub(crate) const NUTRIENT_ENGULFED_EDGE: [f32; 4] = [0.060, 0.038, 0.032, 1.0];
pub(crate) const NUTRIENT_ENGULFED_ENERGY: [f32; 4] = [0.96, 0.66, 0.11, 1.0];
pub(crate) const WASTE_BODY: [f32; 4] = [0.24, 0.20, 0.13, 0.98];
pub(crate) const WASTE_CORE: [f32; 4] = [0.38, 0.31, 0.19, 0.98];
pub(crate) const WASTE_EDGE: [f32; 4] = [0.045, 0.042, 0.038, 1.0];
pub(crate) const WASTE_ENERGY: [f32; 4] = [0.48, 0.34, 0.16, 0.82];
pub(crate) const DIGESTED_BODY: [f32; 4] = [0.34, 0.20, 0.12, 0.94];
pub(crate) const DIGESTED_CORE: [f32; 4] = [0.48, 0.31, 0.18, 0.94];
pub(crate) const DIGESTED_EDGE: [f32; 4] = [0.06, 0.038, 0.032, 0.98];
pub(crate) const DIGESTED_ENERGY: [f32; 4] = [0.46, 0.34, 0.10, 0.94];

pub(crate) const ACID_TRAIL: [f32; 4] = [0.55, 1.0, 0.12, 0.72];
pub(crate) const ACID_BODY: [f32; 4] = [0.68, 1.0, 0.16, 0.98];
pub(crate) const ACID_CORE: [f32; 4] = [0.92, 1.0, 0.55, 0.92];
pub(crate) const AMBIENT_DROP: [f32; 4] = [0.02, 0.72, 0.82, 1.0];
pub(crate) const BUBBLE_CENTER: [f32; 4] = [0.68, 0.88, 0.42, 0.24];
pub(crate) const BUBBLE_EDGE: [f32; 4] = [0.88, 0.96, 0.62, 0.96];

pub(crate) const HUD_CONTROLS_BG: [f32; 4] = [0.035, 0.065, 0.12, 1.0];
pub(crate) const HUD_METRICS_BG: [f32; 4] = [0.025, 0.075, 0.075, 1.0];
pub(crate) const HUD_TEXT: [f32; 4] = [0.88, 0.96, 1.0, 1.0];
pub(crate) const HUD_METRICS_TEXT: [f32; 4] = [0.68, 1.0, 0.88, 1.0];
pub(crate) const HUD_TEXT_BG: [f32; 4] = [0.025, 0.045, 0.085, 0.92];
pub(crate) const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.72];

pub(crate) const DEBUG_PLATFORM: [f32; 4] = [1.0, 0.28, 0.08, 0.96];
pub(crate) const DEBUG_BOUNDS: [f32; 4] = [0.12, 0.85, 1.0, 0.92];
pub(crate) const DEBUG_SPAWN: [f32; 4] = [0.25, 1.0, 0.35, 1.0];
pub(crate) const DEBUG_HAZARD: [f32; 4] = [1.0, 0.08, 0.18, 0.94];
pub(crate) const DEBUG_ROUTE: [f32; 4] = [1.0, 0.72, 0.18, 0.72];
pub(crate) const DEBUG_EXPULSION: [f32; 4] = [0.82, 0.35, 1.0, 1.0];
pub(crate) const DEBUG_PARTICLE: [f32; 4] = [1.0, 0.82, 0.30, 0.58];
pub(crate) const DEBUG_ATTACHMENT: [f32; 4] = [1.0, 0.90, 0.42, 0.82];
pub(crate) const DEBUG_SPRING: [f32; 4] = [0.12, 0.55, 0.48, 0.42];
pub(crate) const DEBUG_CENTER: [f32; 4] = [0.72, 0.42, 0.95, 1.0];
pub(crate) const CHARGE_GLOW: [f32; 4] = [1.0, 0.72, 0.12, 0.20];
pub(crate) const CHARGE_ARC: [f32; 4] = [1.0, 0.78, 0.16, 0.96];
pub(crate) const LAB_PLATFORM: [f32; 4] = [0.18, 0.27, 0.38, 1.0];
pub(crate) const LAB_FIXTURE: [f32; 4] = [0.24, 0.38, 0.52, 1.0];
pub(crate) const HAZARD_SURFACE: [f32; 4] = [0.64, 1.0, 0.06, 0.94];
pub(crate) const HAZARD_BUBBLE: [f32; 4] = [0.72, 1.0, 0.08, 0.72];
pub(crate) const ROUTE_LABEL: [f32; 4] = [1.0, 0.82, 0.28, 1.0];
pub(crate) const TRANSLUCENT_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 0.66];

/// Fallback wastewater only. Authored levels override it in JSON.
pub(crate) const DEFAULT_WASTEWATER_RUNTIME: [f32; 4] = [0.4, 0.5, 0.1, 0.8];
#[cfg(test)]
pub(crate) const DEFAULT_WASTEWATER: [f32; 4] = DEFAULT_WASTEWATER_RUNTIME;

pub(crate) fn color(value: [f32; 4]) -> Color {
    Color::srgba(value[0], value[1], value[2], value[3])
}

pub(crate) fn with_alpha(mut value: [f32; 4], alpha: f32) -> Color {
    value[3] = alpha;
    color(value)
}

pub(crate) fn scale_rgb(mut value: [f32; 4], factor: f32) -> [f32; 4] {
    let factor = factor.max(0.0);
    value[0] = (value[0] * factor).min(1.0);
    value[1] = (value[1] * factor).min(1.0);
    value[2] = (value[2] * factor).min(1.0);
    value
}

pub(crate) fn mix(first: [f32; 4], second: [f32; 4], amount: f32) -> [f32; 4] {
    let amount = amount.clamp(0.0, 1.0);
    std::array::from_fn(|index| first[index] + (second[index] - first[index]) * amount)
}
