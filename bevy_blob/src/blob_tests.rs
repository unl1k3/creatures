#[cfg(test)]
mod tests {
    use super::*;

    fn polygon_self_intersects(particles: &[Particle]) -> bool {
        has_self_intersections(particles)
    }

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
    fn collapse_limit_keeps_particles_outside_the_inner_core() {
        let mut blob = Blob::new_with_count(Vec2::ZERO, 15.0, 14);
        blob.particles[0].position = Vec2::new(0.1, 0.0);
        blob.particles[0].previous = Vec2::new(2.0, 0.0);
        blob.limit_collapse();

        let center = blob.center();
        let nearest = blob
            .particles
            .iter()
            .map(|particle| particle.position.distance(center))
            .fold(f32::INFINITY, f32::min);
        assert!(nearest >= blob.rest_radius * MIN_COLLAPSE_RATIO - 0.01);
    }

    #[test]
    fn crossed_membrane_is_repaired_without_losing_translation() {
        let mut blob = Blob::new_with_count(Vec2::new(20.0, 30.0), 15.0, 14);
        let translation = Vec2::new(2.0, 3.0);
        let opposite = blob.particles.len() / 2;
        let first_position = blob.particles[0].position;
        blob.particles[0].position = blob.particles[opposite].position;
        blob.particles[opposite].position = first_position;
        for particle in &mut blob.particles {
            particle.previous = particle.position - translation;
        }
        let center_before = blob.center();

        assert!(has_self_intersections(&blob.particles));
        assert!(blob.repair_self_intersection());
        assert!(!has_self_intersections(&blob.particles));
        assert!(blob.center().distance(center_before) < 0.0001);
        assert!(blob.velocity().distance(translation) < 0.0001);
    }

    #[test]
    fn tiny_blob_does_not_fold_during_a_full_jump() {
        let radius = 15.0;
        let floor = Platform {
            center: Vec2::new(0.0, -radius - 10.0),
            half_size: Vec2::new(300.0, 10.0),
        };
        let mut blob = Blob::new_with_count(Vec2::ZERO, radius, 14);
        let dt = 1.0 / 120.0;

        for _ in 0..60 {
            blob.step(dt, 0.0, false, &[floor]);
        }
        for _ in 0..90 {
            blob.step(dt, 0.0, true, &[floor]);
        }
        blob.step(dt, 0.0, false, &[floor]);
        for _ in 0..180 {
            blob.step(dt, 0.0, false, &[floor]);
            assert!(!polygon_self_intersects(&blob.particles));
        }
    }

    #[test]
    fn fast_particle_is_returned_to_the_face_it_entered() {
        let particle = Particle {
            previous: Vec2::new(0.0, -20.0),
            position: Vec2::new(0.0, 8.0),
        };
        assert_eq!(
            collision_entry_side(&particle, Vec2::new(-50.0, -10.0), Vec2::new(50.0, 10.0)),
            2
        );
    }

    #[test]
    fn tiny_blob_does_not_wrap_around_a_ceiling() {
        let radius = 15.0;
        let floor = Platform {
            center: Vec2::new(0.0, -radius - 10.0),
            half_size: Vec2::new(300.0, 10.0),
        };
        let ceiling = Platform {
            center: Vec2::new(0.0, 105.0),
            half_size: Vec2::new(300.0, 14.0),
        };
        let mut blob = Blob::new_with_count(Vec2::ZERO, radius, 14);
        let dt = 1.0 / 120.0;

        for _ in 0..60 {
            blob.step(dt, 0.0, false, &[floor, ceiling]);
        }
        for _ in 0..90 {
            blob.step(dt, 0.0, true, &[floor, ceiling]);
        }
        blob.step(dt, 0.0, false, &[floor, ceiling]);
        for _ in 0..240 {
            blob.step(dt, 0.0, false, &[floor, ceiling]);
            assert!(!polygon_self_intersects(&blob.particles));

            let underside = ceiling.center.y - ceiling.half_size.y;
            let top = ceiling.center.y + ceiling.half_size.y;
            let below = blob
                .particles
                .iter()
                .any(|particle| particle.position.y <= underside);
            let above = blob
                .particles
                .iter()
                .any(|particle| particle.position.y >= top);
            assert!(!(below && above));
        }
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
        let low = jump_speed_for_charge(0.1, DEFAULT_GAMEPLAY_RADIUS);
        let middle = jump_speed_for_charge(0.5, DEFAULT_GAMEPLAY_RADIUS);
        let full = jump_speed_for_charge(1.0, DEFAULT_GAMEPLAY_RADIUS);

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
    fn jump_height_is_inverse_to_creature_radius() {
        let small_radius = DEFAULT_GAMEPLAY_RADIUS * 0.5;
        let large_radius = DEFAULT_GAMEPLAY_RADIUS;
        let small_speed = jump_speed_for_charge(1.0, small_radius);
        let large_speed = jump_speed_for_charge(1.0, large_radius);

        assert!(small_speed > large_speed);
        // Ballistic height is proportional to speed squared. Therefore this
        // ratio must match the inverse ratio of the radii.
        let height_ratio = small_speed.powi(2) / large_speed.powi(2);
        let expected_ratio = large_radius / small_radius;
        assert!((height_ratio - expected_ratio).abs() < 0.001);
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

    #[test]
    fn cascading_split_requires_enough_source_particles() {
        assert!(!Blob::new_with_count(Vec2::ZERO, 20.0, 15).can_split());
        assert!(Blob::new_with_count(Vec2::ZERO, 20.0, 16).can_split());
    }

    #[test]
    fn smaller_split_child_reaches_a_higher_apex() {
        let parent = Blob::new(Vec2::ZERO, DEFAULT_GAMEPLAY_RADIUS);
        let [first, second] = parent.split_pair_uneven(1.0 / 120.0, 9, true);
        let (small, large) = if first.rest_radius < second.rest_radius {
            (first, second)
        } else {
            (second, first)
        };

        let small_height = measured_full_jump_height(small);
        let large_height = measured_full_jump_height(large);
        assert!(
            small_height > large_height * 1.08,
            "small child rose {small_height}px, large child rose {large_height}px"
        );
    }

    #[test]
    fn recursive_fragments_have_monotonic_jump_heights() {
        let root = Blob::new(Vec2::ZERO, DEFAULT_GAMEPLAY_RADIUS);
        let [first, second] = root.split_pair_uneven(1.0 / 120.0, 9, true);
        let [a, b] = first.split_pair_uneven(1.0 / 120.0, 7, true);
        let [c, d] = second.split_pair_uneven(1.0 / 120.0, 10, false);
        let mut samples = [a, b, c, d]
            .into_iter()
            .map(|blob| (blob.rest_radius, measured_full_jump_height(blob)))
            .collect::<Vec<_>>();
        samples.sort_by(|left, right| left.0.total_cmp(&right.0));
        for pair in samples.windows(2) {
            assert!(
                pair[0].1 >= pair[1].1,
                "radius/height samples are {samples:?}"
            );
        }
    }

    fn measured_full_jump_height(mut blob: Blob) -> f32 {
        let floor = Platform {
            center: Vec2::new(blob.center().x, -70.0),
            half_size: Vec2::new(500.0, 10.0),
        };
        let dt = 1.0 / 120.0;
        for _ in 0..120 {
            blob.step(dt, 0.0, false, &[floor]);
        }
        for _ in 0..90 {
            blob.step(dt, 0.0, true, &[floor]);
        }
        let launch_height = blob.center().y;
        blob.step(dt, 0.0, false, &[floor]);
        let mut apex = blob.center().y;
        for _ in 0..240 {
            blob.step(dt, 0.0, false, &[floor]);
            apex = apex.max(blob.center().y);
        }
        apex - launch_height
    }
}
