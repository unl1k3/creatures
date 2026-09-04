//! Player locomotion, jump charging, and material-aware movement.

use super::jump::JumpParticleFrame;
use super::*;

/// Player or scripted intent applied during one fixed physics step.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BlobStepInput {
    pub(crate) horizontal: f32,
    pub(crate) charging: bool,
}

/// Collision geometry and material classification used by one movement step.
///
/// The material lists contain indices into `platforms`; keeping them together
/// makes that relationship explicit and prevents mismatched slices at call sites.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BlobStepEnvironment<'a> {
    pub(crate) platforms: &'a [Platform],
    pub(crate) ice_platform_indices: &'a [usize],
    pub(crate) glue_platform_indices: &'a [usize],
    pub(crate) fixtures: &'a [Vec<Vec2>],
}

/// Biological state that scales movement without changing collision geometry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BlobStepProfile {
    pub(crate) vigor: f32,
    pub(crate) animate_idle: bool,
    pub(crate) retain_tonicity: bool,
}

impl BlobStepProfile {
    #[cfg(test)]
    pub(crate) const fn new(vigor: f32, animate_idle: bool, retain_tonicity: bool) -> Self {
        Self {
            vigor,
            animate_idle,
            retain_tonicity,
        }
    }
}

impl Blob {
    #[cfg(test)]
    pub fn step(&mut self, dt: f32, horizontal: f32, charging: bool, platforms: &[Platform]) {
        self.step_with_vigor(
            dt,
            horizontal,
            charging,
            platforms,
            &[],
            BlobStepProfile::new(1.0, true, true),
        );
    }

    #[cfg(test)]
    pub fn step_with_vigor(
        &mut self,
        dt: f32,
        horizontal: f32,
        charging: bool,
        platforms: &[Platform],
        fixtures: &[Vec<Vec2>],
        profile: BlobStepProfile,
    ) {
        self.step_with_vigor_on_ice(
            dt,
            BlobStepInput {
                horizontal,
                charging,
            },
            BlobStepEnvironment {
                platforms,
                ice_platform_indices: &[],
                glue_platform_indices: &[],
                fixtures,
            },
            profile,
        );
    }

    /// Sets the traction available when the membrane rests on an ice surface.
    /// A bare gel body cannot push against ice; deployed pseudo-spines supply
    /// a deliberately small grip without making the surface behave as stone.
    pub fn set_ice_traction(&mut self, traction: f32) {
        self.ice_traction = traction.clamp(0.0, 1.0);
    }

    pub fn on_glue(&self) -> bool {
        self.on_glue
    }

    /// True only while the membrane has a real supporting contact with ice.
    /// This is deliberately separate from traction: spines can add grip
    /// without changing which material produced the contact.
    pub fn on_ice(&self) -> bool {
        self.on_ice
    }

    /// As [`Self::step_with_vigor`], with a list of low-traction platform
    /// indices in the supplied collision slice. Keeping this parallel list
    /// avoids making the core `Platform` geometry depend on presentation data.
    pub fn step_with_vigor_on_ice(
        &mut self,
        dt: f32,
        input: BlobStepInput,
        environment: BlobStepEnvironment<'_>,
        profile: BlobStepProfile,
    ) {
        let BlobStepInput {
            horizontal,
            charging,
        } = input;
        let BlobStepProfile {
            vigor,
            animate_idle,
            retain_tonicity,
        } = profile;
        self.last_impact_speed = 0.0;
        self.launch_grace = (self.launch_grace - dt).max(0.0);
        if self.support_contact_count > 0 {
            self.support_normal = self.support_normal_sum.normalize_or(Vec2::Y);
        }
        let previous_ground_traction = self.ground_traction;
        let previous_ground_idle_damping = self.ground_idle_damping;
        let previous_ground_is_glue = self.ground_is_glue;
        self.ground_traction = 1.0;
        self.ground_idle_damping = 0.72;
        self.on_ice = false;
        self.on_glue = false;
        self.ground_is_glue = false;
        self.support_normal_sum = Vec2::ZERO;
        self.support_contact_count = 0;
        let target_tonicity = if retain_tonicity { 1.0 } else { 0.0 };
        let tonicity_response = if retain_tonicity { 2.0 } else { 1.35 };
        self.tonicity +=
            (target_tonicity - self.tonicity) * (tonicity_response * dt).clamp(0.0, 1.0);
        // One unit is one localized breath followed by a resting pause.
        self.idle_phase += dt / 2.6;
        let wants_idle = animate_idle
            && (self.grounded || self.spider_cling.is_some())
            && horizontal == 0.0
            && !charging;
        let idle_anchor_x = wants_idle.then(|| self.center().x);
        let idle_target = if wants_idle { 1.0 } else { 0.0 };
        let idle_response = if wants_idle { 1.8 } else { 7.0 };
        self.idle_amount += (idle_target - self.idle_amount) * (idle_response * dt).clamp(0.0, 1.0);
        if !animate_idle || horizontal != 0.0 || charging {
            self.idle_amount = 0.0;
        }

        let jump = self.prepare_jump_step(dt, horizontal, charging, previous_ground_is_glue);
        let locomotion = self.prepare_locomotion_step(
            dt,
            horizontal,
            vigor,
            previous_ground_traction,
            previous_ground_idle_damping,
            previous_ground_is_glue,
        );
        let jump_particle_frame = JumpParticleFrame {
            center: locomotion.center,
            rest_edge: self.rest_edge,
            charge: self.charge,
            charge_direction: self.charge_direction,
            dt,
            vigor,
            charging,
            armed: self.jump_armed,
        };
        let grounded = self.grounded;
        let rest_radius = self.rest_radius;
        for particle in &mut self.particles {
            let mut velocity = particle.position - particle.previous;
            if !animate_idle {
                // Dissipate deformation without stopping the whole body's
                // fall or slide: dead tissue does not wobble or spring back.
                velocity =
                    locomotion.center_velocity + (velocity - locomotion.center_velocity) * 0.40;
            }
            locomotion.apply_to_particle(particle, &mut velocity, grounded, rest_radius);
            velocity.x = velocity.x.clamp(
                -MAX_PARTICLE_HORIZONTAL_SPEED * dt,
                MAX_PARTICLE_HORIZONTAL_SPEED * dt,
            );
            velocity.y = velocity.y.clamp(
                -MAX_VERTICAL_SPEED * dt,
                MAX_VERTICAL_SPEED * jump.size_factor * dt,
            );

            particle.previous = particle.position;
            particle.position += velocity + locomotion.gravity_direction * GRAVITY * dt * dt;

            jump.apply_to_particle(particle, &jump_particle_frame);
        }
        self.finish_jump_step(charging, jump);
        self.solve_movement_constraints(environment, animate_idle, idle_anchor_x, dt);
    }
}
