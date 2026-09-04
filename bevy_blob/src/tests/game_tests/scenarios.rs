use super::*;

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
            BlobStepProfile::new(1.0, true, true),
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
                BlobStepProfile::new(1.0, true, true),
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
                BlobStepProfile::new(1.0, true, true),
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
        blob.step_with_vigor(
            dt,
            0.0,
            false,
            &platforms,
            &[],
            BlobStepProfile::new(1.0, true, true),
        );
    }
    for _ in 0..90 {
        blob.step_with_vigor(
            dt,
            0.0,
            true,
            &platforms,
            &[],
            BlobStepProfile::new(1.0, true, true),
        );
    }
    blob.step_with_vigor(
        dt,
        0.0,
        false,
        &platforms,
        &[],
        BlobStepProfile::new(1.0, true, true),
    );

    let mut reached_ceiling = false;
    for _ in 0..180 {
        blob.step_with_vigor(
            dt,
            0.0,
            false,
            &platforms,
            &[],
            BlobStepProfile::new(1.0, true, true),
        );
        reached_ceiling |= blob.center().y + radius >= overhead.center.y - overhead.half_size.y;
        assert!(blob.particles.iter().all(|particle| {
            let inside_x = (particle.position.x - overhead.center.x).abs() < overhead.half_size.x;
            let inside_y = (particle.position.y - overhead.center.y).abs() < overhead.half_size.y;
            !(inside_x && inside_y)
        }));
    }
    assert!(reached_ceiling, "test fragment never reached the overhead platform");
}
