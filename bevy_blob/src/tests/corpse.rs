use super::*;

#[test]
fn corpse_progressively_loses_membrane_tonicity() {
    let mut blob = Blob::new(Vec2::ZERO, 50.0);
    let dt = 1.0 / 120.0;
    for _ in 0..240 {
        blob.step_with_vigor(
            dt,
            0.0,
            false,
            &[],
            &[],
            BlobStepProfile::new(0.0, false, false),
        );
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
        blob.step_with_vigor(
            dt,
            0.0,
            false,
            &[platform],
            &[],
            BlobStepProfile::new(0.0, false, false),
        );
    }

    let contact_y = platform.center.y + platform.half_size.y + 5.0 * blob.size_scale();
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
        BlobStepProfile::new(0.0, false, false),
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
