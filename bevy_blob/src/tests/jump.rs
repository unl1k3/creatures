use super::*;

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
        blob.step_with_vigor(
            dt,
            0.0,
            false,
            &[],
            fixtures,
            BlobStepProfile::new(1.0, true, true),
        );
    }
    for _ in 0..75 {
        blob.step_with_vigor(
            dt,
            0.0,
            true,
            &[],
            fixtures,
            BlobStepProfile::new(1.0, true, true),
        );
    }
    let velocity_before_release = blob.velocity();
    let support_normal = blob.support_normal;
    blob.step_with_vigor(
        dt,
        0.0,
        false,
        &[],
        fixtures,
        BlobStepProfile::new(1.0, true, true),
    );

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
        blob.step_with_vigor(
            dt,
            0.0,
            false,
            &[floor],
            &[],
            BlobStepProfile::new(1.0, true, true),
        );
    }
    for _ in 0..60 {
        blob.step_with_vigor(
            dt,
            1.0,
            true,
            &[floor],
            &[],
            BlobStepProfile::new(1.0, true, true),
        );
    }
    let before = blob.velocity();
    blob.step_with_vigor(
        dt,
        0.0,
        false,
        &[floor],
        &[],
        BlobStepProfile::new(1.0, true, true),
    );
    let launch_impulse = blob.velocity() - before;

    assert!(launch_impulse.y > 0.0);
    assert!(launch_impulse.x > 0.0);
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
fn charge_only_arms_from_a_real_support_contact() {
    let mut blob = Blob::new(Vec2::ZERO, 50.0);
    let dt = 1.0 / 120.0;
    for _ in 0..45 {
        blob.step(dt, 0.0, true, &[]);
    }
    assert!(!blob.jump_armed);
    assert_eq!(blob.charge, 0.0);

    blob.grounded = true;
    blob.step(dt, 0.0, true, &[]);
    assert!(blob.jump_armed);
    assert!(blob.charge > 0.0);
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
    // Ballistic height is proportional to speed squared. The stronger size
    // exponent intentionally gives small blobs a clear height advantage.
    let height_ratio = small_speed.powi(2) / large_speed.powi(2);
    let expected_ratio = (large_radius / small_radius).powf(1.6);
    assert!((height_ratio - expected_ratio).abs() < 0.001);
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
