//! Iterative soft-body constraints and final environment projection.

use super::*;

impl Blob {
    pub(super) fn solve_movement_constraints(
        &mut self,
        environment: BlobStepEnvironment<'_>,
        animate_idle: bool,
        idle_anchor_x: Option<f32>,
        dt: f32,
    ) {
        self.grounded = false;
        for _ in 0..SOLVER_ITERATIONS {
            self.solve_constraint_pass(environment, true);
        }
        if animate_idle {
            self.solve_idle_shape();
        }

        // Shape recovery can move a point after collision projection. End the
        // frame with another environment pass so the membrane remains outside.
        self.solve_collisions(
            environment.platforms,
            environment.ice_platform_indices,
            environment.glue_platform_indices,
        );
        self.solve_fixture_collisions(environment.fixtures);
        if self.repair_self_intersection() {
            self.solve_collisions(
                environment.platforms,
                environment.ice_platform_indices,
                environment.glue_platform_indices,
            );
            self.solve_fixture_collisions(environment.fixtures);
        }
        if let Some(anchor_x) = idle_anchor_x {
            // A local breath may retain a minute lateral contact correction;
            // resting deformation must never become autonomous locomotion.
            self.translate(Vec2::X * (anchor_x - self.center().x));
        }

        // Small fragments cover more of their radius per step and receive
        // extra constraint passes without repeating input or jump impulses.
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
            self.solve_constraint_pass(environment, false);
        }
        self.last_impact_speed /= dt.max(0.000_001);
    }

    fn solve_constraint_pass(
        &mut self,
        environment: BlobStepEnvironment<'_>,
        repeat_after_repair: bool,
    ) {
        self.solve_edges();
        self.solve_curvature();
        self.solve_area();
        self.limit_collapse();
        self.limit_stretch();
        self.solve_collisions(
            environment.platforms,
            environment.ice_platform_indices,
            environment.glue_platform_indices,
        );
        self.solve_fixture_collisions(environment.fixtures);
        if self.repair_self_intersection() && repeat_after_repair {
            self.solve_collisions(
                environment.platforms,
                environment.ice_platform_indices,
                environment.glue_platform_indices,
            );
            self.solve_fixture_collisions(environment.fixtures);
            self.repair_self_intersection();
        }
    }

    /// Removes rolling velocity without changing centre-of-mass translation.
    pub(super) fn remove_angular_velocity(&mut self) {
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
