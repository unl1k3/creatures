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
    fn submerged_blob_receives_buoyancy_and_only_splashes_on_entry() {
        let mut blob = Blob::new(Vec2::new(0.0, -10.0), 50.0);
        let first = blob
            .apply_wastewater_forces(0.0, -120.0, 1.0 / 120.0)
            .expect("the lower membrane is in water");

        assert!(first.entered);
        assert!(first.submerged_fraction > 0.1);
        assert!(blob.velocity().y > 0.0);
        assert!(
            blob.velocity().y < 0.2,
            "buoyancy must be an acceleration, not a bounce impulse"
        );
        let second = blob
            .apply_wastewater_forces(0.0, -120.0, 1.0 / 120.0)
            .expect("the blob remains in water");
        assert!(!second.entered);
    }

    #[test]
    fn submerged_motion_gently_rolls_and_flattens_the_blob() {
        let mut blob = Blob::new(Vec2::new(0.0, -28.0), 50.0);
        blob.add_velocity(Vec2::new(3.0, 0.0));
        for _ in 0..12 {
            blob.apply_wastewater_forces_with_spine_drag(
                12.0,
                -120.0,
                1.0 / 120.0,
                1.0,
                1.0,
            )
                .expect("the blob remains immersed");
        }

        let center = blob.center();
        let width = blob
            .particles
            .iter()
            .map(|particle| particle.position.x)
            .fold(f32::NEG_INFINITY, f32::max)
            - blob
                .particles
                .iter()
                .map(|particle| particle.position.x)
                .fold(f32::INFINITY, f32::min);
        let height = blob
            .particles
            .iter()
            .map(|particle| particle.position.y)
            .fold(f32::NEG_INFINITY, f32::max)
            - blob
                .particles
                .iter()
                .map(|particle| particle.position.y)
                .fold(f32::INFINITY, f32::min);
        let angular_motion = blob
            .particles
            .iter()
            .map(|particle| {
                let offset = particle.position - center;
                let local_velocity = particle.position - particle.previous - blob.velocity();
                offset.perp_dot(local_velocity) / offset.length_squared().max(1.0)
            })
            .sum::<f32>();

        assert!(width > height, "water should prevent a perfect circle");
        assert!(angular_motion.abs() > 0.001, "water motion should induce a gentle roll");
    }

    #[test]
    fn extended_spines_increase_water_rotation() {
        let mut bare = Blob::new(Vec2::new(0.0, -28.0), 50.0);
        let mut shielded = bare.clone();
        bare.add_velocity(Vec2::new(3.0, 0.0));
        shielded.add_velocity(Vec2::new(3.0, 0.0));
        for _ in 0..8 {
            bare.apply_wastewater_forces_with_spine_drag(12.0, -120.0, 1.0 / 120.0, 0.0, 0.0)
                .expect("bare blob is immersed");
            shielded
                .apply_wastewater_forces_with_spine_drag(12.0, -120.0, 1.0 / 120.0, 1.0, 1.0)
                .expect("shielded blob is immersed");
        }

        let angular_motion = |blob: &Blob| {
            let center = blob.center();
            let velocity = blob.velocity();
            blob.particles
                .iter()
                .map(|particle| {
                    let offset = particle.position - center;
                    let local_velocity = particle.position - particle.previous - velocity;
                    offset.perp_dot(local_velocity) / offset.length_squared().max(1.0)
                })
                .sum::<f32>()
                .abs()
        };
        assert!(angular_motion(&shielded) > angular_motion(&bare));
    }

    #[test]
    fn safety_bounds_stop_a_blob_without_a_rebound() {
        let mut blob = Blob::new(Vec2::new(40.0, 0.0), 20.0);
        blob.add_velocity(Vec2::new(5.0, 0.0));

        assert!(blob.contain_within_safety_bounds(Vec2::new(-50.0, -50.0), Vec2::new(49.0, 50.0)));
        assert!(blob.particles.iter().all(|particle| {
            particle.position.x <= 49.0
                && particle.position.x >= -50.0
        }));
        assert!(blob
            .particles
            .iter()
            .filter(|particle| particle.position.x >= 48.999)
            .all(|particle| particle.previous.x == particle.position.x));
    }

    #[test]
    fn corpse_progressively_loses_membrane_tonicity() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        let dt = 1.0 / 120.0;
        for _ in 0..240 {
            blob.step_with_vigor(dt, 0.0, false, &[], &[], 0.0, false, false);
        }
        assert!(blob.tonicity < 0.17);
        assert!(!has_self_intersections(&blob.particles));
    }

    #[test]
    fn corpse_keeps_volume_and_stays_above_its_support() {
        let radius = 50.0;
        let platform = Platform {
            center: Vec2::new(0.0, -70.0),
            half_size: Vec2::new(200.0, 10.0),
        };
        let mut blob = Blob::new(Vec2::ZERO, radius);
        let dt = 1.0 / 120.0;
        for _ in 0..600 {
            blob.step_with_vigor(dt, 0.0, false, &[platform], &[], 0.0, false, false);
        }

        let contact_y = platform.center.y
            + platform.half_size.y
            + 5.0 * blob.size_scale();
        let lowest = blob
            .particles
            .iter()
            .map(|particle| particle.position.y)
            .fold(f32::INFINITY, f32::min);
        let area = polygon_area(&blob.particles).abs();
        assert!(lowest >= contact_y - 0.05);
        assert!(area >= blob.rest_area * 0.90);
        assert!(!has_self_intersections(&blob.particles));
    }

    #[test]
    fn deformed_corpse_cannot_leave_a_point_inside_a_platform() {
        let platform = Platform {
            center: Vec2::new(0.0, -50.0),
            half_size: Vec2::new(90.0, 12.0),
        };
        let mut blob = Blob::new(Vec2::new(65.0, 0.0), 45.0);
        blob.tonicity = 0.0;
        blob.idle_amount = 1.0;
        // Reproduce a sharp local deformation crossing the platform corner.
        let lowest = blob
            .particles
            .iter()
            .enumerate()
            .min_by(|(_, first), (_, second)| first.position.y.total_cmp(&second.position.y))
            .map(|(index, _)| index)
            .unwrap();
        blob.particles[lowest].position = Vec2::new(82.0, -50.0);
        blob.particles[lowest].previous = blob.particles[lowest].position;

        blob.step_with_vigor(
            1.0 / 120.0,
            0.0,
            false,
            &[platform],
            &[],
            0.0,
            false,
            false,
        );

        let minimum = platform.center - platform.half_size;
        let maximum = platform.center + platform.half_size;
        assert!(blob.idle_amount == 0.0);
        assert!(!blob.particles.iter().any(|particle| {
            particle.position.x > minimum.x
                && particle.position.x < maximum.x
                && particle.position.y > minimum.y
                && particle.position.y < maximum.y
        }));
        assert!(!has_self_intersections(&blob.particles));
    }

    #[test]
    fn convex_fixture_projects_particles_out_without_folding_membrane() {
        let fixture = vec![
            Vec2::new(-120.0, -80.0),
            Vec2::new(120.0, -80.0),
            Vec2::new(120.0, 40.0),
        ];
        let mut blob = Blob::new(Vec2::new(70.0, 35.0), 35.0);
        for _ in 0..120 {
            blob.step_with_vigor(
                1.0 / 120.0,
                0.0,
                false,
                &[],
                std::slice::from_ref(&fixture),
                0.0,
                false,
                true,
            );
        }
        assert!(!blob
            .particles
            .iter()
            .any(|particle| convex_penetration(particle.position, &fixture).is_some()));
        assert!(!has_self_intersections(&blob.particles));
    }

    #[test]
    fn releasing_jump_from_fixture_moves_outward_without_false_impact() {
        let fixture = vec![
            Vec2::new(-200.0, -100.0),
            Vec2::new(200.0, -100.0),
            Vec2::new(200.0, 100.0),
        ];
        let fixtures = std::slice::from_ref(&fixture);
        let dt = 1.0 / 120.0;
        let mut blob = Blob::new(Vec2::new(0.0, 38.0), 30.0);
        for _ in 0..90 {
            blob.step_with_vigor(dt, 0.0, false, &[], fixtures, 1.0, true, true);
        }
        for _ in 0..75 {
            blob.step_with_vigor(dt, 0.0, true, &[], fixtures, 1.0, true, true);
        }
        let velocity_before_release = blob.velocity();
        let support_normal = blob.support_normal;
        blob.step_with_vigor(dt, 0.0, false, &[], fixtures, 1.0, true, true);

        let launch_impulse = blob.velocity() - velocity_before_release;
        assert!(launch_impulse.dot(support_normal) > 0.0);
        assert!(launch_impulse.normalize_or_zero().dot(support_normal) > 0.90);
        assert!(blob.ignores_impact_trauma());
        assert!(blob.last_impact_speed < 1.0);
        assert!(!has_self_intersections(&blob.particles));
    }

    #[test]
    fn charged_jump_remembers_the_selected_movement_direction() {
        let floor = Platform {
            center: Vec2::new(0.0, -55.0),
            half_size: Vec2::new(400.0, 10.0),
        };
        let dt = 1.0 / 120.0;
        let mut blob = Blob::new(Vec2::ZERO, 40.0);
        for _ in 0..60 {
            blob.step_with_vigor(dt, 0.0, false, &[floor], &[], 1.0, true, true);
        }
        for _ in 0..60 {
            blob.step_with_vigor(dt, 1.0, true, &[floor], &[], 1.0, true, true);
        }
        let before = blob.velocity();
        blob.step_with_vigor(dt, 0.0, false, &[floor], &[], 1.0, true, true);
        let launch_impulse = blob.velocity() - before;

        assert!(launch_impulse.y > 0.0);
        assert!(launch_impulse.x > 0.0);
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
    fn external_projection_is_rebalanced_before_rendering() {
        let mut blob = Blob::new_with_count(Vec2::ZERO, 15.0, 14);
        blob.particles[0].position += Vec2::X * 42.0;
        blob.stabilize_after_external_projection();

        let center = blob.center();
        let furthest = blob
            .particles
            .iter()
            .map(|particle| particle.position.distance(center))
            .fold(0.0, f32::max);
        assert!(furthest <= blob.rest_radius * MAX_STRETCH_RATIO + 0.001);
        assert!(!has_self_intersections(&blob.particles));
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
    fn swept_collision_catches_a_particle_beyond_the_opposite_face() {
        let min = Vec2::new(-50.0, -10.0);
        let max = Vec2::new(50.0, 10.0);
        assert_eq!(
            swept_aabb_entry(Vec2::new(0.0, -35.0), Vec2::new(0.0, 35.0), min, max),
            Some((2, 25.0 / 70.0))
        );
        assert_eq!(
            swept_aabb_entry(Vec2::new(0.0, 35.0), Vec2::new(0.0, -35.0), min, max),
            Some((3, 25.0 / 70.0))
        );
    }

    #[test]
    fn attached_platform_seam_uses_the_outer_approach_face() {
        let particle = Particle {
            position: Vec2::new(4.0, 0.0),
            previous: Vec2::new(4.0, -1.0),
        };
        let minimum = Vec2::new(-105.0, -15.0);
        let maximum = Vec2::new(5.0, 15.0);
        assert_eq!(collision_entry_side(&particle, minimum, maximum), 1);
        assert_eq!(
            collision_side_from_reference(
                &particle,
                Vec2::new(0.0, -40.0),
                minimum,
                maximum,
            ),
            2,
            "the internal vertical seam must not override the lower surface"
        );
    }

    #[test]
    fn particle_is_projected_out_of_two_attached_platforms_at_the_seam() {
        let mut blob = Blob::new(Vec2::new(0.0, -40.0), 20.0);
        blob.particles[0].position = Vec2::ZERO;
        blob.particles[0].previous = Vec2::new(0.0, -1.0);
        let platforms = [
            Platform {
                center: Vec2::new(-50.0, 0.0),
                half_size: Vec2::new(50.0, 10.0),
            },
            Platform {
                center: Vec2::new(50.0, 0.0),
                half_size: Vec2::new(50.0, 10.0),
            },
        ];
        blob.solve_collisions(&platforms);
        assert!(
            blob.particles[0].position.y < -10.0,
            "particle entered the attached-platform seam: {:?}",
            blob.particles[0].position
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

        assert!(low < 380.0, "low charge is too strong: {low}");
        assert!(
            middle > 540.0 && middle < 640.0,
            "middle charge is {middle}"
        );
        assert_eq!(full, JUMP_MAX_SPEED);
        assert!(middle - low > 220.0);
        assert!(full - middle > 350.0);
    }

    #[test]
    fn jump_height_is_inverse_to_creature_radius() {
        let small_radius = DEFAULT_GAMEPLAY_RADIUS * 0.5;
        let large_radius = DEFAULT_GAMEPLAY_RADIUS;
        let small_speed = jump_speed_for_charge(1.0, small_radius);
        let large_speed = jump_speed_for_charge(1.0, large_radius);

        assert!(small_speed > large_speed);
        // Ballistic height is proportional to speed squared. The stronger
        // size exponent intentionally gives small blobs more than the former
        // inverse-radius height advantage.
        let height_ratio = small_speed.powi(2) / large_speed.powi(2);
        let expected_ratio = (large_radius / small_radius).powf(1.6);
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
    fn breathing_uses_current_upper_membrane_after_material_rotation() {
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        // Reassociate material indices with the opposite side, as happens
        // naturally after the body rolls while settling.
        let half_turn = blob.particles.len() / 2;
        blob.particles.rotate_left(half_turn);
        blob.idle_amount = 1.0;
        blob.idle_phase = 0.25;
        let lower_before = blob
            .particles
            .iter()
            .map(|particle| particle.position)
            .collect::<Vec<_>>();

        blob.solve_idle_shape();

        for (particle, before) in blob.particles.iter().zip(lower_before) {
            if before.y < -0.001 {
                assert!(particle.position.distance(before) < 0.0001);
            }
        }
    }

    #[test]
    fn consecutive_idle_breaths_alternate_membrane_sides() {
        let sides = (0..8)
            .map(|cycle| idle_lobe_center(cycle).cos().signum())
            .collect::<Vec<_>>();
        assert!(sides.windows(2).all(|pair| pair[0] == -pair[1]));
    }

    #[test]
    fn idle_breath_cycles_do_not_accumulate_ground_drift() {
        let floor = Platform {
            center: Vec2::new(0.0, -60.0),
            half_size: Vec2::new(200.0, 10.0),
        };
        let mut blob = Blob::new(Vec2::ZERO, 50.0);
        let dt = 1.0 / 120.0;
        for _ in 0..240 {
            blob.step(dt, 0.0, false, &[floor]);
        }
        blob.idle_phase = 0.0;
        blob.idle_amount = 1.0;
        let mut previous_x = blob.center().x;
        let mut drifts = Vec::new();
        for _ in 0..4 {
            for _ in 0..312 {
                blob.step(dt, 0.0, false, &[floor]);
            }
            let current_x = blob.center().x;
            drifts.push(current_x - previous_x);
            previous_x = current_x;
        }
        assert!(
            drifts.iter().all(|drift| drift.abs() < 0.01),
            "idle breathing accumulated visible ground drift: {drifts:?}"
        );
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
