#[cfg(test)]
mod tests {
    use super::*;
    use crate::blob::polygon_area;

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
        let scaled_gap = BLOB_CONTACT_VISUAL_CLEARANCE * blobs[0].body.size_scale();
        assert!(blob_surface_gap(&blobs[0].body, &blobs[1].body) >= scaled_gap - 0.01);
    }

    #[test]
    fn scenario_one_split_children_do_not_drift_left_while_breathing() {
        let (level, spawn) = Level::test_scenario(1);
        let dt = 1.0 / 120.0;
        let mut parent = Blob::new(spawn, INITIAL_RADIUS);
        for _ in 0..360 {
            parent.step_with_vigor(
                dt,
                0.0,
                false,
                &level.platforms,
                &level.fixtures,
                1.0,
                true,
                true,
            );
        }

        let mut world = BlobWorld {
            active: vec![active(0, None, parent)],
            selected: 0,
            rejoin_parent: None,
            rejoin_elapsed: 0.0,
            parent_links: HashMap::new(),
            next_id: 1,
        };
        let mut rng = SplitRng(0x5eed);
        for _ in 0..3 {
            split_selected(&mut world, &mut rng, dt);
        }
        let mut children = world.active;
        for _ in 0..360 {
            for child in &mut children {
                child.body.step_with_vigor(
                    dt,
                    0.0,
                    false,
                    &level.platforms,
                    &level.fixtures,
                    1.0,
                    true,
                    true,
                );
            }
            resolve_blob_collisions(&mut children);
        }
        let settled = children
            .iter()
            .map(|child| child.body.center())
            .collect::<Vec<_>>();

        for _ in 0..3_120 {
            for child in &mut children {
                child.body.step_with_vigor(
                    dt,
                    0.0,
                    false,
                    &level.platforms,
                    &level.fixtures,
                    1.0,
                    true,
                    true,
                );
            }
            resolve_blob_collisions(&mut children);
        }

        let drift = children
            .iter()
            .zip(&settled)
            .map(|(child, start)| child.body.center().x - start.x)
            .collect::<Vec<_>>();
        assert!(
            drift.iter().all(|distance| distance.abs() < 1.0),
            "scenario 1 breathing drifted after split: {drift:?}"
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
        assert!(blob_surface_gap(&blobs[0].body, &blobs[1].body) >= 0.0);
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

    #[test]
    fn four_way_sized_fragment_cannot_enter_first_overhead_platform() {
        let floor = Platform {
            center: Vec2::new(0.0, -370.0),
            half_size: Vec2::new(330.0, 19.0),
        };
        let overhead = Platform {
            center: Vec2::new(-250.0, -150.0),
            half_size: Vec2::new(130.0, 14.0),
        };
        let platforms = [floor, overhead];
        let radius = INITIAL_RADIUS * 0.48;
        let spawn = Vec2::new(-250.0, floor.center.y + floor.half_size.y + radius);
        let mut blob = Blob::new_with_count(spawn, radius, 18);
        let dt = 1.0 / 120.0;

        for _ in 0..45 {
            blob.step_with_vigor(dt, 0.0, false, &platforms, &[], 1.0, true, true);
        }
        for _ in 0..90 {
            blob.step_with_vigor(dt, 0.0, true, &platforms, &[], 1.0, true, true);
        }
        blob.step_with_vigor(dt, 0.0, false, &platforms, &[], 1.0, true, true);

        let mut reached_ceiling = false;
        for _ in 0..180 {
            blob.step_with_vigor(dt, 0.0, false, &platforms, &[], 1.0, true, true);
            reached_ceiling |= blob.center().y + radius >= overhead.center.y - overhead.half_size.y;
            assert!(blob.particles.iter().all(|particle| {
                let inside_x = (particle.position.x - overhead.center.x).abs()
                    < overhead.half_size.x;
                let inside_y = (particle.position.y - overhead.center.y).abs()
                    < overhead.half_size.y;
                !(inside_x && inside_y)
            }));
        }
        assert!(reached_ceiling, "test fragment never reached the overhead platform");
    }
}
