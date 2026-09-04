//! Elastic constraints and topology protection for the blob membrane.
//!
//! These routines only change the contour. Input, jumping, gravity and level
//! contacts remain in the parent module, which keeps the movement pipeline
//! readable while allowing the soft-body parameters to evolve independently.

use super::*;

impl Blob {
    pub(super) fn solve_edges(&mut self) {
        let count = self.particles.len();
        let mut corrections = vec![Vec2::ZERO; count];
        for index in 0..count {
            let next = (index + 1) % count;
            let delta = self.particles[next].position - self.particles[index].position;
            let length = delta.length();
            if length > 0.0001 {
                let limpness = 1.0 - self.tonicity;
                let rest_length = self.edge_rest_lengths[index]
                    * (1.0 + corpse_material_variation(index, 0) * 0.12 * limpness);
                let tension = (0.20 + 0.28 * self.tonicity)
                    * (1.0 + corpse_material_variation(index, 1) * 0.16 * limpness);
                let correction = delta * ((length - rest_length) / length * tension);
                corrections[index] += correction;
                corrections[next] -= correction;
            }
        }
        apply_shape_corrections(&mut self.particles, corrections, self.tonicity);
    }

    pub(super) fn solve_area(&mut self) {
        let area = polygon_area(&self.particles);
        if area.abs() < 0.001 {
            return;
        }
        let center = self.center();
        let scale = (self.rest_area / area.abs()).sqrt();
        let pressure = 0.075 + 0.045 * self.tonicity;
        for particle in &mut self.particles {
            let corrected =
                center + (particle.position - center) * (1.0 + (scale - 1.0) * pressure);
            let correction = corrected - particle.position;
            particle.position = corrected;
            if self.tonicity < 0.5 {
                particle.previous += correction;
            }
        }
    }

    /// Expands an upper lobe, returns to rest, pauses and then picks another.
    /// Corrections are balanced so the animation cannot translate the blob.
    pub(super) fn solve_idle_shape(&mut self) {
        if self.idle_amount < 0.001 {
            return;
        }
        const ACTIVE_PART: f32 = 0.68;
        let local_time = self.idle_phase.fract();
        if local_time >= ACTIVE_PART {
            return;
        }

        let center = self.center();
        let breath_phase = local_time / ACTIVE_PART * std::f32::consts::PI;
        let pulse = breath_phase.sin().powi(2) * self.idle_amount;
        let lobe_center = idle_lobe_center(self.idle_phase.floor() as usize % 4);
        let mut corrections = vec![Vec2::ZERO; self.particles.len()];
        let mut active = 0;
        for (index, particle) in self.particles.iter().enumerate() {
            let offset = particle.position - center;
            let distance = offset.length();
            if distance < 0.001 {
                continue;
            }
            let angle = offset.y.atan2(offset.x).rem_euclid(std::f32::consts::TAU);
            let angle_delta = (angle - lobe_center)
                .abs()
                .min(std::f32::consts::TAU - (angle - lobe_center).abs());
            let locality = (-angle_delta.powi(2) / (2.0 * 0.48_f32.powi(2))).exp();
            let upper = angle.sin().max(0.0).powf(0.7);
            if upper > 0.0 {
                let target_radius = self.rest_radius * (1.0 + 0.34 * pulse * locality * upper);
                corrections[index] = offset / distance * (target_radius - distance) * 0.18 * pulse;
                active += 1;
            }
        }
        if active == 0 {
            return;
        }
        let average = corrections.iter().copied().sum::<Vec2>() / active as f32;
        for (index, particle) in self.particles.iter_mut().enumerate() {
            if corrections[index] == Vec2::ZERO {
                continue;
            }
            let correction = corrections[index] - average;
            particle.position += correction;
            particle.previous += correction;
        }
    }

    pub(super) fn solve_curvature(&mut self) {
        let count = self.particles.len();
        let mut corrections = vec![Vec2::ZERO; count];
        for index in 0..count {
            let next = (index + 2) % count;
            let delta = self.particles[next].position - self.particles[index].position;
            let length = delta.length();
            if length > 0.0001 {
                let limpness = 1.0 - self.tonicity;
                let rest_length = self.curvature_rest_lengths[index]
                    * (1.0 + corpse_material_variation(index, 2) * 0.08 * limpness);
                let correction =
                    delta * ((length - rest_length) / length * (0.05 + 0.11 * self.tonicity));
                corrections[index] += correction;
                corrections[next] -= correction;
            }
        }
        apply_shape_corrections(&mut self.particles, corrections, self.tonicity);
    }

    pub(super) fn limit_stretch(&mut self) {
        let maximum_radius = self.rest_radius * (MAX_STRETCH_RATIO + (1.0 - self.tonicity) * 0.22);
        for _ in 0..4 {
            let center = self.center();
            for particle in &mut self.particles {
                let offset = particle.position - center;
                if offset.length_squared() > maximum_radius * maximum_radius {
                    particle.position = center + offset.clamp_length_max(maximum_radius);
                    let velocity = particle.position - particle.previous;
                    particle.previous = particle.position - velocity * 0.35;
                }
            }
        }
    }

    pub(super) fn limit_collapse(&mut self) {
        let minimum_radius =
            self.rest_radius * (0.28 + (MIN_COLLAPSE_RATIO - 0.28) * self.tonicity);
        for _ in 0..4 {
            let center = self.center();
            for particle in &mut self.particles {
                let offset = particle.position - center;
                let distance = offset.length();
                if distance >= minimum_radius {
                    continue;
                }
                let direction = if distance > 0.0001 {
                    offset / distance
                } else {
                    Vec2::Y
                };
                let velocity = particle.position - particle.previous;
                particle.position = center + direction * minimum_radius;
                let outward_speed = velocity.dot(direction).max(0.0);
                let tangent = velocity - direction * velocity.dot(direction);
                particle.previous = particle.position - tangent * 0.55 - direction * outward_speed;
            }
        }
    }

    pub(super) fn repair_self_intersection(&mut self) -> bool {
        if !has_self_intersections(&self.particles) {
            return false;
        }
        let center = self.center();
        let velocity = self.velocity();
        let count = self.particles.len();
        let mut phase_vector = Vec2::ZERO;
        let mut average_radius = 0.0;
        for (index, particle) in self.particles.iter().enumerate() {
            let offset = particle.position - center;
            average_radius += offset.length();
            if offset.length_squared() > 0.0001 {
                let material_angle = index as f32 / count as f32 * std::f32::consts::TAU;
                phase_vector += Vec2::from_angle(offset.to_angle() - material_angle);
            }
        }
        average_radius =
            (average_radius / count as f32).clamp(self.rest_radius * 0.72, self.rest_radius * 1.12);
        let phase = if phase_vector.length_squared() > 0.0001 {
            phase_vector.to_angle()
        } else {
            0.0
        };
        for (index, particle) in self.particles.iter_mut().enumerate() {
            let angle = phase + index as f32 / count as f32 * std::f32::consts::TAU;
            particle.position = center + Vec2::from_angle(angle) * average_radius;
            particle.previous = particle.position - velocity;
        }
        true
    }
}

fn apply_shape_corrections(particles: &mut [Particle], corrections: Vec<Vec2>, tonicity: f32) {
    for (particle, correction) in particles.iter_mut().zip(corrections) {
        particle.position += correction;
        if tonicity < 0.5 {
            particle.previous += correction;
        }
    }
}

pub(super) fn idle_lobe_center(cycle: usize) -> f32 {
    [0.38, 2.22, 0.92, 2.76][cycle % 4]
}

fn corpse_material_variation(index: usize, channel: u64) -> f32 {
    let mut value = (index as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(channel.wrapping_mul(0xbf58_476d_1ce4_e5b9));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value as u32) as f32 / u32::MAX as f32 * 2.0 - 1.0
}

pub(super) fn has_self_intersections(particles: &[Particle]) -> bool {
    let count = particles.len();
    for first in 0..count {
        let first_next = (first + 1) % count;
        for second in (first + 1)..count {
            let second_next = (second + 1) % count;
            if first == second_next || first_next == second || first == second {
                continue;
            }
            let a = particles[first].position;
            let b = particles[first_next].position;
            let c = particles[second].position;
            let d = particles[second_next].position;
            let ab_c = (b - a).perp_dot(c - a);
            let ab_d = (b - a).perp_dot(d - a);
            let cd_a = (d - c).perp_dot(a - c);
            let cd_b = (d - c).perp_dot(b - c);
            if ab_c * ab_d < 0.0 && cd_a * cd_b < 0.0 {
                return true;
            }
        }
    }
    false
}
