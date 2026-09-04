//! Hydrodynamics for the soft-body membrane.
//!
//! Keeping this policy separate from the elastic solver makes the liquid
//! behaviour easier to tune without obscuring ordinary ground movement.

use super::*;

impl Blob {
    /// Applies a light-body buoyancy model to the deformable membrane.
    ///
    /// The water is not a solid collider: a partly immersed blob remains
    /// controllable and can bob with the surface, while uniform drag keeps the
    /// solver stable instead of pinning individual membrane particles.
    #[cfg(test)]
    pub fn apply_wastewater_forces(
        &mut self,
        surface_y: f32,
        bottom_y: f32,
        dt: f32,
    ) -> Option<WastewaterContact> {
        self.apply_wastewater_forces_with_spine_drag(surface_y, bottom_y, dt, 0.0, 0.0)
    }

    /// Gameplay variant: deployed spines increase drag and supply controlled
    /// propulsion through the wastewater.
    pub fn apply_wastewater_forces_with_spine_drag(
        &mut self,
        surface_y: f32,
        bottom_y: f32,
        dt: f32,
        spine_extension: f32,
        swim_direction: f32,
    ) -> Option<WastewaterContact> {
        let submerged_fraction = self
            .particles
            .iter()
            .map(|particle| {
                if particle.position.y <= bottom_y || particle.position.y >= surface_y {
                    0.0
                } else {
                    ((surface_y - particle.position.y) / (self.rest_radius * 1.35)).clamp(0.0, 1.0)
                }
            })
            .sum::<f32>()
            / self.particles.len() as f32;
        if submerged_fraction <= WATER_EXIT_FRACTION {
            self.water_exit_elapsed += dt;
            if self.water_exit_elapsed >= WATER_EXIT_GRACE {
                self.water_submerged = false;
            }
            return None;
        }
        self.water_exit_elapsed = 0.0;

        let entry_speed = (-self.velocity().y / dt.max(0.000_001)).max(0.0);
        let drag_rate = if spine_extension > 0.05 {
            WATER_SPINED_DRAG_RATE
        } else {
            WATER_BARE_DRAG_RATE
        };
        self.damp_velocity((-drag_rate * submerged_fraction.sqrt() * dt).exp());
        self.apply_water_motion_shape(
            surface_y,
            submerged_fraction,
            spine_extension,
            swim_direction,
            dt,
        );
        // Verlet stores velocity as displacement per fixed step, so a force
        // must be converted with dt² to avoid a trampoline-like water impact.
        self.add_velocity(Vec2::Y * GRAVITY * 2.15 * submerged_fraction.powf(0.82) * dt * dt);
        let entered = !self.water_submerged && submerged_fraction >= WATER_ENTRY_FRACTION;
        if submerged_fraction >= WATER_ENTRY_FRACTION {
            self.water_submerged = true;
        }
        Some(WastewaterContact {
            surface_y,
            submerged_fraction,
            entered,
            entry_speed,
        })
    }

    /// Water resists translation but carries the membrane into a gentle roll.
    /// The shape change is transient: springs restore the neutral contour on
    /// exit from the liquid.
    fn apply_water_motion_shape(
        &mut self,
        surface_y: f32,
        submerged_fraction: f32,
        spine_extension: f32,
        swim_direction: f32,
        dt: f32,
    ) {
        let center = self.center();
        let center_velocity = self.velocity();
        let angular_displacement = self
            .particles
            .iter()
            .map(|particle| {
                let offset = particle.position - center;
                let relative_velocity = particle.position - particle.previous - center_velocity;
                offset.perp_dot(relative_velocity) / offset.length_squared().max(1.0)
            })
            .sum::<f32>()
            / self.particles.len() as f32;
        let extension = spine_extension.clamp(0.0, 1.0);
        let spine_drag = 1.0 + extension * WATER_SPINE_DRAG_MULTIPLIER;
        let swimming = extension > 0.05 && swim_direction.abs() > 0.01;
        let direction = swim_direction.signum();
        let target_rotation = if swimming {
            -direction * WATER_SWIM_ROTATION * (0.35 + extension * 0.65)
        } else {
            -center_velocity.x * WATER_ROTATION_RATE * spine_drag
        };
        let rotation_correction =
            (target_rotation - angular_displacement) * WATER_ROTATION_RESPONSE * submerged_fraction;
        let flattening = WATER_FLATTENING * submerged_fraction;
        let stroke =
            (0.58 + 0.42 * (self.idle_phase * std::f32::consts::TAU * 1.7).sin()) * extension;
        for particle in &mut self.particles {
            let offset = particle.position - center;
            let shape_target = center
                + Vec2::new(
                    offset.x * (1.0 + flattening * 0.45),
                    offset.y * (1.0 - flattening),
                );
            // Apply shape correction to both Verlet positions: this reshapes
            // the body without injecting a new velocity.
            let shape_offset = (shape_target - particle.position) * 0.16;
            particle.position += shape_offset;
            particle.previous += shape_offset;
            particle.previous -= offset.perp() * rotation_correction;

            if swimming {
                let immersed =
                    ((surface_y - particle.position.y) / self.rest_radius).clamp(0.0, 1.0);
                let trailing_side =
                    ((-direction * offset.x / self.rest_radius) + 1.0).clamp(0.0, 1.0);
                let paddle_weight = immersed * (0.30 + trailing_side * 0.70) * stroke;
                particle.previous -=
                    Vec2::X * direction * WATER_SWIM_THRUST * paddle_weight * dt * dt;
                particle.previous -=
                    offset.perp() * (-direction * WATER_SWIM_ROTATION * paddle_weight);
                let contraction = Vec2::X * (-direction * trailing_side * stroke * 0.10);
                particle.position += contraction;
                particle.previous += contraction;
            }
        }
    }
}
