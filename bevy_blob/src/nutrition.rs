use super::*;

const ENGULF_DURATION: f32 = 1.25;
const DIGESTION_DURATION: f32 = 6.0;
const EXPULSION_DURATION: f32 = 1.2;
const INTERNAL_WASTE_DRAG: f32 = 2.2;
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
}

#[derive(Resource, Default)]
pub(super) struct NutritionWorld {
    nutrients: Vec<Nutrient>,
    probe: Option<ExploratoryProbe>,
    variation_serial: u64,
}

impl NutritionWorld {
    pub(super) fn reset_near(&mut self, spawn: Vec2) {
        self.probe = None;
        self.nutrients = [
            (Vec2::new(145.0, 65.0), 13.0),
            (Vec2::new(-175.0, 210.0), 11.0),
            (Vec2::new(210.0, 320.0), 15.0),
        ]
        .into_iter()
        .map(|(offset, radius)| Nutrient {
            position: spawn + offset,
            radius,
            original_radius: radius,
            state: NutrientState::Available {
                velocity: Vec2::ZERO,
            },
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

pub(super) fn setup_nutrition(mut commands: Commands) {
    let mut nutrition = NutritionWorld::default();
    nutrition.reset_near(BLOB_START);
    commands.insert_resource(nutrition);
}

pub(super) fn simulate_nutrition(
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    mut vitality: ResMut<VitalityWorld>,
    mut nutrition: ResMut<NutritionWorld>,
) {
    let dt = time.delta_secs();
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
                    nutrient.radius,
                    &mut velocity,
                    &blobs,
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
                integrate_free_object(
                    &mut nutrient.position,
                    nutrient.radius,
                    &mut velocity,
                    dt,
                    &level,
                );
                push_free_object_from_blobs(
                    &mut nutrient.position,
                    nutrient.radius,
                    &mut velocity,
                    &blobs,
                );
                nutrient.state = NutrientState::Waste { velocity };
            }
        }
    }
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
        let delta = *position - blob.body.center();
        let minimum = blob.body.rest_radius + radius;
        if delta.length_squared() < minimum * minimum {
            let normal = delta.normalize_or(Vec2::Y);
            *position = blob.body.center() + normal * minimum;
            *velocity += blob.body.velocity() * 28.0 + normal * 8.0;
        }
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
    velocity.y -= OBJECT_GRAVITY * dt;
    *velocity *= 0.995;
    let travel = velocity.length() * dt;
    let steps = (travel / (radius * 0.35).max(1.5)).ceil().clamp(1.0, 64.0) as usize;
    let step_dt = dt / steps as f32;
    for _ in 0..steps {
        *position += *velocity * step_dt;
        for platform in &level.platforms {
            if let Some((depth, normal)) =
                circle_aabb_penetration(*position, radius, platform.center, platform.half_size)
            {
                *position += normal * depth;
                resolve_object_velocity(velocity, normal);
            }
        }
        for vertices in &level.fixtures {
            if let Some((depth, normal)) = circle_convex_penetration(*position, radius, vertices) {
                *position += normal * depth;
                resolve_object_velocity(velocity, normal);
            }
        }
    }
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

pub(super) fn draw_nutrition(mut gizmos: Gizmos, nutrition: Res<NutritionWorld>) {
    for nutrient in &nutrition.nutrients {
        let (outer, inner) = match nutrient.state {
            NutrientState::Available { .. } => (
                Color::srgba(0.94, 0.72, 0.18, 0.98),
                Color::srgba(1.0, 0.92, 0.45, 0.95),
            ),
            NutrientState::Engulfing { .. } => (
                Color::srgba(0.82, 0.58, 0.16, 0.96),
                Color::srgba(0.96, 0.78, 0.30, 0.90),
            ),
            NutrientState::Digesting { elapsed, .. } => {
                let p = (elapsed / DIGESTION_DURATION).clamp(0.0, 1.0);
                (
                    Color::srgba(0.75 - p * 0.28, 0.48, 0.16, 0.92),
                    Color::srgba(0.96 - p * 0.38, 0.72 - p * 0.20, 0.24, 0.86),
                )
            }
            NutrientState::Expelling { .. } | NutrientState::Waste { .. } => (
                Color::srgba(0.30, 0.20, 0.12, 0.98),
                Color::srgba(0.48, 0.32, 0.17, 0.92),
            ),
        };
        gizmos.circle_2d(nutrient.position, nutrient.radius, outer);
        gizmos.circle_2d(nutrient.position, nutrient.radius * 0.48, inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
