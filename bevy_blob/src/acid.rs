use super::*;
use crate::palette;
use std::collections::{HashMap, HashSet};

const MIN_ACID_RADIUS: f32 = INITIAL_RADIUS * 0.55;
const ACID_COOLDOWN: f32 = 0.85;
const ACID_LIFETIME: f32 = 1.65;
const ACID_GRAVITY: f32 = 180.0;

#[derive(Clone, Copy)]
pub(super) struct AcidDrop {
    pub position: Vec2,
    pub previous: Vec2,
    velocity: Vec2,
    lifetime: f32,
    radius: f32,
}

#[derive(Resource)]
pub(super) struct AcidWorld {
    pub drops: Vec<AcidDrop>,
    cooldowns: HashMap<u64, f32>,
    rng: u64,
}

impl AcidWorld {
    pub(super) fn new(seed: u64) -> Self {
        Self {
            drops: Vec::new(),
            cooldowns: HashMap::new(),
            rng: seed.max(1),
        }
    }

    fn random_unit(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng as u32) as f32 / u32::MAX as f32
    }

    pub(super) fn reset(&mut self) {
        self.drops.clear();
        self.cooldowns.clear();
    }
}

pub(super) fn fire_acid(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut blobs: ResMut<BlobWorld>,
    mut acid: ResMut<AcidWorld>,
    shields: Res<ShieldWorld>,
    mut vitality: ResMut<VitalityWorld>,
    nutrition: Res<NutritionWorld>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    if !keyboard.just_pressed(KeyCode::Space) {
        return;
    }
    let selected = blobs.selected;
    let Some(active_blob) = blobs.active.get_mut(selected) else {
        return;
    };
    if active_blob.body.rest_radius < MIN_ACID_RADIUS
        || !vitality.is_alive(active_blob.id)
        || shields.is_active(active_blob.id)
        || acid.cooldowns.get(&active_blob.id).copied().unwrap_or(0.0) > 0.0
    {
        return;
    }

    if !vitality.spend(active_blob.id, 0.055) {
        return;
    }

    emit_acid(
        active_blob,
        &mut acid,
        vitality.vigor(active_blob.id) * nutrition.capability_factor(active_blob.id),
    );
    sound_events.write(BlobSoundEvent::AcidBurst);
}

fn emit_acid(blob: &mut ActiveBlob, acid: &mut AcidWorld, vigor: f32) {
    let count = ((acid_drop_count(blob.body.rest_radius) as f32 * (0.55 + 0.45 * vigor)).round()
        as usize)
        .max(3);
    let center = blob.body.center();
    let phase = acid.random_unit() * std::f32::consts::TAU;
    let inherited_velocity = blob.body.velocity() * 120.0;
    let mut recoil = Vec2::ZERO;

    for index in 0..count {
        let sector = index as f32 / count as f32 * std::f32::consts::TAU;
        let jitter = (acid.random_unit() - 0.5) * std::f32::consts::TAU / count as f32;
        let direction = Vec2::from_angle(phase + sector + jitter);
        let source = blob
            .body
            .particles
            .iter()
            .max_by(|first, second| {
                (first.position - center)
                    .dot(direction)
                    .total_cmp(&(second.position - center).dot(direction))
            })
            .map(|particle| particle.position)
            .unwrap_or(center);
        let speed = (360.0 + acid.random_unit() * 170.0) * (0.62 + 0.38 * vigor);
        let velocity = direction * speed + inherited_velocity * 0.55;
        let radius = (2.8 + acid.random_unit() * 2.2) * blob.body.size_scale().max(0.55);
        let lifetime = ACID_LIFETIME * (0.82 + acid.random_unit() * 0.28);
        acid.drops.push(AcidDrop {
            position: source + direction * (radius + 1.0),
            previous: source,
            velocity,
            lifetime,
            radius,
        });
        recoil -= direction;
    }

    // The nearly radial distribution keeps recoil modest, while imperfect
    // randomness still gives the creature a small organic kick.
    blob.body.add_velocity(recoil * 0.18);
    acid.cooldowns.insert(blob.id, ACID_COOLDOWN);
}

pub(super) fn simulate_acid(
    time: Res<Time<Fixed>>,
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    mut acid: ResMut<AcidWorld>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    let dt = time.delta_secs();
    let active_ids = blobs
        .active
        .iter()
        .map(|blob| blob.id)
        .collect::<HashSet<_>>();
    acid.cooldowns.retain(|id, cooldown| {
        if !active_ids.contains(id) {
            return false;
        }
        *cooldown = (*cooldown - dt).max(0.0);
        true
    });

    for drop in &mut acid.drops {
        drop.previous = drop.position;
        drop.velocity.y -= ACID_GRAVITY * dt;
        drop.velocity *= 0.997;
        drop.position += drop.velocity * dt;
        drop.lifetime -= dt;
    }
    let mut impacted = false;
    acid.drops.retain(|drop| {
        let hit_surface = level.platforms.iter().any(|platform| {
            let minimum = platform.center - platform.half_size - Vec2::splat(drop.radius);
            let maximum = platform.center + platform.half_size + Vec2::splat(drop.radius);
            drop.position.cmpge(minimum).all() && drop.position.cmple(maximum).all()
        });
        impacted |= hit_surface;
        drop.lifetime > 0.0 && !hit_surface
    });
    if impacted {
        sound_events.write(BlobSoundEvent::AcidImpact);
    }
}

pub(super) fn draw_acid(mut gizmos: Gizmos, acid: Res<AcidWorld>) {
    for drop in &acid.drops {
        let tail = drop.previous.lerp(drop.position, 0.25);
        gizmos.line_2d(tail, drop.position, palette::color(palette::ACID_TRAIL));
        gizmos.circle_2d(
            drop.position,
            drop.radius,
            palette::color(palette::ACID_BODY),
        );
        gizmos.circle_2d(
            drop.position,
            drop.radius * 0.48,
            palette::color(palette::ACID_CORE),
        );
    }
}

fn acid_drop_count(radius: f32) -> usize {
    let size = ((radius - MIN_ACID_RADIUS) / (INITIAL_RADIUS - MIN_ACID_RADIUS)).clamp(0.0, 1.5);
    (5.0 + size * 7.0).round() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acid_is_reserved_for_large_enough_blobs() {
        assert!(MIN_ACID_RADIUS > INITIAL_RADIUS * 0.5);
        assert_eq!(acid_drop_count(MIN_ACID_RADIUS), 5);
        assert!(acid_drop_count(INITIAL_RADIUS) >= 12);
    }

    #[test]
    fn acid_burst_covers_the_whole_circle() {
        let mut blob = ActiveBlob {
            id: 3,
            parent_id: None,
            body: Blob::new(Vec2::ZERO, INITIAL_RADIUS),
        };
        let mut acid = AcidWorld::new(42);
        emit_acid(&mut blob, &mut acid, 1.0);

        assert_eq!(acid.drops.len(), acid_drop_count(INITIAL_RADIUS));
        let directions = acid
            .drops
            .iter()
            .map(|drop| (drop.position - blob.body.center()).normalize())
            .collect::<Vec<_>>();
        assert!(directions.iter().any(|direction| direction.x > 0.7));
        assert!(directions.iter().any(|direction| direction.x < -0.7));
        assert!(directions.iter().any(|direction| direction.y > 0.7));
        assert!(directions.iter().any(|direction| direction.y < -0.7));
    }

    #[test]
    fn reset_removes_drops_and_weapon_cooldowns() {
        let mut blob = ActiveBlob {
            id: 3,
            parent_id: None,
            body: Blob::new(Vec2::ZERO, INITIAL_RADIUS),
        };
        let mut acid = AcidWorld::new(42);
        emit_acid(&mut blob, &mut acid, 1.0);

        acid.reset();
        assert!(acid.drops.is_empty());
        assert!(acid.cooldowns.is_empty());
    }
}
