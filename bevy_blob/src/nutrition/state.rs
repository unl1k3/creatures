//! Biological state and public queries for nutrients and digestion.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct ExploratoryProbe {
    pub(super) blob_id: u64,
    pub(super) age: f32,
    pub(super) extension: f32,
    pub(super) direction: Vec2,
    pub(super) tip: Vec2,
    pub(super) variation: f32,
    pub(super) anchor_edge: usize,
    pub(super) anchor_t: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum NutrientState {
    Available {
        velocity: Vec2,
    },
    Engulfing {
        blob_id: u64,
        elapsed: f32,
        origin: Vec2,
        reach: f32,
        probe_tip: Vec2,
        contact_elapsed: Option<f32>,
        variation: f32,
        anchor_edge: usize,
        anchor_t: f32,
    },
    Digesting {
        blob_id: u64,
        elapsed: f32,
        local_position: Vec2,
        velocity: Vec2,
    },
    Expelling {
        blob_id: u64,
        elapsed: f32,
        velocity: Vec2,
    },
    Waste {
        velocity: Vec2,
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Nutrient {
    pub(super) position: Vec2,
    pub(super) radius: f32,
    pub(super) original_radius: f32,
    pub(super) health: f32,
    pub(super) state: NutrientState,
    pub(super) was_submerged: bool,
}

impl Nutrient {
    pub(super) fn is_edible(&self) -> bool {
        self.health > 0.001 && matches!(self.state, NutrientState::Available { .. })
    }
}

#[derive(Resource, Default)]
pub(crate) struct NutritionWorld {
    pub(super) nutrients: Vec<Nutrient>,
    pub(super) probe: Option<ExploratoryProbe>,
    pub(super) variation_serial: u64,
}

#[derive(Resource)]
pub(crate) struct NutrientRenderAssets {
    pub(super) mesh: Handle<Mesh>,
    // Never shrink the extracted mesh: Bevy's render slab can still reference
    // the previous allocation during a scenario switch.
    pub(super) slots: usize,
}

impl NutritionWorld {
    pub(crate) fn is_free_index(&self, index: usize) -> bool {
        self.nutrients.get(index).is_some_and(|nutrient| {
            matches!(
                nutrient.state,
                NutrientState::Available { .. } | NutrientState::Waste { .. }
            )
        })
    }

    pub(crate) fn collision_radius(&self, index: usize) -> Option<f32> {
        self.nutrients.get(index).map(free_nutrient_contact_radius)
    }

    pub(crate) fn reset_from_definitions(&mut self, definitions: &[NutrientDefinition]) {
        self.probe = None;
        self.nutrients = definitions
            .iter()
            .map(|definition| Nutrient {
                position: definition.position,
                radius: definition.radius,
                original_radius: definition.radius,
                health: 1.0,
                state: NutrientState::Available {
                    velocity: Vec2::ZERO,
                },
                was_submerged: false,
            })
            .collect();
    }

    pub(crate) fn digestion_progress(&self, blob_id: u64) -> Option<f32> {
        self.nutrients
            .iter()
            .find_map(|nutrient| match nutrient.state {
                NutrientState::Engulfing {
                    blob_id: id,
                    elapsed,
                    ..
                } if id == blob_id => Some(-(elapsed / ENGULF_DURATION).clamp(0.0, 1.0)),
                NutrientState::Digesting {
                    blob_id: id,
                    elapsed,
                    ..
                } if id == blob_id => Some((elapsed / DIGESTION_DURATION).clamp(0.0, 1.0)),
                NutrientState::Expelling {
                    blob_id: id,
                    elapsed,
                    ..
                } if id == blob_id => Some(1.0 + (elapsed / EXPULSION_DURATION).clamp(0.0, 1.0)),
                _ => None,
            })
    }

    pub(crate) fn capability_factor(&self, blob_id: u64) -> f32 {
        match self.digestion_progress(blob_id) {
            Some(progress) if progress < 0.0 => 0.62 - progress.abs() * 0.14,
            Some(progress) if progress <= 1.0 => 0.48 + 0.52 * progress.sqrt(),
            Some(_) => 0.82,
            None => 1.0,
        }
    }

    pub(crate) fn is_digesting(&self, blob_id: u64) -> bool {
        self.digestion_progress(blob_id).is_some()
    }

    pub(crate) fn has_external_protrusion(&self, blob_id: u64) -> bool {
        self.probe
            .is_some_and(|probe| probe.blob_id == blob_id && probe.extension > 0.01)
            || self.nutrients.iter().any(|nutrient| {
                matches!(
                    nutrient.state,
                    NutrientState::Engulfing {
                        blob_id: id,
                        elapsed,
                        ..
                    } if id == blob_id && elapsed > 0.005
                )
            })
    }

    pub(crate) fn internal_load(&self, blob_id: u64) -> Option<(Vec2, f32, f32, f32, usize, f32)> {
        self.nutrients
            .iter()
            .find_map(|nutrient| match nutrient.state {
                NutrientState::Engulfing {
                    blob_id: id,
                    elapsed,
                    probe_tip,
                    contact_elapsed,
                    variation,
                    anchor_edge,
                    anchor_t,
                    ..
                } if id == blob_id => {
                    let extension = smoothstep((elapsed / 0.48).clamp(0.0, 1.0));
                    let grip = contact_elapsed
                        .map(|value| (value / 0.22).clamp(0.0, 1.0))
                        .unwrap_or(0.0);
                    Some((
                        probe_tip,
                        (nutrient.radius * (0.34 + grip * 0.22)).max(3.2),
                        extension,
                        variation,
                        anchor_edge,
                        anchor_t,
                    ))
                }
                _ => None,
            })
            .or_else(|| {
                self.probe
                    .filter(|probe| probe.blob_id == blob_id)
                    .map(|probe| {
                        (
                            probe.tip,
                            4.2,
                            smoothstep(probe.extension),
                            probe.variation,
                            probe.anchor_edge,
                            probe.anchor_t,
                        )
                    })
            })
    }

    pub(crate) fn physical_load(&self, blob_id: u64) -> Option<(Vec2, f32, f32)> {
        self.nutrients
            .iter()
            .find_map(|nutrient| match nutrient.state {
                NutrientState::Digesting {
                    blob_id: id,
                    elapsed,
                    ..
                } if id == blob_id => {
                    let progress = (elapsed / DIGESTION_DURATION).clamp(0.0, 1.0);
                    Some((nutrient.position, nutrient.radius, 1.0 - progress * 0.55))
                }
                _ => None,
            })
    }
}
