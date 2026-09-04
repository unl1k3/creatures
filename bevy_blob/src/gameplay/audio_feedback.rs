//! Audio feedback derived from the fixed-step blob simulation.

use super::*;
use crate::blob::WastewaterContact;
use bevy::audio::Volume;

pub(super) struct SurfaceAudioFrame {
    pub(super) dt: f32,
    pub(super) charge_before_step: f32,
    pub(super) movement: f32,
    pub(super) shield_extension: f32,
    pub(super) spider_clinging: bool,
}

pub(super) struct WaterAudioFrame {
    pub(super) dt: f32,
    pub(super) movement: f32,
    pub(super) shield_extension: f32,
}

pub(super) fn tick_audio_cooldowns(audio: &mut BlobAudio, dt: f32) {
    for cooldown in audio.landing_cooldowns.values_mut() {
        *cooldown = (*cooldown - dt).max(0.0);
    }
    for cooldown in audio.rolling_cooldowns.values_mut() {
        *cooldown = (*cooldown - dt).max(0.0);
    }
    for cooldown in audio.water_movement_cooldowns.values_mut() {
        *cooldown = (*cooldown - dt).max(0.0);
    }
}

pub(super) fn play_surface_feedback(
    commands: &mut Commands,
    sound_events: &mut MessageWriter<BlobSoundEvent>,
    audio: &mut BlobAudio,
    active_blob: &ActiveBlob,
    frame: SurfaceAudioFrame,
) {
    if frame.charge_before_step <= 0.01 && active_blob.body.charge > 0.01 {
        commands.spawn((
            AudioPlayer::new(audio.charge.clone()),
            one_shot_playback().with_volume(Volume::Linear(0.65)),
        ));
    }
    if frame.charge_before_step > 0.05 && active_blob.body.charge <= 0.01 {
        sound_events.write(BlobSoundEvent::JumpRelease {
            charge: frame.charge_before_step,
        });
    }

    let surface_speed = active_blob.body.velocity().length() / frame.dt.max(0.000_001);
    let angular_rate = active_blob.body.angular_displacement().abs() / frame.dt.max(0.000_001);
    let roll_ready = audio
        .rolling_cooldowns
        .get(&active_blob.id)
        .copied()
        .unwrap_or_default()
        <= 0.0;
    let spine_motion =
        frame.shield_extension > 0.05 && (active_blob.body.grounded || frame.spider_clinging);
    let on_ice = active_blob.body.on_ice() && !spine_motion;
    let rotational_surface_speed = angular_rate * active_blob.body.rest_radius;
    let ice_self_slide = on_ice
        && rotational_surface_speed >= 14.0
        && surface_speed < rotational_surface_speed * 0.85;
    let ice_inertial_motion = on_ice && surface_speed >= 18.0;

    if ice_self_slide {
        let ice_slide_sound = audio.ice_slide.clone();
        audio
            .ice_slide_loops
            .entry(active_blob.id)
            .or_insert_with(|| {
                commands
                    .spawn((
                        AudioPlayer::new(ice_slide_sound),
                        PlaybackSettings::LOOP
                            .with_speed(0.98)
                            .with_volume(Volume::Linear(0.20)),
                    ))
                    .id()
            });
    } else if let Some(loop_entity) = audio.ice_slide_loops.remove(&active_blob.id) {
        commands.entity(loop_entity).despawn();
    }

    let movement_rate = if spine_motion {
        angular_rate
    } else {
        surface_speed
    };
    let movement_threshold = if spine_motion {
        0.04
    } else if ice_self_slide {
        14.0
    } else if on_ice {
        20.0
    } else {
        48.0
    };
    if (active_blob.body.grounded || spine_motion)
        && (frame.movement.abs() > 0.01 || ice_inertial_motion)
        && !ice_self_slide
        && active_blob.body.charge <= 0.01
        && movement_rate >= movement_threshold
        && roll_ready
    {
        let speed_ratio = (movement_rate / if spine_motion { 0.85 } else { 330.0 }).clamp(0.0, 1.0);
        commands.spawn((
            AudioPlayer::new(if spine_motion {
                audio.spine_scrape.clone()
            } else {
                audio.roll.clone()
            }),
            one_shot_playback()
                .with_speed(if spine_motion {
                    0.92 + speed_ratio * 0.16
                } else {
                    0.88 + speed_ratio * 0.22
                })
                .with_volume(Volume::Linear(if spine_motion {
                    0.09 + speed_ratio * 0.13
                } else if on_ice {
                    0.14 + speed_ratio * 0.16
                } else {
                    0.10 + speed_ratio * 0.16
                })),
        ));
        audio.rolling_cooldowns.insert(
            active_blob.id,
            if spine_motion {
                0.28
            } else {
                0.38 - speed_ratio * 0.12
            },
        );
    }

    let impact_speed = active_blob.body.last_impact_speed;
    let landing_ready = audio
        .landing_cooldowns
        .get(&active_blob.id)
        .copied()
        .unwrap_or_default()
        <= 0.0;
    if landing_ready && impact_speed >= 420.0 {
        let volume = (impact_speed / 1_500.0).clamp(0.18, 0.52);
        commands.spawn((
            AudioPlayer::new(audio.land.clone()),
            one_shot_playback().with_volume(Volume::Linear(volume)),
        ));
        audio.landing_cooldowns.insert(active_blob.id, 0.28);
    }
}

pub(super) fn play_water_feedback(
    commands: &mut Commands,
    audio: &mut BlobAudio,
    active_blob: &ActiveBlob,
    contact: WastewaterContact,
    frame: WaterAudioFrame,
) {
    if contact.entered {
        let impact_strength = (contact.entry_speed / 430.0).clamp(0.45, 1.45);
        commands.spawn((
            AudioPlayer::new(audio.water_impact.clone()),
            one_shot_playback()
                .with_volume(Volume::Linear((0.18 + impact_strength * 0.28).min(0.55))),
        ));
    }

    let water_motion_ready = audio
        .water_movement_cooldowns
        .get(&active_blob.id)
        .copied()
        .unwrap_or_default()
        <= 0.0;
    let spines_in_water = frame.shield_extension > 0.05;
    let rotation_rate = active_blob.body.angular_displacement().abs() / frame.dt.max(0.000_001);
    if water_motion_ready
        && !contact.entered
        && contact.submerged_fraction >= 0.20
        && frame.movement.abs() > 0.01
        && rotation_rate >= if spines_in_water { 0.04 } else { 0.025 }
    {
        let rotation_ratio =
            (rotation_rate / if spines_in_water { 0.65 } else { 0.40 }).clamp(0.0, 1.0);
        let audio_rotation_ratio = if spines_in_water {
            rotation_ratio
        } else {
            rotation_ratio.max(0.18)
        };
        commands.spawn((
            AudioPlayer::new(if spines_in_water {
                audio.water_move_spined.clone()
            } else {
                audio.water_move_bare.clone()
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
        audio.water_movement_cooldowns.insert(
            active_blob.id,
            if spines_in_water {
                0.78 - audio_rotation_ratio * 0.10
            } else {
                0.92 - audio_rotation_ratio * 0.10
            },
        );
    }
}

pub(super) fn play_merge_feedback(commands: &mut Commands, audio: &BlobAudio) {
    commands.spawn((
        AudioPlayer::new(audio.merge.clone()),
        one_shot_playback()
            .with_speed(0.72)
            .with_volume(Volume::Linear(0.34)),
    ));
}

/// Rejoining can remove a blob between fixed frames; release its loop entity.
pub(super) fn cleanup_stale_slide_loops(
    commands: &mut Commands,
    audio: &mut BlobAudio,
    blobs: &BlobWorld,
) {
    let stale_ids: Vec<u64> = audio
        .ice_slide_loops
        .keys()
        .copied()
        .filter(|id| !blobs.active.iter().any(|blob| blob.id == *id))
        .collect();
    for id in stale_ids {
        if let Some(loop_entity) = audio.ice_slide_loops.remove(&id) {
            commands.entity(loop_entity).despawn();
        }
    }
}
