use super::*;
use std::collections::{HashMap, HashSet};

const PASSIVE_DRAIN: f32 = 0.0012;
const MOVEMENT_DRAIN: f32 = 0.0035;
const CHARGE_DRAIN: f32 = 0.006;
const SHIELD_DRAIN: f32 = 0.010;
const STARVATION_DAMAGE: f32 = 0.018;
/// Toxic wastewater damages exposed living matter at a shared rate.
pub(super) const WASTEWATER_DAMAGE_PER_SECOND: f32 = 0.28;
const PAINFUL_IMPACT_SPEED: f32 = 720.0;
const LETHAL_IMPACT_SPEED: f32 = 1_180.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DeathCause {
    Wasting,
    Trauma,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LifeState {
    Alive,
    Corpse(DeathCause),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Vitality {
    pub(super) energy: f32,
    pub(super) health: f32,
    pub(super) trauma: f32,
    pub(super) last_impact: f32,
    pub(super) state: LifeState,
    shape_scale: f32,
}

impl Default for Vitality {
    fn default() -> Self {
        Self {
            energy: 1.0,
            health: 1.0,
            trauma: 0.0,
            last_impact: 0.0,
            state: LifeState::Alive,
            shape_scale: 1.0,
        }
    }
}

impl Vitality {
    pub(super) fn is_alive(self) -> bool {
        self.state == LifeState::Alive
    }

    pub(super) fn vigor(self) -> f32 {
        if !self.is_alive() {
            return 0.0;
        }
        (0.34 + self.energy * 0.66) * (0.55 + self.health * 0.45)
    }
}

#[derive(Resource, Default)]
pub(super) struct VitalityWorld {
    states: HashMap<u64, Vitality>,
}

impl VitalityWorld {
    pub(super) fn get(&self, id: u64) -> Vitality {
        self.states.get(&id).copied().unwrap_or_default()
    }

    pub(super) fn is_alive(&self, id: u64) -> bool {
        self.get(id).is_alive()
    }

    pub(super) fn vigor(&self, id: u64) -> f32 {
        self.get(id).vigor()
    }

    pub(super) fn spend(&mut self, id: u64, amount: f32) -> bool {
        let vitality = self.states.entry(id).or_default();
        if !vitality.is_alive() || vitality.energy < amount {
            return false;
        }
        vitality.energy = (vitality.energy - amount).max(0.0);
        true
    }

    pub(super) fn restore_energy(&mut self, id: u64, amount: f32) {
        let vitality = self.states.entry(id).or_default();
        if vitality.is_alive() {
            vitality.energy = (vitality.energy + amount.max(0.0)).min(1.0);
        }
    }

    pub(super) fn damage(&mut self, id: u64, amount: f32) {
        let vitality = self.states.entry(id).or_default();
        if vitality.is_alive() {
            vitality.health = (vitality.health - amount.max(0.0)).max(0.0);
        }
    }

    pub(super) fn split(&mut self, parent: u64, children: [u64; 2]) {
        let inherited = self.states.remove(&parent).unwrap_or_default();
        self.states.insert(children[0], inherited);
        self.states.insert(children[1], inherited);
    }

    pub(super) fn merge(&mut self, children: [u64; 2], parent: u64) {
        let first = self.states.remove(&children[0]).unwrap_or_default();
        let second = self.states.remove(&children[1]).unwrap_or_default();
        self.states.insert(
            parent,
            Vitality {
                energy: (first.energy + second.energy) * 0.5,
                health: (first.health + second.health) * 0.5,
                trauma: first.trauma.max(second.trauma),
                last_impact: 0.0,
                ..default()
            },
        );
    }

    pub(super) fn reset(&mut self) {
        self.states.clear();
    }
}

pub(super) fn simulate_vitality(
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    shields: Res<ShieldWorld>,
    mut blobs: ResMut<BlobWorld>,
    mut vitality_world: ResMut<VitalityWorld>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    let dt = time.delta_secs();
    let active_ids = blobs
        .active
        .iter()
        .map(|blob| blob.id)
        .collect::<HashSet<_>>();
    vitality_world
        .states
        .retain(|id, _| active_ids.contains(id));
    let selected = blobs.selected;
    let mut death_occurred = false;

    for (index, active_blob) in blobs.active.iter_mut().enumerate() {
        let vitality = vitality_world.states.entry(active_blob.id).or_default();
        if !vitality.is_alive() {
            active_blob.body.cease_idle_animation();
            continue;
        }

        let moving = index == selected
            && (keyboard.pressed(KeyCode::KeyA)
                || keyboard.pressed(KeyCode::KeyD)
                || keyboard.pressed(KeyCode::ArrowLeft)
                || keyboard.pressed(KeyCode::ArrowRight));
        let charging = index == selected && keyboard.pressed(KeyCode::ArrowDown);
        let shielded = shields.extension(active_blob.id) > 0.05;
        let drain = PASSIVE_DRAIN
            + if moving { MOVEMENT_DRAIN } else { 0.0 }
            + if charging { CHARGE_DRAIN } else { 0.0 }
            + if shielded { SHIELD_DRAIN } else { 0.0 };
        vitality.energy = (vitality.energy - drain * dt).max(0.0);
        if vitality.energy <= 0.001 {
            vitality.health = (vitality.health - STARVATION_DAMAGE * dt).max(0.0);
        }

        let shield_absorption = shields.extension(active_blob.id) * 0.38;
        vitality.last_impact = active_blob.body.last_impact_speed * (1.0 - shield_absorption);
        vitality.trauma = (vitality.trauma - dt * 0.16).max(0.0);
        if vitality.last_impact > PAINFUL_IMPACT_SPEED {
            let severity = ((vitality.last_impact - PAINFUL_IMPACT_SPEED)
                / (LETHAL_IMPACT_SPEED - PAINFUL_IMPACT_SPEED))
                .clamp(0.0, 1.5);
            vitality.trauma = (vitality.trauma + severity * 0.42).min(1.5);
            vitality.health = (vitality.health - severity * 0.16).max(0.0);
        }

        let wasting_scale = 0.88 + 0.12 * vitality.energy.min(vitality.health);
        deflate_towards(&mut active_blob.body, vitality, wasting_scale, dt * 0.015);

        let cause = death_cause(*vitality);
        if let Some(cause) = cause {
            vitality.state = LifeState::Corpse(cause);
            active_blob.body.cancel_jump_charge();
            sound_events.write(BlobSoundEvent::Death {
                family: crate::palette::blob_family_index(active_blob.parent_id),
            });
            death_occurred = true;
        }
    }
    if death_occurred {
        blobs.rejoin_parent = None;
        if blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| !vitality_world.is_alive(blob.id))
        {
            if let Some(next_living) = blobs
                .active
                .iter()
                .position(|blob| vitality_world.is_alive(blob.id))
            {
                blobs.selected = next_living;
            }
        }
    }
}

fn death_cause(vitality: Vitality) -> Option<DeathCause> {
    if vitality.last_impact >= LETHAL_IMPACT_SPEED || vitality.trauma >= 1.0 {
        Some(DeathCause::Trauma)
    } else if vitality.health <= 0.001 {
        Some(DeathCause::Wasting)
    } else {
        None
    }
}

fn deflate_towards(blob: &mut Blob, vitality: &mut Vitality, target: f32, max_change: f32) {
    let difference = target - vitality.shape_scale;
    let next = if difference.abs() <= max_change {
        target
    } else {
        vitality.shape_scale + difference.signum() * max_change
    };
    let ratio = next / vitality.shape_scale.max(0.001);
    blob.scale_rest_shape(ratio);
    vitality.shape_scale = next;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_hazard_damage_reduces_living_blob_health() {
        let mut world = VitalityWorld::default();
        world.damage(7, 0.25);
        assert_eq!(world.get(7).health, 0.75);
    }

    #[test]
    fn low_energy_reduces_vigor_without_stopping_movement_completely() {
        let tired = Vitality {
            energy: 0.0,
            ..default()
        };
        assert!(tired.vigor() > 0.25);
        assert!(tired.vigor() < Vitality::default().vigor());
    }

    #[test]
    fn corpses_have_no_vigor() {
        let corpse = Vitality {
            state: LifeState::Corpse(DeathCause::Trauma),
            ..default()
        };
        assert_eq!(corpse.vigor(), 0.0);
    }

    #[test]
    fn lethal_impact_and_depletion_have_distinct_causes() {
        let trauma = Vitality {
            last_impact: LETHAL_IMPACT_SPEED,
            ..default()
        };
        let wasting = Vitality {
            health: 0.0,
            ..default()
        };
        assert_eq!(death_cause(trauma), Some(DeathCause::Trauma));
        assert_eq!(death_cause(wasting), Some(DeathCause::Wasting));
    }

    #[test]
    fn corpse_deflation_reaches_an_exact_finite_scale() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        let mut vitality = Vitality::default();
        let target = 0.88;
        for _ in 0..300 {
            deflate_towards(&mut blob, &mut vitality, target, 0.001);
        }
        assert_eq!(vitality.shape_scale, target);
        let settled_radius = blob.rest_radius;
        for _ in 0..300 {
            deflate_towards(&mut blob, &mut vitality, target, 0.001);
        }
        assert_eq!(blob.rest_radius, settled_radius);
    }
}
