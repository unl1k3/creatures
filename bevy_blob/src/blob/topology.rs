//! Blob construction, splitting, merging, and rest-shape scaling.

use super::*;

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
}
