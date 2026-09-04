//! Fixed-step gameplay simulation for controllable blobs.
//!
//! The surrounding modules keep ownership of level loading, rendering and
//! auxiliary effects. This module owns the central per-blob movement cycle.

mod audio_feedback;
mod environment_contacts;
mod platform_context;

use self::audio_feedback::{
    SurfaceAudioFrame, WaterAudioFrame, cleanup_stale_slide_loops, play_merge_feedback,
    play_surface_feedback, play_water_feedback, tick_audio_cooldowns,
};
use self::environment_contacts::{
    NutrientContactParams, WastewaterFrame, apply_wastewater_contact, resolve_nutrient_contacts,
};
use self::platform_context::build_collision_platforms;
use super::*;
use bevy::ecs::system::SystemParam;

/// Resources that define one fixed-step update of the controllable blobs.
/// Grouping them keeps the system boundary readable without hiding ownership:
/// mutable resources remain visibly mutable fields.
#[derive(SystemParam)]
pub(crate) struct BlobSimulationParams<'w> {
    time: Res<'w, Time<Fixed>>,
    keyboard: Res<'w, ButtonInput<KeyCode>>,
    dance: ResMut<'w, BlobDancePreview>,
    level: Res<'w, Level>,
    shields: Res<'w, ShieldWorld>,
    nutrition: Res<'w, NutritionWorld>,
    blob_audio: ResMut<'w, BlobAudio>,
    vitality: ResMut<'w, VitalityWorld>,
    blobs: ResMut<'w, BlobWorld>,
    wastewater_effects: ResMut<'w, WastewaterEffects>,
}

pub(crate) fn simulate_blob(
    simulation: BlobSimulationParams,
    mut nutrient_contacts: NutrientContactParams,
    mut commands: Commands,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    let BlobSimulationParams {
        time,
        keyboard,
        mut dance,
        level,
        shields,
        nutrition,
        mut blob_audio,
        mut vitality,
        mut blobs,
        mut wastewater_effects,
    } = simulation;
    dance.advance(time.delta_secs());
    let dance_movement = blobs
        .active
        .get(blobs.selected)
        .and_then(|active_blob| dance.movement_intent(active_blob.body.center().x));
    advance_rejoin_timeout(&mut blobs, time.delta_secs());
    tick_audio_cooldowns(&mut blob_audio, time.delta_secs());

    let horizontal = (keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight))
        as i8
        - (keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft)) as i8;
    let collision_context = build_collision_platforms(&level, &blobs);
    let rejoin_directions = rejoin_roll_directions(&blobs, &collision_context.platforms);
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
            &collision_context.platforms,
            &level.fixtures,
        );
        active_blob
            .body
            .set_spider_cling(spider_anchor.map(|anchor| (anchor.direction, anchor.wall_top)));
        let charge_before_step = active_blob.body.charge;
        active_blob.body.step_with_vigor_on_ice(
            time.delta_secs(),
            BlobStepInput {
                horizontal: movement,
                charging: rejoin_directions.is_none()
                    && is_selected
                    && alive
                    && !protrusion_active
                    && shield_extension < 0.05
                    && scripted_movement
                        .map_or_else(|| keyboard.pressed(KeyCode::ArrowDown), |_| false),
            },
            BlobStepEnvironment {
                platforms: &collision_context.platforms,
                ice_platform_indices: &collision_context.ice_indices,
                glue_platform_indices: &collision_context.glue_indices,
                fixtures: &level.fixtures,
            },
            BlobStepProfile {
                vigor,
                animate_idle: alive,
                retain_tonicity: true,
            },
        );
        if scripted_movement.is_some() && active_blob.body.grounded && dance.take_tiny_hop() {
            let _ = active_blob.body.tiny_ground_hop(time.delta_secs());
        }
        if active_blob.body.on_glue() && movement.abs() > 0.01 {
            // Working against adhesive sludge is tiring whether the spines
            // are deployed or not; spines improve control, not efficiency.
            let _ = vitality.spend(active_blob.id, time.delta_secs() * 0.030);
        }
        play_surface_feedback(
            &mut commands,
            &mut sound_events,
            &mut blob_audio,
            active_blob,
            SurfaceAudioFrame {
                dt: time.delta_secs(),
                charge_before_step,
                movement,
                shield_extension,
                spider_clinging: spider_anchor.is_some(),
            },
        );
        resolve_nutrient_contacts(active_blob, &nutrition, &mut nutrient_contacts.bodies);
        if let Some(contact) = apply_wastewater_contact(
            active_blob,
            &level,
            &mut vitality,
            &mut wastewater_effects,
            WastewaterFrame {
                elapsed: time.elapsed_secs(),
                dt: time.delta_secs(),
                movement,
                shield_extension,
                alive,
            },
        ) {
            play_water_feedback(
                &mut commands,
                &mut blob_audio,
                active_blob,
                contact,
                WaterAudioFrame {
                    dt: time.delta_secs(),
                    movement,
                    shield_extension,
                },
            );
        }
    }
    if let Some((children, parent)) =
        update_rejoining(&mut blobs, &level.platforms, &level.fixtures)
    {
        vitality.merge(children, parent);
        play_merge_feedback(&mut commands, &blob_audio);
    }
    cleanup_stale_slide_loops(&mut commands, &mut blob_audio, &blobs);
    resolve_blob_collisions_with_vitality(&mut blobs.active, &vitality);
}
