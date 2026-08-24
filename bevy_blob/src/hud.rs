use super::*;
use bevy::{
    camera::{ClearColorConfig, RenderTarget, visibility::RenderLayers},
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    sprite::{Anchor, Text2dShadow},
    text::{FontWeight, TextBackgroundColor},
    window::{EnabledButtons, PrimaryWindow, WindowPosition, WindowRef, WindowResolution},
};

const CONTROLS_LAYER: usize = 1;
const METRICS_LAYER: usize = 2;
// WindowResolution describes the client area and excludes native borders and
// title bars. Keep enough logical space for those decorations on every OS.
const NATIVE_WINDOW_GAP: f32 = 64.0;
const CONTROLS: &str = "CONTROLS\n\nA / D or arrows   Roll and move\nHold Down         Charge jump\nRelease Down      Jump\nHold C            Probe for nutrient\nQ                 Pseudo-spine shield\nSpace             Radial acid burst\nX                 Split selected blob\nTab               Select next blob\nE                 Rejoin siblings\nR                 Reset game\nM                 Toggle ink style preview\nF1                Standard level\nF2                Supports lab\nF3                Curves lab\nF4                U and low passage\nF5                Fall and impact lab\nF6                V and split bridge\nF7                Level physics overlay\nI / J / K / L     Move debug camera\nU / O             Debug camera zoom\nP                 Return and follow blob\nH                 Show / hide this window\nEsc               Exit";

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

pub(super) fn setup_legend(mut commands: Commands) {
    commands.insert_resource(LegendState { visible: true });

    let controls_window = commands
        .spawn((
            ControlsWindow,
            Window {
                title: "Blob — Controls".into(),
                resolution: WindowResolution::new(390, 605),
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
            clear_color: ClearColorConfig::Custom(Color::srgb(0.035, 0.065, 0.12)),
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
        TextColor(Color::srgb(0.88, 0.96, 1.0)),
        TextBackgroundColor(Color::srgba(0.025, 0.045, 0.085, 0.92)),
        Text2dShadow {
            offset: Vec2::new(1.0, -1.0),
            color: Color::srgba(0.0, 0.0, 0.0, 0.72),
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
            clear_color: ClearColorConfig::Custom(Color::srgb(0.025, 0.075, 0.075)),
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
        TextColor(Color::srgb(0.68, 1.0, 0.88)),
        TextBackgroundColor(Color::srgba(0.025, 0.045, 0.085, 0.92)),
        Text2dShadow {
            offset: Vec2::new(1.0, -1.0),
            color: Color::srgba(0.0, 0.0, 0.0, 0.76),
        },
        Anchor::TOP_LEFT,
        Transform::from_xyz(-195.0, 320.0, -0.01),
        RenderLayers::layer(METRICS_LAYER),
    ));
}

pub(super) fn arrange_auxiliary_windows(
    primary: Single<
        &Window,
        (
            With<PrimaryWindow>,
            Without<ControlsWindow>,
            Without<MetricsWindow>,
        ),
    >,
    mut controls: Single<
        &mut Window,
        (
            With<ControlsWindow>,
            Without<PrimaryWindow>,
            Without<MetricsWindow>,
        ),
    >,
    mut metrics: Single<
        &mut Window,
        (
            With<MetricsWindow>,
            Without<PrimaryWindow>,
            Without<ControlsWindow>,
        ),
    >,
    mut layout_frames: Local<u8>,
) {
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
    diagnostics: Res<DiagnosticsStore>,
    blobs: Res<BlobWorld>,
    shields: Res<ShieldWorld>,
    acid: Res<AcidWorld>,
    vitality_world: Res<VitalityWorld>,
    nutrition: Res<NutritionWorld>,
    avian_contacts: Res<AvianContactDiagnostics>,
    mut metrics: Single<&mut Text2d, With<MetricsText>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|diagnostic| diagnostic.smoothed())
        .unwrap_or(0.0);
    let frame_time = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|diagnostic| diagnostic.smoothed())
        .unwrap_or(0.0);
    let Some(selected) = blobs.active.get(blobs.selected) else {
        return;
    };
    let vitality = vitality_world.get(selected.id);
    let state = match vitality.state {
        LifeState::Alive => "alive",
        LifeState::Corpse(DeathCause::Wasting) => "corpse: wasting",
        LifeState::Corpse(DeathCause::Trauma) => "corpse: trauma",
    };
    let digestion = nutrition
        .digestion_progress(selected.id)
        .map(|progress| format!("{:5.1}%", progress * 100.0))
        .unwrap_or_else(|| "   -- ".to_string());
    metrics.0 = format!(
        "METRICS\n\nFPS          {fps:5.1}\nFrame        {frame_time:5.2} ms\nPhysics      120 Hz\nBlobs        {}\nPoints       {}\nSize         {:5.1}%\nState        {state}\nEnergy       {:5.1}%\nDigestion    {digestion}\nCapacity     {:5.1}%\nHealth       {:5.1}%\nTrauma       {:5.1}%\nImpact       {:5.0}\nShield       {:5.1}%\nAcid drops   {}\nAvian touch  {} / {}\nAgreement    {:5.1}%\nContact pts  {}\nSurfaces     {}\nGround pts   {}\nMax depth    {:5.2}\nSpan         {:5.1}\nFixture fix  {}\nLateral fix  {}\nShared skip  {}",
        blobs.active.len(),
        selected.body.particles.len(),
        selected.body.rest_radius / INITIAL_RADIUS * 100.0,
        vitality.energy * 100.0,
        nutrition.capability_factor(selected.id) * 100.0,
        vitality.health * 100.0,
        vitality.trauma.min(1.0) * 100.0,
        vitality.last_impact,
        shields.energy(selected.id) * 100.0,
        acid.drops.len(),
        avian_contacts.avian_contacts,
        avian_contacts.legacy_contacts,
        avian_contacts.agreement * 100.0,
        avian_contacts.selected_particles,
        avian_contacts.selected_surfaces,
        avian_contacts.selected_ground_contacts,
        avian_contacts.selected_max_depth,
        avian_contacts.selected_contact_span,
        avian_contacts.fixture_corrections,
        avian_contacts.lateral_fixture_corrections,
        avian_contacts.shared_edge_corrections,
    );
}
