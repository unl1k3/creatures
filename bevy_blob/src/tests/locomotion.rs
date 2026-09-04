use super::*;

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

fn step_on_ice(blob: &mut Blob, floor: Platform, horizontal: f32, traction: f32) {
    blob.set_ice_traction(traction);
    blob.step_with_vigor_on_ice(
        1.0 / 120.0,
        BlobStepInput {
            horizontal,
            charging: false,
        },
        BlobStepEnvironment {
            platforms: std::slice::from_ref(&floor),
            ice_platform_indices: &[0],
            glue_platform_indices: &[],
            fixtures: &[],
        },
        BlobStepProfile::new(1.0, true, true),
    );
}

#[test]
fn bare_ice_preserves_inertia_while_input_spins_the_membrane() {
    let floor = Platform {
        center: Vec2::new(0.0, -70.0),
        half_size: Vec2::new(500.0, 10.0),
    };
    let mut blob = Blob::new(Vec2::ZERO, 50.0);
    for _ in 0..90 {
        step_on_ice(&mut blob, floor, 0.0, 0.0);
    }
    blob.add_velocity(Vec2::X * 1.2);
    let start_x = blob.center().x;
    for _ in 0..24 {
        step_on_ice(&mut blob, floor, 1.0, 0.0);
    }

    assert!(blob.center().x > start_x + 12.0, "ice removed inertial motion");
    assert!(
        blob.angular_displacement() < -0.0001,
        "bare ice input did not produce the expected empty spin: {}",
        blob.angular_displacement()
    );
}

#[test]
fn bare_ice_spin_does_not_create_sideways_drift() {
    let floor = Platform {
        center: Vec2::new(0.0, -70.0),
        half_size: Vec2::new(500.0, 10.0),
    };
    let mut blob = Blob::new(Vec2::ZERO, 50.0);
    for _ in 0..90 {
        step_on_ice(&mut blob, floor, 0.0, 0.0);
    }
    let start_x = blob.center().x;
    for _ in 0..60 {
        step_on_ice(&mut blob, floor, 1.0, 0.0);
    }

    assert!(
        (blob.center().x - start_x).abs() < 0.05,
        "bare ice spin created sideways drift: {}",
        blob.center().x - start_x
    );
    assert!(blob.angular_displacement() < -0.0001);
}

#[test]
fn releasing_input_on_bare_ice_stops_spin_but_keeps_slide_inertia() {
    let floor = Platform {
        center: Vec2::new(0.0, -70.0),
        half_size: Vec2::new(500.0, 10.0),
    };
    let mut blob = Blob::new(Vec2::ZERO, 50.0);
    for _ in 0..90 {
        step_on_ice(&mut blob, floor, 0.0, 0.0);
    }
    blob.add_velocity(Vec2::X * 1.1);
    step_on_ice(&mut blob, floor, 1.0, 0.0);
    let release_x = blob.center().x;
    step_on_ice(&mut blob, floor, 0.0, 0.0);

    assert!(
        blob.center().x > release_x + 0.9,
        "release removed slide inertia"
    );
    assert!(
        blob.angular_displacement().abs() < 0.0002,
        "release left the blob spinning: {}",
        blob.angular_displacement()
    );
}

#[test]
fn deployed_spines_restore_limited_ice_rolling() {
    let floor = Platform {
        center: Vec2::new(0.0, -70.0),
        half_size: Vec2::new(500.0, 10.0),
    };
    let mut blob = Blob::new(Vec2::ZERO, 50.0);
    for _ in 0..90 {
        step_on_ice(&mut blob, floor, 0.0, 0.0);
    }
    let start_x = blob.center().x;
    for _ in 0..40 {
        step_on_ice(&mut blob, floor, 1.0, 0.28);
    }

    assert!(blob.center().x > start_x + 1.0, "spines did not provide ice traction");
    assert!(
        blob.angular_displacement() < -0.0001,
        "spines did not restore clockwise rolling: {}",
        blob.angular_displacement()
    );
}
