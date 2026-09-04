//! Fixed-step transitions that drive a nutrient through digestion.

use super::*;

/// Extends or retracts the exploratory probe and promotes a touched nutrient
/// to the initial engulfing state.
pub(super) fn advance_probe_and_capture(
    dt: f32,
    keyboard: &ButtonInput<KeyCode>,
    rolling_command: bool,
    blobs: &BlobWorld,
    level: &Level,
    vitality: &VitalityWorld,
    nutrition: &mut NutritionWorld,
) {
    if let Some(mut probe) = nutrition.probe {
        if let Some(blob) = super::living_host(blobs, vitality, probe.blob_id) {
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
            probe.tip = super::constrain_protrusion_load(
                &blob.body,
                blob.id,
                blobs,
                desired_tip,
                4.2,
                super::smoothstep(probe.extension),
                probe.variation,
                probe.anchor_edge,
                probe.anchor_t,
                level,
            );
            nutrition.probe = (probe.extension > 0.001
                || (keyboard.pressed(KeyCode::KeyC) && !rolling_command))
                .then_some(probe);
        } else {
            nutrition.probe = None;
        }
    }

    if !(keyboard.pressed(KeyCode::KeyC) && !rolling_command) {
        return;
    }
    let Some(probe) = nutrition.probe else {
        return;
    };
    if probe.extension <= 0.88 {
        return;
    }
    let Some(blob) = super::living_host(blobs, vitality, probe.blob_id) else {
        return;
    };

    let contact = nutrition
        .nutrients
        .iter()
        .enumerate()
        .find(|(_, nutrient)| {
            nutrient.is_edible()
                && nutrient.radius <= blob.body.rest_radius * 0.48
                && probe.tip.distance(nutrient.position) <= nutrient.radius + 4.2
                && super::phagocytosis_path_clear(
                    blob.body.center(),
                    blob.body.rest_radius,
                    nutrient.position,
                    nutrient.radius,
                    level,
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
        });
    let Some((index, reach)) = contact else {
        return;
    };

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

/// Advances an attached probe until it crosses the membrane, becomes an
/// internal nutrient, or is interrupted by player movement.
pub(super) fn advance_engulfing(
    nutrient: &mut Nutrient,
    dt: f32,
    rolling_command: bool,
    blobs: &BlobWorld,
    level: &Level,
    vitality: &VitalityWorld,
    sound_events: &mut MessageWriter<BlobSoundEvent>,
) -> Option<ExploratoryProbe> {
    let NutrientState::Engulfing {
        blob_id,
        mut elapsed,
        origin,
        reach,
        probe_tip,
        mut contact_elapsed,
        variation,
        anchor_edge,
        anchor_t,
    } = nutrient.state
    else {
        return None;
    };

    nutrient.was_submerged = false;
    let Some(blob) = super::living_host(blobs, vitality, blob_id) else {
        super::feeding::make_waste(nutrient, Vec2::new(35.0, 80.0));
        return None;
    };
    if rolling_command && contact_elapsed.is_none() {
        let probe = ExploratoryProbe {
            blob_id,
            age: 0.0,
            extension: (elapsed / 0.48).clamp(0.0, 1.0),
            direction: (probe_tip - blob.body.center()).normalize_or(Vec2::X),
            tip: probe_tip,
            variation,
            anchor_edge,
            anchor_t,
        };
        nutrient.state = NutrientState::Available {
            velocity: Vec2::ZERO,
        };
        return Some(probe);
    }

    elapsed += dt;
    let extension = super::smoothstep((elapsed / 0.48).clamp(0.0, 1.0));
    let base_direction = (origin - blob.body.center()).normalize_or(Vec2::X);
    let angle = (elapsed * 7.2).sin() * 0.20 * (1.0 - extension * 0.72);
    let probing_direction = Vec2::from_angle(angle).rotate(base_direction);
    let mut probe_tip = blob.body.center()
        + probing_direction * (blob.body.rest_radius + reach * extension + nutrient.radius * 0.62);
    let grip = contact_elapsed
        .map(|value| (value / 0.22).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    probe_tip = super::constrain_protrusion_load(
        &blob.body,
        blob.id,
        blobs,
        probe_tip,
        (nutrient.radius * (0.34 + grip * 0.22)).max(3.2),
        extension,
        variation,
        anchor_edge,
        anchor_t,
        level,
    );
    if contact_elapsed.is_none() && probe_tip.distance(origin) <= nutrient.radius * 0.82 + 3.2 {
        contact_elapsed = Some(0.0);
    }
    if let Some(value) = &mut contact_elapsed {
        *value += dt;
    }
    let pull = contact_elapsed
        .map(|value| (value / ENGULF_DURATION).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let target = blob.body.center()
        + Vec2::new(
            super::host_side(blob_id) * blob.body.rest_radius * 0.30,
            blob.body.rest_radius * 0.06,
        );
    nutrient.position = origin.lerp(target, super::smoothstep(pull));
    if contact_elapsed.is_some() {
        probe_tip = nutrient.position;
    }
    let crossed_membrane = contact_elapsed.is_some()
        && nutrient.position.distance(blob.body.center())
            <= blob.body.rest_radius - nutrient.radius * 0.35;
    nutrient.state = if crossed_membrane {
        sound_events.write(BlobSoundEvent::Engulf);
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
    None
}

/// Advances digestion inside the host, restoring energy before starting the
/// outward expulsion phase.
pub(super) fn advance_digesting(
    nutrient: &mut Nutrient,
    dt: f32,
    blobs: &BlobWorld,
    vitality: &mut VitalityWorld,
    sound_events: &mut MessageWriter<BlobSoundEvent>,
) {
    let NutrientState::Digesting {
        blob_id,
        mut elapsed,
        mut local_position,
        mut velocity,
    } = nutrient.state
    else {
        return;
    };
    nutrient.was_submerged = false;
    let Some(blob) = super::living_host(blobs, vitality, blob_id) else {
        super::feeding::make_waste(nutrient, Vec2::new(35.0, 80.0));
        return;
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
    let membrane_bottom = super::membrane_lower_boundary(&blob.body, world_x);
    let minimum_world_y = membrane_bottom + nutrient.radius + 2.0;
    if blob.body.center().y + local_position.y < minimum_world_y {
        local_position.y = minimum_world_y - blob.body.center().y;
        if velocity.y < 0.0 {
            velocity.y *= -0.12;
            velocity.x *= 0.82;
        }
    }
    nutrient.position = blob.body.center() + local_position;
    vitality.restore_energy(blob_id, ENERGY_YIELD / DIGESTION_DURATION * dt);
    nutrient.state = if progress >= 1.0 {
        let (direction, launch_speed) =
            super::feeding::expulsion_launch(blob_id, nutrient.position);
        sound_events.write(BlobSoundEvent::Expel);
        NutrientState::Expelling {
            blob_id,
            elapsed: 0.0,
            velocity: blob.body.velocity() * 24.0 + direction * launch_speed,
        }
    } else {
        NutrientState::Digesting {
            blob_id,
            elapsed,
            local_position,
            velocity,
        }
    };
}

/// Moves an expelled residue through the host membrane before Avian resumes
/// ownership of its free-body movement.
pub(super) fn advance_expelling(nutrient: &mut Nutrient, dt: f32, blobs: &BlobWorld) {
    let NutrientState::Expelling {
        blob_id,
        mut elapsed,
        mut velocity,
    } = nutrient.state
    else {
        return;
    };
    let Some(blob) = blobs.active.iter().find(|blob| blob.id == blob_id) else {
        super::feeding::make_waste(nutrient, velocity);
        return;
    };
    elapsed += dt;
    nutrient.radius = nutrient.original_radius * 0.42;
    velocity *= (-INTERNAL_WASTE_DRAG * dt).exp();
    velocity.y -= OBJECT_GRAVITY * 0.10 * dt;
    nutrient.position += velocity * dt;
    nutrient.state =
        if super::circle_outside_blob_membrane(nutrient.position, nutrient.radius, &blob.body) {
            NutrientState::Waste { velocity }
        } else {
            NutrientState::Expelling {
                blob_id,
                elapsed,
                velocity,
            }
        };
}
