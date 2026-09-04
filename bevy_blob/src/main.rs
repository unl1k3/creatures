mod acid;
mod audio;
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
pub(crate) use audio::*;
use avian2d::collision::collider::contact_query::contact_manifolds;
use avian2d::prelude::PhysicsPlugins;
use avian2d::prelude::{Collider, ContactManifold, Gravity};
use bevy::{
    app::AppExit,
    diagnostic::FrameTimeDiagnosticsPlugin,
    prelude::*,
    window::{ExitCondition, WindowPosition, WindowResolution},
};
use blob::{
    Blob, BlobStepEnvironment, BlobStepInput, BlobStepProfile, DEFAULT_CREATURE_SCALE, Platform,
    REFERENCE_RADIUS,
};
pub(crate) use blob_world::*;
#[cfg(test)]
use camera::selected_camera_target;
use camera::{GameCamera, follow_camera};
use dance::{BlobDancePreview, toggle_blob_dance};
#[cfg(feature = "dev-tools")]
use environment::switch_test_scenario;
use environment::{
    AvianContactDiagnostics, Level, LevelDebugOverlay, RouteProgress, TestScenario,
    WastewaterEffects, advance_route_progress, draw_level_chains, resolve_avian_environment,
    resolve_blob_chain_contacts, sample_avian_contacts, setup_environment,
    simulate_counterbalances, simulate_level_hazards, sync_chain_lighting, toggle_level_debug,
    update_parallax_layers,
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
use schedule::GameScheduleAppExt;
use shield::{ShieldWorld, simulate_shields, spider_climb_anchor_direction};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use vitality::{DeathCause, LifeState, Vitality, VitalityWorld, simulate_vitality};

fn main() {
    App::new()
        .insert_resource(ClearColor(palette::color(palette::IVORY)))
        .insert_resource(Time::<Fixed>::from_hz(120.0))
        .init_resource::<InkStylePreview>()
        .init_resource::<BlobDancePreview>()
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
        .register_game_systems()
        .run();
}

fn setup_game_world(mut commands: Commands, level: Res<Level>) {
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

include!("game_tests.rs");
