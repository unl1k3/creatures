//! Surface locomotion, traction and spine-assisted wall movement.

use super::*;

pub(super) struct LocomotionStep {
    pub(super) center: Vec2,
    pub(super) center_velocity: Vec2,
    pub(super) gravity_direction: Vec2,
    horizontal: f32,
    dt: f32,
    angular_displacement: f32,
    spider_cling: Option<SpiderCling>,
    rim_progress: f32,
    acceleration: f32,
    maximum_speed: f32,
    previous_ground_traction: f32,
    previous_ground_idle_damping: f32,
    previous_ground_is_glue: bool,
}

impl Blob {
    pub(super) fn prepare_locomotion_step(
        &self,
        dt: f32,
        horizontal: f32,
        vigor: f32,
        previous_ground_traction: f32,
        previous_ground_idle_damping: f32,
        previous_ground_is_glue: bool,
    ) -> LocomotionStep {
        let center = self.center();
        let center_velocity = self
            .particles
            .iter()
            .map(|particle| particle.position - particle.previous)
            .sum::<Vec2>()
            / self.particles.len() as f32;
        let angular_displacement = self
            .particles
            .iter()
            .map(|particle| {
                let offset = particle.position - center;
                let local_velocity = particle.position - particle.previous - center_velocity;
                offset.perp_dot(local_velocity) / offset.length_squared().max(1.0)
            })
            .sum::<f32>()
            / self.particles.len() as f32;
        let spider_cling = self.spider_cling;
        let has_support = self.grounded || spider_cling.is_some();
        let acceleration = if has_support {
            GROUND_ACCELERATION
        } else {
            AIR_ACCELERATION
        } * vigor
            * if has_support {
                previous_ground_traction
            } else {
                1.0
            };
        let maximum_speed = if has_support {
            MAX_GROUND_SPEED
        } else {
            MAX_AIR_SPEED
        } * vigor;
        let rim_progress = spider_cling.map_or(0.0, |cling| {
            ((center.y - (cling.wall_top - self.rest_radius * 0.82)) / (self.rest_radius * 0.82))
                .clamp(0.0, 1.0)
        });
        let gravity_direction = spider_cling
            .map(|cling| {
                (Vec2::X * cling.wall_direction)
                    .lerp(Vec2::NEG_Y, rim_progress)
                    .normalize_or(Vec2::NEG_Y)
            })
            .unwrap_or(Vec2::NEG_Y);

        LocomotionStep {
            center,
            center_velocity,
            gravity_direction,
            horizontal,
            dt,
            angular_displacement,
            spider_cling,
            rim_progress,
            acceleration,
            maximum_speed,
            previous_ground_traction,
            previous_ground_idle_damping,
            previous_ground_is_glue,
        }
    }
}

impl LocomotionStep {
    /// Applies player steering and material response to one Verlet velocity.
    pub(super) fn apply_to_particle(
        &self,
        particle: &Particle,
        velocity: &mut Vec2,
        grounded: bool,
        rest_radius: f32,
    ) {
        let steering = self.acceleration * self.dt * self.dt;
        if self.spider_cling.is_some() {
            let climb_intent = self.horizontal.abs();
            let target_velocity_y = climb_intent * self.maximum_speed * self.dt;
            velocity.y += (target_velocity_y - self.center_velocity.y).clamp(-steering, steering);
            let target_velocity_x =
                self.horizontal * self.maximum_speed * self.dt * self.rim_progress;
            velocity.x += (target_velocity_x - self.center_velocity.x).clamp(-steering, steering)
                * self.rim_progress;
        } else if self.previous_ground_is_glue {
            velocity.x *= 0.18;
        } else {
            let target_velocity_x = self.horizontal * self.maximum_speed * self.dt;
            velocity.x += (target_velocity_x - self.center_velocity.x).clamp(-steering, steering);
        }

        if grounded {
            let offset = particle.position - self.center;
            let target_angular_displacement = -self.horizontal
                * GROUND_ROLL_RATE
                * if self.previous_ground_is_glue {
                    (self.previous_ground_traction * 2.0).min(0.18)
                } else {
                    1.0
                }
                * self.dt;
            let angular_correction =
                (target_angular_displacement - self.angular_displacement) * 0.34;
            *velocity += offset.perp() * angular_correction;

            if self.previous_ground_is_glue {
                velocity.x += -target_angular_displacement * rest_radius * 0.72;
            }
            let lower_weight =
                ((self.center.y - particle.position.y) / rest_radius).clamp(0.0, 1.0);
            if !self.previous_ground_is_glue {
                velocity.x += self.horizontal
                    * steering
                    * lower_weight
                    * 0.65
                    * self.previous_ground_traction;
            }
        } else if self.spider_cling.is_some() {
            let offset = particle.position - self.center;
            let target_angular_displacement = -self.horizontal * GROUND_ROLL_RATE * self.dt;
            let angular_correction =
                (target_angular_displacement - self.angular_displacement) * 0.34;
            *velocity += offset.perp() * angular_correction;
        } else {
            let relative_velocity = *velocity - self.center_velocity;
            *velocity = self.center_velocity + relative_velocity * INTERNAL_DAMPING_AIR;
        }

        if self.horizontal == 0.0 {
            velocity.x *= if grounded {
                self.previous_ground_idle_damping
            } else {
                0.992
            };
        }
    }
}
