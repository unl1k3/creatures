use super::*;

#[test]
fn tab_selection_wraps_between_two_blobs() {
    assert_eq!(next_selection(0, 2), 1);
    assert_eq!(next_selection(1, 2), 0);
}

#[test]
fn camera_target_is_the_selected_blob() {
    let world = BlobWorld {
        active: vec![
            active(1, Some(0), Blob::new(Vec2::new(-80.0, 20.0), 20.0)),
            active(2, Some(0), Blob::new(Vec2::new(90.0, 160.0), 20.0)),
        ],
        selected: 1,
        rejoin_parent: None,
        rejoin_elapsed: 0.0,
        parent_links: HashMap::from([(0, None)]),
        next_id: 3,
    };
    assert!(
        selected_camera_target(&world)
            .unwrap()
            .distance(Vec2::new(90.0, 160.0))
            < 0.0001
    );
}

#[test]
fn first_split_has_a_new_shared_color() {
    assert_ne!(blob_family_color(None), blob_family_color(Some(0)));
}

#[test]
fn later_siblings_share_a_new_stable_color() {
    assert_eq!(blob_family_color(Some(4)), blob_family_color(Some(4)));
    assert_ne!(blob_family_color(Some(4)), blob_family_color(Some(5)));
    assert_ne!(blob_family_color(Some(0)), blob_family_color(Some(4)));
}

#[test]
fn every_blob_family_is_visibly_distinct() {
    let colors = [
        blob_family_color(None),
        blob_family_color(Some(0)),
        blob_family_color(Some(1)),
        blob_family_color(Some(2)),
        blob_family_color(Some(3)),
        blob_family_color(Some(4)),
    ];
    for first in 0..colors.len() {
        for second in first + 1..colors.len() {
            let first = colors[first].to_srgba();
            let second = colors[second].to_srgba();
            let distance = Vec3::new(
                first.red - second.red,
                first.green - second.green,
                first.blue - second.blue,
            )
            .length();
            assert!(
                distance >= 0.40,
                "blob family colors are too similar: distance {distance:.3}"
            );
        }
    }
}

#[test]
fn split_lineage_assigns_color_by_sibling_group() {
    let mut world = BlobWorld {
        active: vec![active(0, None, Blob::new(Vec2::ZERO, INITIAL_RADIUS))],
        selected: 0,
        rejoin_parent: None,
        rejoin_elapsed: 0.0,
        parent_links: HashMap::new(),
        next_id: 1,
    };
    let mut rng = SplitRng(0x1234_5678);

    split_selected(&mut world, &mut rng, 1.0 / 120.0);
    assert_eq!(world.active[0].parent_id, Some(0));
    assert_eq!(world.active[1].parent_id, Some(0));
    assert_ne!(
        blob_family_color(world.active[0].parent_id),
        blob_family_color(None)
    );

    split_selected(&mut world, &mut rng, 1.0 / 120.0);
    let child_color = blob_family_color(world.active[0].parent_id);
    assert_eq!(child_color, blob_family_color(world.active[1].parent_id));
    assert_ne!(child_color, blob_family_color(Some(0)));
}

#[test]
fn selected_blob_fill_is_visibly_different_but_keeps_its_family() {
    let selected = crate::rendering::blob_fill_color(Some(4), true);
    let inactive_sibling = crate::rendering::blob_fill_color(Some(4), false);
    let selected_other_family = crate::rendering::blob_fill_color(Some(5), true);

    assert_ne!(selected, inactive_sibling);
    assert_ne!(selected, selected_other_family);
}

#[test]
fn rendered_blob_mesh_has_a_triangle_for_every_membrane_edge() {
    let blob = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
    let mesh = crate::rendering::create_blob_mesh(&blob);

    assert_eq!(mesh.count_vertices(), blob.particles.len() + 1);
    assert_eq!(
        mesh.indices().map(|indices| indices.len()),
        Some(blob.particles.len() * 3)
    );
}

#[test]
fn jump_charge_indicator_stays_outside_a_deformed_blob() {
    let mut blob = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
    blob.particles[0].position += Vec2::X * 12.0;
    let center = blob.center();
    let outermost = blob
        .particles
        .iter()
        .map(|particle| particle.position.distance(center))
        .fold(0.0, f32::max);

    assert!(crate::rendering::charge_indicator_radius(&blob) > outermost);
}
