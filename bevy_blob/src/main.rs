mod acid;
mod blob;
mod blob_world;
mod camera;
mod dance;
mod environment;
mod gameplay;
mod hud;
mod input;
mod level_format;
mod nutrition;
mod palette;
mod rendering;
mod schedule;
mod shield;
mod vitality;

use acid::{AcidWorld, draw_acid, fire_acid, simulate_acid};
use avian2d::collision::collider::contact_query::contact_manifolds;
use avian2d::prelude::LinearVelocity;
use avian2d::prelude::PhysicsPlugins;
use avian2d::prelude::{Collider, ContactManifold, Gravity};
use bevy::{
    app::AppExit,
    audio::{AudioSink, Volume},
    diagnostic::FrameTimeDiagnosticsPlugin,
    prelude::*,
    window::{ExitCondition, WindowPosition, WindowResolution},
};
use blob::{Blob, DEFAULT_CREATURE_SCALE, Platform, REFERENCE_RADIUS};
pub(crate) use blob_world::*;
#[cfg(test)]
use camera::selected_camera_target;
use camera::{GameCamera, follow_camera};
use dance::{BlobDancePreview, toggle_blob_dance};
use environment::{
    AvianContactDiagnostics, Level, LevelDebugOverlay, RouteProgress, TestScenario,
    WastewaterEffects, advance_route_progress, draw_level_chains, resolve_avian_environment,
    resolve_blob_chain_contacts, sample_avian_contacts, setup_environment,
    simulate_counterbalances, simulate_level_hazards, switch_test_scenario, sync_chain_lighting,
    toggle_level_debug, update_parallax_layers,
};
use gameplay::simulate_blob;
use hud::{arrange_auxiliary_windows, setup_legend, toggle_legend, update_metrics};
#[cfg(test)]
use input::next_selection;
use input::{cycle_selection, exit_on_escape, handle_blob_actions, toggle_pause};
use nutrition::{
    NutrientPhysics, NutritionWorld, circle_blob_penetration, draw_nutrition, setup_nutrition,
    simulate_nutrition, spawn_nutrient_bodies, start_phagocytosis,
};
#[cfg(test)]
use rendering::blob_family_color;
use rendering::{
    InkStylePreview, draw_world, setup_ambient_drop_assets, simulate_ambient_drops,
    simulate_wastewater, simulate_wastewater_bubbles, simulate_wastewater_impacts,
    sync_blob_meshes, sync_counterbalance_visuals, sync_ink_atmosphere, sync_ink_preview,
    sync_route_markers, toggle_foreground, toggle_ink_style, trigger_drop_shower,
};
use schedule::{FixedGameSet, FrameSet, GameScheduleAppExt};
use shield::{ShieldWorld, simulate_shields, spider_climb_anchor_direction};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use vitality::{
    DeathCause, LifeState, Vitality, VitalityWorld, WASTEWATER_DAMAGE_PER_SECOND, simulate_vitality,
};

/// Audio assets and short per-blob cooldowns used by creature actions.
#[derive(Resource)]
struct BlobAudio {
    charge: Handle<AudioSource>,
    split: Handle<AudioSource>,
    merge: Handle<AudioSource>,
    land: Handle<AudioSource>,
    water_impact: Handle<AudioSource>,
    water_move_bare: Handle<AudioSource>,
    water_move_spined: Handle<AudioSource>,
    roll: Handle<AudioSource>,
    spine_scrape: Handle<AudioSource>,
    probe: Handle<AudioSource>,
    engulf: Handle<AudioSource>,
    expel: Handle<AudioSource>,
    nutrient_impact: Handle<AudioSource>,
    nutrient_water: Handle<AudioSource>,
    ambient_drop: Handle<AudioSource>,
    ambient_bubble: Handle<AudioSource>,
    chain_impact: Handle<AudioSource>,
    death_motifs: [Handle<AudioSource>; palette::BLOB_FAMILIES.len()],
    jump_release: Handle<AudioSource>,
    shield_deploy: Handle<AudioSource>,
    shield_retract: Handle<AudioSource>,
    acid_burst: Handle<AudioSource>,
    acid_impact: Handle<AudioSource>,
    mechanism_move: Handle<AudioSource>,
    landing_cooldowns: HashMap<u64, f32>,
    rolling_cooldowns: HashMap<u64, f32>,
    water_movement_cooldowns: HashMap<u64, f32>,
}

/// Sound cues emitted by the phagocytosis state machine.
#[derive(Message)]
enum BlobSoundEvent {
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
struct BackgroundMusic {
    enabled: bool,
}

/// Every gameplay cue is finite. `DESPAWN` releases the playback entity after
/// it ends, while the explicit cap also protects the game from a malformed or
/// unexpectedly long decoded effect. Ambient music is intentionally excluded.
const MAX_EFFECT_PLAYBACK: Duration = Duration::from_millis(1_350);

fn one_shot_playback() -> PlaybackSettings {
    PlaybackSettings::DESPAWN.with_duration(MAX_EFFECT_PLAYBACK)
}

#[derive(Component)]
struct AmbientMusic;

fn main() {
    App::new()
        .insert_resource(ClearColor(palette::color(palette::IVORY)))
        .insert_resource(Time::<Fixed>::from_hz(120.0))
        .init_resource::<InkStylePreview>()
        .init_resource::<BlobDancePreview>()
        // The sewer ambience is opt-in: gameplay starts quietly and B enables
        // the loop when the player wants it.
        .insert_resource(BackgroundMusic { enabled: false })
        .add_message::<BlobSoundEvent>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Blob — X divide, E ricongiunge, R reset, TAB seleziona".into(),
                resolution: WindowResolution::new(900, 900),
                position: WindowPosition::At(IVec2::new(20, 30)),
                ..default()
            }),
            exit_condition: ExitCondition::OnPrimaryClosed,
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default().with_length_unit(100.0))
        .insert_resource(Gravity(Vec2::new(0.0, -1_150.0)))
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .configure_game_schedules()
        .add_systems(
            Startup,
            (
                setup_environment,
                setup,
                setup_ambient_music,
                setup_nutrition,
                setup_ambient_drop_assets,
                setup_legend,
            )
                .chain(),
        )
        .add_systems(
            FixedUpdate,
            simulate_shields.in_set(FixedGameSet::Actuation),
        )
        .add_systems(
            FixedUpdate,
            (simulate_counterbalances, simulate_blob)
                .chain()
                .in_set(FixedGameSet::Motion),
        )
        .add_systems(
            FixedUpdate,
            (
                resolve_blob_chain_contacts,
                resolve_avian_environment,
                enforce_blob_safety_bounds,
            )
                .chain()
                .in_set(FixedGameSet::Contacts),
        )
        .add_systems(
            FixedUpdate,
            (
                simulate_level_hazards,
                simulate_vitality,
                simulate_nutrition,
                simulate_acid,
            )
                .chain()
                .in_set(FixedGameSet::Consequences),
        )
        .add_systems(
            Update,
            (
                exit_on_escape,
                arrange_auxiliary_windows,
                toggle_legend,
                toggle_level_debug,
                toggle_ink_style,
                toggle_pause,
                toggle_foreground,
                toggle_background_music,
                toggle_blob_dance,
            )
                .in_set(FrameSet::Input),
        )
        .add_systems(
            Update,
            (
                switch_test_scenario,
                handle_blob_actions,
                start_phagocytosis,
                fire_acid,
                cycle_selection,
                advance_route_progress,
                sample_avian_contacts,
            )
                .chain()
                .in_set(FrameSet::Gameplay),
        )
        .add_systems(
            Update,
            (follow_camera, update_parallax_layers)
                .chain()
                .in_set(FrameSet::Camera),
        )
        .add_systems(
            Update,
            (
                trigger_drop_shower,
                simulate_ambient_drops,
                simulate_wastewater_impacts,
                simulate_wastewater,
                simulate_wastewater_bubbles,
            )
                .chain()
                .in_set(FrameSet::Ambient),
        )
        .add_systems(
            Update,
            (
                sync_ink_preview,
                sync_ink_atmosphere,
                sync_counterbalance_visuals,
                sync_blob_meshes,
                sync_route_markers,
                sync_chain_lighting,
                draw_level_chains,
                draw_world,
                draw_acid,
                draw_nutrition,
                update_metrics,
            )
                .chain()
                .in_set(FrameSet::Presentation),
        )
        .add_systems(
            Update,
            play_blob_sound_events
                .after(start_phagocytosis)
                .in_set(FrameSet::Audio),
        )
        .run();
}

/// Starts the authored sewer ambience once for the whole application.
fn setup_ambient_music(
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
fn toggle_background_music(
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

fn setup(mut commands: Commands, level: Res<Level>, asset_server: Res<AssetServer>) {
    commands.insert_resource(BlobAudio {
        charge: asset_server.load("audio/blob_jump.wav"),
        split: asset_server.load("audio/blob_split.wav"),
        // The lower-pitched split sample is a temporary distinct merge cue.
        // It can be replaced by an authored merge sound without code changes.
        merge: asset_server.load("audio/blob_split.wav"),
        land: asset_server.load("audio/blob_land.wav"),
        water_impact: asset_server.load("audio/blob_splash.wav"),
        water_move_bare: asset_server.load("audio/blob_water_bare.wav"),
        water_move_spined: asset_server.load("audio/blob_water_spines.wav"),
        roll: asset_server.load("audio/blob_roll.wav"),
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
        water_movement_cooldowns: HashMap::new(),
    });
    commands.spawn((Camera2d, GameCamera));
    commands.insert_resource(BlobWorld {
        active: vec![ActiveBlob {
            id: 0,
            parent_id: None,
            body: Blob::new(level.spawn_position, INITIAL_RADIUS),
        }],
        selected: 0,
        rejoin_parent: None,
        rejoin_elapsed: 0.0,
        parent_links: HashMap::new(),
        next_id: 1,
    });
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15)
        .max(1);
    commands.insert_resource(SplitRng(seed));
    commands.insert_resource(AcidWorld::new(seed.rotate_left(29)));
    commands.insert_resource(ShieldWorld::default());
    commands.insert_resource(VitalityWorld::default());
}

/// Keeps digestion audio tied to meaningful state changes rather than key holds.
fn play_blob_sound_events(
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

include!("game_tests.rs");
