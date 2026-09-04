//! Jump charging, membrane compression and launch impulses.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct JumpStep {
    pub(super) normal: Vec2,
    pub(super) tangent: Vec2,
    pub(super) right_tangent: Vec2,
    pub(super) released: bool,
    pub(super) compression_anchor: f32,
    pub(super) size_factor: f32,
    pub(super) surface_factor: f32,
}

pub(super) struct JumpParticleFrame {
    pub(super) center: Vec2,
    pub(super) rest_edge: f32,
    pub(super) charge: f32,
    pub(super) charge_direction: f32,
    pub(super) dt: f32,
    pub(super) vigor: f32,
    pub(super) charging: bool,
    pub(super) armed: bool,
}

impl Blob {
    pub fn cancel_jump_charge(&mut self) {
        self.charge = 0.0;
        self.charge_direction = 0.0;
        self.was_charging = false;
        self.jump_armed = false;
    }

    /// Applies a deliberately subtle, uncharged lift for the dance preview.
    pub fn tiny_ground_hop(&mut self, dt: f32) -> bool {
        if !self.grounded || self.charge > 0.01 || self.water_submerged {
            return false;
        }
        let lift = self.support_normal.normalize_or(Vec2::Y) * (128.0 * dt);
        self.add_velocity(lift);
        self.grounded = false;
        self.launch_grace = 0.04;
        true
    }

    pub(super) fn prepare_jump_step(
        &mut self,
        dt: f32,
        horizontal: f32,
        charging: bool,
        previous_ground_is_glue: bool,
    ) -> JumpStep {
        let normal = self.support_normal.normalize_or(Vec2::Y);
        let tangent = normal.perp();
        let right_tangent = if tangent.x >= 0.0 { tangent } else { -tangent };

        // Charging starts only from a real support contact; no coyote window
        // is retained because it allowed charging to begin in mid-air.
        if charging && !self.jump_armed && self.grounded {
            self.jump_armed = true;
        }
        if charging && self.jump_armed {
            self.charge = (self.charge + dt / CHARGE_DURATION).min(1.0);
            if horizontal.abs() > 0.01 {
                self.charge_direction = horizontal.signum();
            }
        }

        JumpStep {
            normal,
            tangent,
            right_tangent,
            released: self.was_charging && !charging && self.charge > 0.0 && self.jump_armed,
            compression_anchor: self
                .particles
                .iter()
                .map(|particle| particle.position.dot(normal))
                .fold(f32::INFINITY, f32::min),
            size_factor: jump_size_factor(self.rest_radius),
            surface_factor: if previous_ground_is_glue { 0.42 } else { 1.0 },
        }
    }

    pub(super) fn finish_jump_step(&mut self, charging: bool, jump: JumpStep) {
        if jump.released {
            let takeoff_clearance = 3.0 * self.size_scale();
            for particle in &mut self.particles {
                particle.position += jump.normal * takeoff_clearance;
            }
            self.remove_angular_velocity();
            self.charge = 0.0;
            self.jump_armed = false;
            self.grounded = false;
            self.launch_grace = 0.12;
            self.charge_direction = 0.0;
        } else if self.was_charging && !charging {
            self.charge = 0.0;
            self.jump_armed = false;
            self.charge_direction = 0.0;
        }
        self.was_charging = charging;
    }
}

impl JumpStep {
    pub(super) fn apply_to_particle(&self, particle: &mut Particle, frame: &JumpParticleFrame) {
        if frame.charging && frame.armed {
            let compression = 3.8 * frame.dt * (0.35 + frame.charge * 0.65);
            let height_above_contact = particle.position.dot(self.normal) - self.compression_anchor;
            particle.position -= self.normal * height_above_contact * compression;
            let tangent_offset = (particle.position - frame.center).dot(self.tangent);
            particle.position += self.tangent * tangent_offset * compression * 0.55;
        }
        if self.released {
            let lower_weight = ((frame.center - particle.position).dot(self.normal)
                / frame.rest_edge)
                .clamp(0.0, 2.5)
                / 2.5;
            let jump_speed = jump_speed_for_size_factor(frame.charge, self.size_factor)
                * frame.vigor
                * self.surface_factor;
            let impulse = jump_speed * frame.dt;
            let deformation = 0.28 / self.size_factor;
            particle.previous -= self.normal * impulse * (0.82 + lower_weight * deformation);
            particle.previous -= self.right_tangent * impulse * frame.charge_direction * 0.42;
        }
    }
}

fn jump_size_factor(radius: f32) -> f32 {
    (DEFAULT_GAMEPLAY_RADIUS / radius.max(1.0))
        .powf(0.8)
        .clamp(0.72, 1.90)
}

fn jump_speed_for_size_factor(charge: f32, size_factor: f32) -> f32 {
    let response = charge.clamp(0.0, 1.0).powf(1.2);
    (JUMP_MIN_SPEED + (JUMP_MAX_SPEED - JUMP_MIN_SPEED) * response) * size_factor
}

#[cfg(test)]
pub(super) fn jump_speed_for_charge(charge: f32, radius: f32) -> f32 {
    jump_speed_for_size_factor(charge, jump_size_factor(radius))
}
