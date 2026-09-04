use super::*;
use std::collections::{HashMap, HashSet};

mod contacts;
mod geometry;

pub(super) use contacts::spider_climb_anchor_direction;
pub(super) use geometry::shield_spine_fans;
#[cfg(test)]
use geometry::{
    clip_spine_tip, contour_arc, shield_spine_count, spine_layout, spine_size_multiplier,
};

const MIN_SHIELD_RADIUS: f32 = INITIAL_RADIUS * 0.30;
const SHIELD_DRAIN_RATE: f32 = 0.28;
const SHIELD_RECHARGE_RATE: f32 = 0.18;
const SHIELD_DEPLOY_RATE: f32 = 5.5;
const SHIELD_RETRACT_RATE: f32 = 7.0;

#[derive(Clone, Copy, Debug)]
struct ShieldStatus {
    energy: f32,
    extension: f32,
}

impl Default for ShieldStatus {
    fn default() -> Self {
        Self {
            energy: 1.0,
            extension: 0.0,
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct ShieldWorld {
    states: HashMap<u64, ShieldStatus>,
}

impl ShieldWorld {
    pub(super) fn extension(&self, blob_id: u64) -> f32 {
        self.states
            .get(&blob_id)
            .map(|state| state.extension)
            .unwrap_or(0.0)
    }

    pub(super) fn is_active(&self, blob_id: u64) -> bool {
        self.extension(blob_id) > 0.08
    }

    pub(super) fn energy(&self, blob_id: u64) -> f32 {
        self.states
            .get(&blob_id)
            .map(|state| state.energy)
            .unwrap_or(1.0)
    }

    pub(super) fn reset(&mut self) {
        self.states.clear();
    }
}

pub(super) fn simulate_shields(
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut blobs: ResMut<BlobWorld>,
    mut shields: ResMut<ShieldWorld>,
    vitality: Res<VitalityWorld>,
    nutrition: Res<NutritionWorld>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    let dt = time.delta_secs();
    let active_ids = blobs
        .active
        .iter()
        .map(|blob| blob.id)
        .collect::<HashSet<_>>();
    shields.states.retain(|id, _| active_ids.contains(id));
    let selected = blobs.selected;
    let rejoining = blobs.rejoin_parent.is_some();

    for (index, active_blob) in blobs.active.iter_mut().enumerate() {
        let status = shields.states.entry(active_blob.id).or_default();
        let extension_before = status.extension;
        let wants_shield = index == selected
            && keyboard.pressed(KeyCode::KeyQ)
            && vitality.is_alive(active_blob.id)
            && active_blob.body.rest_radius >= MIN_SHIELD_RADIUS
            && !rejoining;
        update_status(
            status,
            wants_shield,
            dt,
            vitality.vigor(active_blob.id) * nutrition.capability_factor(active_blob.id),
        );
        if extension_before <= 0.02 && status.extension > 0.02 {
            sound_events.write(BlobSoundEvent::ShieldDeploy);
        } else if extension_before > 0.08 && status.extension <= 0.02 {
            sound_events.write(BlobSoundEvent::ShieldRetract);
        }
        if status.extension > 0.02 {
            active_blob.body.cancel_jump_charge();
        }
    }
}

fn update_status(status: &mut ShieldStatus, wants_shield: bool, dt: f32, vigor: f32) {
    if wants_shield && status.energy > 0.001 {
        status.energy = (status.energy - SHIELD_DRAIN_RATE * dt).max(0.0);
        status.extension = (status.extension + SHIELD_DEPLOY_RATE * vigor * dt)
            .clamp(0.0, (status.energy * vigor).min(1.0));
    } else {
        status.extension = (status.extension - SHIELD_RETRACT_RATE * dt).max(0.0);
        if status.extension <= 0.001 {
            status.energy = (status.energy + SHIELD_RECHARGE_RATE * dt).min(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shield_consumes_energy_and_recharges_when_retracted() {
        let mut status = ShieldStatus::default();
        update_status(&mut status, true, 1.0, 1.0);
        let drained = status.energy;
        assert!(drained < 1.0 && status.extension > 0.0);
        update_status(&mut status, false, 1.0, 1.0);
        assert_eq!(status.extension, 0.0);
        assert!(status.energy > drained);
    }

    #[test]
    fn smaller_blobs_receive_fewer_spines() {
        assert!(shield_spine_count(INITIAL_RADIUS * 0.4) < shield_spine_count(INITIAL_RADIUS));
    }

    #[test]
    fn spine_layout_is_stable_but_irregular() {
        let first = spine_layout(12, 14);
        let second = spine_layout(12, 14);
        assert_eq!(first, second);
        let shortest = first
            .iter()
            .map(|spine| spine.length_factor)
            .fold(f32::INFINITY, f32::min);
        let longest = first
            .iter()
            .map(|spine| spine.length_factor)
            .fold(0.0, f32::max);
        assert!(longest - shortest > 0.10);
        assert!(first.iter().enumerate().any(|(index, spine)| {
            let regular = (index as f32 + 0.5) / first.len() as f32;
            (spine.contour_fraction - regular).abs() > 0.01
        }));
    }

    #[test]
    fn small_blobs_receive_larger_relative_spines() {
        let normal = spine_size_multiplier(INITIAL_RADIUS);
        let small = spine_size_multiplier(INITIAL_RADIUS * 0.45);

        assert!(normal > 1.0);
        assert!(small > normal * 1.35);
    }

    #[test]
    fn spine_stops_smoothly_before_a_contact_surface() {
        let platform = Platform {
            center: Vec2::new(50.0, 0.0),
            half_size: Vec2::new(10.0, 10.0),
        };
        let clipped = clip_spine_tip(Vec2::new(39.0, 0.0), Vec2::new(60.0, 0.0), &[platform], 0.5);
        assert!((clipped.x - 39.5).abs() < 0.001);

        let unobstructed =
            clip_spine_tip(Vec2::new(0.0, 40.0), Vec2::new(0.0, 60.0), &[platform], 0.5);
        assert_eq!(unobstructed, Vec2::new(0.0, 60.0));
    }

    #[test]
    fn spine_base_keeps_all_membrane_vertices_across_the_seam() {
        let contour = [
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
        ];
        let arc = contour_arc(&contour, 35.0, 45.0, 40.0);

        assert_eq!(arc.len(), 3);
        assert_eq!(arc[1], contour[0]);
    }

    #[test]
    fn reset_discards_fragment_states() {
        let mut shields = ShieldWorld::default();
        shields.states.insert(4, ShieldStatus::default());
        shields.reset();
        assert!(shields.states.is_empty());
    }
}
