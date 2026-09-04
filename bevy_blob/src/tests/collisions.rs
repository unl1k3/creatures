use super::*;

#[test]
fn safety_bounds_stop_a_blob_without_a_rebound() {
    let mut blob = Blob::new(Vec2::new(40.0, 0.0), 20.0);
    blob.add_velocity(Vec2::new(5.0, 0.0));

    assert!(blob.contain_within_safety_bounds(Vec2::new(-50.0, -50.0), Vec2::new(49.0, 50.0)));
    assert!(
        blob.particles
            .iter()
            .all(|particle| particle.position.x <= 49.0 && particle.position.x >= -50.0)
    );
    assert!(
        blob.particles
            .iter()
            .filter(|particle| particle.position.x >= 48.999)
            .all(|particle| particle.previous.x == particle.position.x)
    );
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
            BlobStepProfile::new(0.0, false, true),
        );
    }
    assert!(
        !blob
            .particles
            .iter()
            .any(|particle| convex_penetration(particle.position, &fixture).is_some())
    );
    assert!(!has_self_intersections(&blob.particles));
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
        collision_side_from_reference(&particle, Vec2::new(0.0, -40.0), minimum, maximum,),
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
    blob.solve_collisions(&platforms, &[], &[]);
    assert!(
        blob.particles[0].position.y < -10.0,
        "particle entered the attached-platform seam: {:?}",
        blob.particles[0].position
    );
}
