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
