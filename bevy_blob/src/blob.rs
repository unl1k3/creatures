use bevy::prelude::*;

pub const PARTICLE_COUNT: usize = 24;
pub const REFERENCE_RADIUS: f32 = 58.0;
const SPLIT_RESOLUTION_MULTIPLIER: usize = 2;
const SOLVER_ITERATIONS: usize = 8;
const GRAVITY: f32 = 1_150.0;
const GROUND_ACCELERATION: f32 = 1_050.0;
const AIR_ACCELERATION: f32 = 310.0;
const MAX_GROUND_SPEED: f32 = 410.0;
const MAX_AIR_SPEED: f32 = 285.0;
const MAX_VERTICAL_SPEED: f32 = 1_450.0;
const MAX_STRETCH_RATIO: f32 = 1.58;
const CHARGE_DURATION: f32 = 0.70;
const JUMP_MIN_SPEED: f32 = 340.0;
const JUMP_MAX_SPEED: f32 = 1_280.0;
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
    pub rest_area: f32,
    pub rest_radius: f32,
    pub grounded: bool,
    pub coyote: f32,
    pub charge: f32,
    was_charging: bool,
    jump_armed: bool,
    idle_phase: f32,
    idle_amount: f32,
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
            rest_area,
            rest_radius: radius,
            grounded: false,
            coyote: 0.0,
            charge: 0.0,
            was_charging: false,
            jump_armed: false,
            idle_phase: 0.0,
            idle_amount: 0.0,
        }
    }

    /// Creates the symmetric pair used by deterministic physics tests.
    #[cfg(test)]
    pub fn split_pair(&self, dt: f32) -> [Self; 2] {
        self.split_pair_uneven(dt, self.particles.len() / 2, true)
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

    pub fn step(&mut self, dt: f32, horizontal: f32, charging: bool, platforms: &[Platform]) {
        // One unit is one localized breath followed by a resting pause.
        self.idle_phase += dt / 2.6;
        let wants_idle = self.grounded && horizontal == 0.0 && !charging;
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
        let compression_anchor_y = self
            .particles
            .iter()
            .map(|particle| particle.position.y)
            .fold(f32::INFINITY, f32::min);
        let acceleration = if self.grounded {
            GROUND_ACCELERATION
        } else {
            AIR_ACCELERATION
        };
        let maximum_speed = if self.grounded {
            MAX_GROUND_SPEED
        } else {
            MAX_AIR_SPEED
        };
        for particle in &mut self.particles {
            let mut velocity = particle.position - particle.previous;

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
            velocity.y = velocity
                .y
                .clamp(-MAX_VERTICAL_SPEED * dt, MAX_VERTICAL_SPEED * dt);

            particle.previous = particle.position;
            particle.position += velocity + Vec2::NEG_Y * GRAVITY * dt * dt;

            if charging && self.jump_armed {
                let compression = 3.8 * dt * (0.35 + self.charge * 0.65);
                // Compress around the lowest contact instead of around the
                // centre, otherwise the feet lift and the jump becomes invalid.
                let height_above_contact = particle.position.y - compression_anchor_y;
                particle.position.y -= height_above_contact * compression;
                particle.position.x += (particle.position.x - center.x) * compression * 0.55;
            }
            if jump_released {
                // Stronger impulse on the lower membrane makes the launch
                // propagate through the body instead of translating it rigidly.
                let lower_weight =
                    ((center.y - particle.position.y) / self.rest_edge).clamp(0.0, 2.5) / 2.5;
                let jump_speed = jump_speed_for_charge(self.charge);
                let impulse = jump_speed * dt;
                particle.previous.y -= impulse * (0.82 + lower_weight * 0.28);
            }
        }
        if jump_released {
            // Clear the contact skin immediately. Without this separation the
            // launch can be repeatedly projected back onto the floor.
            let takeoff_clearance = 3.0 * self.size_scale();
            for particle in &mut self.particles {
                particle.position.y += takeoff_clearance;
            }
            self.remove_angular_velocity();
            self.charge = 0.0;
            self.coyote = 0.0;
            self.jump_armed = false;
            self.grounded = false;
        } else if self.was_charging && !charging {
            self.charge = 0.0;
            self.jump_armed = false;
        }
        self.was_charging = charging;

        self.grounded = false;
        for _ in 0..SOLVER_ITERATIONS {
            self.solve_edges();
            self.solve_curvature();
            self.solve_area();
            self.limit_stretch();
            self.solve_collisions(platforms);
        }
        self.solve_idle_shape();
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
                let correction = delta * ((length - self.rest_edge) / length * 0.48);
                corrections[index] += correction;
                corrections[next] -= correction;
            }
        }
        for (particle, correction) in self.particles.iter_mut().zip(corrections) {
            particle.position += correction;
        }
    }

    fn solve_area(&mut self) {
        let area = polygon_area(&self.particles);
        if area.abs() < 0.001 {
            return;
        }
        let center = self.center();
        let scale = (self.rest_area / area.abs()).sqrt();
        let correction_scale = 1.0 + (scale - 1.0) * 0.12;
        for particle in &mut self.particles {
            particle.position = center + (particle.position - center) * correction_scale;
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
                let correction = delta * ((length - self.rest_second_neighbor) / length * 0.16);
                corrections[index] += correction;
                corrections[next] -= correction;
            }
        }
        for (particle, correction) in self.particles.iter_mut().zip(corrections) {
            particle.position += correction;
        }
    }

    /// Prevents a single collision from pulling the membrane into an
    /// uncontrollable needle. Area conservation alone cannot prevent this.
    fn limit_stretch(&mut self) {
        let maximum_radius = self.rest_radius * MAX_STRETCH_RATIO;
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

    fn solve_collisions(&mut self, platforms: &[Platform]) {
        let skin = 4.0 * self.size_scale();
        for particle in &mut self.particles {
            for platform in platforms {
                let min = platform.center - platform.half_size;
                let max = platform.center + platform.half_size;
                if particle.position.x < min.x - skin
                    || particle.position.x > max.x + skin
                    || particle.position.y < min.y - skin
                    || particle.position.y > max.y + skin
                {
                    continue;
                }

                let distances = [
                    (particle.position.x - min.x).abs(),
                    (max.x - particle.position.x).abs(),
                    (particle.position.y - min.y).abs(),
                    (max.y - particle.position.y).abs(),
                ];
                let side = distances
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(index, _)| index)
                    .unwrap_or(3);
                match side {
                    0 => {
                        particle.position.x = min.x - skin;
                        damp_normal_velocity(particle, Vec2::NEG_X, 0.12);
                    }
                    1 => {
                        particle.position.x = max.x + skin;
                        damp_normal_velocity(particle, Vec2::X, 0.12);
                    }
                    2 => {
                        particle.position.y = min.y - skin;
                        damp_normal_velocity(particle, Vec2::NEG_Y, 0.08);
                    }
                    _ => {
                        particle.position.y = max.y + skin;
                        damp_normal_velocity(particle, Vec2::Y, 0.0);
                        self.grounded = true;
                    }
                }
            }
        }
    }
}

fn jump_speed_for_charge(charge: f32) -> f32 {
    // Short taps remain small, the middle range stays broad and controllable,
    // and only a complete charge reaches maximum launch speed.
    let response = charge.clamp(0.0, 1.0).powf(1.2);
    JUMP_MIN_SPEED + (JUMP_MAX_SPEED - JUMP_MIN_SPEED) * response
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_blob_has_expected_area() {
        let blob = Blob::new(Vec2::ZERO, 50.0);
        let expected = std::f32::consts::PI * 50.0 * 50.0;
        assert!((blob.rest_area - expected).abs() / expected < 0.02);
    }

    #[test]
    fn edge_constraint_recovers_a_stretched_edge() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        blob.particles[1].position += Vec2::X * 20.0;
        let before = blob.particles[0]
            .position
            .distance(blob.particles[1].position);
        blob.solve_edges();
        let after = blob.particles[0]
            .position
            .distance(blob.particles[1].position);
        assert!((after - blob.rest_edge).abs() < (before - blob.rest_edge).abs());
    }

    #[test]
    fn symmetric_solver_does_not_create_horizontal_drift() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        let before = blob.center().x;
        for _ in 0..20 {
            blob.solve_edges();
            blob.solve_area();
        }
        assert!((blob.center().x - before).abs() < 0.0001);
    }

    #[test]
    fn stretch_limit_bounds_every_particle() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        blob.particles[0].position = Vec2::new(200.0, 0.0);
        blob.limit_stretch();
        let center = blob.center();
        let furthest = blob
            .particles
            .iter()
            .map(|particle| particle.position.distance(center))
            .fold(0.0, f32::max);
        assert!(furthest <= blob.rest_radius * MAX_STRETCH_RATIO + 0.001);
    }

    #[test]
    fn charged_jump_remains_armed_after_coyote_time() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        blob.grounded = true;
        let dt = 1.0 / 120.0;
        for _ in 0..45 {
            blob.step(dt, 0.0, true, &[]);
        }
        assert!(blob.jump_armed);
        assert!(blob.charge > 0.5);

        blob.step(dt, 0.0, false, &[]);
        assert!(!blob.jump_armed);
        assert_eq!(blob.charge, 0.0);
    }

    #[test]
    fn full_charge_clears_first_platform_height() {
        let floor = Platform {
            center: Vec2::new(0.0, -70.0),
            half_size: Vec2::new(300.0, 10.0),
        };
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        let dt = 1.0 / 120.0;

        // Let the body settle before charging.
        for _ in 0..90 {
            blob.step(dt, 0.0, false, &[floor]);
        }
        for _ in 0..90 {
            blob.step(dt, 0.0, true, &[floor]);
        }
        let launch_height = blob.center().y;
        blob.step(dt, 0.0, false, &[floor]);

        let mut apex = blob.center().y;
        for _ in 0..180 {
            blob.step(dt, 0.0, false, &[floor]);
            apex = apex.max(blob.center().y);
        }
        assert!(
            apex - launch_height > 215.0,
            "full charge only rose {} pixels",
            apex - launch_height
        );
    }

    #[test]
    fn blob_can_move_and_charge_at_the_same_time() {
        let floor = Platform {
            center: Vec2::new(0.0, -70.0),
            half_size: Vec2::new(500.0, 10.0),
        };
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        let dt = 1.0 / 120.0;
        for _ in 0..90 {
            blob.step(dt, 0.0, false, &[floor]);
        }

        let start_x = blob.center().x;
        for _ in 0..35 {
            blob.step(dt, 1.0, true, &[floor]);
        }

        assert!(blob.jump_armed);
        assert!(blob.charge > 0.4);
        assert!(blob.center().x - start_x > 20.0);
    }

    #[test]
    fn grounded_movement_rotates_the_membrane() {
        let floor = Platform {
            center: Vec2::new(0.0, -70.0),
            half_size: Vec2::new(500.0, 10.0),
        };
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        let dt = 1.0 / 120.0;
        for _ in 0..90 {
            blob.step(dt, 0.0, false, &[floor]);
        }
        for _ in 0..30 {
            blob.step(dt, 1.0, false, &[floor]);
        }

        let center = blob.center();
        let rotation = blob
            .particles
            .iter()
            .map(|particle| {
                let offset = particle.position - center;
                let velocity = particle.position - particle.previous;
                offset.perp_dot(velocity) / offset.length_squared().max(1.0)
            })
            .sum::<f32>()
            / blob.particles.len() as f32;
        assert!(
            rotation < -0.001,
            "expected clockwise rolling, got {rotation}"
        );
    }

    #[test]
    fn jump_charge_has_distinct_low_mid_and_full_power() {
        let low = jump_speed_for_charge(0.1);
        let middle = jump_speed_for_charge(0.5);
        let full = jump_speed_for_charge(1.0);

        assert!(low < 450.0, "low charge is too strong: {low}");
        assert!(
            middle > 700.0 && middle < 850.0,
            "middle charge is {middle}"
        );
        assert_eq!(full, JUMP_MAX_SPEED);
        assert!(middle - low > 250.0);
        assert!(full - middle > 400.0);
    }

    #[test]
    fn organic_idle_fades_in_and_out() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        let dt = 1.0 / 120.0;
        blob.grounded = true;
        for _ in 0..120 {
            // Preserve the grounded flag as a collision would in the game.
            blob.grounded = true;
            blob.step(dt, 0.0, false, &[]);
        }
        assert!(blob.idle_amount > 0.75);

        for _ in 0..60 {
            blob.step(dt, 1.0, false, &[]);
        }
        assert!(blob.idle_amount < 0.1);
    }

    #[test]
    fn organic_idle_creates_a_visible_irregular_silhouette() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        blob.idle_amount = 1.0;
        blob.idle_phase = 1.2;
        for _ in 0..20 {
            blob.solve_idle_shape();
        }
        let center = blob.center();
        let radii = blob
            .particles
            .iter()
            .map(|particle| particle.position.distance(center))
            .collect::<Vec<_>>();
        let minimum = radii.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum = radii.iter().copied().fold(0.0, f32::max);
        assert!(maximum - minimum > 7.0);
    }

    #[test]
    fn localized_breathing_preserves_center_and_velocity() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        blob.idle_amount = 1.0;
        blob.idle_phase = 0.25;
        let center_before = blob.center();
        blob.solve_idle_shape();
        let center_after = blob.center();
        let injected_velocity = blob
            .particles
            .iter()
            .map(|particle| particle.position - particle.previous)
            .sum::<Vec2>()
            / blob.particles.len() as f32;

        assert!(center_after.distance(center_before) < 0.0001);
        assert!(injected_velocity.length() < 0.0001);
    }

    #[test]
    fn takeoff_removes_spin_but_preserves_translation() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        let center = blob.center();
        let translation = Vec2::new(2.0, 5.0);
        for particle in &mut blob.particles {
            let offset = particle.position - center;
            let velocity = translation + offset.perp() * 0.08;
            particle.previous = particle.position - velocity;
        }

        blob.remove_angular_velocity();
        let average_velocity = blob
            .particles
            .iter()
            .map(|particle| particle.position - particle.previous)
            .sum::<Vec2>()
            / blob.particles.len() as f32;
        let residual_spin = blob
            .particles
            .iter()
            .map(|particle| {
                let offset = particle.position - center;
                let relative_velocity = particle.position - particle.previous - average_velocity;
                offset.perp_dot(relative_velocity)
            })
            .sum::<f32>();

        assert!(average_velocity.distance(translation) < 0.0001);
        assert!(residual_spin.abs() < 0.001);
    }

    #[test]
    fn split_creates_two_high_resolution_half_area_children() {
        let mut parent = Blob::new(Vec2::new(12.0, 34.0), 50.0);
        let inherited_velocity = Vec2::new(1.5, -0.75);
        for particle in &mut parent.particles {
            particle.previous = particle.position - inherited_velocity;
        }

        let [left, right] = parent.split_pair(1.0 / 120.0);
        assert_eq!(left.particles.len(), parent.particles.len());
        assert_eq!(right.particles.len(), parent.particles.len());
        let midpoint = (left.center() + right.center()) * 0.5;
        assert!(midpoint.distance(parent.center()) < 0.0001);
        assert!(left.center().distance(right.center()) > left.rest_radius + right.rest_radius);
        let relative_area_error =
            ((left.rest_area + right.rest_area) - parent.rest_area).abs() / parent.rest_area;
        assert!(relative_area_error < 0.0001);

        let children_momentum = left.velocity() * left.mass() + right.velocity() * right.mass();
        let parent_momentum = parent.velocity() * parent.mass();
        assert!(children_momentum.distance(parent_momentum) / parent.mass() < 0.00001);
    }

    #[test]
    fn merge_restores_particle_count_area_and_momentum() {
        let parent = Blob::new(Vec2::ZERO, 50.0);
        let [mut left, mut right] = parent.split_pair(1.0 / 120.0);
        left.add_velocity(Vec2::new(0.8, 0.3));
        right.add_velocity(Vec2::new(-0.2, 0.5));
        let expected_momentum = left.velocity() * left.mass() + right.velocity() * right.mass();

        let merged = Blob::merge_pair(&left, &right);
        assert_eq!(merged.particles.len(), parent.particles.len());
        assert!((merged.rest_area - parent.rest_area).abs() / parent.rest_area < 0.0001);
        let merged_momentum = merged.velocity() * merged.mass();
        assert!(merged_momentum.distance(expected_momentum) / merged.mass() < 0.00001);
    }

    #[test]
    fn uneven_split_creates_different_sizes_and_preserves_mass() {
        let parent = Blob::new(Vec2::new(4.0, 7.0), 50.0);
        let [small, large] = parent.split_pair_uneven(1.0 / 120.0, 9, true);

        assert_eq!(small.particles.len(), 18);
        assert_eq!(large.particles.len(), 30);
        assert!(small.rest_radius < large.rest_radius);
        assert!(small.rest_area < large.rest_area);
        assert!((small.rest_area + large.rest_area - parent.rest_area).abs() < 0.01);
        let combined_center = (small.center() * small.mass() + large.center() * large.mass())
            / (small.mass() + large.mass());
        assert!(combined_center.distance(parent.center()) < 0.0001);
    }
}
