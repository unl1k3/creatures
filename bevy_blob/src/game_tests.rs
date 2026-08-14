#[cfg(test)]
mod tests {
    use super::*;

    fn active(id: u64, parent_id: Option<u64>, body: Blob) -> ActiveBlob {
        ActiveBlob {
            id,
            parent_id,
            body,
        }
    }

    fn sibling_world(first: Blob, second: Blob, rejoining: bool) -> BlobWorld {
        BlobWorld {
            active: vec![active(1, Some(0), first), active(2, Some(0), second)],
            selected: 0,
            rejoin_parent: rejoining.then_some(0),
            rejoin_elapsed: 0.0,
            parent_links: HashMap::from([(0, None)]),
            next_id: 3,
        }
    }

    #[test]
    fn active_blobs_are_separated_when_they_overlap() {
        let mut blobs = vec![
            active(0, None, Blob::new(Vec2::ZERO, 30.0)),
            active(1, None, Blob::new(Vec2::ZERO, 30.0)),
        ];
        resolve_blob_collisions(&mut blobs);
        let distance = blobs[0].body.center().distance(blobs[1].body.center());
        let scaled_gap = 1.5 * blobs[0].body.size_scale();
        assert!(
            distance >= blobs[0].body.rest_radius + blobs[1].body.rest_radius + scaled_gap - 0.01
        );
    }

    #[test]
    fn collision_uses_deformed_outline_instead_of_rest_radius() {
        let mut first = Blob::new(Vec2::new(-34.0, 0.0), 30.0);
        let mut second = Blob::new(Vec2::new(34.0, 0.0), 30.0);
        // Push the facing membrane points beyond their nominal radii while the
        // two rest circles remain separated.
        first.particles[0].position.x += 12.0;
        first.particles[0].previous.x += 12.0;
        let leftmost = second.particles.len() / 2;
        second.particles[leftmost].position.x -= 12.0;
        second.particles[leftmost].previous.x -= 12.0;
        let center_of_mass_before = (first.center() * first.mass()
            + second.center() * second.mass())
            / (first.mass() + second.mass());
        let mut blobs = vec![active(0, None, first), active(1, None, second)];

        assert!(blob_surface_gap(&blobs[0].body, &blobs[1].body) < 0.0);
        resolve_blob_collisions(&mut blobs);

        let expected_gap = 1.5 * (blobs[0].body.size_scale() + blobs[1].body.size_scale()) * 0.5;
        assert!(blob_surface_gap(&blobs[0].body, &blobs[1].body) >= expected_gap - 0.01);
        let center_of_mass_after = (blobs[0].body.center() * blobs[0].body.mass()
            + blobs[1].body.center() * blobs[1].body.mass())
            / (blobs[0].body.mass() + blobs[1].body.mass());
        assert!(center_of_mass_after.distance(center_of_mass_before) < 0.0001);
    }

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
    fn siblings_share_a_color_and_other_families_do_not() {
        assert_eq!(blob_family_color(Some(4)), blob_family_color(Some(4)));
        assert_ne!(blob_family_color(Some(4)), blob_family_color(Some(5)));
        assert_ne!(blob_family_color(None), blob_family_color(Some(4)));
    }

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

        update_rejoining(&mut world, &[]);
        assert_eq!(world.active.len(), 1);
        assert!(world.rejoin_parent.is_none());
        assert_eq!(world.active[0].id, 0);
    }

    #[test]
    fn separated_children_roll_before_they_can_merge() {
        let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
        let [first, second] = parent.split_pair(1.0 / 120.0);
        let mut world = sibling_world(first, second, true);

        let directions = rejoin_roll_directions(&world, &[]).unwrap();
        assert_eq!(directions, vec![1.0, -1.0]);
        update_rejoining(&mut world, &[]);
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

        update_rejoining(&mut world, &[]);
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
        update_rejoining(&mut world, &[]);
        assert_eq!(world.active.len(), 2);
        assert_eq!(world.active[0].id, inner_parent);

        // The reconstructed parent can now merge with its own sibling.
        world.selected = 0;
        assert!(start_selected_rejoin(&mut world));
        touch_rejoin_pair(&mut world);
        update_rejoining(&mut world, &[]);
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
}
