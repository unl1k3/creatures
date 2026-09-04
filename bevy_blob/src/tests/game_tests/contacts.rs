use super::*;
use crate::blob::polygon_area;

#[test]
fn active_blobs_are_separated_when_they_overlap() {
    let mut blobs = vec![
        active(0, None, Blob::new(Vec2::ZERO, 30.0)),
        active(1, None, Blob::new(Vec2::ZERO, 30.0)),
    ];
    resolve_blob_collisions(&mut blobs);
    let scaled_gap = BLOB_CONTACT_VISUAL_CLEARANCE * blobs[0].body.size_scale();
    assert!(blob_surface_gap(&blobs[0].body, &blobs[1].body) >= scaled_gap - 0.01);
}

#[test]
fn crowded_blob_contact_does_not_amplify_a_neighbours_impulse() {
    let mut middle = Blob::new(Vec2::new(25.0, 0.0), 30.0);
    middle.add_velocity(Vec2::new(20.0, 0.0));
    let mut blobs = vec![
        active(0, None, Blob::new(Vec2::new(-25.0, 0.0), 30.0)),
        active(1, None, middle),
        active(2, None, Blob::new(Vec2::new(75.0, 0.0), 30.0)),
    ];

    resolve_blob_collisions(&mut blobs);

    assert!(blobs.iter().all(|blob| blob.body.center().is_finite()));
    assert!(
        blobs[2].body.velocity().length() <= 5.0,
        "crowded contact transferred an excessive impulse: {}",
        blobs[2].body.velocity().length()
    );
}

#[test]
fn living_blob_does_not_make_a_corpse_rebound() {
    let corpse = active(1, None, Blob::new(Vec2::ZERO, 30.0));
    let mut living = active(2, None, Blob::new(Vec2::new(0.0, 54.0), 30.0));
    living.body.add_velocity(Vec2::NEG_Y * 8.0);
    let corpse_center = corpse.body.center();
    let mut blobs = vec![corpse, living];

    resolve_blob_collisions_impl(&mut blobs, |id| (id != 1, true));

    assert!(blobs[0].body.center().distance(corpse_center) < 2.0);
    assert!(blobs[0].body.velocity().length() < 0.001);
    assert!(support_extent(&blobs[0].body, Vec2::Y) < 30.0);
    assert!(blobs[1].body.center().y > 54.0);
    assert!(blobs[1].body.grounded);
    assert!(support_extent(&blobs[1].body, Vec2::NEG_Y) < 30.0);
    blobs[1].body.step(1.0 / 120.0, 0.0, true, &[]);
    assert!(blobs[1].body.charge > 0.0);
}

#[test]
fn stacked_living_blobs_keep_deformed_contact_surfaces() {
    let lower = active(1, None, Blob::new(Vec2::ZERO, 30.0));
    let upper = active(2, None, Blob::new(Vec2::new(0.0, 54.0), 30.0));
    let mut blobs = vec![lower, upper];

    resolve_blob_collisions(&mut blobs);

    assert!(blobs[1].body.grounded);
    assert!(support_extent(&blobs[0].body, Vec2::Y) < 30.0);
    assert!(support_extent(&blobs[1].body, Vec2::NEG_Y) < 30.0);
    let expected_gap = BLOB_CONTACT_VISUAL_CLEARANCE
        * (blobs[0].body.size_scale() + blobs[1].body.size_scale())
        * 0.5;
    assert!(blob_surface_gap(&blobs[0].body, &blobs[1].body) >= expected_gap - 0.01);
}

#[test]
fn repeated_corpse_support_does_not_collapse_or_jitter() {
    let corpse = active(1, None, Blob::new(Vec2::ZERO, 30.0));
    let upper = active(2, None, Blob::new(Vec2::new(0.0, 54.0), 30.0));
    let corpse_area = corpse.body.rest_area;
    let mut blobs = vec![corpse, upper];
    for _ in 0..20 {
        resolve_blob_collisions_impl(&mut blobs, |id| (id != 1, true));
    }
    let settled_upper = blobs[1].body.center();
    for _ in 0..180 {
        resolve_blob_collisions_impl(&mut blobs, |id| (id != 1, true));
    }

    assert!(polygon_area(&blobs[0].body.particles).abs() >= corpse_area * 0.96);
    assert!(blobs[1].body.center().distance(settled_upper) < 0.05);
    assert!(blobs[0].body.velocity().length() < 0.001);
}

#[test]
fn corpse_participates_in_blob_stacking_and_can_be_moved() {
    let lower = active(1, None, Blob::new(Vec2::ZERO, 30.0));
    let upper = active(2, None, Blob::new(Vec2::new(0.0, 40.0), 30.0));
    let mut blobs = vec![lower, upper];
    let shell_center = blobs[0].body.center();
    let upper_center = blobs[1].body.center();
    resolve_blob_collisions_impl(&mut blobs, |_| (true, true));
    assert!(blobs[0].body.center().y < shell_center.y);
    assert!(blobs[1].body.center().y > upper_center.y);
}

#[test]
fn collision_uses_deformed_outline_instead_of_rest_radius() {
    let mut first = Blob::new(Vec2::new(-34.0, 0.0), 30.0);
    let mut second = Blob::new(Vec2::new(34.0, 0.0), 30.0);
    // The facing membrane points overlap while rest circles remain separated.
    first.particles[0].position.x += 12.0;
    first.particles[0].previous.x += 12.0;
    let leftmost = second.particles.len() / 2;
    second.particles[leftmost].position.x -= 12.0;
    second.particles[leftmost].previous.x -= 12.0;
    let center_of_mass_before = (first.center() * first.mass() + second.center() * second.mass())
        / (first.mass() + second.mass());
    let mut blobs = vec![active(0, None, first), active(1, None, second)];

    assert!(blob_surface_gap(&blobs[0].body, &blobs[1].body) < 0.0);
    resolve_blob_collisions(&mut blobs);

    let expected_gap = BLOB_CONTACT_VISUAL_CLEARANCE
        * (blobs[0].body.size_scale() + blobs[1].body.size_scale())
        * 0.5;
    let resulting_gap = blob_surface_gap(&blobs[0].body, &blobs[1].body);
    assert!(
        resulting_gap >= expected_gap - 0.01,
        "resulting gap {resulting_gap}, expected at least {expected_gap}"
    );
    assert!(
        resulting_gap <= expected_gap + 0.5,
        "contact correction left an excessive gap of {resulting_gap}"
    );
    let center_of_mass_after = (blobs[0].body.center() * blobs[0].body.mass()
        + blobs[1].body.center() * blobs[1].body.mass())
        / (blobs[0].body.mass() + blobs[1].body.mass());
    assert!(center_of_mass_after.distance(center_of_mass_before) < 0.0001);
}
