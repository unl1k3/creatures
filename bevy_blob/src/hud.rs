use super::*;
use crate::palette;
use bevy::{
    camera::{ClearColorConfig, RenderTarget, visibility::RenderLayers},
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    ecs::system::SystemParam,
    sprite::{Anchor, Text2dShadow},
    text::{FontWeight, TextBackgroundColor},
    window::{EnabledButtons, PrimaryWindow, WindowPosition, WindowRef, WindowResolution},
};

const CONTROLS_LAYER: usize = 1;
const METRICS_LAYER: usize = 2;
// WindowResolution describes the client area and excludes native borders and
// title bars. Keep enough logical space for those decorations on every OS.
const NATIVE_WINDOW_GAP: f32 = 64.0;
const CONTROLS: &str = "CONTROLS\n\nA / D or arrows   Roll and move\nHold Down         Charge jump\nRelease Down      Jump\nHold C            Probe for nutrient\nQ                 Pseudo-spine shield\nSpace             Radial acid burst\nX                 Split selected blob\nTab               Select next blob\nE                 Rejoin siblings\nR                 Reset game\nP                 Pause / resume\nB                 Toggle background music\nM                 Toggle ink style preview\nT                 Toggle blob dance preview\nV                 Toggle test rain\n\nLEVELS\n1                 Sewer entrance\n2                 Supports lab\n3                 Curves lab\n4                 Low passage lab\n5                 Impact lab\n6                 Split bridge lab\n7                 Small fragment seams\n8                 Nutrient wall regression\n9                 Coral basin regression\n0                 Physics overlay\n\nI / J / K / L     Move debug camera\nU / O             Debug camera zoom\nF                 Return and follow blob\nH                 Show / hide this window\nEsc               Exit";

#[derive(Resource)]
pub(super) struct LegendState {
    visible: bool,
}

#[derive(Component)]
pub(super) struct ControlsWindow;

#[derive(Component)]
pub(super) struct MetricsWindow;

#[derive(Component)]
pub(super) struct MetricsText;

type PrimaryGameWindow<'w, 's> = Single<
    'w,
    's,
    &'static Window,
    (
        With<PrimaryWindow>,
        Without<ControlsWindow>,
        Without<MetricsWindow>,
    ),
>;
type ControlsPanelWindow<'w, 's> = Single<
    'w,
    's,
    &'static mut Window,
    (
        With<ControlsWindow>,
        Without<PrimaryWindow>,
        Without<MetricsWindow>,
    ),
>;
type MetricsPanelWindow<'w, 's> = Single<
    'w,
    's,
    &'static mut Window,
    (
        With<MetricsWindow>,
        Without<PrimaryWindow>,
        Without<ControlsWindow>,
    ),
>;

#[derive(SystemParam)]
pub(super) struct AuxiliaryWindows<'w, 's> {
    primary: PrimaryGameWindow<'w, 's>,
    controls: ControlsPanelWindow<'w, 's>,
    metrics: MetricsPanelWindow<'w, 's>,
}

#[derive(SystemParam)]
pub(super) struct MetricsSources<'w> {
    diagnostics: Res<'w, DiagnosticsStore>,
    blobs: Res<'w, BlobWorld>,
    shields: Res<'w, ShieldWorld>,
    acid: Res<'w, AcidWorld>,
    vitality: Res<'w, VitalityWorld>,
    nutrition: Res<'w, NutritionWorld>,
    contacts: Res<'w, AvianContactDiagnostics>,
}

pub(super) fn setup_legend(mut commands: Commands) {
    commands.insert_resource(LegendState { visible: true });

    let controls_window = commands
        .spawn((
            ControlsWindow,
            Window {
                title: "Blob — Controls".into(),
                resolution: WindowResolution::new(390, 790),
                position: WindowPosition::At(IVec2::new(1_000, 30)),
                resizable: false,
                enabled_buttons: EnabledButtons {
                    close: false,
                    ..default()
                },
                ..default()
            },
        ))
        .id();
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(palette::color(palette::HUD_CONTROLS_BG)),
            ..default()
        },
        RenderLayers::layer(CONTROLS_LAYER),
        RenderTarget::Window(WindowRef::Entity(controls_window)),
    ));
    commands.spawn((
        Text2d::new(CONTROLS),
        TextFont {
            font_size: FontSize::Px(16.0),
            weight: FontWeight::MEDIUM,
            ..default()
        },
        TextLayout::no_wrap(),
        TextColor(palette::color(palette::HUD_TEXT)),
        TextBackgroundColor(palette::color(palette::HUD_TEXT_BG)),
        Text2dShadow {
            offset: Vec2::new(1.0, -1.0),
            color: palette::color(palette::SHADOW),
        },
        Anchor::TOP_LEFT,
        Transform::from_xyz(-178.0, 285.0, -0.01),
        RenderLayers::layer(CONTROLS_LAYER),
    ));

    let metrics_window = commands
        .spawn((
            MetricsWindow,
            Window {
                title: "Blob — Metrics".into(),
                resolution: WindowResolution::new(430, 680),
                position: WindowPosition::At(IVec2::new(1_000, 540)),
                resizable: true,
                ..default()
            },
        ))
        .id();
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(palette::color(palette::HUD_METRICS_BG)),
            ..default()
        },
        RenderLayers::layer(METRICS_LAYER),
        RenderTarget::Window(WindowRef::Entity(metrics_window)),
    ));
    commands.spawn((
        MetricsText,
        Text2d::new("METRICS\n\ncollecting frame data..."),
        TextFont {
            font_size: FontSize::Px(18.0),
            weight: FontWeight::MEDIUM,
            ..default()
        },
        TextLayout::no_wrap(),
        TextColor(palette::color(palette::HUD_METRICS_TEXT)),
        TextBackgroundColor(palette::color(palette::HUD_TEXT_BG)),
        Text2dShadow {
            offset: Vec2::new(1.0, -1.0),
            color: palette::with_alpha(palette::SHADOW, 0.76),
        },
        Anchor::TOP_LEFT,
        Transform::from_xyz(-195.0, 320.0, -0.01),
        RenderLayers::layer(METRICS_LAYER),
    ));
}

pub(super) fn arrange_auxiliary_windows(windows: AuxiliaryWindows, mut layout_frames: Local<u8>) {
    let AuxiliaryWindows {
        primary,
        mut controls,
        mut metrics,
    } = windows;
    // Winit may report the final DPI scale a few frames after creating native
    // windows. Reapply the layout briefly so all three windows use that scale.
    if *layout_frames >= 30 || primary.physical_width() == 0 {
        return;
    }

    let WindowPosition::At(game_origin) = primary.position else {
        return;
    };
    let gap = (NATIVE_WINDOW_GAP * primary.resolution.scale_factor()).round() as i32;
    let right_x = game_origin.x + primary.physical_width() as i32 + gap;
    let controls_height =
        (controls.resolution.height() * primary.resolution.scale_factor()).round() as i32;

    controls.position = WindowPosition::At(IVec2::new(right_x, game_origin.y));
    metrics.position =
        WindowPosition::At(IVec2::new(right_x, game_origin.y + controls_height + gap));
    *layout_frames += 1;
}

pub(super) fn toggle_legend(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<LegendState>,
    mut window: Single<&mut Window, With<ControlsWindow>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyH) {
        return;
    }
    state.visible = !state.visible;
    window.visible = state.visible;
}

pub(super) fn update_metrics(
    sources: MetricsSources,
    mut metrics: Single<&mut Text2d, With<MetricsText>>,
) {
    let fps = sources
        .diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|diagnostic| diagnostic.smoothed())
        .unwrap_or(0.0);
    let frame_time = sources
        .diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|diagnostic| diagnostic.smoothed())
        .unwrap_or(0.0);
    let Some(selected) = sources.blobs.active.get(sources.blobs.selected) else {
        return;
    };
    let vitality = sources.vitality.get(selected.id);
    let state = match vitality.state {
        LifeState::Alive => "alive",
        LifeState::Corpse(DeathCause::Wasting) => "corpse: wasting",
        LifeState::Corpse(DeathCause::Trauma) => "corpse: trauma",
    };
    let digestion = sources
        .nutrition
        .digestion_progress(selected.id)
        .map(|progress| format!("{:5.1}%", progress * 100.0))
        .unwrap_or_else(|| "   -- ".to_string());
    metrics.0 = format!(
        "METRICS\n\nFPS          {fps:5.1}\nFrame        {frame_time:5.2} ms\nPhysics      120 Hz\nLighting     diffuse sewer\nBlobs        {}\nPoints       {}\nSize         {:5.1}%\nState        {state}\nEnergy       {:5.1}%\nDigestion    {digestion}\nCapacity     {:5.1}%\nHealth       {:5.1}%\nTrauma       {:5.1}%\nImpact       {:5.0}\nShield       {:5.1}%\nAcid drops   {}\nAvian touch  {} / {}\nAgreement    {:5.1}%\nContact pts  {}\nSurfaces     {}\nGround pts   {}\nMax depth    {:5.2}\nSpan         {:5.1}\nFixture fix  {}\nLateral fix  {}\nShared skip  {}",
        sources.blobs.active.len(),
        selected.body.particles.len(),
        selected.body.rest_radius / INITIAL_RADIUS * 100.0,
        vitality.energy * 100.0,
        sources.nutrition.capability_factor(selected.id) * 100.0,
        vitality.health * 100.0,
        vitality.trauma.min(1.0) * 100.0,
        vitality.last_impact,
        sources.shields.energy(selected.id) * 100.0,
        sources.acid.drops.len(),
        sources.contacts.avian_contacts,
        sources.contacts.legacy_contacts,
        sources.contacts.agreement * 100.0,
        sources.contacts.selected_particles,
        sources.contacts.selected_surfaces,
        sources.contacts.selected_ground_contacts,
        sources.contacts.selected_max_depth,
        sources.contacts.selected_contact_span,
        sources.contacts.fixture_corrections,
        sources.contacts.lateral_fixture_corrections,
        sources.contacts.shared_edge_corrections,
    );
}
