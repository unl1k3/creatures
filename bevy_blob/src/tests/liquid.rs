use super::*;

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
        blob.apply_wastewater_forces_with_spine_drag(12.0, -120.0, 1.0 / 120.0, 1.0, 1.0)
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
    assert!(
        angular_motion.abs() > 0.001,
        "water motion should induce a gentle roll"
    );
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
