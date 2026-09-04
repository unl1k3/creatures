use bevy::prelude::*;

mod contacts;
mod jump;
mod liquid;
mod locomotion;
mod movement;
mod shape;
mod solver;
mod topology;

#[cfg(test)]
use jump::jump_speed_for_charge;
pub(crate) use movement::{BlobStepEnvironment, BlobStepInput, BlobStepProfile};

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
