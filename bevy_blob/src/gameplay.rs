//! Fixed-step gameplay simulation for controllable blobs.
//!
//! The surrounding modules keep ownership of level loading, rendering and
//! auxiliary effects. This module owns the central per-blob movement cycle.

use super::*;

pub(crate) fn simulate_blob(
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut dance: ResMut<BlobDancePreview>,
    level: Res<Level>,
    shields: Res<ShieldWorld>,
    nutrition: Res<NutritionWorld>,
    mut blob_audio: ResMut<BlobAudio>,
    mut commands: Commands,
    mut vitality: ResMut<VitalityWorld>,
    mut blobs: ResMut<BlobWorld>,
    mut wastewater_effects: ResMut<WastewaterEffects>,
    mut nutrient_bodies: Query<(&NutrientPhysics, &mut Transform, &mut LinearVelocity)>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    dance.advance(time.delta_secs());
    let dance_movement = blobs
        .active
        .get(blobs.selected)
        .and_then(|active_blob| dance.movement_intent(active_blob.body.center().x));
    advance_rejoin_timeout(&mut blobs, time.delta_secs());
    for cooldown in blob_audio.landing_cooldowns.values_mut() {
        *cooldown = (*cooldown - time.delta_secs()).max(0.0);
    }
    for cooldown in blob_audio.rolling_cooldowns.values_mut() {
        *cooldown = (*cooldown - time.delta_secs()).max(0.0);
    }
    for cooldown in blob_audio.water_movement_cooldowns.values_mut() {
        *cooldown = (*cooldown - time.delta_secs()).max(0.0);
    }

    let horizontal = (keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight))
        as i8
        - (keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft)) as i8;
    // A returning counterweight plate may pass through a blob that has just
    // jumped away from it. It is visually moving, but must not become a
    // second moving collision surface underneath the airborne soft body.
    let airborne_counterweight_plates: Vec<usize> = level
        .counterbalances
        .iter()
        .filter_map(|balance| {
            let plate = level.platforms[balance.plate_platform];
            let has_rider = blobs.active.iter().any(|blob| {
                let center = blob.body.center();
                let radius = blob.body.rest_radius;
                (center.x - plate.center.x).abs() <= plate.half_size.x + radius * 0.3
                    && center.y - radius <= plate.center.y + plate.half_size.y + 5.0
                    && center.y >= plate.center.y
            });
            let has_airborne_blob_above = blobs.active.iter().any(|blob| {
                let center = blob.body.center();
                (center.x - plate.center.x).abs() <= plate.half_size.x + blob.body.rest_radius * 0.3
                    && center.y > plate.center.y
            });
            (!has_rider && has_airborne_blob_above).then_some(balance.plate_platform)
        })
        .collect();
    let mut collision_platforms = Vec::with_capacity(level.platforms.len());
    let mut ice_collision_platforms = Vec::new();
    let mut glue_collision_platforms = Vec::new();
    for (level_index, platform) in level.platforms.iter().copied().enumerate() {
        if airborne_counterweight_plates.contains(&level_index) {
            continue;
        }
        let collision_index = collision_platforms.len();
        collision_platforms.push(platform);
        if level.ice_platforms.contains(&level_index) {
            ice_collision_platforms.push(collision_index);
        }
        if level.glue_platforms.contains(&level_index) {
            glue_collision_platforms.push(collision_index);
        }
    }
    let rejoin_directions = rejoin_roll_directions(&blobs, &collision_platforms);
    let selected = blobs.selected;
    for (index, active_blob) in blobs.active.iter_mut().enumerate() {
        let is_selected = index == selected;
        let alive = vitality.is_alive(active_blob.id);
        let protrusion_active = nutrition.has_external_protrusion(active_blob.id);
        if !alive || protrusion_active {
            active_blob.body.cancel_jump_charge();
        }
        let vigor = vitality.vigor(active_blob.id) * nutrition.capability_factor(active_blob.id);
        if let Some((position, radius, strength)) = nutrition.physical_load(active_blob.id) {
            active_blob
                .body
                .apply_internal_bulge(position, radius, strength);
        }
        let shield_extension = shields.extension(active_blob.id);
        active_blob
            .body
            .set_ice_traction(if shield_extension > 0.05 {
                // Spines bite shallowly into the ice: enough grip to roll, but
                // substantially less than an ordinary stone platform.
                0.28
            } else {
                0.0
            });
        let scripted_movement =
            dance_movement.filter(|_| is_selected && rejoin_directions.is_none());
        let movement = if alive {
            rejoin_directions
                .as_ref()
                .map(|directions| directions[index])
                .unwrap_or(if is_selected {
                    scripted_movement.map_or(horizontal as f32, |movement| movement)
                        * (1.0 - shield_extension * 0.58)
                        * if nutrition.has_external_protrusion(active_blob.id) {
                            0.0
                        } else {
                            1.0
                        }
                } else {
                    0.0
                })
        } else {
            0.0
        };
        let spider_anchor = spider_climb_anchor_direction(
            active_blob.id,
            &active_blob.body,
            shield_extension,
            &collision_platforms,
            &level.fixtures,
        );
        active_blob
            .body
            .set_spider_cling(spider_anchor.map(|anchor| (anchor.direction, anchor.wall_top)));
        let charge_before_step = active_blob.body.charge;
        active_blob.body.step_with_vigor_on_ice(
            time.delta_secs(),
            movement,
            rejoin_directions.is_none()
                && is_selected
                && alive
                && !protrusion_active
                && shield_extension < 0.05
                && scripted_movement
                    .map_or_else(|| keyboard.pressed(KeyCode::ArrowDown), |_| false),
            &collision_platforms,
            &ice_collision_platforms,
            &glue_collision_platforms,
            &level.fixtures,
            vigor,
            alive,
            true,
        );
        if scripted_movement.is_some() && active_blob.body.grounded && dance.take_tiny_hop() {
            let _ = active_blob.body.tiny_ground_hop(time.delta_secs());
        }
        if active_blob.body.on_glue() && movement.abs() > 0.01 {
            // Working against adhesive sludge is tiring whether the spines
            // are deployed or not; spines improve control, not efficiency.
            let _ = vitality.spend(active_blob.id, time.delta_secs() * 0.030);
        }
        if charge_before_step <= 0.01 && active_blob.body.charge > 0.01 {
            commands.spawn((
                AudioPlayer::new(blob_audio.charge.clone()),
                one_shot_playback().with_volume(Volume::Linear(0.65)),
            ));
        }
        if charge_before_step > 0.05 && active_blob.body.charge <= 0.01 {
            sound_events.write(BlobSoundEvent::JumpRelease {
                charge: charge_before_step,
            });
        }
        let surface_speed = active_blob.body.velocity().length() / time.delta_secs().max(0.000_001);
        let angular_rate =
            active_blob.body.angular_displacement().abs() / time.delta_secs().max(0.000_001);
        let roll_ready = blob_audio
            .rolling_cooldowns
            .get(&active_blob.id)
            .copied()
            .unwrap_or_default()
            <= 0.0;
        let spine_motion =
            shield_extension > 0.05 && (active_blob.body.grounded || spider_anchor.is_some());
        // A normal roll has a dependable link between translation and its
        // sound. Deployed spines are different: their audible contact is tied
        // to the membrane turning against the surface, including a wall climb.
        let movement_rate = if spine_motion {
            angular_rate
        } else {
            surface_speed
        };
        let movement_threshold = if spine_motion { 0.04 } else { 48.0 };
        if (active_blob.body.grounded || spine_motion)
            && movement.abs() > 0.01
            && active_blob.body.charge <= 0.01
            && movement_rate >= movement_threshold
            && roll_ready
        {
            let speed_ratio =
                (movement_rate / if spine_motion { 0.85 } else { 330.0 }).clamp(0.0, 1.0);
            commands.spawn((
                AudioPlayer::new(if spine_motion {
                    blob_audio.spine_scrape.clone()
                } else {
                    blob_audio.roll.clone()
                }),
                one_shot_playback()
                    .with_speed(if spine_motion {
                        0.92 + speed_ratio * 0.16
                    } else {
                        0.88 + speed_ratio * 0.22
                    })
                    .with_volume(Volume::Linear(if spine_motion {
                        0.09 + speed_ratio * 0.13
                    } else {
                        0.10 + speed_ratio * 0.16
                    })),
            ));
            blob_audio.rolling_cooldowns.insert(
                active_blob.id,
                if spine_motion {
                    0.28
                } else {
                    // The roll source lasts 0.21 s (slightly longer at low
                    // pitch), so it must finish before its next contact cue.
                    0.38 - speed_ratio * 0.12
                },
            );
        }
        let impact_speed = active_blob.body.last_impact_speed;
        let landing_ready = blob_audio
            .landing_cooldowns
            .get(&active_blob.id)
            .copied()
            .unwrap_or_default()
            <= 0.0;
        if landing_ready && impact_speed >= 420.0 {
            let volume = (impact_speed / 1_500.0).clamp(0.18, 0.52);
            commands.spawn((
                AudioPlayer::new(blob_audio.land.clone()),
                one_shot_playback().with_volume(Volume::Linear(volume)),
            ));
            blob_audio.landing_cooldowns.insert(active_blob.id, 0.28);
        }
        for (nutrient, mut transform, mut velocity) in &mut nutrient_bodies {
            if !nutrition.is_free_index(nutrient.index) {
                continue;
            }
            let center = transform.translation.truncate();
            let Some(radius) = nutrition.collision_radius(nutrient.index) else {
                continue;
            };
            let Some((depth, normal)) = circle_blob_penetration(center, radius, &active_blob.body)
            else {
                continue;
            };
            // The nutrient is owned by Avian: release it from the membrane by
            // moving its physics body, then give the soft body a small equal
            // reaction without translating the whole blob rigidly.
            transform.translation += (normal * (depth + 0.15)).extend(0.0);
            let inward = velocity.0.dot(normal);
            if inward < 0.0 {
                velocity.0 -= normal * inward;
            }
            active_blob.body.add_velocity(-normal * depth * 0.10);
        }
        let water_contact =
            level
                .wastewater_areas
                .iter()
                .copied()
                .enumerate()
                .find_map(|(area_index, area)| {
                    let center = active_blob.body.center();
                    area.contains_x(center.x).then(|| {
                        let surface_y = area.surface_y(center.x, time.elapsed_secs());
                        let bottom_y = area.position.y - area.size.y * 0.5;
                        active_blob
                            .body
                            .apply_wastewater_forces_with_spine_drag(
                                surface_y,
                                bottom_y,
                                time.delta_secs(),
                                shield_extension,
                                movement,
                            )
                            .map(|contact| (area_index, area, contact))
                    })?
                });
        if let Some((area_index, area, contact)) = water_contact {
            let immune = area.immune_family.is_some_and(|family| {
                crate::palette::blob_family_index(active_blob.parent_id) == family
            });
            if alive && !immune {
                vitality.damage(
                    active_blob.id,
                    WASTEWATER_DAMAGE_PER_SECOND * contact.submerged_fraction * time.delta_secs(),
                );
            }
            if contact.entered {
                let impact_strength = (contact.entry_speed / 430.0).clamp(0.45, 1.45);
                wastewater_effects.emit(
                    area_index,
                    Vec2::new(active_blob.body.center().x, contact.surface_y),
                    active_blob.body.rest_radius * (0.46 + contact.submerged_fraction * 0.42),
                    impact_strength,
                );
                commands.spawn((
                    AudioPlayer::new(blob_audio.water_impact.clone()),
                    one_shot_playback()
                        .with_volume(Volume::Linear((0.18 + impact_strength * 0.28).min(0.55))),
                ));
            }

            // Liquid motion needs its own quiet cue: without spines it is a
            // heavy displaced-water sound; deployed spines make a faster,
            // fibrous swish while the blob paddles or climbs out.
            let water_motion_ready = blob_audio
                .water_movement_cooldowns
                .get(&active_blob.id)
                .copied()
                .unwrap_or_default()
                <= 0.0;
            let spines_in_water = shield_extension > 0.05;
            let rotation_rate =
                active_blob.body.angular_displacement().abs() / time.delta_secs().max(0.000_001);
            if water_motion_ready
                && !contact.entered
                && contact.submerged_fraction >= 0.20
                && movement.abs() > 0.01
                // The cue must follow actual body rotation. Previously a bare
                // blob could retrigger its long swish from input alone while
                // nearly stationary, causing overlapping "stuck" rustles.
                && rotation_rate >= if spines_in_water { 0.04 } else { 0.025 }
            {
                // Water motion is rotational: the rate of the membrane's
                // actual roll, rather than centre translation, controls the
                // cadence, pitch and volume of the submerged-body cue.
                let rotation_ratio =
                    (rotation_rate / if spines_in_water { 0.65 } else { 0.40 }).clamp(0.0, 1.0);
                let audio_rotation_ratio = if spines_in_water {
                    rotation_ratio
                } else {
                    // Audible at the smallest attempted roll, then genuinely
                    // follows angular velocity as the body gains momentum.
                    rotation_ratio.max(0.18)
                };
                commands.spawn((
                    AudioPlayer::new(if spines_in_water {
                        blob_audio.water_move_spined.clone()
                    } else {
                        blob_audio.water_move_bare.clone()
                    }),
                    one_shot_playback()
                        .with_speed(if spines_in_water {
                            0.84 + audio_rotation_ratio * 0.34
                        } else {
                            0.82 + audio_rotation_ratio * 0.22
                        })
                        .with_volume(Volume::Linear(if spines_in_water {
                            0.12 + audio_rotation_ratio * 0.14
                        } else {
                            0.18 + audio_rotation_ratio * 0.14
                        })),
                ));
                blob_audio.water_movement_cooldowns.insert(
                    active_blob.id,
                    if spines_in_water {
                        // Both source clips must finish before another cue
                        // may start; otherwise consecutive one-shots blend
                        // into an unintended continuous sound.
                        0.78 - audio_rotation_ratio * 0.10
                    } else {
                        0.92 - audio_rotation_ratio * 0.10
                    },
                );
            }
        }
    }
    if let Some((children, parent)) =
        update_rejoining(&mut blobs, &level.platforms, &level.fixtures)
    {
        vitality.merge(children, parent);
        commands.spawn((
            AudioPlayer::new(blob_audio.merge.clone()),
            one_shot_playback()
                .with_speed(0.72)
                .with_volume(Volume::Linear(0.34)),
        ));
    }
    resolve_blob_collisions_with_vitality(&mut blobs.active, &vitality);
}
