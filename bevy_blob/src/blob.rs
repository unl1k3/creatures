use bevy::prelude::*;

pub const PARTICLE_COUNT: usize = 24;
pub const REFERENCE_RADIUS: f32 = 58.0;
pub const DEFAULT_CREATURE_SCALE: f32 = 0.65;
const DEFAULT_GAMEPLAY_RADIUS: f32 = REFERENCE_RADIUS * DEFAULT_CREATURE_SCALE;
const SPLIT_RESOLUTION_MULTIPLIER: usize = 2;
const MIN_SPLIT_SOURCE_PARTICLES: usize = 16;
const SOLVER_ITERATIONS: usize = 8;
const GRAVITY: f32 = 1_150.0;
const GROUND_ACCELERATION: f32 = 1_050.0;
const AIR_ACCELERATION: f32 = 310.0;
const MAX_GROUND_SPEED: f32 = 410.0;
const MAX_AIR_SPEED: f32 = 285.0;
const MAX_VERTICAL_SPEED: f32 = 1_450.0;
const MAX_STRETCH_RATIO: f32 = 1.58;
const MIN_COLLAPSE_RATIO: f32 = 0.34;
const CHARGE_DURATION: f32 = 0.70;
const JUMP_MIN_SPEED: f32 = 300.0;
const JUMP_MAX_SPEED: f32 = 960.0;
const GROUND_ROLL_RATE: f32 = 5.2;
const MAX_PARTICLE_HORIZONTAL_SPEED: f32 = 760.0;
const INTERNAL_DAMPING_AIR: f32 = 0.955;

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub position: Vec2,
    pub previous: Vec2,
}

#[derive(Clone, Copy, Debug)]
pub struct Platform {
    pub center: Vec2,
    pub half_size: Vec2,
}

#[derive(Resource, Clone)]
pub struct Blob {
    pub particles: Vec<Particle>,
    pub rest_edge: f32,
    pub rest_second_neighbor: f32,
    edge_rest_lengths: Vec<f32>,
    curvature_rest_lengths: Vec<f32>,
    pub rest_area: f32,
    pub rest_radius: f32,
    pub grounded: bool,
    pub coyote: f32,
    pub charge: f32,
    pub last_impact_speed: f32,
    launch_grace: f32,
    support_normal: Vec2,
    support_normal_sum: Vec2,
    support_contact_count: usize,
    charge_direction: f32,
    was_charging: bool,
    jump_armed: bool,
    idle_phase: f32,
    idle_amount: f32,
    tonicity: f32,
}

impl Blob {
    pub fn new(center: Vec2, radius: f32) -> Self {
        Self::new_with_count(center, radius, PARTICLE_COUNT)
    }

    pub fn new_with_count(center: Vec2, radius: f32, particle_count: usize) -> Self {
        assert!(particle_count >= 6, "a blob needs at least six particles");
        let particles = (0..particle_count)
            .map(|index| {
                let angle = index as f32 / particle_count as f32 * std::f32::consts::TAU;
                let position = center + Vec2::from_angle(angle) * radius;
                Particle {
                    position,
                    previous: position,
                }
            })
            .collect::<Vec<_>>();
        let rest_edge = particles[0].position.distance(particles[1].position);
        let rest_second_neighbor = particles[0].position.distance(particles[2].position);
        let rest_area = polygon_area(&particles).abs();
        Self {
            particles,
            rest_edge,
            rest_second_neighbor,
            edge_rest_lengths: vec![rest_edge; particle_count],
            curvature_rest_lengths: vec![rest_second_neighbor; particle_count],
            rest_area,
            rest_radius: radius,
            grounded: false,
            coyote: 0.0,
            charge: 0.0,
            last_impact_speed: 0.0,
            launch_grace: 0.0,
            support_normal: Vec2::Y,
            support_normal_sum: Vec2::ZERO,
            support_contact_count: 0,
            charge_direction: 0.0,
            was_charging: false,
            jump_armed: false,
            idle_phase: 0.0,
            idle_amount: 0.0,
            tonicity: 1.0,
        }
    }

    /// Creates the symmetric pair used by deterministic physics tests.
    #[cfg(test)]
    pub fn split_pair(&self, dt: f32) -> [Self; 2] {
        self.split_pair_uneven(dt, self.particles.len() / 2, true)
    }

    pub fn can_split(&self) -> bool {
        self.particles.len() >= MIN_SPLIT_SOURCE_PARTICLES
    }

    /// Splits mass, points and area according to the requested smaller child.
    /// Position and separation impulse are mass-weighted, preserving the
    /// parent's centre of mass and total momentum.
    pub fn split_pair_uneven(
        &self,
        dt: f32,
        smaller_count: usize,
        smaller_on_left: bool,
    ) -> [Self; 2] {
        let total_count = self.particles.len();
        let smaller_count = smaller_count.clamp(6, total_count.saturating_sub(6));
        let larger_count = total_count - smaller_count;
        let (left_mass_units, right_mass_units) = if smaller_on_left {
            (smaller_count, larger_count)
        } else {
            (larger_count, smaller_count)
        };
        let left_fraction = left_mass_units as f32 / total_count as f32;
        let right_fraction = right_mass_units as f32 / total_count as f32;
        let left_count = left_mass_units * SPLIT_RESOLUTION_MULTIPLIER;
        let right_count = right_mass_units * SPLIT_RESOLUTION_MULTIPLIER;
        let radius_for = |count: usize, area_fraction: f32| {
            let polygon_factor = count as f32 * (std::f32::consts::TAU / count as f32).sin();
            (2.0 * self.rest_area * area_fraction / polygon_factor).sqrt()
        };
        let left_radius = radius_for(left_count, left_fraction);
        let right_radius = radius_for(right_count, right_fraction);
        let center = self.center();
        let parent_velocity = self.velocity();

        let mut left = Self::new_with_count(center, left_radius, left_count);
        let mut right = Self::new_with_count(center, right_radius, right_count);
        let separation = Vec2::X * 85.0 * self.size_scale() * dt;
        for particle in &mut left.particles {
            particle.previous = particle.position - parent_velocity + separation * right_fraction;
        }
        for particle in &mut right.particles {
            particle.previous = particle.position - parent_velocity - separation * left_fraction;
        }
        let separation_distance = (left_radius + right_radius) * 1.35;
        left.translate(Vec2::NEG_X * separation_distance * right_fraction);
        right.translate(Vec2::X * separation_distance * left_fraction);
        [left, right]
    }

    pub fn merge_pair(first: &Self, second: &Self) -> Self {
        let particle_count = PARTICLE_COUNT;
        let total_area = first.rest_area + second.rest_area;
        let polygon_factor =
            particle_count as f32 * (std::f32::consts::TAU / particle_count as f32).sin();
        let radius = (2.0 * total_area / polygon_factor).sqrt();
        let first_mass = first.mass();
        let second_mass = second.mass();
        let total_mass = first_mass + second_mass;
        let center = (first.center() * first_mass + second.center() * second_mass) / total_mass;
        let velocity =
            (first.velocity() * first_mass + second.velocity() * second_mass) / total_mass;

        let mut merged = Self::new_with_count(center, radius, particle_count);
        for particle in &mut merged.particles {
            particle.previous = particle.position - velocity;
        }
        merged
    }

    pub fn velocity(&self) -> Vec2 {
        self.particles
            .iter()
            .map(|particle| particle.position - particle.previous)
            .sum::<Vec2>()
            / self.particles.len() as f32
    }

    pub fn mass(&self) -> f32 {
        self.rest_area
    }

    pub fn ignores_impact_trauma(&self) -> bool {
        self.launch_grace > 0.0
    }

    pub fn record_support_normal(&mut self, normal: Vec2) {
        if normal.y > 0.55 {
            self.support_normal_sum += normal.normalize_or(Vec2::Y);
            self.support_contact_count += 1;
        }
    }

    pub fn translate(&mut self, offset: Vec2) {
        for particle in &mut self.particles {
            particle.position += offset;
            particle.previous += offset;
        }
    }

    pub fn add_velocity(&mut self, velocity: Vec2) {
        for particle in &mut self.particles {
            particle.previous -= velocity;
        }
    }

    pub fn damp_velocity(&mut self, retention: f32) {
        let retention = retention.clamp(0.0, 1.0);
        for particle in &mut self.particles {
            let velocity = particle.position - particle.previous;
            particle.previous = particle.position - velocity * retention;
        }
    }

    pub fn apply_contact_patch(
        &mut self,
        contact_point: Vec2,
        contact_direction: Vec2,
        depth: f32,
        inelastic: bool,
    ) {
        let direction = contact_direction.normalize_or(Vec2::NEG_Y);
        let center = self.center();
        let tangent = direction.perp();
        let patch_width = (self.rest_radius * 0.72).max(1.0);
        let corrections = self
            .particles
            .iter()
            .map(|particle| {
                let offset = particle.position - center;
                let facing = (offset.dot(direction) / self.rest_radius).clamp(0.0, 1.0);
                let tangent_distance = (particle.position - contact_point).dot(tangent);
                let locality = (-tangent_distance.powi(2) / (2.0 * patch_width.powi(2))).exp();
                let weight = facing.powf(0.75) * locality;
                if weight <= 0.001 {
                    Vec2::ZERO
                } else {
                    -direction * depth * weight * 0.48
                }
            })
            .collect::<Vec<_>>();
        let average = corrections.iter().copied().sum::<Vec2>() / corrections.len() as f32;
        for (particle, correction) in self.particles.iter_mut().zip(corrections) {
            let correction = correction - average;
            particle.position += correction;
            if inelastic {
                particle.previous += correction;
            } else {
                // Contact shaping should mostly change the silhouette, not
                // inject a fresh Verlet impulse every fixed tick.
                particle.previous += correction * 0.84;
            }
        }
    }

    pub fn cancel_jump_charge(&mut self) {
        self.charge = 0.0;
        self.charge_direction = 0.0;
        self.was_charging = false;
        self.jump_armed = false;
    }

    pub fn center(&self) -> Vec2 {
        self.particles
            .iter()
            .map(|particle| particle.position)
            .sum::<Vec2>()
            / self.particles.len() as f32
    }

    pub fn size_scale(&self) -> f32 {
        self.rest_radius / REFERENCE_RADIUS
    }

    pub fn scale_rest_shape(&mut self, factor: f32) {
        self.rest_edge *= factor;
        self.rest_second_neighbor *= factor;
        for length in &mut self.edge_rest_lengths {
            *length *= factor;
        }
        for length in &mut self.curvature_rest_lengths {
            *length *= factor;
        }
        self.rest_area *= factor * factor;
        self.rest_radius *= factor;
    }

    pub fn cease_idle_animation(&mut self) {
        self.idle_amount = 0.0;
        self.cancel_jump_charge();
    }

    pub fn apply_internal_bulge(&mut self, load_position: Vec2, load_radius: f32, strength: f32) {
        let center = self.center();
        let direction = (load_position - center).normalize_or(Vec2::X);
        let target_extra = load_radius * 0.72 * strength.clamp(0.0, 1.0);
        let corrections = self
            .particles
            .iter()
            .map(|particle| {
                let offset = particle.position - center;
                let radial = offset.normalize_or(direction);
                let facing = ((radial.dot(direction) - 0.15) / 0.85).clamp(0.0, 1.0);
                let desired_radius = self.rest_radius + target_extra * facing.powi(2);
                radial * (desired_radius - offset.length()) * 0.12 * facing
            })
            .collect::<Vec<_>>();
        let average = corrections.iter().copied().sum::<Vec2>() / corrections.len() as f32;
        for (particle, correction) in self.particles.iter_mut().zip(corrections) {
            let correction = correction - average;
            particle.position += correction;
            particle.previous += correction;
        }
    }

    #[cfg(test)]
    pub fn step(&mut self, dt: f32, horizontal: f32, charging: bool, platforms: &[Platform]) {
        self.step_with_vigor(dt, horizontal, charging, platforms, &[], 1.0, true, true);
    }

    pub fn step_with_vigor(
        &mut self,
        dt: f32,
        horizontal: f32,
        charging: bool,
        platforms: &[Platform],
        fixtures: &[Vec<Vec2>],
        vigor: f32,
        animate_idle: bool,
        retain_tonicity: bool,
    ) {
        self.last_impact_speed = 0.0;
        self.launch_grace = (self.launch_grace - dt).max(0.0);
        if self.support_contact_count > 0 {
            self.support_normal = self.support_normal_sum.normalize_or(Vec2::Y);
        }
        let jump_normal = self.support_normal.normalize_or(Vec2::Y);
        let jump_tangent = jump_normal.perp();
        let right_tangent = if jump_tangent.x >= 0.0 {
            jump_tangent
        } else {
            -jump_tangent
        };
        self.support_normal_sum = Vec2::ZERO;
        self.support_contact_count = 0;
        let target_tonicity = if retain_tonicity { 1.0 } else { 0.0 };
        let tonicity_response = if retain_tonicity { 2.0 } else { 1.35 };
        self.tonicity +=
            (target_tonicity - self.tonicity) * (tonicity_response * dt).clamp(0.0, 1.0);
        // One unit is one localized breath followed by a resting pause.
        self.idle_phase += dt / 2.6;
        let wants_idle = animate_idle && self.grounded && horizontal == 0.0 && !charging;
        let idle_target = if wants_idle { 1.0 } else { 0.0 };
        let idle_response = if wants_idle { 1.8 } else { 7.0 };
        self.idle_amount += (idle_target - self.idle_amount) * (idle_response * dt).clamp(0.0, 1.0);
        if horizontal != 0.0 || charging {
            self.idle_amount = 0.0;
        }

        self.coyote = if self.grounded {
            0.16
        } else {
            (self.coyote - dt).max(0.0)
        };

        // Arm whenever Down is held while ground contact is available. This
        // also works if the key was pressed just before landing or while moving.
        if charging && !self.jump_armed && self.coyote > 0.0 {
            self.jump_armed = true;
        }
        if charging && self.jump_armed {
            self.charge = (self.charge + dt / CHARGE_DURATION).min(1.0);
            if horizontal.abs() > 0.01 {
                self.charge_direction = horizontal.signum();
            }
        }

        let jump_released = self.was_charging && !charging && self.charge > 0.0 && self.jump_armed;
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
        let compression_anchor = self
            .particles
            .iter()
            .map(|particle| particle.position.dot(jump_normal))
            .fold(f32::INFINITY, f32::min);
        let acceleration = if self.grounded {
            GROUND_ACCELERATION
        } else {
            AIR_ACCELERATION
        } * vigor;
        let maximum_speed = if self.grounded {
            MAX_GROUND_SPEED
        } else {
            MAX_AIR_SPEED
        } * vigor;
        let jump_size_factor = jump_size_factor(self.rest_radius);
        for particle in &mut self.particles {
            let mut velocity = particle.position - particle.previous;
            if !animate_idle {
                // Dissipate deformation without stopping the whole body's
                // fall or slide: dead tissue does not wobble or spring back.
                velocity = center_velocity + (velocity - center_velocity) * 0.40;
            }

            // Steer the centre of mass. In air every particle receives the same
            // correction so input cannot inject torque into the membrane.
            let target_velocity_x = horizontal * maximum_speed * dt;
            let steering = acceleration * dt * dt;
            velocity.x += (target_velocity_x - center_velocity.x).clamp(-steering, steering);

            if self.grounded {
                let offset = particle.position - center;
                let target_angular_displacement = -horizontal * GROUND_ROLL_RATE * dt;
                let angular_correction =
                    (target_angular_displacement - angular_displacement) * 0.34;
                velocity += offset.perp() * angular_correction;

                // Extra traction near the floor transfers the torque through
                // the membrane instead of pulling from the centre.
                let lower_weight =
                    ((center.y - particle.position.y) / self.rest_radius).clamp(0.0, 1.0);
                velocity.x += horizontal * steering * lower_weight * 0.65;
            } else {
                // Dampen only motion relative to the centre. Translation is
                // preserved while post-impact wobble gradually loses energy.
                let relative_velocity = velocity - center_velocity;
                velocity = center_velocity + relative_velocity * INTERNAL_DAMPING_AIR;
            }
            if horizontal == 0.0 {
                velocity.x *= if self.grounded { 0.72 } else { 0.992 };
            }
            velocity.x = velocity.x.clamp(
                -MAX_PARTICLE_HORIZONTAL_SPEED * dt,
                MAX_PARTICLE_HORIZONTAL_SPEED * dt,
            );
            velocity.y = velocity.y.clamp(
                -MAX_VERTICAL_SPEED * dt,
                MAX_VERTICAL_SPEED * jump_size_factor * dt,
            );

            particle.previous = particle.position;
            particle.position += velocity + Vec2::NEG_Y * GRAVITY * dt * dt;

            if charging && self.jump_armed {
                let compression = 3.8 * dt * (0.35 + self.charge * 0.65);
                // Compress around the lowest contact instead of around the
                // centre, otherwise the feet lift and the jump becomes invalid.
                let height_above_contact = particle.position.dot(jump_normal) - compression_anchor;
                particle.position -= jump_normal * height_above_contact * compression;
                let tangent_offset = (particle.position - center).dot(jump_tangent);
                particle.position += jump_tangent * tangent_offset * compression * 0.55;
            }
            if jump_released {
                // Stronger impulse on the lower membrane makes the launch
                // propagate through the body instead of translating it rigidly.
                let lower_weight = ((center - particle.position).dot(jump_normal) / self.rest_edge)
                    .clamp(0.0, 2.5)
                    / 2.5;
                let jump_speed = jump_speed_for_charge(self.charge, self.rest_radius) * vigor;
                let impulse = jump_speed * dt;
                // Small fragments jump faster, so their non-uniform impulse
                // must be reduced to avoid folding the membrane through its
                // own centre. The common impulse, and therefore jump height,
                // remains unchanged.
                let deformation = 0.28 / jump_size_factor;
                particle.previous -= jump_normal * impulse * (0.82 + lower_weight * deformation);
                particle.previous -= right_tangent * impulse * self.charge_direction * 0.42;
            }
        }
        if jump_released {
            // Clear the contact skin immediately. Without this separation the
            // launch can be repeatedly projected back onto the floor.
            let takeoff_clearance = 3.0 * self.size_scale();
            for particle in &mut self.particles {
                particle.position += jump_normal * takeoff_clearance;
            }
            self.remove_angular_velocity();
            self.charge = 0.0;
            self.coyote = 0.0;
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

        self.grounded = false;
        for _ in 0..SOLVER_ITERATIONS {
            self.solve_edges();
            self.solve_curvature();
            self.solve_area();
            self.limit_collapse();
            self.limit_stretch();
            self.solve_collisions(platforms);
            self.solve_fixture_collisions(fixtures);
            if self.repair_self_intersection() {
                // The recovered contour may overlap the surface that caused
                // the fold. Project it once more, then guarantee that the
                // frame ends with a valid membrane topology.
                self.solve_collisions(platforms);
                self.solve_fixture_collisions(fixtures);
                self.repair_self_intersection();
            }
        }
        self.solve_idle_shape();
        self.last_impact_speed /= dt.max(0.000_001);
    }

    /// Removes rolling velocity without changing centre-of-mass translation.
    /// The body can still deform in flight, but it no longer inherits a spin
    /// that was generated by ground traction.
    fn remove_angular_velocity(&mut self) {
        let center = self.center();
        let center_velocity = self
            .particles
            .iter()
            .map(|particle| particle.position - particle.previous)
            .sum::<Vec2>()
            / self.particles.len() as f32;
        let (angular_momentum, inertia) =
            self.particles
                .iter()
                .fold((0.0, 0.0), |(momentum, inertia), particle| {
                    let offset = particle.position - center;
                    let relative_velocity = particle.position - particle.previous - center_velocity;
                    (
                        momentum + offset.perp_dot(relative_velocity),
                        inertia + offset.length_squared(),
                    )
                });
        if inertia <= 0.001 {
            return;
        }
        let angular_displacement = angular_momentum / inertia;
        for particle in &mut self.particles {
            let offset = particle.position - center;
            let velocity = particle.position - particle.previous;
            let without_spin = velocity - offset.perp() * angular_displacement;
            particle.previous = particle.position - without_spin;
        }
    }

    fn solve_edges(&mut self) {
        let count = self.particles.len();
        let mut corrections = vec![Vec2::ZERO; count];
        for index in 0..count {
            let next = (index + 1) % count;
            let delta = self.particles[next].position - self.particles[index].position;
            let length = delta.length();
            if length > 0.0001 {
                let edge_tension = 0.20 + 0.28 * self.tonicity;
                let limpness = 1.0 - self.tonicity;
                let uneven_rest = self.edge_rest_lengths[index]
                    * (1.0 + corpse_material_variation(index, 0) * 0.12 * limpness);
                let uneven_tension =
                    edge_tension * (1.0 + corpse_material_variation(index, 1) * 0.16 * limpness);
                let correction = delta * ((length - uneven_rest) / length * uneven_tension);
                corrections[index] += correction;
                corrections[next] -= correction;
            }
        }
        for (particle, correction) in self.particles.iter_mut().zip(corrections) {
            particle.position += correction;
            if self.tonicity < 0.5 {
                particle.previous += correction;
            }
        }
    }

    fn solve_area(&mut self) {
        let area = polygon_area(&self.particles);
        if area.abs() < 0.001 {
            return;
        }
        let center = self.center();
        let current_area = area.abs();
        let scale = (self.rest_area / current_area).sqrt();
        let internal_pressure = 0.075 + 0.045 * self.tonicity;
        let correction_scale = 1.0 + (scale - 1.0) * internal_pressure;
        for particle in &mut self.particles {
            let corrected = center + (particle.position - center) * correction_scale;
            let correction = corrected - particle.position;
            particle.position = corrected;
            if self.tonicity < 0.5 {
                particle.previous += correction;
            }
        }
    }

    /// Inflates one upper lobe, returns fully to rest, pauses, then selects a
    /// different upper lobe. Nothing travels horizontally across the body.
    fn solve_idle_shape(&mut self) {
        if self.idle_amount < 0.001 {
            return;
        }

        let center = self.center();
        let count = self.particles.len();
        let local_time = self.idle_phase.fract();
        const ACTIVE_PART: f32 = 0.68;
        if local_time >= ACTIVE_PART {
            return;
        }

        // sin² begins and ends with zero slope, preventing mechanical steps.
        let breath_phase = local_time / ACTIVE_PART * std::f32::consts::PI;
        let pulse = breath_phase.sin().powi(2) * self.idle_amount;
        let lobe_index = self.idle_phase.floor() as usize % 4;
        let lobe_center = [0.38, 0.92, 2.22, 2.76][lobe_index];

        let mut corrections = vec![Vec2::ZERO; count];
        let mut active_particles = 0;
        for (index, particle) in self.particles.iter().enumerate() {
            let offset = particle.position - center;
            let distance = offset.length();
            if distance < 0.001 {
                continue;
            }

            let angle = index as f32 / count as f32 * std::f32::consts::TAU;
            let angular_distance = (angle - lobe_center)
                .abs()
                .min(std::f32::consts::TAU - (angle - lobe_center).abs());
            let locality = (-angular_distance.powi(2) / (2.0 * 0.48_f32.powi(2))).exp();
            let upper_mask = angle.sin().max(0.0).powf(0.7);
            let target_radius = self.rest_radius * (1.0 + 0.34 * pulse * locality * upper_mask);
            let radial_error = target_radius - distance;
            if upper_mask > 0.0 {
                corrections[index] = offset / distance * radial_error * 0.18 * pulse;
                active_particles += 1;
            }
        }

        // A local bulge must not translate the creature. Balance corrections
        // only across the free upper membrane, leaving ground contacts intact.
        if active_particles == 0 {
            return;
        }
        let average = corrections.iter().copied().sum::<Vec2>() / active_particles as f32;
        for (index, particle) in self.particles.iter_mut().enumerate() {
            if corrections[index] == Vec2::ZERO {
                continue;
            }
            let balanced = corrections[index] - average;
            particle.position += balanced;
            // Shape animation is not physical momentum in Verlet integration.
            particle.previous += balanced;
        }
    }

    /// Second-neighbour constraints resist sharp corners while still allowing
    /// the membrane to squash into a smooth ellipse.
    fn solve_curvature(&mut self) {
        let count = self.particles.len();
        let mut corrections = vec![Vec2::ZERO; count];
        for index in 0..count {
            let next = (index + 2) % count;
            let delta = self.particles[next].position - self.particles[index].position;
            let length = delta.length();
            if length > 0.0001 {
                let curvature = 0.05 + 0.11 * self.tonicity;
                let limpness = 1.0 - self.tonicity;
                let uneven_rest = self.curvature_rest_lengths[index]
                    * (1.0 + corpse_material_variation(index, 2) * 0.08 * limpness);
                let correction = delta * ((length - uneven_rest) / length * curvature);
                corrections[index] += correction;
                corrections[next] -= correction;
            }
        }
        for (particle, correction) in self.particles.iter_mut().zip(corrections) {
            particle.position += correction;
            if self.tonicity < 0.5 {
                particle.previous += correction;
            }
        }
    }

    /// Prevents a single collision from pulling the membrane into an
    /// uncontrollable needle. Area conservation alone cannot prevent this.
    fn limit_stretch(&mut self) {
        let maximum_radius = self.rest_radius * (MAX_STRETCH_RATIO + (1.0 - self.tonicity) * 0.22);
        // Recompute the centroid because clamping an extreme point moves it
        // slightly. A few cheap passes make the bound independent of topology.
        for _ in 0..4 {
            let center = self.center();
            for particle in &mut self.particles {
                let offset = particle.position - center;
                if offset.length_squared() > maximum_radius * maximum_radius {
                    particle.position = center + offset.clamp_length_max(maximum_radius);
                    let outward_velocity = particle.position - particle.previous;
                    particle.previous = particle.position - outward_velocity * 0.35;
                }
            }
        }
    }

    /// Keeps the ordered membrane outside a small core around its centroid.
    /// Without this bound, a very small blob can fold through itself during a
    /// powerful launch and briefly produce a figure-eight silhouette.
    fn limit_collapse(&mut self) {
        // A corpse may flatten substantially, but retaining a small protected
        // core prevents the membrane from folding through itself.
        let minimum_ratio = 0.28 + (MIN_COLLAPSE_RATIO - 0.28) * self.tonicity;
        let minimum_radius = self.rest_radius * minimum_ratio;
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
                let tangential_velocity = velocity - direction * velocity.dot(direction);
                particle.previous =
                    particle.position - tangential_velocity * 0.55 - direction * outward_speed;
            }
        }
    }

    /// Emergency recovery for a folded membrane. Normal constraints preserve
    /// the material order, but a sufficiently violent or oblique collision can
    /// make two non-adjacent edges cross. Rebuild a smooth ordered contour only
    /// in that pathological case, preserving centre-of-mass translation.
    fn repair_self_intersection(&mut self) -> bool {
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

    fn solve_collisions(&mut self, platforms: &[Platform]) {
        // Keep contact thickness constant while tonicity changes. A growing
        // collision envelope would move a resting corpse on its own.
        let skin = 5.0 * self.size_scale();
        let blob_center = self.center();
        let mut support_sum = Vec2::ZERO;
        let mut support_count = 0;
        for particle in &mut self.particles {
            for platform in platforms {
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

                // A fast, small blob can cross the middle of a thin platform
                // in one frame. Use the entry face from its swept path so all
                // membrane points are returned to the side they came from,
                // instead of splitting the contour across both faces.
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
                        support_sum += Vec2::Y;
                        support_count += 1;
                    }
                }
            }
        }
        self.support_normal_sum += support_sum;
        self.support_contact_count += support_count;
    }

    fn solve_fixture_collisions(&mut self, fixtures: &[Vec<Vec2>]) {
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

/// Stable signed variation tied to membrane material rather than simulation
/// time. Dead tissue therefore settles asymmetrically without visual jitter.
fn corpse_material_variation(index: usize, channel: u64) -> f32 {
    let mut value = (index as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(channel.wrapping_mul(0xbf58_476d_1ce4_e5b9));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value as u32) as f32 / u32::MAX as f32 * 2.0 - 1.0
}

fn collision_entry_side(particle: &Particle, min: Vec2, max: Vec2) -> usize {
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

fn collision_side_from_reference(
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

fn swept_aabb_entry(start: Vec2, end: Vec2, min: Vec2, max: Vec2) -> Option<(usize, f32)> {
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

fn has_self_intersections(particles: &[Particle]) -> bool {
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

fn jump_size_factor(radius: f32) -> f32 {
    (DEFAULT_GAMEPLAY_RADIUS / radius.max(1.0))
        .powf(0.8)
        .clamp(0.72, 1.75)
}

fn convex_penetration(point: Vec2, vertices: &[Vec2]) -> Option<(f32, Vec2)> {
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

fn jump_speed_for_charge(charge: f32, radius: f32) -> f32 {
    // Short taps remain small, the middle range stays broad and controllable,
    // and only a complete charge reaches maximum launch speed.
    let response = charge.clamp(0.0, 1.0).powf(1.2);
    (JUMP_MIN_SPEED + (JUMP_MAX_SPEED - JUMP_MIN_SPEED) * response) * jump_size_factor(radius)
}

fn damp_normal_velocity(particle: &mut Particle, normal: Vec2, restitution: f32) {
    let velocity = particle.position - particle.previous;
    let normal_speed = velocity.dot(normal);
    if normal_speed < 0.0 {
        let corrected_velocity = velocity - normal * normal_speed * (1.0 + restitution);
        particle.previous = particle.position - corrected_velocity;
    }
}

pub fn polygon_area(particles: &[Particle]) -> f32 {
    particles
        .iter()
        .zip(particles.iter().cycle().skip(1))
        .take(particles.len())
        .map(|(a, b)| a.position.perp_dot(b.position))
        .sum::<f32>()
        * 0.5
}

include!("blob_tests.rs");
