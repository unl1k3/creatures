//! Collision policy for the membrane against authored level geometry.

use super::*;

impl Blob {
    pub(super) fn solve_collisions(
        &mut self,
        platforms: &[Platform],
        ice_platform_indices: &[usize],
        glue_platform_indices: &[usize],
    ) {
        // Keep contact thickness constant while tonicity changes. A growing
        // collision envelope would move a resting corpse on its own.
        let skin = (5.0 * self.size_scale()).max(MIN_COLLISION_SKIN);
        let blob_center = self.center();
        let mut support_sum = Vec2::ZERO;
        let mut support_count = 0;
        for particle in &mut self.particles {
            for (platform_index, platform) in platforms.iter().enumerate() {
                let min = platform.center - platform.half_size - Vec2::splat(skin);
                let max = platform.center + platform.half_size + Vec2::splat(skin);
                let inside = particle.position.x >= min.x
                    && particle.position.x <= max.x
                    && particle.position.y >= min.y
                    && particle.position.y <= max.y;
                let swept_entry = swept_aabb_entry(particle.previous, particle.position, min, max);
                if !inside && swept_entry.is_none() {
                    continue;
                }

                let side = swept_entry.map(|(side, _)| side).unwrap_or_else(|| {
                    collision_side_from_reference(particle, blob_center, min, max)
                });
                let normal = [Vec2::NEG_X, Vec2::X, Vec2::NEG_Y, Vec2::Y][side];
                let impact_speed = -(particle.position - particle.previous).dot(normal);
                self.last_impact_speed = self.last_impact_speed.max(impact_speed.max(0.0));
                if !inside && let Some((_, time)) = swept_entry {
                    particle.position = particle.previous.lerp(particle.position, time);
                }
                match side {
                    0 => {
                        particle.position.x = min.x;
                        damp_normal_velocity(particle, Vec2::NEG_X, 0.12 * self.tonicity);
                    }
                    1 => {
                        particle.position.x = max.x;
                        damp_normal_velocity(particle, Vec2::X, 0.12 * self.tonicity);
                    }
                    2 => {
                        particle.position.y = min.y;
                        damp_normal_velocity(particle, Vec2::NEG_Y, 0.08 * self.tonicity);
                    }
                    _ => {
                        particle.position.y = max.y;
                        damp_normal_velocity(particle, Vec2::Y, 0.0);
                        self.grounded = true;
                        if ice_platform_indices.contains(&platform_index) {
                            self.on_ice = true;
                            self.ground_traction = self.ground_traction.min(self.ice_traction);
                            self.ground_idle_damping = self.ground_idle_damping.max(0.96);
                        }
                        if glue_platform_indices.contains(&platform_index) {
                            let size_ratio =
                                (self.rest_radius / DEFAULT_GAMEPLAY_RADIUS).clamp(0.25, 1.0);
                            self.ground_traction =
                                self.ground_traction.min(0.035 + size_ratio * 0.045);
                            self.ground_idle_damping =
                                self.ground_idle_damping.min(0.05 + size_ratio * 0.05);
                            self.on_glue = true;
                            self.ground_is_glue = true;
                        }
                        support_sum += Vec2::Y;
                        support_count += 1;
                    }
                }
            }
        }
        self.support_normal_sum += support_sum;
        self.support_contact_count += support_count;
    }

    pub(super) fn solve_fixture_collisions(&mut self, fixtures: &[Vec<Vec2>]) {
        let skin = 0.8 * self.size_scale();
        let record_impact = self.launch_grace <= 0.0;
        let mut support_sum = Vec2::ZERO;
        let mut support_count = 0;
        for particle in &mut self.particles {
            for vertices in fixtures {
                let Some((depth, outward)) = convex_penetration(particle.position, vertices) else {
                    continue;
                };
                let velocity = particle.position - particle.previous;
                if record_impact {
                    self.last_impact_speed = self
                        .last_impact_speed
                        .max((-velocity.dot(outward)).max(0.0));
                }
                particle.position += outward * (depth + skin);
                let normal_speed = velocity.dot(outward);
                let corrected_velocity = if normal_speed < 0.0 {
                    velocity - outward * normal_speed
                } else {
                    velocity
                };
                particle.previous = particle.position - corrected_velocity * 0.82;
                self.grounded |= outward.y > 0.55;
                if outward.y > 0.55 {
                    support_sum += outward;
                    support_count += 1;
                }
            }
        }
        self.support_normal_sum += support_sum;
        self.support_contact_count += support_count;
    }
}

pub(super) fn collision_entry_side(particle: &Particle, min: Vec2, max: Vec2) -> usize {
    let movement = particle.position - particle.previous;
    let mut entry: Option<(f32, usize)> = None;
    let candidates = [
        (
            particle.previous.x < min.x && movement.x > 0.0,
            (min.x - particle.previous.x) / movement.x,
            0,
        ),
        (
            particle.previous.x > max.x && movement.x < 0.0,
            (max.x - particle.previous.x) / movement.x,
            1,
        ),
        (
            particle.previous.y < min.y && movement.y > 0.0,
            (min.y - particle.previous.y) / movement.y,
            2,
        ),
        (
            particle.previous.y > max.y && movement.y < 0.0,
            (max.y - particle.previous.y) / movement.y,
            3,
        ),
    ];
    for (valid, time, side) in candidates {
        if valid && (0.0..=1.0).contains(&time) && entry.is_none_or(|(best, _)| time < best) {
            entry = Some((time, side));
        }
    }
    entry.map(|(_, side)| side).unwrap_or_else(|| {
        [
            (particle.position.x - min.x).abs(),
            (max.x - particle.position.x).abs(),
            (particle.position.y - min.y).abs(),
            (max.y - particle.position.y).abs(),
        ]
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .map(|(side, _)| side)
        .unwrap_or(3)
    })
}

pub(super) fn collision_side_from_reference(
    particle: &Particle,
    reference: Vec2,
    min: Vec2,
    max: Vec2,
) -> usize {
    let width = (max.x - min.x).max(0.001);
    let height = (max.y - min.y).max(0.001);
    let outside = [
        ((min.x - reference.x) / width, 0),
        ((reference.x - max.x) / width, 1),
        ((min.y - reference.y) / height, 2),
        ((reference.y - max.y) / height, 3),
    ];
    let (distance, side) = outside
        .into_iter()
        .max_by(|first, second| first.0.total_cmp(&second.0))
        .unwrap_or((0.0, 0));
    if distance > 0.0 {
        side
    } else {
        collision_entry_side(particle, min, max)
    }
}

pub(super) fn swept_aabb_entry(
    start: Vec2,
    end: Vec2,
    min: Vec2,
    max: Vec2,
) -> Option<(usize, f32)> {
    if start.x >= min.x && start.x <= max.x && start.y >= min.y && start.y <= max.y {
        return None;
    }
    let movement = end - start;
    let mut entry_time = 0.0_f32;
    let mut exit_time = 1.0_f32;
    let mut entry_side = 0;
    for axis in 0..2 {
        let origin = if axis == 0 { start.x } else { start.y };
        let delta = if axis == 0 { movement.x } else { movement.y };
        let lower = if axis == 0 { min.x } else { min.y };
        let upper = if axis == 0 { max.x } else { max.y };
        if delta.abs() < 0.000_001 {
            if origin < lower || origin > upper {
                return None;
            }
            continue;
        }
        let mut near = (lower - origin) / delta;
        let mut far = (upper - origin) / delta;
        let near_side = if axis == 0 {
            if delta > 0.0 { 0 } else { 1 }
        } else if delta > 0.0 {
            2
        } else {
            3
        };
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        if near > entry_time {
            entry_time = near;
            entry_side = near_side;
        }
        exit_time = exit_time.min(far);
        if entry_time > exit_time {
            return None;
        }
    }
    (entry_time <= 1.0 && exit_time >= 0.0).then_some((entry_side, entry_time.max(0.0)))
}

pub(super) fn convex_penetration(point: Vec2, vertices: &[Vec2]) -> Option<(f32, Vec2)> {
    if vertices.len() < 3 {
        return None;
    }
    let signed_area = vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
        .map(|(first, second)| first.perp_dot(*second))
        .sum::<f32>();
    let orientation = signed_area.signum();
    if orientation == 0.0 {
        return None;
    }
    let mut nearest = f32::INFINITY;
    let mut outward = Vec2::Y;
    for (first, second) in vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
    {
        let edge = *second - *first;
        let length = edge.length();
        if length <= f32::EPSILON {
            continue;
        }
        let inward_distance = edge.perp_dot(point - *first) * orientation / length;
        if inward_distance < 0.0 {
            return None;
        }
        if inward_distance < nearest {
            nearest = inward_distance;
            outward = -edge.perp() * orientation / length;
        }
    }
    nearest.is_finite().then_some((nearest, outward))
}

fn damp_normal_velocity(particle: &mut Particle, normal: Vec2, restitution: f32) {
    let velocity = particle.position - particle.previous;
    let normal_speed = velocity.dot(normal);
    if normal_speed < 0.0 {
        particle.previous =
            particle.position - (velocity - normal * normal_speed * (1.0 + restitution));
    }
}
