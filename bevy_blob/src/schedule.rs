//! Explicit Bevy scheduling phases for the prototype.
//!
//! Keeping these phases outside `main.rs` makes system ordering discoverable
//! without mixing it with application setup or gameplay code.

use bevy::prelude::*;

use super::*;

/// Ordered fixed-timestep phases for physics-affecting work.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FixedGameSet {
    Actuation,
    Motion,
    Contacts,
    Consequences,
}

/// Ordered per-frame phases for input, visuals, and audio dispatch.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FrameSet {
    Input,
    Gameplay,
    Camera,
    Ambient,
    Presentation,
    Audio,
}

pub(crate) trait GameScheduleAppExt {
    fn configure_game_schedules(&mut self) -> &mut Self;
    fn register_game_systems(&mut self) -> &mut Self;
}

impl GameScheduleAppExt for App {
    fn configure_game_schedules(&mut self) -> &mut Self {
        self.configure_sets(
            FixedUpdate,
            (
                FixedGameSet::Actuation,
                FixedGameSet::Motion,
                FixedGameSet::Contacts,
                FixedGameSet::Consequences,
            )
                .chain(),
        )
        .configure_sets(
            Update,
            (
                FrameSet::Input,
                FrameSet::Gameplay,
                FrameSet::Camera,
                FrameSet::Ambient,
                FrameSet::Presentation,
                FrameSet::Audio,
            )
                .chain(),
        )
    }

    /// Registers each system exactly once in its owning execution phase.
    /// This is the single source of truth for cross-module ordering.
    fn register_game_systems(&mut self) -> &mut Self {
        self.add_systems(
            Startup,
            (
                setup_environment,
                setup_game_world,
                setup_audio,
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
                #[cfg(feature = "dev-tools")]
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
    }
}
