use super::*;
use crate::environment::WastewaterEffects;
use crate::level_format::NutrientDefinition;
use crate::palette;
use avian2d::prelude::{Collider, LinearVelocity};

mod digestion;
mod feeding;
mod geometry;
mod membrane;
mod physics;
mod render;

use digestion::{
    advance_digesting, advance_engulfing, advance_expelling, advance_probe_and_capture,
};
pub(super) use feeding::start_phagocytosis;
#[cfg(test)]
use geometry::circle_convex_penetration;
pub(super) use membrane::circle_blob_penetration;
#[cfg(test)]
use membrane::circle_intersects_blob_membrane;
use membrane::{
    circle_outside_blob_membrane, constrain_protrusion_load, membrane_anchor,
    membrane_lower_boundary, phagocytosis_path_clear,
};
pub(super) use physics::{NutrientPhysics, spawn_nutrient_bodies};
use physics::{
    free_nutrient_contact_radius, sync_free_nutrients_before_digestion,
    sync_nutrient_bodies_after_digestion,
};
use render::update_nutrient_mesh;
#[cfg(test)]
use render::{append_nutrient_mesh, nutrient_palette};
pub(super) use render::{draw_nutrition, empty_nutrient_mesh};

const ENGULF_DURATION: f32 = 1.25;
const DIGESTION_DURATION: f32 = 6.0;
const EXPULSION_DURATION: f32 = 1.2;
const INTERNAL_WASTE_DRAG: f32 = 2.2;
// The procedural nutrient is slightly squashed, but its collision envelope is
// kept close to the rendered profile so it never looks embedded in a surface.
// The small skin also covers Avian's resting-contact tolerance.
const NUTRIENT_STRUCTURE_CONTACT_SCALE: f32 = 0.96;
const NUTRIENT_CONTACT_SKIN: f32 = 1.1;
const ENERGY_YIELD: f32 = 0.46;
const OBJECT_GRAVITY: f32 = 900.0;
const PHAGOCYTOSIS_REACH: f32 = 44.0;

#[derive(Clone, Copy, Debug)]
struct ExploratoryProbe {
    blob_id: u64,
    age: f32,
    extension: f32,
    direction: Vec2,
    tip: Vec2,
    variation: f32,
    anchor_edge: usize,
    anchor_t: f32,
}

#[derive(Clone, Copy, Debug)]
enum NutrientState {
    Available {
        velocity: Vec2,
    },
    Engulfing {
        blob_id: u64,
        elapsed: f32,
        origin: Vec2,
        reach: f32,
        probe_tip: Vec2,
        contact_elapsed: Option<f32>,
        variation: f32,
        anchor_edge: usize,
        anchor_t: f32,
    },
    Digesting {
        blob_id: u64,
        elapsed: f32,
        local_position: Vec2,
        velocity: Vec2,
    },
    Expelling {
        blob_id: u64,
        elapsed: f32,
        velocity: Vec2,
    },
    Waste {
        velocity: Vec2,
    },
}

#[derive(Clone, Copy, Debug)]
struct Nutrient {
    position: Vec2,
    radius: f32,
    original_radius: f32,
    health: f32,
    state: NutrientState,
    was_submerged: bool,
}

impl Nutrient {
    fn is_edible(&self) -> bool {
        self.health > 0.001 && matches!(self.state, NutrientState::Available { .. })
    }
}

#[derive(Resource, Default)]
pub(super) struct NutritionWorld {
    nutrients: Vec<Nutrient>,
    probe: Option<ExploratoryProbe>,
    variation_serial: u64,
}

#[derive(Resource)]
pub(super) struct NutrientRenderAssets {
    mesh: Handle<Mesh>,
    // Never shrink the extracted mesh: Bevy's render slab can still reference
    // the previous allocation during a scenario switch.
    slots: usize,
}

impl NutritionWorld {
    pub(super) fn is_free_index(&self, index: usize) -> bool {
        self.nutrients.get(index).is_some_and(|nutrient| {
            matches!(
                nutrient.state,
                NutrientState::Available { .. } | NutrientState::Waste { .. }
            )
        })
    }

    pub(super) fn collision_radius(&self, index: usize) -> Option<f32> {
        self.nutrients.get(index).map(free_nutrient_contact_radius)
    }
    pub(super) fn reset_from_definitions(&mut self, definitions: &[NutrientDefinition]) {
        self.probe = None;
        self.nutrients = definitions
            .iter()
            .map(|definition| Nutrient {
                position: definition.position,
                radius: definition.radius,
                original_radius: definition.radius,
                health: 1.0,
                state: NutrientState::Available {
                    velocity: Vec2::ZERO,
                },
                was_submerged: false,
            })
            .collect();
    }

    pub(super) fn digestion_progress(&self, blob_id: u64) -> Option<f32> {
        self.nutrients
            .iter()
            .find_map(|nutrient| match nutrient.state {
                NutrientState::Engulfing {
                    blob_id: id,
                    elapsed,
                    ..
                } if id == blob_id => Some(-(elapsed / ENGULF_DURATION).clamp(0.0, 1.0)),
                NutrientState::Digesting {
                    blob_id: id,
                    elapsed,
                    ..
                } if id == blob_id => Some((elapsed / DIGESTION_DURATION).clamp(0.0, 1.0)),
                NutrientState::Expelling {
                    blob_id: id,
                    elapsed,
                    ..
                } if id == blob_id => Some(1.0 + (elapsed / EXPULSION_DURATION).clamp(0.0, 1.0)),
                _ => None,
            })
    }

    pub(super) fn capability_factor(&self, blob_id: u64) -> f32 {
        match self.digestion_progress(blob_id) {
            Some(progress) if progress < 0.0 => 0.62 - progress.abs() * 0.14,
            Some(progress) if progress <= 1.0 => 0.48 + 0.52 * progress.sqrt(),
            Some(_) => 0.82,
            None => 1.0,
        }
    }

    pub(super) fn is_digesting(&self, blob_id: u64) -> bool {
        self.digestion_progress(blob_id).is_some()
    }

    pub(super) fn has_external_protrusion(&self, blob_id: u64) -> bool {
        self.probe
            .is_some_and(|probe| probe.blob_id == blob_id && probe.extension > 0.01)
            || self.nutrients.iter().any(|nutrient| {
                matches!(
                    nutrient.state,
                    NutrientState::Engulfing {
                        blob_id: id,
                        elapsed,
                        ..
                    } if id == blob_id && elapsed > 0.005
                )
            })
    }

    pub(super) fn internal_load(&self, blob_id: u64) -> Option<(Vec2, f32, f32, f32, usize, f32)> {
        self.nutrients
            .iter()
            .find_map(|nutrient| match nutrient.state {
                NutrientState::Engulfing {
                    blob_id: id,
                    elapsed,
                    probe_tip,
                    contact_elapsed,
                    variation,
                    anchor_edge,
                    anchor_t,
                    ..
                } if id == blob_id => {
                    let extension = smoothstep((elapsed / 0.48).clamp(0.0, 1.0));
                    let grip = contact_elapsed
                        .map(|value| (value / 0.22).clamp(0.0, 1.0))
                        .unwrap_or(0.0);
                    Some((
                        probe_tip,
                        (nutrient.radius * (0.34 + grip * 0.22)).max(3.2),
                        extension,
                        variation,
                        anchor_edge,
                        anchor_t,
                    ))
                }
                _ => None,
            })
            .or_else(|| {
                self.probe
                    .filter(|probe| probe.blob_id == blob_id)
                    .map(|probe| {
                        (
                            probe.tip,
                            4.2,
                            smoothstep(probe.extension),
                            probe.variation,
                            probe.anchor_edge,
                            probe.anchor_t,
                        )
                    })
            })
    }

    pub(super) fn physical_load(&self, blob_id: u64) -> Option<(Vec2, f32, f32)> {
        self.nutrients
            .iter()
            .find_map(|nutrient| match nutrient.state {
                NutrientState::Digesting {
                    blob_id: id,
                    elapsed,
                    ..
                } if id == blob_id => {
                    let progress = (elapsed / DIGESTION_DURATION).clamp(0.0, 1.0);
                    Some((nutrient.position, nutrient.radius, 1.0 - progress * 0.55))
                }
                _ => None,
            })
    }
}

pub(super) fn setup_nutrition(
    mut commands: Commands,
    level: Res<Level>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut nutrition = NutritionWorld::default();
    nutrition.reset_from_definitions(&level.nutrients);
    let slots = nutrition.nutrients.len().max(1);
    let mut nutrient_mesh = empty_nutrient_mesh();
    update_nutrient_mesh(&mut nutrient_mesh, &nutrition.nutrients, slots, 0.0, &[]);
    commands.insert_resource(nutrition);
    spawn_nutrient_bodies(&mut commands, &level.nutrients);
    let mesh = meshes.add(nutrient_mesh);
    commands.spawn((
        Mesh2d(mesh.clone()),
        // Vertex alpha shows nutrients through the translucent blob membrane.
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        Transform::from_xyz(0.0, 0.0, -0.06),
    ));
    commands.insert_resource(NutrientRenderAssets { mesh, slots });
}

pub(super) fn simulate_nutrition(
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    mut vitality: ResMut<VitalityWorld>,
    mut nutrition: ResMut<NutritionWorld>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
    mut wastewater_effects: ResMut<WastewaterEffects>,
    mut physics_nutrients: Query<(
        &NutrientPhysics,
        &mut Transform,
        &mut LinearVelocity,
        &mut Collider,
    )>,
) {
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();
    let rolling_command = movement_command(&keyboard);
    sync_free_nutrients_before_digestion(
        dt,
        elapsed,
        &blobs,
        &level,
        &mut nutrition,
        &mut sound_events,
        &mut wastewater_effects,
        &mut physics_nutrients,
    );
    advance_probe_and_capture(
        dt,
        &keyboard,
        rolling_command,
        &blobs,
        &level,
        &vitality,
        &mut nutrition,
    );

    let mut interrupted_probe = None;
    for nutrient in &mut nutrition.nutrients {
        match nutrient.state {
            NutrientState::Available { velocity } => {
                nutrient.state = NutrientState::Available { velocity };
            }
            NutrientState::Engulfing { .. } => {
                if let Some(probe) = advance_engulfing(
                    nutrient,
                    dt,
                    rolling_command,
                    &blobs,
                    &level,
                    &vitality,
                    &mut sound_events,
                ) {
                    interrupted_probe = Some(probe);
                }
            }
            NutrientState::Digesting { .. } => {
                advance_digesting(nutrient, dt, &blobs, &mut vitality, &mut sound_events)
            }
            NutrientState::Expelling { .. } => advance_expelling(nutrient, dt, &blobs),
            NutrientState::Waste { velocity } => {
                nutrient.state = NutrientState::Waste { velocity };
            }
        }
    }
    sync_nutrient_bodies_after_digestion(&nutrition, &mut physics_nutrients);
    if let Some(probe) = interrupted_probe {
        nutrition.probe = Some(probe);
    }
}

fn movement_command(keyboard: &ButtonInput<KeyCode>) -> bool {
    keyboard.pressed(KeyCode::ArrowLeft)
        || keyboard.pressed(KeyCode::ArrowRight)
        || keyboard.pressed(KeyCode::KeyA)
        || keyboard.pressed(KeyCode::KeyD)
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn living_host<'a>(
    blobs: &'a BlobWorld,
    vitality: &VitalityWorld,
    id: u64,
) -> Option<&'a ActiveBlob> {
    blobs
        .active
        .iter()
        .find(|blob| blob.id == id && vitality.is_alive(id))
}

fn host_side(blob_id: u64) -> f32 {
    if blob_id & 1 == 0 { 1.0 } else { -1.0 }
}

#[cfg(test)]
mod tests {
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
        assert!(ENGULF_DURATION > 0.5);
        assert!(EXPULSION_DURATION > 0.3);
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
}
