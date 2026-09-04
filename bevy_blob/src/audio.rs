//! Audio resources and systems for creature and environment feedback.

use bevy::{
    audio::{AudioSink, Volume},
    prelude::*,
};
use std::{collections::HashMap, time::Duration};

use crate::palette;

/// Audio assets and short per-blob cooldowns used by creature actions.
#[derive(Resource)]
pub(crate) struct BlobAudio {
    pub(crate) charge: Handle<AudioSource>,
    pub(crate) split: Handle<AudioSource>,
    pub(crate) merge: Handle<AudioSource>,
    pub(crate) land: Handle<AudioSource>,
    pub(crate) water_impact: Handle<AudioSource>,
    pub(crate) water_move_bare: Handle<AudioSource>,
    pub(crate) water_move_spined: Handle<AudioSource>,
    pub(crate) roll: Handle<AudioSource>,
    pub(crate) ice_slide: Handle<AudioSource>,
    pub(crate) spine_scrape: Handle<AudioSource>,
    pub(crate) probe: Handle<AudioSource>,
    pub(crate) engulf: Handle<AudioSource>,
    pub(crate) expel: Handle<AudioSource>,
    pub(crate) nutrient_impact: Handle<AudioSource>,
    pub(crate) nutrient_water: Handle<AudioSource>,
    pub(crate) ambient_drop: Handle<AudioSource>,
    pub(crate) ambient_bubble: Handle<AudioSource>,
    pub(crate) chain_impact: Handle<AudioSource>,
    pub(crate) death_motifs: [Handle<AudioSource>; palette::BLOB_FAMILIES.len()],
    pub(crate) jump_release: Handle<AudioSource>,
    pub(crate) shield_deploy: Handle<AudioSource>,
    pub(crate) shield_retract: Handle<AudioSource>,
    pub(crate) acid_burst: Handle<AudioSource>,
    pub(crate) acid_impact: Handle<AudioSource>,
    pub(crate) mechanism_move: Handle<AudioSource>,
    pub(crate) landing_cooldowns: HashMap<u64, f32>,
    pub(crate) rolling_cooldowns: HashMap<u64, f32>,
    pub(crate) ice_slide_loops: HashMap<u64, Entity>,
    pub(crate) water_movement_cooldowns: HashMap<u64, f32>,
}

/// Sound cues emitted by the phagocytosis state machine.
#[derive(Message)]
pub(crate) enum BlobSoundEvent {
    Probe,
    Engulf,
    Expel,
    NutrientImpact { strength: f32 },
    NutrientWater { strength: f32 },
    AmbientDrop,
    AmbientBubble,
    ChainImpact { strength: f32 },
    Death { family: usize },
    JumpRelease { charge: f32 },
    ShieldDeploy,
    ShieldRetract,
    AcidBurst,
    AcidImpact,
    MechanismMove,
}

/// User preference for the looping environmental music. Effects remain audible.
#[derive(Resource)]
pub(crate) struct BackgroundMusic {
    enabled: bool,
}

/// Every gameplay cue is finite. `DESPAWN` releases the playback entity after
/// it ends, while the explicit cap also protects the game from a malformed or
/// unexpectedly long decoded effect. Ambient music is intentionally excluded.
const MAX_EFFECT_PLAYBACK: Duration = Duration::from_millis(1_350);

pub(crate) fn one_shot_playback() -> PlaybackSettings {
    PlaybackSettings::DESPAWN.with_duration(MAX_EFFECT_PLAYBACK)
}

#[derive(Component)]
pub(crate) struct AmbientMusic;

/// Loads all finite creature effects and initializes muted ambience.
pub(crate) fn setup_audio(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(BlobAudio {
        charge: asset_server.load("audio/blob_jump.wav"),
        split: asset_server.load("audio/blob_split.wav"),
        // The lower-pitched split sample is a temporary distinct merge cue.
        merge: asset_server.load("audio/blob_split.wav"),
        land: asset_server.load("audio/blob_land.wav"),
        water_impact: asset_server.load("audio/blob_splash.wav"),
        water_move_bare: asset_server.load("audio/blob_water_bare.wav"),
        water_move_spined: asset_server.load("audio/blob_water_spines.wav"),
        roll: asset_server.load("audio/blob_roll.wav"),
        ice_slide: asset_server.load("audio/blob_ice_slide.wav"),
        spine_scrape: asset_server.load("audio/blob_spine_scrape.wav"),
        probe: asset_server.load("audio/blob_probe.wav"),
        engulf: asset_server.load("audio/blob_gulp.wav"),
        expel: asset_server.load("audio/blob_fart.wav"),
        nutrient_impact: asset_server.load("audio/nutrient_tap.wav"),
        nutrient_water: asset_server.load("audio/nutrient_plop.wav"),
        ambient_drop: asset_server.load("audio/ambient_drop.wav"),
        ambient_bubble: asset_server.load("audio/ambient_bubble.wav"),
        chain_impact: asset_server.load("audio/chain_impact.wav"),
        death_motifs: std::array::from_fn(|family| {
            asset_server.load(format!("audio/blob_death_{family}.wav"))
        }),
        jump_release: asset_server.load("audio/blob_jump_release.wav"),
        shield_deploy: asset_server.load("audio/shield_deploy.wav"),
        shield_retract: asset_server.load("audio/shield_retract.wav"),
        acid_burst: asset_server.load("audio/acid_burst.wav"),
        acid_impact: asset_server.load("audio/acid_impact.wav"),
        mechanism_move: asset_server.load("audio/mechanism_move.wav"),
        landing_cooldowns: HashMap::new(),
        rolling_cooldowns: HashMap::new(),
        ice_slide_loops: HashMap::new(),
        water_movement_cooldowns: HashMap::new(),
    });
    commands.insert_resource(BackgroundMusic { enabled: false });
}

/// Starts the authored sewer ambience once for the whole application.
pub(crate) fn setup_ambient_music(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    music: Res<BackgroundMusic>,
) {
    let volume = if music.enabled {
        Volume::Linear(0.22)
    } else {
        Volume::SILENT
    };
    commands.spawn((
        AmbientMusic,
        AudioPlayer::new(asset_server.load("audio/music/underworld-echoes.mp3")),
        PlaybackSettings::LOOP.with_volume(volume),
    ));
}

/// B mutes/unmutes the ambient loop without affecting gameplay effects.
pub(crate) fn toggle_background_music(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut music: ResMut<BackgroundMusic>,
    mut sinks: Query<&mut AudioSink, With<AmbientMusic>>,
) {
    if keyboard.just_pressed(KeyCode::KeyB) {
        music.enabled = !music.enabled;
    }
    let volume = if music.enabled {
        Volume::Linear(0.22)
    } else {
        Volume::SILENT
    };
    for mut sink in &mut sinks {
        sink.set_volume(volume);
    }
}

/// Keeps digestion audio tied to meaningful state changes rather than key holds.
pub(crate) fn play_blob_sound_events(
    mut events: MessageReader<BlobSoundEvent>,
    blob_audio: Res<BlobAudio>,
    mut commands: Commands,
    time: Res<Time>,
    mut chain_cooldown: Local<f32>,
    mut acid_cooldown: Local<f32>,
    mut mechanism_cooldown: Local<f32>,
) {
    *chain_cooldown = (*chain_cooldown - time.delta_secs()).max(0.0);
    *acid_cooldown = (*acid_cooldown - time.delta_secs()).max(0.0);
    *mechanism_cooldown = (*mechanism_cooldown - time.delta_secs()).max(0.0);
    for event in events.read() {
        if let BlobSoundEvent::ChainImpact { strength } = event {
            if *chain_cooldown <= 0.0 {
                commands.spawn((
                    AudioPlayer::new(blob_audio.chain_impact.clone()),
                    one_shot_playback()
                        .with_volume(Volume::Linear((0.10 + strength * 0.15).clamp(0.10, 0.25))),
                ));
                *chain_cooldown = 0.16;
            }
            continue;
        }
        if let BlobSoundEvent::Death { family } = event {
            if let Some(motif) = blob_audio.death_motifs.get(*family) {
                commands.spawn((
                    AudioPlayer::new(motif.clone()),
                    one_shot_playback().with_volume(Volume::Linear(0.86)),
                ));
            }
            continue;
        }
        if matches!(event, BlobSoundEvent::AcidImpact) {
            if *acid_cooldown <= 0.0 {
                commands.spawn((
                    AudioPlayer::new(blob_audio.acid_impact.clone()),
                    one_shot_playback().with_volume(Volume::Linear(0.22)),
                ));
                *acid_cooldown = 0.18;
            }
            continue;
        }
        if matches!(event, BlobSoundEvent::MechanismMove) {
            if *mechanism_cooldown <= 0.0 {
                commands.spawn((
                    AudioPlayer::new(blob_audio.mechanism_move.clone()),
                    one_shot_playback().with_volume(Volume::Linear(0.20)),
                ));
                *mechanism_cooldown = 0.52;
            }
            continue;
        }
        let (sound, settings) = match event {
            BlobSoundEvent::Probe => (
                blob_audio.probe.clone(),
                one_shot_playback().with_volume(Volume::Linear(0.28)),
            ),
            BlobSoundEvent::Engulf => (
                blob_audio.engulf.clone(),
                one_shot_playback().with_volume(Volume::Linear(0.40)),
            ),
            BlobSoundEvent::Expel => (
                blob_audio.expel.clone(),
                one_shot_playback().with_volume(Volume::Linear(0.58)),
            ),
            BlobSoundEvent::NutrientImpact { strength } => (
                blob_audio.nutrient_impact.clone(),
                one_shot_playback()
                    .with_volume(Volume::Linear((0.10 + strength * 0.20).clamp(0.10, 0.30))),
            ),
            BlobSoundEvent::NutrientWater { strength } => (
                blob_audio.nutrient_water.clone(),
                one_shot_playback()
                    .with_volume(Volume::Linear((0.12 + strength * 0.18).clamp(0.12, 0.28))),
            ),
            BlobSoundEvent::AmbientDrop => (
                blob_audio.ambient_drop.clone(),
                one_shot_playback().with_volume(Volume::Linear(0.13)),
            ),
            BlobSoundEvent::AmbientBubble => (
                blob_audio.ambient_bubble.clone(),
                one_shot_playback().with_volume(Volume::Linear(0.10)),
            ),
            BlobSoundEvent::ChainImpact { .. } => unreachable!("handled above"),
            BlobSoundEvent::Death { .. } => unreachable!("handled above"),
            BlobSoundEvent::JumpRelease { charge } => (
                blob_audio.jump_release.clone(),
                one_shot_playback()
                    .with_volume(Volume::Linear((0.14 + charge * 0.28).clamp(0.14, 0.42))),
            ),
            BlobSoundEvent::ShieldDeploy => (
                blob_audio.shield_deploy.clone(),
                one_shot_playback().with_volume(Volume::Linear(0.26)),
            ),
            BlobSoundEvent::ShieldRetract => (
                blob_audio.shield_retract.clone(),
                one_shot_playback().with_volume(Volume::Linear(0.20)),
            ),
            BlobSoundEvent::AcidBurst => (
                blob_audio.acid_burst.clone(),
                one_shot_playback().with_volume(Volume::Linear(0.30)),
            ),
            BlobSoundEvent::AcidImpact | BlobSoundEvent::MechanismMove => {
                unreachable!("handled above")
            }
        };
        commands.spawn((AudioPlayer::new(sound), settings));
    }
}
