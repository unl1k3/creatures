use super::*;
use crate::environment::WastewaterEffects;
use crate::level_format::NutrientDefinition;
use crate::palette;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};

const ENGULF_DURATION: f32 = 1.25;
const DIGESTION_DURATION: f32 = 6.0;
const EXPULSION_DURATION: f32 = 1.2;
const INTERNAL_WASTE_DRAG: f32 = 2.2;
// The procedural nutrient is an organic capsule whose nominal vertical extent
// is 88% of its logical radius. The same contact profile is used against every
// level collider to avoid an invisible gap around the rendered mesh.
const NUTRIENT_STRUCTURE_CONTACT_SCALE: f32 = 0.88;
const NUTRIENT_STRUCTURE_VISUAL_OFFSET: f32 = 5.0 * DEFAULT_CREATURE_SCALE;
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
    state: NutrientState,
    was_submerged: bool,
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
    pub(super) fn reset_from_definitions(&mut self, definitions: &[NutrientDefinition]) {
        self.probe = None;
        self.nutrients = definitions
            .iter()
            .map(|definition| Nutrient {
                position: definition.position,
                radius: definition.radius,
                original_radius: definition.radius,
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

fn protrusion_variation(blob_id: u64, salt: u32) -> f32 {
    let mut value = blob_id ^ (salt as u64).rotate_left(23) ^ 0xa076_1d64_78bd_642f;
    value ^= value >> 32;
    value = value.wrapping_mul(0xe703_7ed1_a0b4_28db);
    ((value >> 40) & 0xffff) as f32 / 65_535.0
}

pub(super) fn start_phagocytosis(
    keyboard: Res<ButtonInput<KeyCode>>,
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    vitality: Res<VitalityWorld>,
    mut nutrition: ResMut<NutritionWorld>,
) {
    if !keyboard.just_pressed(KeyCode::KeyC) || movement_command(&keyboard) {
        return;
    }
    let Some(blob) = blobs.active.get(blobs.selected) else {
        return;
    };
    if !vitality.is_alive(blob.id) || nutrition.is_digesting(blob.id) {
        return;
    }
    let center = blob.body.center();
    let nearest_direction = nutrition
        .nutrients
        .iter()
        .filter(|nutrient| matches!(nutrient.state, NutrientState::Available { .. }))
        .min_by(|a, b| {
            center
                .distance_squared(a.position)
                .total_cmp(&center.distance_squared(b.position))
        })
        .map(|nutrient| (nutrient.position - center).normalize_or(Vec2::X))
        .unwrap_or(Vec2::X);
    let candidate = nutrition
        .nutrients
        .iter()
        .enumerate()
        .filter(|(_, nutrient)| {
            matches!(nutrient.state, NutrientState::Available { .. })
                && nutrient.radius <= blob.body.rest_radius * 0.48
                && phagocytosis_path_clear(
                    center,
                    blob.body.rest_radius,
                    nutrient.position,
                    nutrient.radius,
                    &level,
                )
        })
        .map(|(index, nutrient)| {
            let gap =
                (center.distance(nutrient.position) - blob.body.rest_radius - nutrient.radius)
                    .max(0.0);
            (index, gap)
        })
        .filter(|(_, gap)| *gap <= PHAGOCYTOSIS_REACH)
        .min_by(|first, second| first.1.total_cmp(&second.1));
    nutrition.variation_serial = nutrition.variation_serial.wrapping_add(1);
    let variation = protrusion_variation(blob.id, nutrition.variation_serial as u32);
    let (anchor_edge, anchor_t) = membrane_anchor(&blob.body, center + nearest_direction * 100.0);
    let Some((index, reach)) = candidate else {
        nutrition.probe = Some(ExploratoryProbe {
            blob_id: blob.id,
            age: 0.0,
            extension: 0.0,
            direction: nearest_direction,
            tip: center + nearest_direction * blob.body.rest_radius,
            variation,
            anchor_edge,
            anchor_t,
        });
        return;
    };
    nutrition.probe = None;
    let nutrient = &mut nutrition.nutrients[index];
    nutrient.state = NutrientState::Engulfing {
        blob_id: blob.id,
        elapsed: 0.0,
        origin: nutrient.position,
        reach,
        probe_tip: center
            + (nutrient.position - center).normalize_or(Vec2::X) * blob.body.rest_radius,
        contact_elapsed: None,
        variation,
        anchor_edge,
        anchor_t,
    };
}

fn membrane_anchor(blob: &Blob, target: Vec2) -> (usize, f32) {
    let count = blob.particles.len();
    (0..count)
        .map(|index| {
            let start = blob.particles[index].position;
            let edge = blob.particles[(index + 1) % count].position - start;
            let t = ((target - start).dot(edge) / edge.length_squared().max(0.001)).clamp(0.0, 1.0);
            (index, t, (start + edge * t).distance_squared(target))
        })
        .min_by(|first, second| first.2.total_cmp(&second.2))
        .map(|(index, t, _)| (index, t))
        .unwrap_or((0, 0.5))
}

fn phagocytosis_path_clear(
    blob_center: Vec2,
    blob_radius: f32,
    nutrient_center: Vec2,
    nutrient_radius: f32,
    level: &Level,
) -> bool {
    let direction = (nutrient_center - blob_center).normalize_or(Vec2::X);
    let start = blob_center + direction * (blob_radius + 2.0);
    let end = nutrient_center - direction * (nutrient_radius + 2.0);
    if start.distance_squared(end) <= 1.0 {
        return true;
    }
    (1..10).all(|sample| {
        let point = start.lerp(end, sample as f32 / 10.0);
        !level.platforms.iter().any(|platform| {
            let delta = point - platform.center;
            delta.x.abs() < platform.half_size.x && delta.y.abs() < platform.half_size.y
        }) && !level
            .fixtures
            .iter()
            .any(|vertices| point_inside_convex(point, vertices))
    })
}

fn point_inside_convex(point: Vec2, vertices: &[Vec2]) -> bool {
    if vertices.len() < 3 {
        return false;
    }
    let mut sign = 0.0_f32;
    for (a, b) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
        let cross = (*b - *a).perp_dot(point - *a);
        if cross.abs() <= 0.001 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    sign != 0.0
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
    update_nutrient_mesh(&mut nutrient_mesh, &nutrition.nutrients, slots, 0.0);
    commands.insert_resource(nutrition);
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
    mut blobs: ResMut<BlobWorld>,
    level: Res<Level>,
    mut vitality: ResMut<VitalityWorld>,
    mut nutrition: ResMut<NutritionWorld>,
    mut wastewater_effects: ResMut<WastewaterEffects>,
) {
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();
    let rolling_command = movement_command(&keyboard);
    if let Some(mut probe) = nutrition.probe {
        if let Some(blob) = living_host(&blobs, &vitality, probe.blob_id) {
            probe.age += dt;
            if keyboard.pressed(KeyCode::KeyC) && !rolling_command {
                probe.extension = (probe.extension + dt / 0.48).min(1.0);
            } else {
                probe.extension = (probe.extension - dt / 0.34).max(0.0);
            }
            let sweep = (probe.age * 5.4).sin() * 0.38;
            let direction = Vec2::from_angle(sweep).rotate(probe.direction);
            let desired_tip = blob.body.center()
                + direction * (blob.body.rest_radius + PHAGOCYTOSIS_REACH * probe.extension);
            probe.tip = constrain_protrusion_load(
                &blob.body,
                blob.id,
                &blobs,
                desired_tip,
                4.2,
                smoothstep(probe.extension),
                probe.variation,
                probe.anchor_edge,
                probe.anchor_t,
                &level,
            );
            nutrition.probe = (probe.extension > 0.001
                || (keyboard.pressed(KeyCode::KeyC) && !rolling_command))
                .then_some(probe);
        } else {
            nutrition.probe = None;
        }
    }
    if keyboard.pressed(KeyCode::KeyC)
        && !rolling_command
        && let Some(probe) = nutrition.probe
        && probe.extension > 0.88
        && let Some(blob) = living_host(&blobs, &vitality, probe.blob_id)
    {
        let contact = nutrition
            .nutrients
            .iter()
            .enumerate()
            .filter(|(_, nutrient)| {
                matches!(nutrient.state, NutrientState::Available { .. })
                    && nutrient.radius <= blob.body.rest_radius * 0.48
                    && probe.tip.distance(nutrient.position) <= nutrient.radius + 4.2
                    && phagocytosis_path_clear(
                        blob.body.center(),
                        blob.body.rest_radius,
                        nutrient.position,
                        nutrient.radius,
                        &level,
                    )
            })
            .map(|(index, nutrient)| {
                (
                    index,
                    (blob.body.center().distance(nutrient.position)
                        - blob.body.rest_radius
                        - nutrient.radius)
                        .max(0.0),
                )
            })
            .next();
        if let Some((index, reach)) = contact {
            let nutrient = &mut nutrition.nutrients[index];
            nutrient.state = NutrientState::Engulfing {
                blob_id: blob.id,
                elapsed: 0.48,
                origin: nutrient.position,
                reach,
                probe_tip: probe.tip,
                contact_elapsed: Some(0.0),
                variation: probe.variation,
                anchor_edge: probe.anchor_edge,
                anchor_t: probe.anchor_t,
            };
            nutrition.probe = None;
        }
    }

    let mut interrupted_probe = None;
    for nutrient in &mut nutrition.nutrients {
        match nutrient.state {
            NutrientState::Available { mut velocity } => {
                integrate_free_object(
                    &mut nutrient.position,
                    nutrient.radius,
                    &mut velocity,
                    dt,
                    &level,
                );
                push_free_object_from_blobs(
                    &mut nutrient.position,
                    nutrient.radius * NUTRIENT_STRUCTURE_CONTACT_SCALE,
                    &mut velocity,
                    &blobs,
                );
                // Blob contact is resolved after free-object integration and
                // can push a small residue into its support. Level geometry
                // has final authority, so restore a valid position before the
                // frame ends.
                resolve_free_object_environment(
                    &mut nutrient.position,
                    nutrient.radius * NUTRIENT_STRUCTURE_CONTACT_SCALE,
                    &mut velocity,
                    &level,
                );
                apply_nutrient_water_interaction(
                    nutrient,
                    &mut velocity,
                    dt,
                    elapsed,
                    &level,
                    &mut wastewater_effects,
                );
                nutrient.state = NutrientState::Available { velocity };
            }
            NutrientState::Engulfing {
                blob_id,
                mut elapsed,
                origin,
                reach,
                probe_tip,
                mut contact_elapsed,
                variation,
                anchor_edge,
                anchor_t,
            } => {
                nutrient.was_submerged = false;
                let Some(blob) = living_host(&blobs, &vitality, blob_id) else {
                    make_waste(nutrient, Vec2::new(35.0, 80.0));
                    continue;
                };
                if rolling_command && contact_elapsed.is_none() {
                    interrupted_probe = Some(ExploratoryProbe {
                        blob_id,
                        age: 0.0,
                        extension: (elapsed / 0.48).clamp(0.0, 1.0),
                        direction: (probe_tip - blob.body.center()).normalize_or(Vec2::X),
                        tip: probe_tip,
                        variation,
                        anchor_edge,
                        anchor_t,
                    });
                    nutrient.state = NutrientState::Available {
                        velocity: Vec2::ZERO,
                    };
                    continue;
                }
                elapsed += dt;
                let extension = smoothstep((elapsed / 0.48).clamp(0.0, 1.0));
                let base_direction = (origin - blob.body.center()).normalize_or(Vec2::X);
                let angle = (elapsed * 7.2).sin() * 0.20 * (1.0 - extension * 0.72);
                let probing_direction = Vec2::from_angle(angle).rotate(base_direction);
                let mut probe_tip = blob.body.center()
                    + probing_direction
                        * (blob.body.rest_radius + reach * extension + nutrient.radius * 0.62);
                let grip = contact_elapsed
                    .map(|value| (value / 0.22).clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                probe_tip = constrain_protrusion_load(
                    &blob.body,
                    blob.id,
                    &blobs,
                    probe_tip,
                    (nutrient.radius * (0.34 + grip * 0.22)).max(3.2),
                    extension,
                    variation,
                    anchor_edge,
                    anchor_t,
                    &level,
                );
                if contact_elapsed.is_none()
                    && probe_tip.distance(origin) <= nutrient.radius * 0.82 + 3.2
                {
                    contact_elapsed = Some(0.0);
                }
                if let Some(value) = &mut contact_elapsed {
                    *value += dt;
                }
                let pull = contact_elapsed
                    .map(|value| (value / ENGULF_DURATION).clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                let smooth = smoothstep(pull);
                let target = blob.body.center()
                    + Vec2::new(
                        host_side(blob_id) * blob.body.rest_radius * 0.30,
                        blob.body.rest_radius * 0.06,
                    );
                nutrient.position = origin.lerp(target, smooth);
                if contact_elapsed.is_some() {
                    // Once attached, the nutrient becomes the rounded end of the
                    // proboscis and travels inward with the retracting membrane.
                    probe_tip = nutrient.position;
                }
                let crossed_membrane = contact_elapsed.is_some()
                    && nutrient.position.distance(blob.body.center())
                        <= blob.body.rest_radius - nutrient.radius * 0.35;
                nutrient.state = if crossed_membrane {
                    NutrientState::Digesting {
                        blob_id,
                        elapsed: 0.0,
                        local_position: nutrient.position - blob.body.center(),
                        velocity: Vec2::new(0.0, -18.0),
                    }
                } else {
                    NutrientState::Engulfing {
                        blob_id,
                        elapsed,
                        origin,
                        reach,
                        probe_tip,
                        contact_elapsed,
                        variation,
                        anchor_edge,
                        anchor_t,
                    }
                };
            }
            NutrientState::Digesting {
                blob_id,
                mut elapsed,
                mut local_position,
                mut velocity,
            } => {
                nutrient.was_submerged = false;
                let Some(blob) = living_host(&blobs, &vitality, blob_id) else {
                    make_waste(nutrient, Vec2::new(35.0, 80.0));
                    continue;
                };
                elapsed += dt;
                let progress = (elapsed / DIGESTION_DURATION).clamp(0.0, 1.0);
                nutrient.radius = nutrient.original_radius * (1.0 - progress * 0.38);
                velocity.y -= 150.0 * dt;
                velocity *= 0.992;
                local_position += velocity * dt;
                let internal_limit = (blob.body.rest_radius - nutrient.radius - 3.0).max(2.0);
                if local_position.length_squared() > internal_limit * internal_limit {
                    let normal = local_position.normalize_or(Vec2::Y);
                    local_position = normal * internal_limit;
                    let outward_speed = velocity.dot(normal);
                    if outward_speed > 0.0 {
                        velocity -= normal * outward_speed * 1.18;
                    }
                    velocity *= 0.76;
                }
                let world_x = blob.body.center().x + local_position.x;
                let membrane_bottom = membrane_lower_boundary(&blob.body, world_x);
                let minimum_world_y = membrane_bottom + nutrient.radius + 2.0;
                let proposed_world_y = blob.body.center().y + local_position.y;
                if proposed_world_y < minimum_world_y {
                    local_position.y = minimum_world_y - blob.body.center().y;
                    if velocity.y < 0.0 {
                        velocity.y *= -0.12;
                        velocity.x *= 0.82;
                    }
                }
                nutrient.position = blob.body.center() + local_position;
                vitality.restore_energy(blob_id, ENERGY_YIELD / DIGESTION_DURATION * dt);
                if progress >= 1.0 {
                    let (direction, launch_speed) = expulsion_launch(blob_id, nutrient.position);
                    nutrient.state = NutrientState::Expelling {
                        blob_id,
                        elapsed: 0.0,
                        velocity: blob.body.velocity() * 24.0 + direction * launch_speed,
                    };
                } else {
                    nutrient.state = NutrientState::Digesting {
                        blob_id,
                        elapsed,
                        local_position,
                        velocity,
                    };
                }
            }
            NutrientState::Expelling {
                blob_id,
                mut elapsed,
                mut velocity,
            } => {
                let Some(blob) = blobs.active.iter().find(|blob| blob.id == blob_id) else {
                    make_waste(nutrient, velocity);
                    continue;
                };
                elapsed += dt;
                nutrient.radius = nutrient.original_radius * 0.42;
                velocity *= (-INTERNAL_WASTE_DRAG * dt).exp();
                velocity.y -= OBJECT_GRAVITY * 0.10 * dt;
                nutrient.position += velocity * dt;
                let outside =
                    circle_outside_blob_membrane(nutrient.position, nutrient.radius, &blob.body);
                nutrient.state = if outside {
                    NutrientState::Waste { velocity }
                } else {
                    NutrientState::Expelling {
                        blob_id,
                        elapsed,
                        velocity,
                    }
                };
            }
            NutrientState::Waste { mut velocity } => {
                let contact_radius = nutrient.radius * NUTRIENT_STRUCTURE_CONTACT_SCALE;
                integrate_free_object(
                    &mut nutrient.position,
                    nutrient.radius,
                    &mut velocity,
                    dt,
                    &level,
                );
                push_free_object_from_blobs(
                    &mut nutrient.position,
                    contact_radius,
                    &mut velocity,
                    &blobs,
                );
                resolve_free_object_environment(
                    &mut nutrient.position,
                    contact_radius,
                    &mut velocity,
                    &level,
                );
                apply_nutrient_water_interaction(
                    nutrient,
                    &mut velocity,
                    dt,
                    elapsed,
                    &level,
                    &mut wastewater_effects,
                );
                nutrient.state = NutrientState::Waste { velocity };
            }
        }
    }
    resolve_free_nutrient_collisions(&mut nutrition.nutrients);
    // Nutrient-to-nutrient separation is the last interaction and can itself
    // displace a small waste object into a nearby platform. Enforce the level
    // boundary once more so no later operation can leave it embedded.
    resolve_all_free_nutrients_environment(&mut nutrition.nutrients, &level);
    resolve_blobs_from_supported_waste(&mut blobs, &nutrition.nutrients, &level);
    if let Some(probe) = interrupted_probe {
        nutrition.probe = Some(probe);
    }
}

fn resolve_blobs_from_supported_waste(
    blobs: &mut BlobWorld,
    nutrients: &[Nutrient],
    level: &Level,
) {
    for nutrient in nutrients {
        if !matches!(nutrient.state, NutrientState::Waste { .. }) {
            continue;
        }
        let radius = free_nutrient_contact_radius(nutrient);
        if !free_object_touches_environment(nutrient.position, radius, level) {
            continue;
        }
        for active_blob in &mut blobs.active {
            // A supported residue cannot be displaced into its support. In
            // that constrained case the reaction is applied to the blob,
            // making the residue behave like a small scenario object.
            for _ in 0..2 {
                let Some((penetration, nutrient_escape_normal)) =
                    circle_blob_penetration(nutrient.position, radius, &active_blob.body)
                else {
                    break;
                };
                let support_normal = -nutrient_escape_normal;
                active_blob
                    .body
                    .translate(support_normal * (penetration + 0.05));
                let inward_speed = active_blob.body.velocity().dot(support_normal);
                if inward_speed < 0.0 {
                    active_blob
                        .body
                        .add_velocity(-support_normal * inward_speed);
                }
            }
        }
    }
}

fn free_object_touches_environment(position: Vec2, radius: f32, level: &Level) -> bool {
    const CONTACT_TOLERANCE: f32 = 0.6;
    level.platforms.iter().any(|platform| {
        circle_aabb_penetration(
            position,
            radius + CONTACT_TOLERANCE,
            platform.center,
            platform.half_size + Vec2::splat(NUTRIENT_STRUCTURE_VISUAL_OFFSET),
        )
        .is_some()
    }) || level.fixtures.iter().any(|vertices| {
        circle_convex_penetration(
            position,
            radius + NUTRIENT_STRUCTURE_VISUAL_OFFSET + CONTACT_TOLERANCE,
            vertices,
        )
        .is_some()
    })
}

fn resolve_all_free_nutrients_environment(nutrients: &mut [Nutrient], level: &Level) {
    for nutrient in nutrients {
        let contact_radius = free_nutrient_contact_radius(nutrient);
        let position = &mut nutrient.position;
        let velocity = match &mut nutrient.state {
            NutrientState::Available { velocity } | NutrientState::Waste { velocity } => velocity,
            _ => continue,
        };
        resolve_free_object_environment(position, contact_radius, velocity, level);
    }
}

fn resolve_free_nutrient_collisions(nutrients: &mut [Nutrient]) {
    for first_index in 0..nutrients.len() {
        let (before_second, from_second) = nutrients.split_at_mut(first_index + 1);
        let first = &mut before_second[first_index];
        let first_contact_radius = free_nutrient_contact_radius(first);
        let Some(first_velocity) = free_nutrient_velocity(&mut first.state) else {
            continue;
        };
        for (offset, second) in from_second.iter_mut().enumerate() {
            let second_contact_radius = free_nutrient_contact_radius(second);
            let Some(second_velocity) = free_nutrient_velocity(&mut second.state) else {
                continue;
            };
            let delta = second.position - first.position;
            let minimum_distance = first_contact_radius + second_contact_radius;
            if delta.length_squared() >= minimum_distance * minimum_distance {
                continue;
            }
            let fallback = if (first_index + offset) & 1 == 0 {
                Vec2::X
            } else {
                Vec2::Y
            };
            let normal = delta.normalize_or(fallback);
            let distance = delta.length();
            let first_mass = first.radius * first.radius;
            let second_mass = second.radius * second.radius;
            let total_mass = (first_mass + second_mass).max(0.001);
            let overlap = minimum_distance - distance;
            first.position -= normal * overlap * second_mass / total_mass;
            second.position += normal * overlap * first_mass / total_mass;

            let relative_speed = (*second_velocity - *first_velocity).dot(normal);
            if relative_speed < 0.0 {
                const RESTITUTION: f32 = 0.12;
                let impulse =
                    -(1.0 + RESTITUTION) * relative_speed / (1.0 / first_mass + 1.0 / second_mass);
                *first_velocity -= normal * impulse / first_mass;
                *second_velocity += normal * impulse / second_mass;
            }
        }
    }
}

fn free_nutrient_contact_radius(nutrient: &Nutrient) -> f32 {
    if matches!(nutrient.state, NutrientState::Waste { .. }) {
        nutrient.radius * NUTRIENT_STRUCTURE_CONTACT_SCALE
    } else {
        nutrient.radius
    }
}

fn free_nutrient_velocity(state: &mut NutrientState) -> Option<&mut Vec2> {
    match state {
        NutrientState::Available { velocity } | NutrientState::Waste { velocity } => Some(velocity),
        _ => None,
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

#[allow(clippy::too_many_arguments)]
fn constrain_protrusion_load(
    blob: &Blob,
    host_id: u64,
    blobs: &BlobWorld,
    desired: Vec2,
    load_radius: f32,
    strength: f32,
    variation: f32,
    anchor_edge: usize,
    anchor_t: f32,
    level: &Level,
) -> Vec2 {
    if strength <= 0.01
        || !protrusion_intersects_environment(
            blob,
            host_id,
            blobs,
            desired,
            load_radius,
            strength,
            variation,
            anchor_edge,
            anchor_t,
            level,
        )
    {
        return desired;
    }
    let base = blob.particles[(anchor_edge + 1) % blob.particles.len()].position;
    let mut clear = 0.0;
    let mut blocked = 1.0;
    for _ in 0..10 {
        let candidate = (clear + blocked) * 0.5;
        let position = base.lerp(desired, candidate);
        if protrusion_intersects_environment(
            blob,
            host_id,
            blobs,
            position,
            load_radius,
            strength,
            variation,
            anchor_edge,
            anchor_t,
            level,
        ) {
            blocked = candidate;
        } else {
            clear = candidate;
        }
    }
    base.lerp(desired, clear)
}

#[allow(clippy::too_many_arguments)]
fn protrusion_intersects_environment(
    blob: &Blob,
    host_id: u64,
    blobs: &BlobWorld,
    load_position: Vec2,
    load_radius: f32,
    strength: f32,
    variation: f32,
    anchor_edge: usize,
    anchor_t: f32,
    level: &Level,
) -> bool {
    const SAMPLES: usize = 22;
    let count = blob.particles.len();
    let edge = anchor_edge % count;
    let start = blob.particles[edge].position;
    let base = blob.particles[(edge + 1) % count].position;
    let end = blob.particles[(edge + 2) % count].position;
    let tip = base.lerp(load_position, strength.clamp(0.0, 1.0));
    let length = base.distance(tip);
    if length < 0.5 {
        return false;
    }
    let load_direction = (load_position - blob.center()).normalize_or(Vec2::X);
    let normal_axis = load_direction.perp();
    let secondary = (variation * 7.137).fract();
    let asymmetry = (anchor_t.clamp(0.0, 1.0) - 0.5) * 0.08;
    let start_attachment = start.lerp(base, 0.18 + asymmetry);
    let end_attachment = base.lerp(end, 0.82 + asymmetry);
    let tangent = (end_attachment - start_attachment).normalize_or(Vec2::X);
    let mut root_normal = tangent.perp();
    if root_normal.dot(base - blob.center()) < 0.0 {
        root_normal = -root_normal;
    }
    let control_a = base
        + root_normal * length * (0.30 + variation * 0.08)
        + tangent * length * (variation - 0.5) * 0.05;
    let control_b = base.lerp(tip, 0.72) + normal_axis * length * (secondary - 0.5) * 0.18;
    let maximum_width = start_attachment.distance(end_attachment) * 0.5;
    let width = (load_radius * (0.55 + strength * 0.45) * (0.88 + variation * 0.24))
        .min(start.distance(end) * 0.48)
        .max(0.5);
    (4..=SAMPLES).any(|sample| {
        let along = sample as f32 / SAMPLES as f32;
        let inverse: f32 = 1.0 - along;
        let centerline = base * inverse.powi(3)
            + control_a * 3.0 * inverse.powi(2) * along
            + control_b * 3.0 * inverse * along.powi(2)
            + tip * along.powi(3);
        let collision_radius = (width * (1.0 - along * 0.58).max(0.18))
            .min(maximum_width * (1.0 - along * 0.58).max(0.18))
            .max(1.2);
        level.platforms.iter().any(|platform| {
            circle_aabb_penetration(
                centerline,
                collision_radius,
                platform.center,
                platform.half_size,
            )
            .is_some()
        }) || level.fixtures.iter().any(|vertices| {
            circle_convex_penetration(centerline, collision_radius, vertices).is_some()
        }) || blobs.active.iter().any(|other| {
            other.id != host_id
                && circle_intersects_blob_membrane(centerline, collision_radius, &other.body)
        })
    })
}

fn circle_intersects_blob_membrane(center: Vec2, radius: f32, blob: &Blob) -> bool {
    point_inside_blob_membrane(center, blob)
        || blob
            .particles
            .iter()
            .zip(blob.particles.iter().cycle().skip(1))
            .any(|(first, second)| {
                let edge = second.position - first.position;
                let t = ((center - first.position).dot(edge) / edge.length_squared().max(0.001))
                    .clamp(0.0, 1.0);
                center.distance_squared(first.position + edge * t) < radius * radius
            })
}

fn membrane_lower_boundary(blob: &Blob, world_x: f32) -> f32 {
    let mut lower = f32::INFINITY;
    for (first, second) in blob
        .particles
        .iter()
        .zip(blob.particles.iter().cycle().skip(1))
    {
        let min_x = first.position.x.min(second.position.x);
        let max_x = first.position.x.max(second.position.x);
        if world_x < min_x || world_x > max_x {
            continue;
        }
        let dx = second.position.x - first.position.x;
        let t = if dx.abs() < 0.001 {
            0.5
        } else {
            ((world_x - first.position.x) / dx).clamp(0.0, 1.0)
        };
        lower = lower.min(first.position.y + (second.position.y - first.position.y) * t);
    }
    if lower.is_finite() {
        lower
    } else {
        blob.particles
            .iter()
            .map(|particle| particle.position.y)
            .fold(blob.center().y - blob.rest_radius, f32::min)
    }
}

fn point_inside_blob_membrane(point: Vec2, blob: &Blob) -> bool {
    let mut inside = false;
    for (first, second) in blob
        .particles
        .iter()
        .zip(blob.particles.iter().cycle().skip(1))
    {
        let a = first.position;
        let b = second.position;
        let crosses_y = (a.y > point.y) != (b.y > point.y);
        if crosses_y {
            let intersection_x = a.x + (point.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if point.x < intersection_x {
                inside = !inside;
            }
        }
    }
    inside
}

fn circle_outside_blob_membrane(center: Vec2, radius: f32, blob: &Blob) -> bool {
    if point_inside_blob_membrane(center, blob) {
        return false;
    }
    let nearest_edge = blob
        .particles
        .iter()
        .zip(blob.particles.iter().cycle().skip(1))
        .map(|(first, second)| {
            let edge = second.position - first.position;
            let t = ((center - first.position).dot(edge) / edge.length_squared().max(0.001))
                .clamp(0.0, 1.0);
            center.distance(first.position + edge * t)
        })
        .fold(f32::INFINITY, f32::min);
    nearest_edge >= radius * 0.92
}

fn expulsion_launch(blob_id: u64, position: Vec2) -> (Vec2, f32) {
    let mut value = blob_id
        ^ (position.x.to_bits() as u64).rotate_left(17)
        ^ (position.y.to_bits() as u64).rotate_left(39)
        ^ 0x9e37_79b9_7f4a_7c15;
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    let horizontal = ((value & 0xffff) as f32 / 65_535.0 - 0.5) * 1.25;
    let speed_variation = ((value >> 16) & 0xffff) as f32 / 65_535.0;
    (
        Vec2::new(horizontal, 1.0).normalize(),
        235.0 + speed_variation * 95.0,
    )
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

fn push_free_object_from_blobs(
    position: &mut Vec2,
    radius: f32,
    velocity: &mut Vec2,
    blobs: &BlobWorld,
) {
    for blob in &blobs.active {
        let Some((penetration, normal)) = circle_blob_penetration(*position, radius, &blob.body)
        else {
            continue;
        };
        *position += normal * penetration;

        // Remove only inward relative motion. This keeps a resting residue in
        // visible contact with a deforming membrane without repeatedly kicking
        // it away from the blob.
        let membrane_velocity = blob.body.velocity() * 28.0;
        let inward_speed = (*velocity - membrane_velocity).dot(normal);
        if inward_speed < 0.0 {
            *velocity -= normal * inward_speed;
        }
    }
}

/// Exact circle-versus-membrane correction used by expelled nutrients.
///
/// The old rest-radius approximation created a large invisible collision halo,
/// particularly around squashed or stretched blobs.
fn circle_blob_penetration(center: Vec2, radius: f32, blob: &Blob) -> Option<(f32, Vec2)> {
    let mut nearest_point = blob.center();
    let mut nearest_distance_squared = f32::INFINITY;
    for (first, second) in blob
        .particles
        .iter()
        .zip(blob.particles.iter().cycle().skip(1))
    {
        let edge = second.position - first.position;
        let along = ((center - first.position).dot(edge) / edge.length_squared().max(0.001))
            .clamp(0.0, 1.0);
        let point = first.position + edge * along;
        let distance_squared = center.distance_squared(point);
        if distance_squared < nearest_distance_squared {
            nearest_distance_squared = distance_squared;
            nearest_point = point;
        }
    }

    let distance = nearest_distance_squared.sqrt();
    if point_inside_blob_membrane(center, blob) {
        let normal =
            (nearest_point - center).normalize_or((center - blob.center()).normalize_or(Vec2::Y));
        Some((radius + distance, normal))
    } else if distance < radius {
        let normal =
            (center - nearest_point).normalize_or((center - blob.center()).normalize_or(Vec2::Y));
        Some((radius - distance, normal))
    } else {
        None
    }
}

fn make_waste(nutrient: &mut Nutrient, velocity: Vec2) {
    nutrient.radius = nutrient.original_radius * 0.42;
    nutrient.state = NutrientState::Waste { velocity };
}

fn integrate_free_object(
    position: &mut Vec2,
    radius: f32,
    velocity: &mut Vec2,
    dt: f32,
    level: &Level,
) {
    let contact_radius = radius * NUTRIENT_STRUCTURE_CONTACT_SCALE;
    velocity.y -= OBJECT_GRAVITY * dt;
    *velocity *= 0.995;
    let travel = velocity.length() * dt;
    let steps = (travel / (radius * 0.35).max(1.5)).ceil().clamp(1.0, 64.0) as usize;
    let step_dt = dt / steps as f32;
    for _ in 0..steps {
        *position += *velocity * step_dt;
        resolve_free_object_environment(position, contact_radius, velocity, level);
    }
}

fn resolve_free_object_environment(
    position: &mut Vec2,
    contact_radius: f32,
    velocity: &mut Vec2,
    level: &Level,
) {
    // Ink platforms are expanded by this skin in every direction so their
    // visible contour matches blob contact. Nutrients use the same contour.
    for platform in &level.platforms {
        if let Some((depth, normal)) = circle_aabb_penetration(
            *position,
            contact_radius,
            platform.center,
            platform.half_size + Vec2::splat(NUTRIENT_STRUCTURE_VISUAL_OFFSET),
        ) {
            *position += normal * depth;
            resolve_object_velocity(velocity, normal);
        }
    }
    for vertices in &level.fixtures {
        // Polygon artwork is expanded outward by the same visual offset.
        if let Some((depth, normal)) = circle_convex_penetration(
            *position,
            contact_radius + NUTRIENT_STRUCTURE_VISUAL_OFFSET,
            vertices,
        ) {
            *position += normal * depth;
            resolve_object_velocity(velocity, normal);
        }
    }
}

fn apply_nutrient_water_interaction(
    nutrient: &mut Nutrient,
    velocity: &mut Vec2,
    dt: f32,
    elapsed: f32,
    level: &Level,
    effects: &mut WastewaterEffects,
) {
    let entry_speed = (-velocity.y).max(0.0);
    let surface = apply_wastewater_buoyancy(
        &mut nutrient.position,
        nutrient.radius,
        velocity,
        dt,
        elapsed,
        level,
    );
    if let Some(surface_y) = surface
        && !nutrient.was_submerged
    {
        let strength = (entry_speed / 180.0).clamp(0.35, 1.35);
        effects.emit(
            Vec2::new(nutrient.position.x, surface_y),
            nutrient.radius,
            strength,
        );
    }
    nutrient.was_submerged = surface.is_some();
}

fn apply_wastewater_buoyancy(
    position: &mut Vec2,
    radius: f32,
    velocity: &mut Vec2,
    dt: f32,
    elapsed: f32,
    level: &Level,
) -> Option<f32> {
    for area in &level.wastewater_areas {
        if !area.contains_x(position.x) {
            continue;
        }
        let surface_y = area.surface_y(position.x, elapsed);
        let bottom_y = area.position.y - area.size.y * 0.5;
        if position.y - radius >= surface_y || position.y + radius <= bottom_y {
            continue;
        }

        let submerged = ((surface_y - (position.y - radius)) / (radius * 2.0)).clamp(0.0, 1.0);
        let previous_surface = area.surface_y(position.x, (elapsed - dt).max(0.0));
        let water_vertical_speed = (surface_y - previous_surface) / dt.max(0.000_001);
        let water_horizontal_speed =
            (position.x * 0.009 + elapsed * area.wave_speed * 0.7).sin() * 7.0;

        // A density ratio above two leaves roughly one third of the capsule
        // immersed. Drag makes it follow the wave without locking it to a
        // prescribed height.
        velocity.y += OBJECT_GRAVITY * 2.35 * submerged.powf(0.82) * dt;
        let drag = 1.0 - (-12.0 * submerged.sqrt() * dt).exp();
        velocity.x += (water_horizontal_speed - velocity.x) * drag * 0.72;
        velocity.y += (water_vertical_speed - velocity.y) * drag;
        return Some(surface_y);
    }
    None
}

fn resolve_object_velocity(velocity: &mut Vec2, normal: Vec2) {
    let normal_speed = velocity.dot(normal);
    if normal_speed < 0.0 {
        *velocity -= normal * normal_speed * 1.08;
        *velocity *= 0.82;
    }
}

fn circle_aabb_penetration(
    center: Vec2,
    radius: f32,
    box_center: Vec2,
    half_size: Vec2,
) -> Option<(f32, Vec2)> {
    let local = center - box_center;
    let closest = local.clamp(-half_size, half_size);
    let delta = local - closest;
    let distance = delta.length();
    if distance > 0.001 {
        return (distance < radius).then(|| (radius - distance, delta / distance));
    }
    let x_clearance = half_size.x - local.x.abs();
    let y_clearance = half_size.y - local.y.abs();
    if x_clearance < y_clearance {
        let side = if local.x >= 0.0 { 1.0 } else { -1.0 };
        Some((radius + x_clearance, Vec2::new(side, 0.0)))
    } else {
        let side = if local.y >= 0.0 { 1.0 } else { -1.0 };
        Some((radius + y_clearance, Vec2::new(0.0, side)))
    }
}

fn circle_convex_penetration(center: Vec2, radius: f32, vertices: &[Vec2]) -> Option<(f32, Vec2)> {
    if vertices.len() < 3 {
        return None;
    }
    let orientation = vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .map(|(a, b)| a.perp_dot(*b))
        .sum::<f32>()
        .signum();
    if orientation == 0.0 {
        return None;
    }
    let mut nearest = (f32::INFINITY, Vec2::Y, Vec2::Y);
    let mut inside = true;
    for (first, second) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
        let edge = *second - *first;
        let length = edge.length().max(0.001);
        inside &= edge.perp_dot(center - *first) * orientation >= 0.0;
        let t = ((center - *first).dot(edge) / edge.length_squared().max(0.001)).clamp(0.0, 1.0);
        let delta = center - (*first + edge * t);
        if delta.length() < nearest.0 {
            let outward = -edge.perp() * orientation / length;
            nearest = (delta.length(), outward, delta.normalize_or(outward));
        }
    }
    if inside {
        Some((radius + nearest.0, nearest.1))
    } else if nearest.0 < radius {
        Some((radius - nearest.0, nearest.2))
    } else {
        None
    }
}

pub(super) fn draw_nutrition(
    time: Res<Time>,
    nutrition: Res<NutritionWorld>,
    mut render_assets: ResMut<NutrientRenderAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(mut mesh) = meshes.get_mut(&render_assets.mesh) else {
        return;
    };
    render_assets.slots = render_assets.slots.max(nutrition.nutrients.len()).max(1);
    update_nutrient_mesh(
        &mut mesh,
        &nutrition.nutrients,
        render_assets.slots,
        time.elapsed_secs(),
    );
}

fn update_nutrient_mesh(mesh: &mut Mesh, nutrients: &[Nutrient], slots: usize, elapsed: f32) {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    for nutrient in nutrients {
        append_nutrient_mesh(nutrient, elapsed, &mut positions, &mut colors, &mut indices);
    }
    for _ in nutrients.len()..slots {
        append_hidden_nutrient_mesh(&mut positions, &mut colors, &mut indices);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}

fn append_hidden_nutrient_mesh(
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    let first_position = positions.len();
    let hidden = Nutrient {
        position: Vec2::ZERO,
        radius: 0.0,
        original_radius: 0.0,
        state: NutrientState::Waste {
            velocity: Vec2::ZERO,
        },
        was_submerged: false,
    };
    append_nutrient_mesh(&hidden, 0.0, positions, colors, indices);
    for color in &mut colors[first_position..] {
        color[3] = 0.0;
    }
}

fn empty_nutrient_mesh() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

fn append_nutrient_mesh(
    nutrient: &Nutrient,
    elapsed: f32,
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    const SIDES: usize = 20;
    let seed = nutrient.original_radius * 0.173
        + nutrient.position.x * 0.011
        + nutrient.position.y * 0.007;
    let (body, center, edge, energy, activity) = nutrient_palette(nutrient);
    let pulse_cycle = elapsed / 1.85 + seed * 0.13;
    let pulse_phase = pulse_cycle.fract();
    let pulse = (pulse_phase * std::f32::consts::PI).sin().powi(2) * activity;
    let lobe_angle = (pulse_cycle.floor() * 2.17 + seed * 1.31).rem_euclid(std::f32::consts::TAU);
    let pulsed_body = mix_rgba(body, energy, pulse * 0.15);
    let pulsed_center = mix_rgba(center, energy, pulse * 0.23);
    let first = positions.len() as u32;
    positions.push([nutrient.position.x, nutrient.position.y, 0.0]);
    colors.push(pulsed_center);

    let mut outline = Vec::with_capacity(SIDES);
    for index in 0..SIDES {
        let angle = index as f32 / SIDES as f32 * std::f32::consts::TAU;
        let irregularity =
            1.0 + (angle * 3.0 + seed).sin() * 0.065 + (angle * 5.0 - seed * 1.7).sin() * 0.032;
        let lobe = (angle - lobe_angle).cos().max(0.0).powi(2);
        let local = Vec2::new(angle.cos() * 0.94, angle.sin() * 0.88)
            * nutrient.radius
            * irregularity
            * (1.0 + pulse * lobe * 0.20);
        outline.push(nutrient.position + local);
    }
    for point in &outline {
        let inner = nutrient.position + (*point - nutrient.position) * 0.78;
        positions.push([inner.x, inner.y, 0.0]);
        colors.push(pulsed_body);
    }
    for point in &outline {
        positions.push([point.x, point.y, 0.0]);
        colors.push(edge);
    }
    for index in 0..SIDES {
        let next = (index + 1) % SIDES;
        let inner = first + 1 + index as u32;
        let inner_next = first + 1 + next as u32;
        let outer = first + 1 + SIDES as u32 + index as u32;
        let outer_next = first + 1 + SIDES as u32 + next as u32;
        indices.extend_from_slice(&[
            first, inner, inner_next, inner, outer, outer_next, inner, outer_next, inner_next,
        ]);
    }

    for nodule in 0..4 {
        let angle = seed * (0.71 + nodule as f32 * 0.13) + nodule as f32 * 1.67;
        let distance = nutrient.radius * (0.22 + nodule as f32 * 0.055);
        let nodule_center =
            nutrient.position + Vec2::new(angle.cos() * distance, angle.sin() * distance * 0.72);
        let radius = nutrient.radius
            * (0.075 + nodule as f32 * 0.012)
            * (1.0 + (elapsed * 2.2 + angle).sin() * 0.10 * activity + pulse * 0.24);
        append_nodule(
            nodule_center,
            radius,
            mix_rgba(energy, palette::NUTRIENT_HIGHLIGHT, pulse * 0.28),
            positions,
            colors,
            indices,
        );
    }
}

fn mix_rgba(first: [f32; 4], second: [f32; 4], amount: f32) -> [f32; 4] {
    let amount = amount.clamp(0.0, 1.0);
    [
        first[0] + (second[0] - first[0]) * amount,
        first[1] + (second[1] - first[1]) * amount,
        first[2] + (second[2] - first[2]) * amount,
        first[3] + (second[3] - first[3]) * amount,
    ]
}

fn append_nodule(
    center: Vec2,
    radius: f32,
    color: [f32; 4],
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
) {
    const SIDES: usize = 10;
    let first = positions.len() as u32;
    positions.push([center.x, center.y, 0.0]);
    colors.push(color);
    for index in 0..SIDES {
        let angle = index as f32 / SIDES as f32 * std::f32::consts::TAU;
        let point = center + Vec2::from_angle(angle) * radius;
        positions.push([point.x, point.y, 0.0]);
        colors.push([color[0] * 0.66, color[1] * 0.66, color[2] * 0.66, color[3]]);
    }
    for index in 0..SIDES {
        indices.extend_from_slice(&[
            first,
            first + index as u32 + 1,
            first + (index as u32 + 1) % SIDES as u32 + 1,
        ]);
    }
}

fn nutrient_palette(nutrient: &Nutrient) -> ([f32; 4], [f32; 4], [f32; 4], [f32; 4], f32) {
    match nutrient.state {
        NutrientState::Available { .. } => (
            palette::NUTRIENT_BODY,
            palette::NUTRIENT_CORE,
            palette::NUTRIENT_EDGE,
            palette::NUTRIENT_ENERGY,
            1.0,
        ),
        NutrientState::Engulfing {
            contact_elapsed, ..
        } => {
            let depth = contact_elapsed
                .map(|elapsed| smoothstep(elapsed / ENGULF_DURATION))
                .unwrap_or(0.0);
            (
                with_opacity(palette::NUTRIENT_ENGULFED_BODY, 1.0 - depth * 0.48),
                with_opacity(palette::NUTRIENT_ENGULFED_CORE, 1.0 - depth * 0.48),
                with_opacity(palette::NUTRIENT_ENGULFED_EDGE, 1.0 - depth * 0.38),
                with_opacity(palette::NUTRIENT_ENGULFED_ENERGY, 1.0 - depth * 0.25),
                0.65,
            )
        }
        NutrientState::Digesting { elapsed, .. } => {
            let progress = (elapsed / DIGESTION_DURATION).clamp(0.0, 1.0);
            (
                with_opacity(
                    mix_rgba(palette::NUTRIENT_BODY, palette::DIGESTED_BODY, progress),
                    0.48,
                ),
                with_opacity(
                    mix_rgba(palette::NUTRIENT_CORE, palette::DIGESTED_CORE, progress),
                    0.52,
                ),
                with_opacity(palette::DIGESTED_EDGE, 0.58),
                with_opacity(
                    mix_rgba(palette::NUTRIENT_ENERGY, palette::DIGESTED_ENERGY, progress),
                    0.72,
                ),
                1.0 - progress,
            )
        }
        NutrientState::Expelling { elapsed, .. } => {
            let visibility = 0.52 + smoothstep(elapsed / EXPULSION_DURATION) * 0.48;
            (
                with_opacity(palette::WASTE_BODY, visibility),
                with_opacity(palette::WASTE_CORE, visibility),
                with_opacity(palette::WASTE_EDGE, visibility),
                with_opacity(palette::WASTE_ENERGY, visibility),
                0.0,
            )
        }
        NutrientState::Waste { .. } => (
            palette::WASTE_BODY,
            palette::WASTE_CORE,
            palette::WASTE_EDGE,
            palette::WASTE_ENERGY,
            0.0,
        ),
    }
}

fn with_opacity(mut color: [f32; 4], opacity: f32) -> [f32; 4] {
    color[3] *= opacity.clamp(0.0, 1.0);
    color
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level_format::{NutrientDefinition, WastewaterAreaDefinition};

    #[test]
    fn nutrient_settles_near_the_animated_water_surface() {
        let mut level = Level::from_test_geometry(Vec::new(), Vec::new());
        level.wastewater_areas.push(WastewaterAreaDefinition {
            position: Vec2::new(0.0, -100.0),
            size: Vec2::new(500.0, 100.0),
            color: palette::DEFAULT_WASTEWATER,
            wave_height: 4.0,
            wave_speed: 0.4,
            depth: -0.12,
            bubbles: None,
        });
        let mut position = Vec2::new(0.0, 20.0);
        let mut velocity = Vec2::ZERO;
        let radius = 10.0;
        let dt = 1.0 / 120.0;
        for step in 0..1_200 {
            let elapsed = step as f32 * dt;
            integrate_free_object(&mut position, radius, &mut velocity, dt, &level);
            apply_wastewater_buoyancy(&mut position, radius, &mut velocity, dt, elapsed, &level);
        }

        let surface = level.wastewater_areas[0].surface_y(position.x, 1_200.0 * dt);
        assert!(position.y > surface - radius * 0.8);
        assert!(position.y < surface + radius * 1.1);
        assert!(velocity.length() < 35.0);
    }

    #[test]
    fn nutrient_emits_one_effect_when_crossing_the_surface() {
        let mut level = Level::from_test_geometry(Vec::new(), Vec::new());
        level.wastewater_areas.push(WastewaterAreaDefinition {
            position: Vec2::new(0.0, -100.0),
            size: Vec2::new(500.0, 100.0),
            color: palette::DEFAULT_WASTEWATER,
            wave_height: 4.0,
            wave_speed: 0.4,
            depth: -0.12,
            bubbles: None,
        });
        let surface = level.wastewater_areas[0].surface_y(0.0, 0.0);
        let mut nutrient = Nutrient {
            position: Vec2::new(0.0, surface + 8.0),
            radius: 10.0,
            original_radius: 10.0,
            state: NutrientState::Available {
                velocity: Vec2::new(0.0, -160.0),
            },
            was_submerged: false,
        };
        let mut velocity = Vec2::new(0.0, -160.0);
        let mut effects = WastewaterEffects::default();

        apply_nutrient_water_interaction(
            &mut nutrient,
            &mut velocity,
            1.0 / 120.0,
            0.0,
            &level,
            &mut effects,
        );
        apply_nutrient_water_interaction(
            &mut nutrient,
            &mut velocity,
            1.0 / 120.0,
            1.0 / 120.0,
            &level,
            &mut effects,
        );

        assert_eq!(effects.pending.len(), 1);
        assert_eq!(effects.ripples.len(), 1);
    }

    #[test]
    fn free_nutrients_separate_and_exchange_normal_velocity() {
        let mut nutrients = [
            Nutrient {
                position: Vec2::ZERO,
                radius: 10.0,
                original_radius: 10.0,
                state: NutrientState::Available {
                    velocity: Vec2::new(20.0, 0.0),
                },
                was_submerged: false,
            },
            Nutrient {
                position: Vec2::new(12.0, 0.0),
                radius: 8.0,
                original_radius: 8.0,
                state: NutrientState::Available {
                    velocity: Vec2::new(-5.0, 0.0),
                },
                was_submerged: false,
            },
        ];

        resolve_free_nutrient_collisions(&mut nutrients);

        assert!(nutrients[0].position.distance(nutrients[1].position) >= 18.0 - 0.001);
        let NutrientState::Available { velocity: first } = nutrients[0].state else {
            unreachable!();
        };
        let NutrientState::Available { velocity: second } = nutrients[1].state else {
            unreachable!();
        };
        assert!(first.x < 20.0);
        assert!(second.x > -5.0);
    }

    #[test]
    fn digested_nutrients_use_their_visible_contact_radius() {
        let radius = 10.0;
        let visible_radius = radius * NUTRIENT_STRUCTURE_CONTACT_SCALE;
        let mut nutrients = [
            Nutrient {
                position: Vec2::ZERO,
                radius,
                original_radius: 24.0,
                state: NutrientState::Waste {
                    velocity: Vec2::ZERO,
                },
                was_submerged: false,
            },
            Nutrient {
                position: Vec2::new(visible_radius * 2.0 + 0.1, 0.0),
                radius,
                original_radius: 24.0,
                state: NutrientState::Waste {
                    velocity: Vec2::ZERO,
                },
                was_submerged: false,
            },
        ];
        let positions_before = [nutrients[0].position, nutrients[1].position];

        resolve_free_nutrient_collisions(&mut nutrients);

        assert_eq!(nutrients[0].position, positions_before[0]);
        assert_eq!(nutrients[1].position, positions_before[1]);
    }

    #[test]
    fn nutrient_render_is_a_filled_capsule_with_energy_nodules() {
        let nutrient = Nutrient {
            position: Vec2::new(12.0, -8.0),
            radius: 14.0,
            original_radius: 14.0,
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
            state: NutrientState::Available {
                velocity: Vec2::ZERO,
            },
            was_submerged: false,
        };
        let mut mesh = empty_nutrient_mesh();
        update_nutrient_mesh(&mut mesh, &[nutrient], 1, 0.0);
        let live_vertices = mesh.count_vertices();
        let live_indices = mesh.indices().expect("live nutrient indices").len();

        update_nutrient_mesh(&mut mesh, &[], 1, 1.0);

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
    fn nutrient_structure_contact_matches_its_visible_capsule_height() {
        let radius = 10.0;
        let contact_radius = radius * NUTRIENT_STRUCTURE_CONTACT_SCALE;
        let platform_center = Vec2::ZERO;
        let platform_half_size = Vec2::new(30.0, 5.0);
        let touching_center = Vec2::new(0.0, platform_half_size.y + contact_radius);

        assert!(
            circle_aabb_penetration(
                touching_center + Vec2::Y * 0.01,
                contact_radius,
                platform_center,
                platform_half_size,
            )
            .is_none()
        );
        assert!(
            circle_aabb_penetration(
                touching_center - Vec2::Y * 0.01,
                contact_radius,
                platform_center,
                platform_half_size,
            )
            .is_some()
        );

        let trapezoid = [
            Vec2::new(-30.0, -5.0),
            Vec2::new(30.0, -5.0),
            Vec2::new(20.0, 5.0),
            Vec2::new(-20.0, 5.0),
        ];
        assert!(circle_convex_penetration(touching_center, contact_radius, &trapezoid).is_none());
        assert!(
            circle_convex_penetration(
                touching_center - Vec2::Y * 0.01,
                contact_radius,
                &trapezoid,
            )
            .is_some()
        );
    }

    #[test]
    fn final_environment_pass_prevents_waste_from_being_pushed_below_a_platform() {
        let platform = Platform {
            center: Vec2::ZERO,
            half_size: Vec2::new(40.0, 5.0),
        };
        let level = Level::from_test_geometry(vec![platform], Vec::new());
        let contact_radius = 4.0;
        let visual_top =
            platform.center.y + NUTRIENT_STRUCTURE_VISUAL_OFFSET + platform.half_size.y;
        let mut position = Vec2::new(0.0, visual_top + contact_radius - 2.0);
        let mut velocity = Vec2::new(0.0, -30.0);

        resolve_free_object_environment(&mut position, contact_radius, &mut velocity, &level);

        assert!(position.y - contact_radius >= visual_top - 0.001);
        assert!(velocity.y >= -0.001);
    }

    #[test]
    fn supported_waste_pushes_back_on_a_blob_when_it_cannot_escape() {
        let platform = Platform {
            center: Vec2::ZERO,
            half_size: Vec2::new(40.0, 5.0),
        };
        let level = Level::from_test_geometry(vec![platform], Vec::new());
        let radius = 4.0;
        let visual_top =
            platform.center.y + NUTRIENT_STRUCTURE_VISUAL_OFFSET + platform.half_size.y;
        let nutrient = Nutrient {
            position: Vec2::new(0.0, visual_top + radius),
            radius: radius / NUTRIENT_STRUCTURE_CONTACT_SCALE,
            original_radius: 10.0,
            state: NutrientState::Waste {
                velocity: Vec2::ZERO,
            },
            was_submerged: false,
        };
        let mut world = BlobWorld {
            active: vec![ActiveBlob {
                id: 0,
                parent_id: None,
                body: Blob::new(Vec2::new(0.0, visual_top + radius + 22.0), 20.0),
            }],
            selected: 0,
            rejoin_parent: None,
            rejoin_elapsed: 0.0,
            parent_links: HashMap::new(),
            next_id: 1,
        };
        assert!(
            circle_blob_penetration(nutrient.position, radius, &world.active[0].body).is_some()
        );

        resolve_blobs_from_supported_waste(&mut world, &[nutrient], &level);

        assert!(
            circle_blob_penetration(nutrient.position, radius, &world.active[0].body).is_none()
        );
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
