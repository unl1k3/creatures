mod acid;
mod blob;
mod camera;
mod environment;
mod hud;
mod input;
mod level_format;
mod nutrition;
mod palette;
mod rendering;
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
#[cfg(test)]
use camera::selected_camera_target;
use camera::{GameCamera, follow_camera};
use environment::{
    AvianContactDiagnostics, Level, LevelDebugOverlay, RouteProgress, TestScenario,
    WastewaterEffects, advance_route_progress, draw_level_chains, resolve_avian_environment,
    resolve_blob_chain_contacts, sample_avian_contacts, setup_environment,
    simulate_counterbalances, simulate_level_hazards, switch_test_scenario, sync_chain_lighting,
    toggle_level_debug, update_parallax_layers,
};
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
use shield::{ShieldWorld, simulate_shields, spider_climb_anchor_direction};
use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use vitality::{
    DeathCause, LifeState, Vitality, VitalityWorld, WASTEWATER_DAMAGE_PER_SECOND, simulate_vitality,
};

const BLOB_START: Vec2 = Vec2::new(0.0, -280.0);
const INITIAL_RADIUS: f32 = REFERENCE_RADIUS * DEFAULT_CREATURE_SCALE;
const MAX_ACTIVE_BLOBS: usize = 4;
const REJOIN_TIMEOUT: f32 = 4.0;
const BLOB_CONTACT_PREDICTION_CLEARANCE: f32 = 1.5;
const BLOB_CONTACT_VISUAL_CLEARANCE: f32 = 0.0;
const BLOB_CONTACT_MAX_CORRECTION: f32 = 4.0;
const BLOB_CONTACT_MAX_TRANSFER_SPEED: f32 = 4.0;

struct ActiveBlob {
    id: u64,
    parent_id: Option<u64>,
    body: Blob,
}

#[derive(Resource)]
struct BlobWorld {
    active: Vec<ActiveBlob>,
    selected: usize,
    rejoin_parent: Option<u64>,
    rejoin_elapsed: f32,
    parent_links: HashMap<u64, Option<u64>>,
    next_id: u64,
}

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

#[derive(Resource)]
struct SplitRng(u64);

impl SplitRng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn split_choice(&mut self, particle_count: usize) -> (usize, bool) {
        let ratio = 0.37 + (self.next() % 10) as f32 * 0.01;
        let smaller_count = ((particle_count as f32 * ratio).round() as usize)
            .clamp(6, particle_count.saturating_sub(6));
        let smaller_on_left = self.next() & 1 == 0;
        (smaller_count, smaller_on_left)
    }
}

fn main() {
    App::new()
        .insert_resource(ClearColor(palette::color(palette::IVORY)))
        .insert_resource(Time::<Fixed>::from_hz(120.0))
        .init_resource::<InkStylePreview>()
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
            (
                simulate_shields,
                simulate_counterbalances,
                simulate_blob,
                resolve_blob_chain_contacts,
                resolve_avian_environment,
                enforce_blob_safety_bounds,
                simulate_level_hazards,
                simulate_vitality,
                simulate_nutrition,
                simulate_acid,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                exit_on_escape,
                arrange_auxiliary_windows,
                toggle_legend,
                toggle_level_debug,
                toggle_ink_style,
                switch_test_scenario,
                handle_blob_actions,
                start_phagocytosis,
                fire_acid,
                cycle_selection,
                (follow_camera, update_parallax_layers).chain(),
                advance_route_progress,
                sample_avian_contacts,
                update_metrics,
                (
                    trigger_drop_shower,
                    simulate_ambient_drops,
                    simulate_wastewater_impacts,
                    simulate_wastewater,
                    simulate_wastewater_bubbles,
                    sync_blob_meshes,
                )
                    .chain(),
                sync_ink_preview,
                sync_route_markers,
                draw_world,
                draw_acid,
                draw_nutrition,
            )
                .chain(),
        )
        .add_systems(Update, sync_ink_atmosphere.after(sync_ink_preview))
        .add_systems(Update, play_blob_sound_events.after(start_phagocytosis))
        .add_systems(Update, toggle_background_music)
        .add_systems(
            Update,
            (
                toggle_pause,
                toggle_foreground,
                sync_chain_lighting,
                draw_level_chains,
                // Ink platforms may be rebuilt when the scenario changes;
                // update movable visual layers only after that rebuild.
                sync_counterbalance_visuals.after(sync_ink_preview),
            ),
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

fn simulate_blob(
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
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
        let movement = if alive {
            rejoin_directions
                .as_ref()
                .map(|directions| directions[index])
                .unwrap_or(if is_selected {
                    horizontal as f32
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
                && keyboard.pressed(KeyCode::ArrowDown),
            &collision_platforms,
            &ice_collision_platforms,
            &glue_collision_platforms,
            &level.fixtures,
            vigor,
            alive,
            true,
        );
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

fn enforce_blob_safety_bounds(level: Res<Level>, mut blobs: ResMut<BlobWorld>) {
    let Some(bounds) = level.safety_bounds else {
        return;
    };
    for active_blob in &mut blobs.active {
        if active_blob
            .body
            .contain_within_safety_bounds(bounds.min, bounds.max)
        {
            active_blob.body.cancel_jump_charge();
            active_blob.body.stabilize_after_external_projection();
        }
    }
}

/// Returns true only for an actual containment, not for the shallow overlap
/// that can occur while a soft membrane is resting on its contact skin.
fn reset_world_at(blobs: &mut BlobWorld, position: Vec2) {
    blobs.active = vec![ActiveBlob {
        id: 0,
        parent_id: None,
        body: Blob::new(position, INITIAL_RADIUS),
    }];
    blobs.selected = 0;
    blobs.rejoin_parent = None;
    blobs.rejoin_elapsed = 0.0;
    blobs.parent_links.clear();
    blobs.next_id = 1;
}

#[cfg(test)]
fn split_selected(blobs: &mut BlobWorld, rng: &mut SplitRng, dt: f32) {
    let _ = split_selected_in_level(blobs, rng, dt, &[], &[]);
}

fn split_selected_in_level(
    blobs: &mut BlobWorld,
    rng: &mut SplitRng,
    dt: f32,
    platforms: &[Platform],
    fixtures: &[Vec<Vec2>],
) -> bool {
    if blobs.active.is_empty() || blobs.active.len() >= MAX_ACTIVE_BLOBS {
        return false;
    }
    let index = blobs.selected.min(blobs.active.len() - 1);
    if !blobs.active[index].body.can_split() {
        return false;
    }
    let parent_body = &blobs.active[index].body;
    let (smaller_count, smaller_on_left) = rng.split_choice(parent_body.particles.len());
    let [mut first_body, mut second_body] =
        parent_body.split_pair_uneven(dt, smaller_count, smaller_on_left);
    // Never replace a valid parent with children already embedded in level
    // geometry. This is most visible next to the thin wall of scenario 8.
    if !place_blob_clear(&mut first_body, platforms, fixtures)
        || !place_blob_clear(&mut second_body, platforms, fixtures)
    {
        return false;
    }

    let parent = blobs.active.remove(index);
    blobs.parent_links.insert(parent.id, parent.parent_id);
    let first_id = blobs.next_id;
    let second_id = blobs.next_id + 1;
    blobs.next_id += 2;
    blobs.active.insert(
        index,
        ActiveBlob {
            id: first_id,
            parent_id: Some(parent.id),
            body: first_body,
        },
    );
    blobs.active.insert(
        index + 1,
        ActiveBlob {
            id: second_id,
            parent_id: Some(parent.id),
            body: second_body,
        },
    );
    blobs.selected = index;
    blobs.rejoin_elapsed = 0.0;
    true
}

fn start_selected_rejoin(blobs: &mut BlobWorld) -> bool {
    let Some(selected) = blobs.active.get(blobs.selected) else {
        return false;
    };
    let Some(parent_id) = selected.parent_id else {
        return false;
    };
    if blobs
        .active
        .iter()
        .filter(|blob| blob.parent_id == Some(parent_id))
        .count()
        != 2
    {
        return false;
    }
    blobs.rejoin_parent = Some(parent_id);
    blobs.rejoin_elapsed = 0.0;
    true
}

fn advance_rejoin_timeout(blobs: &mut BlobWorld, dt: f32) {
    if blobs.rejoin_parent.is_none() {
        blobs.rejoin_elapsed = 0.0;
        return;
    }
    blobs.rejoin_elapsed += dt;
    if blobs.rejoin_elapsed >= REJOIN_TIMEOUT {
        blobs.rejoin_parent = None;
        blobs.rejoin_elapsed = 0.0;
    }
}

fn rejoin_pair_indices(blobs: &BlobWorld) -> Option<(usize, usize, u64)> {
    let parent_id = blobs.rejoin_parent?;
    let mut indices = blobs
        .active
        .iter()
        .enumerate()
        .filter_map(|(index, blob)| (blob.parent_id == Some(parent_id)).then_some(index));
    let first = indices.next()?;
    let second = indices.next()?;
    indices
        .next()
        .is_none()
        .then_some((first, second, parent_id))
}

fn rejoin_roll_directions(blobs: &BlobWorld, platforms: &[Platform]) -> Option<Vec<f32>> {
    let (first_index, second_index, _) = rejoin_pair_indices(blobs)?;
    let first_center = blobs.active[first_index].body.center();
    let second_center = blobs.active[second_index].body.center();
    if !path_is_clear(first_center, second_center, platforms) {
        return None;
    }
    let horizontal_delta = second_center.x - first_center.x;
    let direction = if horizontal_delta.abs() > 1.0 {
        horizontal_delta.signum()
    } else {
        0.0
    };
    let mut directions = vec![0.0; blobs.active.len()];
    directions[first_index] = direction;
    directions[second_index] = -direction;
    Some(directions)
}

fn update_rejoining(
    blobs: &mut BlobWorld,
    platforms: &[Platform],
    fixtures: &[Vec<Vec2>],
) -> Option<([u64; 2], u64)> {
    let Some((first_index, second_index, parent_id)) = rejoin_pair_indices(blobs) else {
        return None;
    };
    let first_center = blobs.active[first_index].body.center();
    let second_center = blobs.active[second_index].body.center();
    if !path_is_clear(first_center, second_center, platforms) {
        return None;
    }
    let pair_scale = (blobs.active[first_index].body.size_scale()
        + blobs.active[second_index].body.size_scale())
        * 0.5;
    let surface_gap = blob_surface_gap(
        &blobs.active[first_index].body,
        &blobs.active[second_index].body,
    );
    if surface_gap <= 2.0 * pair_scale {
        let child_ids = [blobs.active[first_index].id, blobs.active[second_index].id];
        let mut merged = Blob::merge_pair(
            &blobs.active[first_index].body,
            &blobs.active[second_index].body,
        );
        if !place_blob_clear(&mut merged, platforms, fixtures) {
            return None;
        }
        let grandparent = blobs.parent_links.remove(&parent_id).flatten();
        let insert_index = first_index.min(second_index);
        blobs.active.remove(first_index.max(second_index));
        blobs.active.remove(insert_index);
        blobs.active.insert(
            insert_index,
            ActiveBlob {
                id: parent_id,
                parent_id: grandparent,
                body: merged,
            },
        );
        blobs.selected = insert_index;
        blobs.rejoin_parent = None;
        blobs.rejoin_elapsed = 0.0;
        return Some((child_ids, parent_id));
    }
    None
}

fn place_blob_clear(blob: &mut Blob, platforms: &[Platform], fixtures: &[Vec<Vec2>]) -> bool {
    let initial_center = blob.center();
    let clearance_radius = blob.rest_radius + 3.0 * blob.size_scale();
    for _ in 0..16 {
        let center = blob.center();
        let correction = platforms
            .iter()
            .find_map(|platform| merge_circle_aabb_penetration(center, clearance_radius, platform))
            .or_else(|| {
                fixtures.iter().find_map(|vertices| {
                    merge_circle_convex_penetration(center, clearance_radius, vertices)
                })
            });
        let Some((depth, normal)) = correction else {
            return blob.center().distance(initial_center) <= blob.rest_radius * 1.1;
        };
        blob.translate(normal * (depth + 0.5));
    }
    false
}

fn merge_circle_aabb_penetration(
    center: Vec2,
    radius: f32,
    platform: &Platform,
) -> Option<(f32, Vec2)> {
    let local = center - platform.center;
    let closest = local.clamp(-platform.half_size, platform.half_size);
    let delta = local - closest;
    let distance = delta.length();
    if distance > 0.001 {
        return (distance < radius).then(|| (radius - distance, delta / distance));
    }
    let x_clearance = platform.half_size.x - local.x.abs();
    let y_clearance = platform.half_size.y - local.y.abs();
    if x_clearance < y_clearance {
        let side = if local.x >= 0.0 { 1.0 } else { -1.0 };
        Some((radius + x_clearance, Vec2::new(side, 0.0)))
    } else {
        let side = if local.y >= 0.0 { 1.0 } else { -1.0 };
        Some((radius + y_clearance, Vec2::new(0.0, side)))
    }
}

fn merge_circle_convex_penetration(
    center: Vec2,
    radius: f32,
    vertices: &[Vec2],
) -> Option<(f32, Vec2)> {
    if vertices.len() < 3 {
        return None;
    }
    let orientation = vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .map(|(first, second)| first.perp_dot(*second))
        .sum::<f32>()
        .signum();
    if orientation == 0.0 {
        return None;
    }
    let mut inside = true;
    let mut nearest = (f32::INFINITY, Vec2::Y, Vec2::Y);
    for (first, second) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
        let edge = *second - *first;
        inside &= edge.perp_dot(center - *first) * orientation >= 0.0;
        let t = ((center - *first).dot(edge) / edge.length_squared().max(0.001)).clamp(0.0, 1.0);
        let delta = center - (*first + edge * t);
        if delta.length() < nearest.0 {
            let outward = -edge.perp() * orientation / edge.length().max(0.001);
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

fn path_is_clear(start: Vec2, end: Vec2, platforms: &[Platform]) -> bool {
    !platforms
        .iter()
        .any(|platform| segment_intersects_aabb(start, end, platform))
}

fn segment_intersects_aabb(start: Vec2, end: Vec2, platform: &Platform) -> bool {
    let minimum = platform.center - platform.half_size;
    let maximum = platform.center + platform.half_size;
    let direction = end - start;
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;

    for (origin, delta, min_axis, max_axis) in [
        (start.x, direction.x, minimum.x, maximum.x),
        (start.y, direction.y, minimum.y, maximum.y),
    ] {
        if delta.abs() < 0.0001 {
            if origin < min_axis || origin > max_axis {
                return false;
            }
            continue;
        }
        let first = (min_axis - origin) / delta;
        let second = (max_axis - origin) / delta;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
        if near > far {
            return false;
        }
    }
    far >= 0.0 && near <= 1.0
}

#[cfg(test)]
fn resolve_blob_collisions(blobs: &mut [ActiveBlob]) {
    resolve_blob_collisions_impl(blobs, |_| (true, true));
}

fn resolve_blob_collisions_with_vitality(blobs: &mut [ActiveBlob], _vitality: &VitalityWorld) {
    resolve_blob_collisions_impl(blobs, |_| (true, true));
}

fn resolve_blob_collisions_impl(
    blobs: &mut [ActiveBlob],
    interaction: impl Fn(u64) -> (bool, bool),
) {
    let crowded = blobs.len() > 2;
    for first_index in 0..blobs.len() {
        let (before_second, from_second) = blobs.split_at_mut(first_index + 1);
        let first_active = &mut before_second[first_index];
        let (first_alive, first_collides) = interaction(first_active.id);
        let first = &mut first_active.body;
        for second in from_second {
            let (second_alive, second_collides) = interaction(second.id);
            if !first_collides || !second_collides {
                continue;
            }
            let second = &mut second.body;
            let pair_scale = (first.size_scale() + second.size_scale()) * 0.5;
            // Keep a generous predictive skin for stable continuous contact,
            // but do not expose that entire skin as a visible gap between the
            // rendered membranes.
            let prediction_clearance = BLOB_CONTACT_PREDICTION_CLEARANCE * pair_scale;
            let visual_clearance = BLOB_CONTACT_VISUAL_CLEARANCE * pair_scale;
            let Some((normal, contact_points, penetration)) =
                avian_blob_contacts(first, second, prediction_clearance)
            else {
                continue;
            };

            // A predictive manifold only says that contact is imminent. Do
            // not deform the membranes or cancel their closing velocity until
            // their visible contours have actually reached one another.
            if blob_surface_gap(first, second) > visual_clearance {
                continue;
            }

            let first_mass = first.mass();
            let second_mass = second.mass();
            let total_mass = first_mass + second_mass;
            let contact_load = if crowded {
                (penetration + 1.5 * pair_scale)
                    .min(first.rest_radius.min(second.rest_radius) * 0.12)
            } else {
                (penetration + 3.0 * pair_scale)
                    .min(first.rest_radius.min(second.rest_radius) * 0.18)
            };
            let point_count = contact_points.len().max(1) as f32;
            let actual_overlap = (penetration - prediction_clearance).max(0.0);
            for point in contact_points {
                let first_load = if first_alive {
                    contact_load / point_count
                } else {
                    actual_overlap * 0.30 / point_count
                };
                let second_load = if second_alive {
                    contact_load / point_count
                } else {
                    actual_overlap * 0.30 / point_count
                };
                if first_load > 0.001 {
                    first.apply_contact_patch(point, normal, first_load, !first_alive);
                }
                if second_load > 0.001 {
                    second.apply_contact_patch(point, -normal, second_load, !second_alive);
                }
            }

            let predicted_post_correction =
                avian_blob_contacts(first, second, prediction_clearance)
                    .map(|(_, _, penetration)| penetration)
                    .unwrap_or(0.0);
            let mut post_penetration =
                (predicted_post_correction - (prediction_clearance - visual_clearance)).max(0.0);
            if crowded {
                post_penetration = post_penetration.min(BLOB_CONTACT_MAX_CORRECTION * pair_scale);
            }
            match (first_alive, second_alive) {
                (true, false) => first.translate(-normal * post_penetration),
                (false, true) => second.translate(normal * post_penetration),
                _ => {
                    first.translate(-normal * post_penetration * second_mass / total_mass);
                    second.translate(normal * post_penetration * first_mass / total_mass);
                }
            }

            // Convex contact normals can rotate slightly after a soft patch is
            // deformed. Close the tiny residual along the new centre axis so
            // the visible contours never remain interpenetrating.
            let final_delta = second.center() - first.center();
            let final_normal = final_delta.normalize_or(normal);
            let residual = (visual_clearance - blob_surface_gap(first, second)).max(0.0);
            match (first_alive, second_alive) {
                (true, false) => first.translate(-final_normal * residual),
                (false, true) => second.translate(final_normal * residual),
                _ => {
                    first.translate(-final_normal * residual * second_mass / total_mass);
                    second.translate(final_normal * residual * first_mass / total_mass);
                }
            }

            // Blob-to-blob contact can support jump charging just like level
            // geometry. Only the upper body is grounded; side contacts do not
            // arm a jump.
            if normal.y > 0.55 && second_alive {
                second.grounded = true;
                second.record_support_normal(normal);
            } else if normal.y < -0.55 && first_alive {
                first.grounded = true;
                first.record_support_normal(-normal);
            }

            let mut relative_normal_speed = (second.velocity() - first.velocity()).dot(normal);
            if crowded {
                relative_normal_speed =
                    relative_normal_speed.max(-BLOB_CONTACT_MAX_TRANSFER_SPEED * pair_scale);
            }
            if relative_normal_speed < 0.0 {
                match (first_alive, second_alive) {
                    (true, true) => {
                        first.add_velocity(normal * relative_normal_speed * 0.5);
                        second.add_velocity(-normal * relative_normal_speed * 0.5);
                    }
                    (true, false) => {
                        first.add_velocity(normal * relative_normal_speed);
                        second.damp_velocity(0.03);
                    }
                    (false, true) => {
                        second.add_velocity(-normal * relative_normal_speed);
                        first.damp_velocity(0.03);
                    }
                    (false, false) => {
                        first.damp_velocity(0.03);
                        second.damp_velocity(0.03);
                    }
                }
            }
        }
    }
}

fn avian_blob_contacts(
    first: &Blob,
    second: &Blob,
    prediction_distance: f32,
) -> Option<(Vec2, Vec<Vec2>, f32)> {
    let first_center = first.center();
    let second_center = second.center();
    let first_collider = Collider::convex_hull(
        first
            .particles
            .iter()
            .map(|particle| particle.position - first_center)
            .collect(),
    )?;
    let second_collider = Collider::convex_hull(
        second
            .particles
            .iter()
            .map(|particle| particle.position - second_center)
            .collect(),
    )?;
    let mut manifolds = Vec::<ContactManifold>::new();
    contact_manifolds(
        &first_collider,
        first_center,
        0.0,
        &second_collider,
        second_center,
        0.0,
        prediction_distance,
        &mut manifolds,
    );
    let manifold = manifolds
        .iter()
        .filter(|manifold| !manifold.points.is_empty())
        .max_by(|first, second| {
            let first_depth = first
                .points
                .iter()
                .map(|point| point.penetration)
                .fold(f32::NEG_INFINITY, f32::max);
            let second_depth = second
                .points
                .iter()
                .map(|point| point.penetration)
                .fold(f32::NEG_INFINITY, f32::max);
            first_depth.total_cmp(&second_depth)
        })?;
    let points = manifold
        .points
        .iter()
        .map(|point| point.point)
        .collect::<Vec<_>>();
    let correction = manifold
        .points
        .iter()
        .map(|point| point.penetration + prediction_distance)
        .fold(0.0, f32::max)
        .max(0.0);
    Some((manifold.normal, points, correction))
}

fn support_extent(blob: &Blob, direction: Vec2) -> f32 {
    let center = blob.center();
    blob.particles
        .iter()
        .map(|particle| (particle.position - center).dot(direction))
        .fold(0.0, f32::max)
}

fn blob_surface_gap(first: &Blob, second: &Blob) -> f32 {
    let delta = second.center() - first.center();
    let distance = delta.length();
    let normal = if distance > 0.001 {
        delta / distance
    } else {
        Vec2::X
    };
    distance - support_extent(first, normal) - support_extent(second, -normal)
}

include!("game_tests.rs");
