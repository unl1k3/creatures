//! Regression tests for nutrition state, geometry and rendering.

use super::*;
use crate::level_format::NutrientDefinition;

#[test]
fn nutrient_render_is_a_filled_capsule_with_energy_nodules() {
    let nutrient = Nutrient {
        position: Vec2::new(12.0, -8.0),
        radius: 14.0,
        original_radius: 14.0,
        health: 1.0,
        state: NutrientState::Available {
            velocity: Vec2::ZERO,
        },
        was_submerged: false,
    };
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();

    append_nutrient_mesh(&nutrient, 0.5, &mut positions, &mut colors, &mut indices);

    assert_eq!(positions.len(), 85);
    assert_eq!(colors.len(), positions.len());
    assert_eq!(indices.len(), 300);
}

#[test]
fn empty_scenario_keeps_nutrient_mesh_allocation_alive() {
    let nutrient = Nutrient {
        position: Vec2::ZERO,
        radius: 10.0,
        original_radius: 10.0,
        health: 1.0,
        state: NutrientState::Available {
            velocity: Vec2::ZERO,
        },
        was_submerged: false,
    };
    let mut mesh = empty_nutrient_mesh();
    update_nutrient_mesh(&mut mesh, &[nutrient], 1, 0.0, &[]);
    let live_vertices = mesh.count_vertices();
    let live_indices = mesh.indices().expect("live nutrient indices").len();

    update_nutrient_mesh(&mut mesh, &[], 1, 1.0, &[]);

    assert_eq!(mesh.count_vertices(), live_vertices);
    assert_eq!(
        mesh.indices().expect("hidden nutrient indices").len(),
        live_indices
    );
}

#[test]
fn digesting_nutrient_is_more_transparent_than_an_external_one() {
    let mut nutrient = Nutrient {
        position: Vec2::ZERO,
        radius: 10.0,
        original_radius: 10.0,
        health: 1.0,
        state: NutrientState::Available {
            velocity: Vec2::ZERO,
        },
        was_submerged: false,
    };
    let external = nutrient_palette(&nutrient);
    nutrient.state = NutrientState::Digesting {
        blob_id: 0,
        elapsed: DIGESTION_DURATION * 0.5,
        local_position: Vec2::ZERO,
        velocity: Vec2::ZERO,
    };
    let internal = nutrient_palette(&nutrient);

    assert!(internal.0[3] < external.0[3]);
    assert!(internal.1[3] < external.1[3]);
    assert!(internal.3[3] > internal.0[3]);
}

#[test]
fn nutrient_positions_and_sizes_come_from_level_definitions() {
    let definitions = [NutrientDefinition {
        position: Vec2::new(42.0, -17.0),
        radius: 9.5,
    }];
    let mut world = NutritionWorld::default();
    world.reset_from_definitions(&definitions);

    assert_eq!(world.nutrients.len(), 1);
    assert_eq!(world.nutrients[0].position, definitions[0].position);
    assert_eq!(world.nutrients[0].radius, definitions[0].radius);
}

#[test]
fn digestive_penalty_recovers_as_absorption_progresses() {
    let mut world = NutritionWorld::default();
    world.nutrients.push(Nutrient {
        position: Vec2::ZERO,
        radius: 10.0,
        original_radius: 10.0,
        health: 1.0,
        state: NutrientState::Digesting {
            blob_id: 7,
            elapsed: 0.0,
            local_position: Vec2::ZERO,
            velocity: Vec2::ZERO,
        },
        was_submerged: false,
    });
    let initial = world.capability_factor(7);
    if let NutrientState::Digesting {
        ref mut elapsed, ..
    } = world.nutrients[0].state
    {
        *elapsed = DIGESTION_DURATION * 0.75;
    }
    assert!(initial < world.capability_factor(7));
}

#[test]
fn engulfing_and_expulsion_are_not_instantaneous() {
    const { assert!(ENGULF_DURATION > 0.5) };
    const { assert!(EXPULSION_DURATION > 0.3) };
}

#[test]
fn convex_collision_detects_embedded_circle() {
    let fixture = vec![
        Vec2::new(-50.0, -20.0),
        Vec2::new(50.0, -20.0),
        Vec2::new(50.0, 20.0),
        Vec2::new(-50.0, 20.0),
    ];
    assert!(circle_convex_penetration(Vec2::ZERO, 5.0, &fixture).is_some());
    assert!(circle_convex_penetration(Vec2::new(0.0, 30.0), 5.0, &fixture).is_none());
    let (depth, normal) = circle_convex_penetration(Vec2::ZERO, 5.0, &fixture).unwrap();
    let projected = normal * depth;
    assert!(circle_convex_penetration(projected, 5.0, &fixture).is_none());
}

#[test]
fn waste_contact_follows_the_deformed_membrane_without_an_invisible_halo() {
    let mut blob = Blob::new(Vec2::ZERO, 30.0);
    for particle in &mut blob.particles {
        particle.position.x *= 0.55;
    }
    let membrane_x = blob
        .particles
        .iter()
        .map(|particle| particle.position.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let waste_radius = 5.0;

    assert!(
        circle_blob_penetration(
            Vec2::new(membrane_x + waste_radius + 0.2, 0.0),
            waste_radius,
            &blob,
        )
        .is_none()
    );
    assert!(
        circle_blob_penetration(
            Vec2::new(membrane_x + waste_radius - 0.5, 0.0),
            waste_radius,
            &blob,
        )
        .is_some()
    );
}

#[test]
fn protrusion_stops_before_crossing_a_platform() {
    let blob = Blob::new(Vec2::ZERO, 20.0);
    let (edge, anchor_t) = membrane_anchor(&blob, Vec2::new(100.0, 0.0));
    let level = Level::from_test_geometry(
        vec![Platform {
            center: Vec2::new(50.0, 0.0),
            half_size: Vec2::new(5.0, 30.0),
        }],
        Vec::new(),
    );
    let blobs = BlobWorld {
        active: Vec::new(),
        selected: 0,
        rejoin_parent: None,
        rejoin_elapsed: 0.0,
        parent_links: HashMap::new(),
        next_id: 1,
    };
    let tip = constrain_protrusion_load(
        &blob,
        0,
        &blobs,
        Vec2::new(100.0, 0.0),
        4.2,
        1.0,
        0.61,
        edge,
        anchor_t,
        &level,
    );
    assert!(
        tip.x < 45.0,
        "constrained tip crossed the platform: {tip:?}"
    );
    assert!(tip.x > 20.0, "protrusion collapsed completely: {tip:?}");
}

#[test]
fn protrusion_section_detects_another_blob_membrane() {
    let other = Blob::new(Vec2::new(40.0, 0.0), 18.0);
    assert!(circle_intersects_blob_membrane(
        Vec2::new(21.0, 0.0),
        4.0,
        &other
    ));
    assert!(!circle_intersects_blob_membrane(
        Vec2::new(0.0, 45.0),
        4.0,
        &other
    ));
}
