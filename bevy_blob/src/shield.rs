use super::*;
use std::collections::{HashMap, HashSet};

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

#[derive(Clone, Copy, Debug, PartialEq)]
struct SpineSpec {
    contour_fraction: f32,
    length_factor: f32,
    width_factor: f32,
}

pub(super) fn shield_spine_fans(
    blob_id: u64,
    blob: &Blob,
    extension: f32,
    platforms: &[Platform],
    contour: &[Vec2],
) -> Vec<(Vec<Vec2>, Vec2)> {
    if extension <= 0.01 {
        return Vec::new();
    }
    let center = blob.center();
    let perimeter = contour
        .iter()
        .copied()
        .zip(contour.iter().copied().cycle().skip(1))
        .take(contour.len())
        .map(|(start, end)| start.distance(end))
        .sum::<f32>();
    spine_layout(blob_id, shield_spine_count(blob.rest_radius))
        .into_iter()
        .map(|spec| {
            let size_multiplier = spine_size_multiplier(blob.rest_radius);
            let center_distance = spec.contour_fraction * perimeter;
            let base_center = sample_closed_contour(contour, center_distance, perimeter);
            let length = blob.rest_radius * spec.length_factor * extension * size_multiplier;
            let desired_tip = base_center + (base_center - center).normalize_or(Vec2::Y) * length;
            let tip = clip_spine_tip(
                base_center,
                desired_tip,
                platforms,
                (0.8 * blob.size_scale()).max(0.35),
            );
            let half_width = blob.rest_edge
                * (0.40 + extension * 0.10)
                * spec.width_factor
                * size_multiplier.powf(0.68);
            (
                contour_arc(
                    contour,
                    center_distance - half_width,
                    center_distance + half_width,
                    perimeter,
                ),
                tip,
            )
        })
        .collect()
}

fn spine_size_multiplier(radius: f32) -> f32 {
    1.12 * (INITIAL_RADIUS / radius.max(MIN_SHIELD_RADIUS))
        .sqrt()
        .clamp(1.0, 1.75)
}

fn sample_closed_contour(contour: &[Vec2], distance: f32, perimeter: f32) -> Vec2 {
    let target = distance.rem_euclid(perimeter);
    let mut traversed = 0.0;
    for (start, end) in contour
        .iter()
        .copied()
        .zip(contour.iter().copied().cycle().skip(1))
        .take(contour.len())
    {
        let edge_length = start.distance(end);
        if traversed + edge_length >= target {
            return start.lerp(end, (target - traversed) / edge_length.max(f32::EPSILON));
        }
        traversed += edge_length;
    }
    contour[0]
}

fn contour_arc(contour: &[Vec2], start: f32, end: f32, perimeter: f32) -> Vec<Vec2> {
    let mut points = vec![sample_closed_contour(contour, start, perimeter)];
    let mut traversed = 0.0;
    let mut interior = Vec::new();
    for index in 0..contour.len() {
        if index > 0 {
            traversed += contour[index - 1].distance(contour[index]);
        }
        let mut unwrapped = traversed;
        while unwrapped <= start {
            unwrapped += perimeter;
        }
        if unwrapped < end {
            interior.push((unwrapped, contour[index]));
        }
    }
    interior.sort_by(|first, second| first.0.total_cmp(&second.0));
    points.extend(interior.into_iter().map(|(_, point)| point));
    points.push(sample_closed_contour(contour, end, perimeter));
    points.dedup_by(|first, second| first.distance_squared(*second) < 0.000_001);
    points
}

fn shield_spine_count(radius: f32) -> usize {
    let relative = (radius / INITIAL_RADIUS).clamp(0.3, 1.4);
    (6.0 + relative * 10.0).round() as usize
}

fn spine_layout(blob_id: u64, count: usize) -> Vec<SpineSpec> {
    let phase = spine_random(blob_id, count as u64, 7) / count as f32;
    (0..count)
        .map(|index| SpineSpec {
            contour_fraction: (phase
                + (index as f32 + 0.16 + spine_random(blob_id, index as u64, 0) * 0.68)
                    / count as f32)
                .fract(),
            length_factor: 0.18 + spine_random(blob_id, index as u64, 1) * 0.24,
            width_factor: 0.72 + spine_random(blob_id, index as u64, 2) * 0.56,
        })
        .collect()
}

fn spine_random(blob_id: u64, index: u64, channel: u64) -> f32 {
    let mut value = blob_id
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(index.wrapping_mul(0xbf58_476d_1ce4_e5b9))
        .wrapping_add(channel.wrapping_mul(0x94d0_49bb_1331_11eb));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value as u32) as f32 / u32::MAX as f32
}

fn clip_spine_tip(base: Vec2, desired_tip: Vec2, platforms: &[Platform], clearance: f32) -> Vec2 {
    let direction = desired_tip - base;
    let length = direction.length();
    if length <= 0.0001 {
        return base;
    }
    let first_contact = platforms
        .iter()
        .filter_map(|platform| segment_aabb_entry(base, desired_tip, platform))
        .reduce(f32::min);
    let Some(first_contact) = first_contact else {
        return desired_tip;
    };
    let clearance_fraction = clearance / length;
    base + direction * (first_contact - clearance_fraction).clamp(0.0, 1.0)
}

fn segment_aabb_entry(start: Vec2, end: Vec2, platform: &Platform) -> Option<f32> {
    let minimum = platform.center - platform.half_size;
    let maximum = platform.center + platform.half_size;
    let direction = end - start;
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;
    for (origin, delta, min_axis, max_axis) in [
        (start.x, direction.x, minimum.x, maximum.x),
        (start.y, direction.y, minimum.y, maximum.y),
    ] {
        if delta.abs() < 0.0001 {
            if origin < min_axis || origin > max_axis {
                return None;
            }
            continue;
        }
        let first = (min_axis - origin) / delta;
        let second = (max_axis - origin) / delta;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
        if near > far {
            return None;
        }
    }
    (far >= 0.0 && near <= 1.0).then_some(near.clamp(0.0, 1.0))
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
