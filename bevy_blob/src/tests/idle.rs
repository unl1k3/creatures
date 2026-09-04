use super::*;

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
