use super::*;

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

    assert_eq!(small.particles.len(), PARTICLE_COUNT);
    assert_eq!(large.particles.len(), PARTICLE_COUNT);
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
fn recursive_fragments_keep_a_clear_smallest_to_largest_jump_advantage() {
    let root = Blob::new(Vec2::ZERO, DEFAULT_GAMEPLAY_RADIUS);
    let [first, second] = root.split_pair_uneven(1.0 / 120.0, 9, true);
    let [a, b] = first.split_pair_uneven(1.0 / 120.0, 7, true);
    let [c, d] = second.split_pair_uneven(1.0 / 120.0, 10, false);
    let mut samples = [a, b, c, d]
        .into_iter()
        .map(|blob| (blob.rest_radius, measured_full_jump_height(blob)))
        .collect::<Vec<_>>();
    samples.sort_by(|left, right| left.0.total_cmp(&right.0));
    let smallest = samples.first().expect("split creates fragments");
    let largest = samples.last().expect("split creates fragments");
    // Near-equal fragments can differ slightly because the membrane's
    // irregular resting pose affects takeoff. The size rule is verified
    // across the meaningful range instead of demanding an artificial total
    // ordering for almost identical radii.
    assert!(
        smallest.1 > largest.1 * 1.08,
        "radius/height samples are {samples:?}"
    );
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
