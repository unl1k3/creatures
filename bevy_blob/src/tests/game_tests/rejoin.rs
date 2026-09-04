use super::*;

#[test]
fn platform_blocks_rejoining_line_of_sight() {
    let wall = Platform {
        center: Vec2::ZERO,
        half_size: Vec2::new(5.0, 80.0),
    };
    assert!(!path_is_clear(
        Vec2::new(-50.0, 0.0),
        Vec2::new(50.0, 0.0),
        &[wall]
    ));
    assert!(path_is_clear(
        Vec2::new(-50.0, 100.0),
        Vec2::new(50.0, 100.0),
        &[wall]
    ));
}

#[test]
fn touching_children_merge_into_one_blob() {
    let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
    let [mut first, mut second] = parent.split_pair(1.0 / 120.0);
    let midpoint = (first.center() + second.center()) * 0.5;
    first.translate(midpoint - first.center() + Vec2::NEG_X * first.rest_radius);
    second.translate(midpoint - second.center() + Vec2::X * second.rest_radius);
    let mut world = sibling_world(first, second, true);

    update_rejoining(&mut world, &[], &[]);
    assert_eq!(world.active.len(), 1);
    assert!(world.rejoin_parent.is_none());
    assert_eq!(world.active[0].id, 0);
}

#[test]
fn touching_children_do_not_merge_inside_a_gap_too_small_for_the_parent() {
    let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
    let [mut first, mut second] = parent.split_pair(1.0 / 120.0);
    first.translate(Vec2::new(-first.rest_radius, 0.0) - first.center());
    second.translate(Vec2::new(second.rest_radius, 0.0) - second.center());
    let mut world = sibling_world(first, second, true);
    let platforms = [
        Platform {
            center: Vec2::new(0.0, -42.0),
            half_size: Vec2::new(180.0, 10.0),
        },
        Platform {
            center: Vec2::new(0.0, 42.0),
            half_size: Vec2::new(180.0, 10.0),
        },
    ];

    update_rejoining(&mut world, &platforms, &[]);

    assert_eq!(world.active.len(), 2);
    assert_eq!(world.rejoin_parent, Some(0));
}

#[test]
fn separated_children_roll_before_they_can_merge() {
    let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
    let [first, second] = parent.split_pair(1.0 / 120.0);
    let mut world = sibling_world(first, second, true);

    let directions = rejoin_roll_directions(&world, &[]).unwrap();
    assert_eq!(directions, vec![1.0, -1.0]);
    update_rejoining(&mut world, &[], &[]);
    assert_eq!(world.active.len(), 2);
}

#[test]
fn touching_children_do_not_merge_until_rejoining_is_enabled() {
    let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
    let [mut first, mut second] = parent.split_pair(1.0 / 120.0);
    let midpoint = (first.center() + second.center()) * 0.5;
    first.translate(midpoint - first.center() + Vec2::NEG_X * first.rest_radius);
    second.translate(midpoint - second.center() + Vec2::X * second.rest_radius);
    let mut world = sibling_world(first, second, false);

    update_rejoining(&mut world, &[], &[]);
    assert_eq!(world.active.len(), 2);
}

#[test]
fn unsuccessful_rejoin_stops_after_timeout() {
    let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
    let [first, second] = parent.split_pair(1.0 / 120.0);
    let mut world = sibling_world(first, second, true);

    advance_rejoin_timeout(&mut world, REJOIN_TIMEOUT - 0.1);
    assert_eq!(world.rejoin_parent, Some(0));
    advance_rejoin_timeout(&mut world, 0.11);
    assert!(world.rejoin_parent.is_none());
    assert_eq!(world.rejoin_elapsed, 0.0);
    assert!(rejoin_roll_directions(&world, &[]).is_none());
}

#[test]
fn selected_blob_can_split_again_and_merge_up_the_lineage() {
    let mut world = BlobWorld {
        active: vec![active(0, None, Blob::new(Vec2::ZERO, INITIAL_RADIUS))],
        selected: 0,
        rejoin_parent: None,
        rejoin_elapsed: 0.0,
        parent_links: HashMap::new(),
        next_id: 1,
    };
    let mut rng = SplitRng(0x1234_5678);
    let dt = 1.0 / 120.0;

    split_selected(&mut world, &mut rng, dt);
    assert_eq!(world.active.len(), 2);
    let root_sibling_id = world.active[1].id;

    // The selected first child is divided again, producing three leaves.
    split_selected(&mut world, &mut rng, dt);
    assert_eq!(world.active.len(), 3);
    let inner_parent = world.active[0].parent_id.unwrap();
    assert_eq!(world.active[1].parent_id, Some(inner_parent));
    assert_eq!(world.active[2].id, root_sibling_id);

    // Merge the deepest siblings first.
    assert!(start_selected_rejoin(&mut world));
    touch_rejoin_pair(&mut world);
    update_rejoining(&mut world, &[], &[]);
    assert_eq!(world.active.len(), 2);
    assert_eq!(world.active[0].id, inner_parent);

    // The reconstructed parent can now merge with its own sibling.
    world.selected = 0;
    assert!(start_selected_rejoin(&mut world));
    touch_rejoin_pair(&mut world);
    update_rejoining(&mut world, &[], &[]);
    assert_eq!(world.active.len(), 1);
    assert_eq!(world.active[0].id, 0);
    assert_eq!(world.active[0].parent_id, None);
}

fn touch_rejoin_pair(world: &mut BlobWorld) {
    let (first_index, second_index, _) = rejoin_pair_indices(world).unwrap();
    let midpoint = (world.active[first_index].body.center()
        + world.active[second_index].body.center())
        * 0.5;
    let first_radius = world.active[first_index].body.rest_radius;
    let second_radius = world.active[second_index].body.rest_radius;
    let first_offset =
        midpoint - world.active[first_index].body.center() + Vec2::NEG_X * first_radius;
    let second_offset =
        midpoint - world.active[second_index].body.center() + Vec2::X * second_radius;
    world.active[first_index].body.translate(first_offset);
    world.active[second_index].body.translate(second_offset);
}
