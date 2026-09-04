use bevy::prelude::*;

mod contacts;
mod liquid;
mod shape;

#[cfg(test)]
use contacts::{
    collision_entry_side, collision_side_from_reference, convex_penetration, swept_aabb_entry,
};

pub const PARTICLE_COUNT: usize = 24;
pub const REFERENCE_RADIUS: f32 = 58.0;
pub const DEFAULT_CREATURE_SCALE: f32 = 0.65;
/// Small fragments need a finite physical envelope around thin level walls.
/// This does not alter their drawn size or elastic rest shape.
pub const MIN_COLLISION_SKIN: f32 = 2.5;
const DEFAULT_GAMEPLAY_RADIUS: f32 = REFERENCE_RADIUS * DEFAULT_CREATURE_SCALE;
const MIN_SPLIT_SOURCE_PARTICLES: usize = 16;
const SOLVER_ITERATIONS: usize = 8;
const MAX_ADAPTIVE_CONTACT_PASSES: usize = 4;
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
const WATER_ROTATION_RESPONSE: f32 = 0.18;
const WATER_ROTATION_RATE: f32 = 0.0024;
const WATER_SPINE_DRAG_MULTIPLIER: f32 = 2.4;
const WATER_FLATTENING: f32 = 0.08;
// Without deployed spines the liquid is deliberately difficult to traverse:
// the blob can bob, but it cannot meaningfully swim through toxic wastewater.
const WATER_BARE_DRAG_RATE: f32 = 58.0;
const WATER_SPINED_DRAG_RATE: f32 = 14.0;
const WATER_SWIM_THRUST: f32 = 860.0;
const WATER_SWIM_ROTATION: f32 = 0.0045;
const WATER_ENTRY_FRACTION: f32 = 0.08;
const WATER_EXIT_FRACTION: f32 = 0.015;
const WATER_EXIT_GRACE: f32 = 0.24;

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub position: Vec2,
    pub previous: Vec2,
}

/// Summary of a blob's contact with one wastewater volume during a fixed step.
#[derive(Clone, Copy, Debug)]
pub struct WastewaterContact {
    pub surface_y: f32,
    pub submerged_fraction: f32,
    pub entered: bool,
    pub entry_speed: f32,
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
    pub charge: f32,
    pub last_impact_speed: f32,
    launch_grace: f32,
    support_normal: Vec2,
    support_normal_sum: Vec2,
    support_contact_count: usize,
    ground_traction: f32,
    ground_idle_damping: f32,
    ice_traction: f32,
    on_ice: bool,
    on_glue: bool,
    ground_is_glue: bool,
    charge_direction: f32,
    was_charging: bool,
    jump_armed: bool,
    idle_phase: f32,
    idle_amount: f32,
    tonicity: f32,
    water_submerged: bool,
    water_exit_elapsed: f32,
    spider_cling: Option<SpiderCling>,
}

#[derive(Clone, Copy, Debug)]
struct SpiderCling {
    wall_direction: f32,
    wall_top: f32,
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
            charge: 0.0,
            last_impact_speed: 0.0,
            launch_grace: 0.0,
            support_normal: Vec2::Y,
            support_normal_sum: Vec2::ZERO,
            support_contact_count: 0,
            ground_traction: 1.0,
            ground_idle_damping: 0.72,
            ice_traction: 0.0,
            on_ice: false,
            on_glue: false,
            ground_is_glue: false,
            charge_direction: 0.0,
            was_charging: false,
            jump_armed: false,
            idle_phase: 0.0,
            idle_amount: 0.0,
            tonicity: 1.0,
            water_submerged: false,
            water_exit_elapsed: 0.0,
            spider_cling: None,
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

    /// Splits mass and area according to the requested smaller child.
    /// Each child keeps the base membrane resolution so even the smallest
    /// fragments retain a smooth, stable collision contour. Position and
    /// separation impulse remain mass-weighted, preserving centre of mass and
    /// total momentum.
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
        let left_count = PARTICLE_COUNT;
        let right_count = PARTICLE_COUNT;
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

    /// Angular displacement of the membrane over the current physics step.
    /// Consumers that need an angular speed should divide this by their step
    /// duration. Keeping it on the body makes visual and audio feedback agree
    /// with the same rotation used by the liquid and climbing simulation.
    pub fn angular_displacement(&self) -> f32 {
        let center = self.center();
        let center_velocity = self.velocity();
        self.particles
            .iter()
            .map(|particle| {
                let offset = particle.position - center;
                let relative_velocity = particle.position - particle.previous - center_velocity;
                offset.perp_dot(relative_velocity) / offset.length_squared().max(1.0)
            })
            .sum::<f32>()
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

    /// Enables high tangential friction while at least one deployed spine is
    /// hooked into a vertical wall. `None` restores ordinary gravity at once.
    pub fn set_spider_cling(&mut self, wall: Option<(f32, f32)>) {
        self.spider_cling = wall.map(|(direction, wall_top)| SpiderCling {
            wall_direction: direction.signum(),
            wall_top,
        });
    }

    /// Stops membrane points at an out-of-play safety boundary without adding
    /// restitution. Playable barriers are still handled by authored colliders.
    pub fn contain_within_safety_bounds(&mut self, min: Vec2, max: Vec2) -> bool {
        let mut corrected = false;
        for particle in &mut self.particles {
            let clamped = particle.position.clamp(min, max);
            if clamped != particle.position {
                particle.position = clamped;
                particle.previous = clamped;
                corrected = true;
            }
        }
        corrected
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

    /// Avian can project one membrane point a long way out of a tight corner
    /// after the internal solver has already run. Rebalance immediately so a
    /// small blob never renders a one-frame spike before the next fixed step.
    pub fn stabilize_after_external_projection(&mut self) {
        for _ in 0..2 {
            self.solve_edges();
            self.solve_curvature();
            self.limit_collapse();
            self.limit_stretch();
        }
        self.repair_self_intersection();
    }

    pub fn cancel_jump_charge(&mut self) {
        self.charge = 0.0;
        self.charge_direction = 0.0;
        self.was_charging = false;
        self.jump_armed = false;
    }

    /// Applies a deliberately subtle, uncharged lift for the dance preview.
    /// Unlike the player jump it never compresses the membrane, never fills
    /// `charge`, and is only permitted while a real support contact exists.
    pub fn tiny_ground_hop(&mut self, dt: f32) -> bool {
        if !self.grounded || self.charge > 0.01 || self.water_submerged {
            return false;
        }
        // Noticeable enough to read as a playful hop, still far below the
        // minimum charged jump used by player movement.
        let lift = self.support_normal.normalize_or(Vec2::Y) * (128.0 * dt);
        self.add_velocity(lift);
        self.grounded = false;
        self.launch_grace = 0.04;
        true
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

    #[cfg(test)]
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
        self.step_with_vigor_on_ice(
            dt,
            horizontal,
            charging,
            platforms,
            &[],
            &[],
            fixtures,
            vigor,
            animate_idle,
            retain_tonicity,
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
        horizontal: f32,
        charging: bool,
        platforms: &[Platform],
        ice_platform_indices: &[usize],
        glue_platform_indices: &[usize],
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
        let previous_ground_traction = self.ground_traction;
        let previous_ground_idle_damping = self.ground_idle_damping;
        let previous_ground_is_glue = self.ground_is_glue;
        self.ground_traction = 1.0;
        self.ground_idle_damping = 0.72;
        self.on_ice = false;
        self.on_glue = false;
        self.ground_is_glue = false;
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

        // A charge begins only from a real support contact. Keeping a short
        // coyote window here made it possible to start charging in mid-air.
        if charging && !self.jump_armed && self.grounded {
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
        let spider_cling = self.spider_cling;
        // Breathing changes only the membrane shape. It must not introduce a
        // net torque, otherwise irregular split fragments slowly crawl.
        let idle_rock_rate = 0.0;
        let compression_anchor = self
            .particles
            .iter()
            .map(|particle| particle.position.dot(jump_normal))
            .fold(f32::INFINITY, f32::min);
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
        let jump_size_factor = jump_size_factor(self.rest_radius);
        // Glue also dissipates the stored compression before it becomes a
        // launch. This is read from the prior support frame, so a sustained
        // charge on the adhesive surface is consistently weaker.
        let jump_surface_factor = if previous_ground_is_glue { 0.42 } else { 1.0 };
        for particle in &mut self.particles {
            let mut velocity = particle.position - particle.previous;
            if !animate_idle {
                // Dissipate deformation without stopping the whole body's
                // fall or slide: dead tissue does not wobble or spring back.
                velocity = center_velocity + (velocity - center_velocity) * 0.40;
            }

            // A hooked wall is a rotated floor: gravity presses into its
            // normal, while input keeps its direct player-selected direction.
            let steering = acceleration * dt * dt;
            if spider_cling.is_some() {
                // Either horizontal input works the hooked spines upward.
                // The signed input remains reserved for roll direction below.
                let climb_intent = horizontal.abs();
                let target_velocity_y = climb_intent * maximum_speed * dt;
                velocity.y += (target_velocity_y - center_velocity.y).clamp(-steering, steering);
                // At the corner, turn the rolling direction out onto the top
                // surface. This is a short edge grip, not a jump impulse.
                let target_velocity_x = horizontal * maximum_speed * dt * rim_progress;
                velocity.x += (target_velocity_x - center_velocity.x).clamp(-steering, steering)
                    * rim_progress;
            } else {
                if previous_ground_is_glue {
                    // No direct slide on adhesive sludge. The membrane may
                    // advance only through the slow angular contact below.
                    velocity.x *= 0.18;
                } else {
                    let target_velocity_x = horizontal * maximum_speed * dt;
                    velocity.x +=
                        (target_velocity_x - center_velocity.x).clamp(-steering, steering);
                }
            }

            if self.grounded {
                let offset = particle.position - center;
                let target_angular_displacement =
                    // Ice does not cancel the body's attempt to roll. Without
                    // spines this becomes visible spin and slip, while the
                    // separate traction terms below still prevent propulsion.
                    (-horizontal
                        * GROUND_ROLL_RATE
                        * if previous_ground_is_glue {
                            (previous_ground_traction * 2.0).min(0.18)
                        } else {
                            1.0
                        }
                        + idle_rock_rate)
                        * dt;
                let angular_correction =
                    (target_angular_displacement - angular_displacement) * 0.34;
                velocity += offset.perp() * angular_correction;

                if previous_ground_is_glue {
                    // Adhesion couples the slow roll to a matching forward
                    // advance: x = r * theta. The retained factor accounts
                    // for the soft membrane flattening at the contact patch.
                    // It is not inertia; releasing input is still stopped by
                    // the glue damping on the next fixed step.
                    velocity.x += -target_angular_displacement * self.rest_radius * 0.72;
                }

                // Extra traction near the floor transfers the torque through
                // the membrane instead of pulling from the centre.
                let lower_weight =
                    ((center.y - particle.position.y) / self.rest_radius).clamp(0.0, 1.0);
                if !previous_ground_is_glue {
                    velocity.x +=
                        horizontal * steering * lower_weight * 0.65 * previous_ground_traction;
                }
            } else if spider_cling.is_some() {
                let offset = particle.position - center;
                let tangential_input = horizontal;
                // The input owns the rotation direction. The wall only
                // provides adhesion; it must not reverse A/D on either side.
                let target_angular_displacement = -tangential_input * GROUND_ROLL_RATE * dt;
                let angular_correction =
                    (target_angular_displacement - angular_displacement) * 0.34;
                velocity += offset.perp() * angular_correction;
            } else {
                // Dampen only motion relative to the centre. Translation is
                // preserved while post-impact wobble gradually loses energy.
                let relative_velocity = velocity - center_velocity;
                velocity = center_velocity + relative_velocity * INTERNAL_DAMPING_AIR;
            }
            if horizontal == 0.0 {
                velocity.x *= if self.grounded {
                    previous_ground_idle_damping
                } else {
                    0.992
                };
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
            particle.position += velocity + gravity_direction * GRAVITY * dt * dt;

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
                let jump_speed = jump_speed_for_charge(self.charge, self.rest_radius)
                    * vigor
                    * jump_surface_factor;
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
            self.solve_collisions(platforms, ice_platform_indices, glue_platform_indices);
            self.solve_fixture_collisions(fixtures);
            if self.repair_self_intersection() {
                // The recovered contour may overlap the surface that caused
                // the fold. Project it once more, then guarantee that the
                // frame ends with a valid membrane topology.
                self.solve_collisions(platforms, ice_platform_indices, glue_platform_indices);
                self.solve_fixture_collisions(fixtures);
                self.repair_self_intersection();
            }
        }
        if animate_idle {
            self.solve_idle_shape();
        }
        // Shape recovery and self-intersection repair can move a point after
        // an iteration's collision pass. End every frame with environment
        // projection so no membrane vertex remains embedded in level geometry.
        self.solve_collisions(platforms, ice_platform_indices, glue_platform_indices);
        self.solve_fixture_collisions(fixtures);
        if self.repair_self_intersection() {
            self.solve_collisions(platforms, ice_platform_indices, glue_platform_indices);
            self.solve_fixture_collisions(fixtures);
        }
        if let Some(anchor_x) = idle_anchor_x {
            // Contact projection after a local breath can otherwise retain a
            // tiny lateral correction. A resting creature may deform, but it
            // must not slowly walk across the platform by itself.
            self.translate(Vec2::X * (anchor_x - self.center().x));
        }
        // Small fragments cover a larger fraction of their radius in one
        // fixed step. Give their contact constraints extra passes without
        // re-running input or jump impulses.
        let maximum_travel = self
            .particles
            .iter()
            .map(|particle| (particle.position - particle.previous).length())
            .fold(0.0_f32, f32::max);
        let contact_step = (self.rest_radius * 0.22).max(3.0);
        let adaptive_passes = (maximum_travel / contact_step)
            .ceil()
            .clamp(1.0, MAX_ADAPTIVE_CONTACT_PASSES as f32) as usize;
        for _ in 1..adaptive_passes {
            self.solve_edges();
            self.solve_curvature();
            self.solve_area();
            self.limit_collapse();
            self.limit_stretch();
            self.solve_collisions(platforms, ice_platform_indices, glue_platform_indices);
            self.solve_fixture_collisions(fixtures);
            self.repair_self_intersection();
        }
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
}

#[cfg(test)]
fn idle_lobe_center(cycle: usize) -> f32 {
    // Alternate membrane sides on every breath. The two angles used on each
    // side differ, so the result remains organic instead of becoming a
    // regular left-right pendulum.
    [0.38, 2.22, 0.92, 2.76][cycle % 4]
}

#[cfg(test)]
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
        // Small fragments retain a modest, readable advantage without
        // turning the smallest pieces into dramatically stronger jumpers.
        .clamp(0.72, 1.90)
}

fn jump_speed_for_charge(charge: f32, radius: f32) -> f32 {
    // Short taps remain small, the middle range stays broad and controllable,
    // and only a complete charge reaches maximum launch speed.
    let response = charge.clamp(0.0, 1.0).powf(1.2);
    (JUMP_MIN_SPEED + (JUMP_MAX_SPEED - JUMP_MIN_SPEED) * response) * jump_size_factor(radius)
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
