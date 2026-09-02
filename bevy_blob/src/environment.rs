use super::*;
use crate::BlobSoundEvent;
use crate::blob::Particle;
use crate::level_format::{
    ChainDefinition, CounterbalanceDefinition, ExpulsionPointDefinition, HazardDefinition,
    LightDefinition, NutrientDefinition, ParsedLevel, SafetyBoundsDefinition, VisualLayer,
    WastewaterAreaDefinition, parse_level,
};
use crate::nutrition::{NutrientPhysics, NutritionWorld, spawn_nutrient_bodies};
use crate::palette as game_palette;
use crate::rendering::light_dynamic_rgba;
use avian2d::prelude::{
    AngularDamping, Collider, CollisionLayers, JointCollisionDisabled, LinearDamping,
    MassPropertiesBundle, PhysicsLayer, RevoluteJoint, RigidBody, ShapeCastConfig, SpatialQuery,
    SpatialQueryFilter,
};
use bevy::{
    asset::RenderAssetUsages,
    prelude::MeshMaterial2d,
    render::{mesh::Indices, render_resource::PrimitiveTopology},
};
use std::collections::{HashMap, HashSet};

#[derive(PhysicsLayer, Clone, Copy, Debug, Default)]
pub(super) enum GameLayer {
    #[default]
    Environment,
    LivingBlob,
    Corpse,
    Projectile,
}

#[derive(Component, Debug)]
pub(super) struct EnvironmentCollider {
    platform_index: Option<usize>,
    fixture_index: Option<usize>,
}

#[derive(Component, Debug)]
pub(super) struct AvianMigratedSurface;

/// A static platform that is translated when its linked counterbalance zone
/// contains enough blob mass.
#[derive(Component)]
pub(super) struct CounterbalanceGate {
    platform_index: usize,
    closed_center: Vec2,
}
#[derive(Component)]
pub(super) struct CounterbalancePlate {
    platform_index: usize,
    closed_center: Vec2,
}

#[derive(Component)]
pub(super) struct LevelArtwork;

#[derive(Component)]
pub(super) struct ForegroundArtwork;

/// Artwork offset from its authored world position to create parallax while
/// leaving colliders and world-aligned foreground art untouched.
#[derive(Component)]
pub(super) struct ParallaxLayer {
    origin: Vec3,
    factor: f32,
}

impl ParallaxLayer {
    pub(super) fn new(origin: Vec3, factor: f32) -> Self {
        Self { origin, factor }
    }
}

#[derive(Component)]
pub(super) struct LevelChain;

#[derive(Component)]
pub(super) struct ChainAnchor {
    chain_index: usize,
}

#[derive(Component)]
pub(super) struct ChainLink {
    radius: f32,
    chain_index: usize,
    link_index: usize,
}

/// Each physical chain element owns a material so its ink darkens and warms
/// independently while swinging through the authored lantern pools.
#[derive(Component)]
pub(super) struct ChainLightMaterial(Handle<ColorMaterial>);

#[derive(Resource)]
pub(super) struct Level {
    _name: String,
    size: Vec2,
    center: Vec2,
    pub(super) safety_bounds: Option<SafetyBoundsDefinition>,
    pub(super) platforms: Vec<Platform>,
    pub(super) fixtures: Vec<Vec<Vec2>>,
    pub(super) spawn_position: Vec2,
    pub(super) route: Vec<Vec2>,
    visual_layers: Vec<VisualLayer>,
    pub(super) ice_platforms: Vec<usize>,
    pub(super) glue_platforms: Vec<usize>,
    decorations: Vec<VisualLayer>,
    pub(super) wastewater_areas: Vec<WastewaterAreaDefinition>,
    pub(super) nutrients: Vec<NutrientDefinition>,
    pub(super) lights: Vec<LightDefinition>,
    pub(super) expulsion_points: Vec<ExpulsionPointDefinition>,
    pub(super) hazards: Vec<HazardDefinition>,
    pub(super) chains: Vec<ChainDefinition>,
    pub(super) counterbalances: Vec<CounterbalanceDefinition>,
}

#[derive(Resource, Default)]
pub(super) struct TestScenario(pub(super) u8);

#[derive(Resource, Default)]
pub(super) struct LevelDebugOverlay {
    pub(super) visible: bool,
    pub(super) camera_detached: bool,
}

#[derive(Resource, Default)]
pub(super) struct RouteProgress {
    pub(super) next: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WastewaterImpact {
    pub(super) area_index: usize,
    pub(super) position: Vec2,
    pub(super) source_radius: f32,
    pub(super) variation: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WastewaterRipple {
    pub(super) area_index: usize,
    pub(super) center_x: f32,
    pub(super) age: f32,
    pub(super) duration: f32,
    pub(super) amplitude: f32,
}

#[derive(Resource, Default)]
pub(super) struct WastewaterEffects {
    pub(super) pending: Vec<WastewaterImpact>,
    pub(super) ripples: Vec<WastewaterRipple>,
    variation_serial: u64,
}

impl WastewaterEffects {
    pub(super) fn emit(
        &mut self,
        area_index: usize,
        position: Vec2,
        source_radius: f32,
        strength: f32,
    ) {
        let variation = self.next_variation();
        let strength = strength.clamp(0.2, 1.8);
        self.pending.push(WastewaterImpact {
            area_index,
            position,
            source_radius,
            variation,
        });
        self.push_ripple(area_index, position.x, source_radius, strength);
    }

    pub(super) fn emit_ripple(
        &mut self,
        area_index: usize,
        position: Vec2,
        source_radius: f32,
        strength: f32,
    ) -> f32 {
        let variation = self.next_variation();
        self.push_ripple(
            area_index,
            position.x,
            source_radius,
            strength.clamp(0.2, 1.8),
        );
        variation
    }

    fn push_ripple(&mut self, area_index: usize, center_x: f32, source_radius: f32, strength: f32) {
        self.ripples.push(WastewaterRipple {
            area_index,
            center_x,
            age: 0.0,
            duration: 1.45,
            amplitude: (source_radius * 0.42 * strength).clamp(2.0, 13.0),
        });
    }

    fn next_variation(&mut self) -> f32 {
        self.variation_serial = self.variation_serial.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.variation_serial;
        value ^= value >> 30;
        value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value ^= value >> 27;
        ((value >> 40) & 0xffff) as f32 / 65_535.0
    }

    pub(super) fn advance(&mut self, dt: f32) {
        for ripple in &mut self.ripples {
            ripple.age += dt;
        }
        self.ripples.retain(|ripple| ripple.age < ripple.duration);
    }

    pub(super) fn surface_offset(&self, area_index: usize, world_x: f32) -> f32 {
        self.ripples
            .iter()
            .filter(|ripple| ripple.area_index == area_index)
            .map(|ripple| {
                let distance = (world_x - ripple.center_x).abs();
                let front = ripple.age * 105.0;
                let band = (1.0 - (distance - front).abs() / 52.0).max(0.0);
                let decay = (1.0 - ripple.age / ripple.duration).powi(2);
                let oscillation = ((distance - front) * 0.15).sin();
                let initial_depression = (1.0 - ripple.age / 0.22).max(0.0)
                    * (-ripple.amplitude * 0.55)
                    * (1.0 - distance / 36.0).max(0.0);
                oscillation * band * decay * ripple.amplitude + initial_depression
            })
            .sum()
    }
}

#[derive(Resource, Default)]
pub(super) struct AvianContactDiagnostics {
    pub(super) particles: usize,
    pub(super) avian_contacts: usize,
    pub(super) legacy_contacts: usize,
    pub(super) agreement: f32,
    pub(super) selected_surfaces: usize,
    pub(super) selected_particles: usize,
    pub(super) selected_ground_contacts: usize,
    pub(super) selected_max_depth: f32,
    pub(super) selected_contact_span: f32,
    pub(super) fixture_corrections: usize,
    pub(super) lateral_fixture_corrections: usize,
    pub(super) shared_edge_corrections: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AvianMembraneContact {
    pub(super) particle_index: usize,
    pub(super) collider: Entity,
    pub(super) point: Vec2,
    pub(super) normal: Vec2,
    pub(super) depth: f32,
}

#[derive(Resource, Default)]
pub(super) struct AvianContactManifolds {
    pub(super) by_blob: HashMap<u64, Vec<AvianMembraneContact>>,
}

impl Level {
    fn prototype() -> Self {
        let parsed = parse_level(include_str!("../assets/levels/sewer_01/level.json"))
            .expect("embedded sewer_01 level must be valid");
        Self::from_parsed(parsed)
    }

    fn from_parsed(parsed: ParsedLevel) -> Self {
        Self {
            _name: parsed.name,
            size: parsed.size,
            center: parsed.center,
            safety_bounds: parsed.safety_bounds,
            platforms: parsed.platforms,
            fixtures: parsed.fixtures,
            spawn_position: parsed.spawn,
            route: parsed.route,
            visual_layers: parsed.visual_layers,
            ice_platforms: parsed.ice_platforms,
            glue_platforms: parsed.glue_platforms,
            nutrients: parsed.nutrients,
            lights: Vec::new(),
            expulsion_points: parsed.expulsion_points,
            hazards: parsed.hazards,
            chains: parsed.chains,
            decorations: parsed.decorations,
            wastewater_areas: parsed.wastewater_areas,
            counterbalances: parsed.counterbalances,
        }
    }

    pub(super) fn has_artwork(&self) -> bool {
        !self.visual_layers.is_empty()
    }

    pub(super) fn size(&self) -> Vec2 {
        self.size
    }

    pub(super) fn center(&self) -> Vec2 {
        self.center
    }

    #[cfg(test)]
    pub(super) fn from_test_geometry(platforms: Vec<Platform>, fixtures: Vec<Vec<Vec2>>) -> Self {
        Self {
            _name: "Test level".into(),
            size: Vec2::splat(1000.0),
            center: Vec2::ZERO,
            safety_bounds: None,
            platforms,
            fixtures,
            spawn_position: Vec2::ZERO,
            route: Vec::new(),
            visual_layers: Vec::new(),
            ice_platforms: Vec::new(),
            glue_platforms: Vec::new(),
            decorations: Vec::new(),
            wastewater_areas: Vec::new(),
            nutrients: Vec::new(),
            lights: Vec::new(),
            expulsion_points: Vec::new(),
            hazards: Vec::new(),
            chains: Vec::new(),
            counterbalances: Vec::new(),
        }
    }

    pub(super) fn test_scenario(index: u8) -> (Self, Vec2) {
        let external_level = match index {
            2 => Some(include_str!("../assets/levels/supports_lab/level.json")),
            3 => Some(include_str!("../assets/levels/curves_lab/level.json")),
            4 => Some(include_str!("../assets/levels/low_passage_lab/level.json")),
            5 => Some(include_str!("../assets/levels/impact_lab/level.json")),
            6 => Some(include_str!("../assets/levels/split_bridge_lab/level.json")),
            7 => Some(include_str!(
                "../assets/levels/regression_fragment_seams/level.json"
            )),
            8 => Some(include_str!(
                "../assets/levels/regression_nutrient_wall/level.json"
            )),
            9 => Some(include_str!(
                "../assets/levels/regression_coral_basin/level.json"
            )),
            _ => None,
        };
        if let Some(source) = external_level {
            return Self::from_embedded_regression(source);
        }
        match index {
            2 => (
                Self {
                    _name: "Supports lab".into(),
                    size: Vec2::new(760.0, 900.0),
                    center: Vec2::ZERO,
                    safety_bounds: None,
                    platforms: vec![
                        platform(0.0, -370.0, 760.0, 38.0),
                        platform(-245.0, -265.0, 70.0, 170.0),
                        platform(-105.0, -315.0, 105.0, 70.0),
                        platform(10.0, -270.0, 105.0, 160.0),
                        platform(170.0, -225.0, 105.0, 250.0),
                        platform(295.0, 55.0, 120.0, 28.0),
                    ],
                    fixtures: Vec::new(),
                    spawn_position: Vec2::new(-320.0, -285.0),
                    route: vec![
                        Vec2::new(-320.0, -285.0),
                        Vec2::new(-245.0, -140.0),
                        Vec2::new(-105.0, -240.0),
                        Vec2::new(10.0, -150.0),
                        Vec2::new(170.0, -60.0),
                        Vec2::new(295.0, 110.0),
                    ],
                    visual_layers: Vec::new(),
                    ice_platforms: Vec::new(),
                    glue_platforms: Vec::new(),
                    decorations: Vec::new(),
                    wastewater_areas: Vec::new(),
                    nutrients: Vec::new(),
                    lights: Vec::new(),
                    expulsion_points: Vec::new(),
                    hazards: Vec::new(),
                    chains: Vec::new(),
                    counterbalances: Vec::new(),
                },
                Vec2::new(-320.0, -285.0),
            ),
            3 => (
                Self {
                    _name: "Curves lab".into(),
                    size: Vec2::new(1000.0, 900.0),
                    center: Vec2::ZERO,
                    safety_bounds: None,
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(350.0, 0.0, 105.0, 24.0),
                        platform(470.0, 145.0, 80.0, 24.0),
                    ],
                    fixtures: {
                        let mut fixtures = vec![vec![
                            Vec2::new(-340.0, -370.0),
                            Vec2::new(80.0, -370.0),
                            Vec2::new(80.0, -280.0),
                        ]];
                        fixtures.push(semicircle_fixture(Vec2::new(220.0, -250.0), 105.0, 28.0));
                        fixtures.extend(wave_fixtures(-330.0, 330.0, 285.0, 220.0, 9));
                        fixtures
                    },
                    // Fall onto the shared vertex between two upper wave
                    // segments so the problematic contact is reproducible.
                    spawn_position: Vec2::new(36.67, 430.0),
                    route: vec![
                        Vec2::new(-300.0, -285.0),
                        Vec2::new(-150.0, -270.0),
                        Vec2::new(20.0, -220.0),
                        Vec2::new(220.0, -105.0),
                        Vec2::new(350.0, 55.0),
                        Vec2::new(470.0, 200.0),
                        Vec2::new(320.0, 330.0),
                        Vec2::new(120.0, 330.0),
                        Vec2::new(-80.0, 330.0),
                        Vec2::new(-260.0, 320.0),
                    ],
                    visual_layers: Vec::new(),
                    ice_platforms: Vec::new(),
                    glue_platforms: Vec::new(),
                    decorations: Vec::new(),
                    wastewater_areas: Vec::new(),
                    nutrients: Vec::new(),
                    lights: Vec::new(),
                    expulsion_points: Vec::new(),
                    hazards: Vec::new(),
                    chains: Vec::new(),
                    counterbalances: Vec::new(),
                },
                Vec2::new(36.67, 430.0),
            ),
            4 => (
                Self {
                    _name: "Low passage lab".into(),
                    size: Vec2::new(760.0, 900.0),
                    center: Vec2::ZERO,
                    safety_bounds: None,
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(-210.0, -285.0, 28.0, 190.0),
                        platform(10.0, -285.0, 28.0, 190.0),
                        platform(-100.0, -365.0, 248.0, 28.0),
                        platform(235.0, -250.0, 250.0, 28.0),
                    ],
                    fixtures: Vec::new(),
                    spawn_position: Vec2::new(-100.0, -245.0),
                    route: vec![
                        Vec2::new(-100.0, -245.0),
                        Vec2::new(-25.0, -145.0),
                        Vec2::new(80.0, -310.0),
                        Vec2::new(220.0, -310.0),
                        Vec2::new(355.0, -310.0),
                    ],
                    visual_layers: Vec::new(),
                    ice_platforms: Vec::new(),
                    glue_platforms: Vec::new(),
                    decorations: Vec::new(),
                    wastewater_areas: Vec::new(),
                    nutrients: Vec::new(),
                    lights: Vec::new(),
                    expulsion_points: Vec::new(),
                    hazards: Vec::new(),
                    chains: Vec::new(),
                    counterbalances: Vec::new(),
                },
                Vec2::new(-100.0, -245.0),
            ),
            5 => (
                Self {
                    _name: "Impact lab".into(),
                    size: Vec2::new(900.0, 1100.0),
                    center: Vec2::ZERO,
                    safety_bounds: None,
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(-185.0, -245.0, 125.0, 24.0),
                        platform(20.0, -105.0, 105.0, 24.0),
                        platform(245.0, 45.0, 105.0, 24.0),
                        platform(20.0, 185.0, 115.0, 24.0),
                        platform(-220.0, 335.0, 95.0, 24.0),
                        platform(-40.0, 475.0, 110.0, 24.0),
                        platform(130.0, 600.0, 120.0, 24.0),
                        platform(245.0, 470.0, 26.0, 260.0),
                        platform(365.0, 470.0, 26.0, 260.0),
                    ],
                    fixtures: Vec::new(),
                    spawn_position: Vec2::new(-300.0, -285.0),
                    route: vec![
                        Vec2::new(-300.0, -285.0),
                        Vec2::new(-185.0, -190.0),
                        Vec2::new(20.0, -50.0),
                        Vec2::new(245.0, 100.0),
                        Vec2::new(20.0, 240.0),
                        Vec2::new(-220.0, 390.0),
                        Vec2::new(-40.0, 530.0),
                        Vec2::new(130.0, 655.0),
                        Vec2::new(305.0, 650.0),
                    ],
                    visual_layers: Vec::new(),
                    ice_platforms: Vec::new(),
                    glue_platforms: Vec::new(),
                    decorations: Vec::new(),
                    wastewater_areas: Vec::new(),
                    nutrients: Vec::new(),
                    lights: Vec::new(),
                    expulsion_points: Vec::new(),
                    hazards: Vec::new(),
                    chains: Vec::new(),
                    counterbalances: Vec::new(),
                },
                Vec2::new(-300.0, -285.0),
            ),
            6 => (
                Self {
                    _name: "Split bridge lab".into(),
                    size: Vec2::new(760.0, 900.0),
                    center: Vec2::ZERO,
                    safety_bounds: None,
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(270.0, -40.0, 105.0, 24.0),
                        platform(155.0, 115.0, 130.0, 24.0),
                        platform(-45.0, 115.0, 130.0, 24.0),
                    ],
                    fixtures: v_valley_fixtures(Vec2::new(0.0, -180.0), 300.0, 120.0),
                    spawn_position: Vec2::new(0.0, -125.0),
                    route: vec![
                        Vec2::new(0.0, -125.0),
                        Vec2::new(145.0, -15.0),
                        Vec2::new(270.0, 15.0),
                        Vec2::new(155.0, 170.0),
                        Vec2::new(-45.0, 170.0),
                    ],
                    visual_layers: Vec::new(),
                    ice_platforms: Vec::new(),
                    glue_platforms: Vec::new(),
                    decorations: Vec::new(),
                    wastewater_areas: Vec::new(),
                    nutrients: Vec::new(),
                    lights: Vec::new(),
                    expulsion_points: Vec::new(),
                    hazards: Vec::new(),
                    chains: Vec::new(),
                    counterbalances: Vec::new(),
                },
                Vec2::new(0.0, -125.0),
            ),
            _ => (Self::prototype(), BLOB_START),
        }
    }

    fn from_embedded_regression(source: &str) -> (Self, Vec2) {
        let level = Self::from_parsed(
            parse_level(source).expect("embedded regression level must be valid"),
        );
        let spawn = level.spawn_position;
        (level, spawn)
    }
}

fn semicircle_fixture(center: Vec2, radius: f32, depth: f32) -> Vec<Vec2> {
    let mut vertices = vec![center + Vec2::new(-radius, -depth)];
    for step in 0..=16 {
        let x = -radius + radius * 2.0 * step as f32 / 16.0;
        let y = (radius * radius - x * x).max(0.0).sqrt();
        vertices.push(center + Vec2::new(x, y));
    }
    vertices.push(center + Vec2::new(radius, -depth));
    vertices
}

fn wave_fixtures(
    minimum_x: f32,
    maximum_x: f32,
    baseline: f32,
    bottom: f32,
    segments: usize,
) -> Vec<Vec<Vec2>> {
    (0..segments)
        .map(|segment| {
            let fraction_a = segment as f32 / segments as f32;
            let fraction_b = (segment + 1) as f32 / segments as f32;
            let x_a = minimum_x + (maximum_x - minimum_x) * fraction_a;
            let x_b = minimum_x + (maximum_x - minimum_x) * fraction_b;
            let y_a = baseline + (fraction_a * std::f32::consts::TAU * 1.5).sin() * 48.0;
            let y_b = baseline + (fraction_b * std::f32::consts::TAU * 1.5).sin() * 48.0;
            vec![
                Vec2::new(x_a, bottom),
                Vec2::new(x_b, bottom),
                Vec2::new(x_b, y_b),
                Vec2::new(x_a, y_a),
            ]
        })
        .collect()
}

fn v_valley_fixtures(center: Vec2, width: f32, depth: f32) -> Vec<Vec<Vec2>> {
    let half = width * 0.5;
    vec![
        vec![
            center + Vec2::new(-half, -depth),
            center + Vec2::new(0.0, -depth),
            center,
            center + Vec2::new(-half, depth),
        ],
        vec![
            center + Vec2::new(0.0, -depth),
            center + Vec2::new(half, -depth),
            center + Vec2::new(half, depth),
            center,
        ],
    ]
}

pub(super) fn setup_environment(
    mut commands: Commands,
    asset_server: Option<Res<AssetServer>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<ColorMaterial>>>,
) {
    let level = Level::prototype();
    spawn_level_colliders(&mut commands, &level);
    spawn_level_artwork(&mut commands, asset_server.as_deref(), &level);
    if let (Some(mut meshes), Some(mut materials)) = (meshes, materials) {
        spawn_level_chains(&mut commands, &level, &mut meshes, &mut materials);
    }
    commands.insert_resource(level);
    commands.insert_resource(TestScenario::default());
    commands.insert_resource(LevelDebugOverlay::default());
    commands.insert_resource(RouteProgress { next: 1 });
    commands.insert_resource(WastewaterEffects::default());
    commands.insert_resource(AvianContactDiagnostics::default());
    commands.insert_resource(AvianContactManifolds::default());
}

pub(super) fn toggle_level_debug(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut overlay: ResMut<LevelDebugOverlay>,
) {
    let selecting_lighting = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    if !selecting_lighting
        && (keyboard.just_pressed(KeyCode::Digit0) || keyboard.just_pressed(KeyCode::Backquote))
    {
        overlay.visible = !overlay.visible;
        if overlay.visible {
            overlay.camera_detached = true;
        }
    }
}

fn spawn_level_artwork(commands: &mut Commands, asset_server: Option<&AssetServer>, level: &Level) {
    let Some(asset_server) = asset_server else {
        return;
    };
    for layer in &level.visual_layers {
        commands.spawn((
            LevelArtwork,
            Sprite {
                image: asset_server.load(layer.image.clone()),
                custom_size: Some(layer.size),
                ..default()
            },
            Transform::from_translation(layer.position.extend(layer.depth)),
            ParallaxLayer::new(layer.position.extend(layer.depth), layer.parallax),
        ));
    }
    for layer in &level.decorations {
        commands.spawn((
            LevelArtwork,
            ForegroundArtwork,
            Sprite {
                image: asset_server.load(layer.image.clone()),
                custom_size: Some(layer.size),
                ..default()
            },
            Transform::from_translation(layer.position.extend(layer.depth)),
            ParallaxLayer::new(layer.position.extend(layer.depth), layer.parallax),
        ));
    }
}

/// Applies parallax in world space after the camera has followed the active
/// blob. A factor of one preserves the exact authored alignment.
pub(super) fn update_parallax_layers(
    camera: Single<&Transform, With<GameCamera>>,
    mut layers: Query<(&ParallaxLayer, &mut Transform), Without<GameCamera>>,
) {
    let camera_position = camera.translation.truncate();
    for (layer, mut transform) in &mut layers {
        let offset = camera_position * (1.0 - layer.factor);
        transform.translation = layer.origin + offset.extend(0.0);
    }
}

fn spawn_level_chains(
    commands: &mut Commands,
    level: &Level,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    for (chain_index, chain) in level.chains.iter().enumerate() {
        let mesh = meshes.add(ellipse_ring_mesh(
            Vec2::new(chain.link_radius * 0.56, chain.link_radius * 0.95),
            Vec2::new(chain.link_radius * 0.88, chain.link_radius * 1.28),
            16,
        ));
        let anchor_material = materials.add(ColorMaterial::from(game_palette::color(
            light_dynamic_rgba(game_palette::INK, chain.anchor, &level.lights),
        )));
        let anchor = commands
            .spawn((
                Name::new(format!("Chain anchor: {}", chain.id)),
                LevelChain,
                ChainAnchor { chain_index },
                RigidBody::Kinematic,
                Mesh2d(meshes.add(ring_mesh(1.5, 5.5, 12))),
                MeshMaterial2d(anchor_material.clone()),
                ChainLightMaterial(anchor_material),
                Transform::from_translation(chain.anchor.extend(0.0)),
            ))
            .id();
        let mut previous = anchor;
        for link_index in 0..chain.links {
            let position = chain.anchor - Vec2::Y * chain.spacing * (link_index + 1) as f32;
            let material = materials.add(ColorMaterial::from(game_palette::color(
                light_dynamic_rgba(game_palette::INK, position, &level.lights),
            )));
            let link = commands
                .spawn((
                    Name::new(format!("Chain link {}: {link_index}", chain.id)),
                    LevelChain,
                    ChainLink {
                        radius: chain.link_radius,
                        chain_index,
                        link_index,
                    },
                    RigidBody::Dynamic,
                    Collider::circle(chain.link_radius),
                    MassPropertiesBundle::from_shape(&Circle::new(chain.link_radius), 0.7),
                    LinearDamping(1.1),
                    AngularDamping(1.8),
                    CollisionLayers::new(
                        [GameLayer::Projectile],
                        [GameLayer::Environment, GameLayer::Projectile],
                    ),
                    Mesh2d(mesh.clone()),
                    MeshMaterial2d(material.clone()),
                    ChainLightMaterial(material),
                    Transform::from_translation(position.extend(0.12)).with_rotation(
                        Quat::from_rotation_z(if link_index % 2 == 0 { 0.0 } else { 0.35 }),
                    ),
                ))
                .id();
            commands.spawn((
                LevelChain,
                RevoluteJoint::new(previous, link)
                    .with_local_anchor2(Vec2::Y * chain.spacing)
                    .with_point_compliance(0.000_01),
                JointCollisionDisabled,
            ));
            previous = link;
        }
    }
}

/// Updates chain ink from the current physical positions. The links are Avian
/// bodies, so their light must be sampled after the physics step rather than
/// from their JSON spawn coordinates.
pub(super) fn sync_chain_lighting(
    level: Res<Level>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    chains: Query<(&Transform, &ChainLightMaterial)>,
) {
    for (transform, material_handle) in &chains {
        let Some(mut material) = materials.get_mut(&material_handle.0) else {
            continue;
        };
        material.color = game_palette::color(light_dynamic_rgba(
            game_palette::INK,
            transform.translation.truncate(),
            &level.lights,
        ));
    }
}

/// Thin bridge marks show the alternating links that are seen edge-on.
pub(super) fn draw_level_chains(
    mut gizmos: Gizmos,
    level: Res<Level>,
    links: Query<(&Transform, &ChainLink)>,
    anchors: Query<(&Transform, &ChainAnchor)>,
) {
    let mut ordered = links
        .iter()
        .map(|(transform, link)| {
            (
                link.chain_index,
                link.link_index,
                transform.translation.truncate(),
            )
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(chain, link, _)| (*chain, *link));
    for pair in ordered.windows(2) {
        let [
            (first_chain, first_link, first),
            (second_chain, second_link, second),
        ] = pair
        else {
            continue;
        };
        if first_chain != second_chain || *second_link != *first_link + 1 {
            continue;
        }
        draw_chain_stroke(&mut gizmos, *first, *second, &level.lights);
    }
    for (anchor_transform, anchor) in &anchors {
        let anchor_position = anchor_transform.translation.truncate();
        if let Some((_, _, first_link)) = ordered.iter().find(|(chain_index, link_index, _)| {
            *chain_index == anchor.chain_index && *link_index == 0
        }) {
            draw_chain_stroke(&mut gizmos, anchor_position, *first_link, &level.lights);
        }
    }
}

fn draw_chain_stroke(gizmos: &mut Gizmos, start: Vec2, end: Vec2, lights: &[LightDefinition]) {
    let direction = (end - start).normalize_or(Vec2::NEG_Y);
    let normal = Vec2::new(-direction.y, direction.x);
    let start = start + direction * 4.0;
    let end = end - direction * 4.0;
    let ink = game_palette::color(light_dynamic_rgba(
        game_palette::INK,
        (start + end) * 0.5,
        lights,
    ));
    for offset in [-1.0, 0.0, 1.0] {
        gizmos.line_2d(start + normal * offset, end + normal * offset, ink);
    }
}

fn ring_mesh(inner_radius: f32, outer_radius: f32, segments: usize) -> Mesh {
    ellipse_ring_mesh(
        Vec2::splat(inner_radius),
        Vec2::splat(outer_radius),
        segments,
    )
}

fn ellipse_ring_mesh(inner_radius: Vec2, outer_radius: Vec2, segments: usize) -> Mesh {
    let segments = segments.max(3);
    let mut positions = Vec::with_capacity(segments * 2);
    for index in 0..segments {
        let angle = index as f32 / segments as f32 * std::f32::consts::TAU;
        positions.push([
            outer_radius.x * angle.cos(),
            outer_radius.y * angle.sin(),
            0.0,
        ]);
        positions.push([
            inner_radius.x * angle.cos(),
            inner_radius.y * angle.sin(),
            0.0,
        ]);
    }
    let mut indices = Vec::with_capacity(segments * 6);
    for index in 0..segments {
        let next = (index + 1) % segments;
        let outer = (index * 2) as u32;
        let inner = outer + 1;
        let next_outer = (next * 2) as u32;
        let next_inner = next_outer + 1;
        indices.extend_from_slice(&[outer, next_outer, inner, inner, next_outer, next_inner]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Couples the blob's custom membrane solver to Avian chain links. Each link
/// resolves only its closest membrane point, preventing a hanging chain from
/// behaving like a continuous rigid wall.
pub(super) fn resolve_blob_chain_contacts(
    time: Res<Time<Fixed>>,
    mut blobs: ResMut<BlobWorld>,
    mut links: Query<(&Transform, &ChainLink, &mut LinearVelocity)>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    for (transform, link, mut velocity) in &mut links {
        let link_position = transform.translation.truncate();
        for active_blob in &mut blobs.active {
            let blob_center = active_blob.body.center();
            let blob_velocity = active_blob
                .body
                .particles
                .iter()
                .map(|particle| particle.position - particle.previous)
                .sum::<Vec2>()
                / active_blob.body.particles.len().max(1) as f32;
            let average_radius = active_blob
                .body
                .particles
                .iter()
                .map(|particle| particle.position.distance(blob_center))
                .sum::<f32>()
                / active_blob.body.particles.len().max(1) as f32;
            let volume_radius = average_radius.max(active_blob.body.rest_radius * 0.70);
            let center_offset = link_position - blob_center;
            let center_distance = center_offset.length();
            // A link inside the body receives pressure from the blob volume,
            // rather than waiting until it reaches one membrane particle.
            if center_distance < volume_radius {
                let volume_normal = center_offset.normalize_or(Vec2::Y);
                let depth = volume_radius - center_distance;
                // Transfer both the body's travel and its volumetric pressure.
                // This is intentionally stronger than a membrane-only hit:
                // a chain should visibly yield when a blob presses through it.
                **velocity += blob_velocity * 0.48 + volume_normal * (depth * 5.2).min(220.0);
            }
            let skin = 2.0 * active_blob.body.size_scale();
            let minimum_distance = link.radius + skin;
            let Some((particle_index, distance)) = active_blob
                .body
                .particles
                .iter()
                .enumerate()
                .map(|(index, particle)| (index, particle.position.distance(link_position)))
                .min_by(|(_, first), (_, second)| first.total_cmp(second))
            else {
                continue;
            };
            if distance >= minimum_distance {
                continue;
            }
            let particle = &mut active_blob.body.particles[particle_index];
            let normal = (particle.position - link_position).normalize_or(Vec2::Y);
            let incoming = particle.position - particle.previous;
            let penetration = minimum_distance - distance;
            // A soft partial correction lets the membrane fold around a link.
            let correction = normal * penetration * 0.42;
            particle.position += correction;
            particle.previous += correction * 0.35;
            let impact = (-incoming.dot(normal)).max(0.0);
            **velocity -= normal * (impact * 0.55 + penetration * 5.0);
            let impact_speed = impact / time.delta_secs().max(0.000_001);
            if impact_speed >= 95.0 {
                sound_events.write(BlobSoundEvent::ChainImpact {
                    strength: (impact_speed / 420.0).clamp(0.0, 1.0),
                });
            }
        }
    }
}

fn spawn_level_colliders(commands: &mut Commands, level: &Level) {
    for (platform_index, platform) in level.platforms.iter().copied().enumerate() {
        let mut entity = commands.spawn((
            EnvironmentCollider {
                platform_index: Some(platform_index),
                fixture_index: None,
            },
            RigidBody::Static,
            Collider::rectangle(platform.half_size.x * 2.0, platform.half_size.y * 2.0),
            CollisionLayers::new(
                [GameLayer::Environment],
                [
                    GameLayer::LivingBlob,
                    GameLayer::Corpse,
                    GameLayer::Projectile,
                ],
            ),
            Transform::from_xyz(platform.center.x, platform.center.y, 0.0),
        ));
        if platform_index <= 3 {
            entity.insert(AvianMigratedSurface);
        }
        if level
            .counterbalances
            .iter()
            .any(|balance| balance.gate_platform == platform_index)
        {
            entity.insert(CounterbalanceGate {
                platform_index,
                closed_center: platform.center,
            });
        }
        if level
            .counterbalances
            .iter()
            .any(|balance| balance.plate_platform == platform_index)
        {
            entity.insert(CounterbalancePlate {
                platform_index,
                closed_center: platform.center,
            });
        }
    }
    for (fixture_index, vertices) in level.fixtures.iter().enumerate() {
        if let Some(collider) = Collider::convex_hull(vertices.clone()) {
            commands.spawn((
                EnvironmentCollider {
                    platform_index: None,
                    fixture_index: Some(fixture_index),
                },
                AvianMigratedSurface,
                RigidBody::Static,
                collider,
                CollisionLayers::new(
                    [GameLayer::Environment],
                    [
                        GameLayer::LivingBlob,
                        GameLayer::Corpse,
                        GameLayer::Projectile,
                    ],
                ),
            ));
        }
    }
}

/// Opens a linked gate while a sufficiently large blob occupies the recessed
/// counterbalance zone. Updating both the authored platform and its Avian
/// collider keeps the soft-body and rigid-body views of the level identical.
pub(super) fn simulate_counterbalances(
    time: Res<Time<Fixed>>,
    mut blobs: ResMut<BlobWorld>,
    mut level: ResMut<Level>,
    // These are disjoint entity sets. State it explicitly so Bevy can safely
    // borrow both mutable transforms in the same system.
    mut gates: Query<(&CounterbalanceGate, &mut Transform), Without<CounterbalancePlate>>,
    mut plates: Query<(&CounterbalancePlate, &mut Transform), Without<CounterbalanceGate>>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    let balances = level.counterbalances.clone();
    for balance in balances {
        let plate_surface = level.platforms[balance.plate_platform];
        let mut load = 0.0;
        let mut riders = Vec::new();
        for (index, blob) in blobs.active.iter().enumerate() {
            let center = blob.body.center();
            let radius = blob.body.rest_radius;
            let plate_top = plate_surface.center.y + plate_surface.half_size.y;
            // Do not arm the mechanism merely because a blob passes next to
            // it. Its lower membrane must first reach the upper plate face.
            let rides_plate = (center.x - plate_surface.center.x).abs()
                <= plate_surface.half_size.x + radius * 0.3
                && center.y - radius <= plate_top + 5.0
                // A blob climbing through the well can brush the plate from
                // below; it must not count as a counterweight in that case.
                && center.y >= plate_surface.center.y;
            if rides_plate {
                load += radius;
                riders.push(index);
            }
        }
        let closed = gates
            .iter()
            .find(|(gate, _)| gate.platform_index == balance.gate_platform)
            .map(|(gate, _)| gate.closed_center)
            .unwrap_or(level.platforms[balance.gate_platform].center);
        let lift = (load / balance.minimum_radius).clamp(0.0, 1.0);
        // A single cable transmits the same travel at both ends: the plate
        // descends exactly as far as the gate rises.
        let current_gate = level.platforms[balance.gate_platform].center;
        let desired = closed + balance.open_offset * lift;
        // Slow enough for a resting soft body to follow the plate instead of
        // receiving a sharp correction that looks like a bounce.
        let blend = 1.0 - (-0.85 * time.delta_secs()).exp();
        let desired = current_gate.lerp(desired, blend);
        if desired.distance_squared(current_gate) > 0.0025 {
            sound_events.write(BlobSoundEvent::MechanismMove);
        }
        level.platforms[balance.gate_platform].center = desired;
        for (gate, mut transform) in &mut gates {
            if gate.platform_index == balance.gate_platform {
                transform.translation = desired.extend(0.0);
            }
        }
        let plate_index = balance.plate_platform;
        for (plate, mut transform) in &mut plates {
            if plate.platform_index == plate_index {
                let plate_target = plate.closed_center - balance.open_offset * lift;
                let previous_position = level.platforms[plate_index].center;
                let plate_position = level.platforms[plate_index]
                    .center
                    .lerp(plate_target, blend);
                let plate_delta = plate_position - previous_position;
                level.platforms[plate_index].center = plate_position;
                transform.translation = plate_position.extend(0.0);
                // The custom soft body does not receive rigid-body conveyor
                // velocity from Avian. Move actual riders with the plate so
                // its surface remains a support rather than passing through
                // their smaller membrane particles.
                for &index in &riders {
                    for particle in &mut blobs.active[index].body.particles {
                        particle.position += plate_delta;
                        particle.previous += plate_delta;
                    }
                }
            }
        }
    }
}

pub(super) fn switch_test_scenario(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    colliders: Query<Entity, With<EnvironmentCollider>>,
    artwork: Query<Entity, With<LevelArtwork>>,
    chains: Query<Entity, With<LevelChain>>,
    asset_server: Option<Res<AssetServer>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<ColorMaterial>>>,
    mut scenario: ResMut<TestScenario>,
    mut route_progress: ResMut<RouteProgress>,
    mut level: ResMut<Level>,
    mut blobs: ResMut<BlobWorld>,
    mut vitality: ResMut<VitalityWorld>,
    mut nutrition: ResMut<NutritionWorld>,
    nutrient_bodies: Query<Entity, With<NutrientPhysics>>,
) {
    // Number keys select levels; modified combinations are deliberately left
    // unused so they cannot trigger a scenario while typing a shortcut.
    if keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        return;
    }
    let requested = [
        (1, KeyCode::Digit1),
        (2, KeyCode::Digit2),
        (3, KeyCode::Digit3),
        (4, KeyCode::Digit4),
        (5, KeyCode::Digit5),
        (6, KeyCode::Digit6),
        (7, KeyCode::Digit7),
        (8, KeyCode::Digit8),
        (9, KeyCode::Digit9),
    ]
    .into_iter()
    .find_map(|(index, key)| keyboard.just_pressed(key).then_some(index));
    let Some(requested) = requested else {
        return;
    };
    for entity in &colliders {
        commands.entity(entity).despawn();
    }
    for entity in &artwork {
        commands.entity(entity).despawn();
    }
    for entity in &chains {
        commands.entity(entity).despawn();
    }
    for entity in &nutrient_bodies {
        commands.entity(entity).despawn();
    }
    let (new_level, spawn) = Level::test_scenario(requested);
    spawn_level_artwork(&mut commands, asset_server.as_deref(), &new_level);
    if let (Some(mut meshes), Some(mut materials)) = (meshes, materials) {
        spawn_level_chains(&mut commands, &new_level, &mut meshes, &mut materials);
    }
    spawn_level_colliders(&mut commands, &new_level);
    *level = new_level;
    scenario.0 = requested;
    route_progress.next = 1;
    reset_world_at(&mut blobs, spawn);
    vitality.reset();
    nutrition.reset_from_definitions(&level.nutrients);
    spawn_nutrient_bodies(&mut commands, &level.nutrients);
}

pub(super) fn simulate_level_hazards(
    time: Res<Time<Fixed>>,
    level: Res<Level>,
    blobs: Res<BlobWorld>,
    mut vitality: ResMut<VitalityWorld>,
) {
    let dt = time.delta_secs();
    for active_blob in &blobs.active {
        for hazard in &level.hazards {
            let half_size = hazard.size * 0.5;
            if active_blob.body.particles.iter().any(|particle| {
                let offset = (particle.position - hazard.position).abs();
                offset.x <= half_size.x && offset.y <= half_size.y
            }) {
                vitality.damage(active_blob.id, hazard.damage_per_second * dt);
            }
        }
    }
}

pub(super) fn advance_route_progress(
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    mut progress: ResMut<RouteProgress>,
) {
    let Some(blob) = blobs.active.get(blobs.selected) else {
        return;
    };
    while let Some(checkpoint) = level.route.get(progress.next) {
        let reach = (blob.body.rest_radius * 1.45).max(52.0);
        if blob.body.center().distance(*checkpoint) > reach {
            break;
        }
        progress.next += 1;
    }
}

pub(super) fn resolve_avian_environment(
    time: Res<Time<Fixed>>,
    spatial_query: SpatialQuery,
    environment_colliders: Query<&EnvironmentCollider>,
    level: Res<Level>,
    mut blobs: ResMut<BlobWorld>,
    mut diagnostics: ResMut<AvianContactDiagnostics>,
) {
    let filter = SpatialQueryFilter::from_mask(GameLayer::Environment);
    let dt = time.delta_secs();
    diagnostics.fixture_corrections = 0;
    diagnostics.lateral_fixture_corrections = 0;
    let selected = blobs.selected;
    for (blob_index, active_blob) in blobs.active.iter_mut().enumerate() {
        let blob_center = active_blob.body.center();
        let skin = (5.0 * active_blob.body.size_scale()).max(crate::blob::MIN_COLLISION_SKIN);
        let probe_radius = (skin * 0.55).max(0.8);
        let probe = Collider::circle(probe_radius);
        let ignore_impact_trauma = active_blob.body.ignores_impact_trauma();
        let mut grounded = false;
        let mut support_normal_sum = Vec2::ZERO;
        let mut support_count = 0;
        let mut impacts = Vec::new();
        let mut had_external_projection = false;
        for particle in &mut active_blob.body.particles {
            let movement = particle.position - particle.previous;
            let movement_length = movement.length();
            let current_projection = spatial_query.project_point_predicate(
                particle.position,
                false,
                &filter,
                &|entity| environment_colliders.contains(entity),
            );
            if let Ok(direction) = Dir2::new(movement)
                && let Some(hit) = spatial_query.cast_shape_predicate(
                    &probe,
                    particle.previous,
                    0.0,
                    direction,
                    &ShapeCastConfig::from_max_distance(movement_length),
                    &filter,
                    &|entity| environment_colliders.contains(entity),
                )
            {
                let shared_edge = environment_colliders
                    .get(hit.entity)
                    .ok()
                    .and_then(|marker| marker.fixture_index)
                    .is_some_and(|fixture_index| {
                        point_on_shared_fixture_edge(hit.point1, fixture_index, &level.fixtures)
                    });
                if blob_index == selected
                    && let Ok(marker) = environment_colliders.get(hit.entity)
                    && marker.fixture_index.is_some()
                {
                    diagnostics.fixture_corrections += 1;
                    diagnostics.lateral_fixture_corrections +=
                        (hit.normal1.y.abs() < 0.55) as usize;
                    diagnostics.shared_edge_corrections += shared_edge as usize;
                }
                if shared_edge {
                    // This edge lies inside the authored solid. Applying the
                    // query hit would create a false lateral wall after the
                    // membrane solver has already completed its iterations.
                    continue;
                }
                let contact = resolve_swept_particle(
                    particle,
                    hit.point1,
                    hit.normal1,
                    probe_radius + skin * 0.45,
                );
                had_external_projection = true;
                grounded |= contact.normal.y > 0.55;
                if contact.normal.y > 0.55 {
                    support_normal_sum += contact.normal;
                    support_count += 1;
                }
                if !ignore_impact_trauma {
                    impacts.push(contact.impact_displacement / dt.max(0.000_001));
                }
                continue;
            }
            let Some(projection) = current_projection else {
                continue;
            };
            let shared_edge = environment_colliders
                .get(projection.entity)
                .ok()
                .and_then(|marker| marker.fixture_index)
                .is_some_and(|fixture_index| {
                    point_on_shared_fixture_edge(projection.point, fixture_index, &level.fixtures)
                });
            if blob_index == selected
                && let Ok(marker) = environment_colliders.get(projection.entity)
                && marker.fixture_index.is_some()
            {
                let normal = (projection.point - particle.position).normalize_or(Vec2::Y);
                diagnostics.fixture_corrections += 1;
                diagnostics.lateral_fixture_corrections += (normal.y.abs() < 0.55) as usize;
                diagnostics.shared_edge_corrections += shared_edge as usize;
            }
            if shared_edge {
                continue;
            }
            let (surface_point, forced_normal) = if projection.is_inside {
                let Ok(marker) = environment_colliders.get(projection.entity) else {
                    continue;
                };
                if let Some(platform_index) = marker.platform_index {
                    let platform = level.platforms[platform_index];
                    let (point, normal) =
                        stable_inside_surface(particle.position, blob_center, platform);
                    (point, Some(normal))
                } else {
                    let normal = (projection.point - particle.position)
                        .normalize_or((particle.position - blob_center).normalize_or(Vec2::Y));
                    (projection.point, Some(normal))
                }
            } else {
                (projection.point, None)
            };
            let Some(contact) = resolve_particle_projection_with_normal(
                particle,
                surface_point,
                projection.is_inside,
                forced_normal,
                skin,
            ) else {
                continue;
            };
            had_external_projection = true;
            grounded |= contact.normal.y > 0.55;
            if contact.normal.y > 0.55 {
                support_normal_sum += contact.normal;
                support_count += 1;
            }
            if !ignore_impact_trauma {
                impacts.push(contact.impact_displacement / dt.max(0.000_001));
            }
        }
        if had_external_projection {
            active_blob.body.stabilize_after_external_projection();
        }
        active_blob.body.grounded |= grounded;
        if support_count > 0 {
            active_blob
                .body
                .record_support_normal(support_normal_sum / support_count as f32);
        }
        active_blob.body.last_impact_speed = active_blob
            .body
            .last_impact_speed
            .max(contact_patch_impact(&mut impacts));
    }
}

fn point_on_shared_fixture_edge(point: Vec2, owner: usize, fixtures: &[Vec<Vec2>]) -> bool {
    const CONTACT_TOLERANCE: f32 = 0.75;
    let Some(polygon) = fixtures.get(owner) else {
        return false;
    };
    polygon
        .iter()
        .copied()
        .zip(polygon.iter().copied().cycle().skip(1))
        .take(polygon.len())
        .any(|(start, end)| {
            point_segment_distance(point, start, end) <= CONTACT_TOLERANCE
                && fixtures.iter().enumerate().any(|(index, candidate)| {
                    index != owner
                        && candidate.iter().any(|candidate_start| {
                            candidate_start.distance_squared(start) <= 0.0001
                        })
                        && candidate
                            .iter()
                            .any(|candidate_end| candidate_end.distance_squared(end) <= 0.0001)
                })
        })
}

fn point_segment_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let edge = end - start;
    let fraction =
        ((point - start).dot(edge) / edge.length_squared().max(f32::EPSILON)).clamp(0.0, 1.0);
    point.distance(start + edge * fraction)
}

fn contact_patch_impact(impacts: &mut [f32]) -> f32 {
    impacts.sort_by(|first, second| second.total_cmp(first));
    match impacts {
        [] => 0.0,
        [single] => *single * 0.68,
        [first, second] => (*first * 0.72 + *second * 0.28) * 0.84,
        [first, second, third, ..] => *first * 0.62 + *second * 0.25 + *third * 0.13,
    }
}

fn resolve_swept_particle(
    particle: &mut Particle,
    surface_point: Vec2,
    normal: Vec2,
    skin: f32,
) -> ProjectionContact {
    let normal = normal.normalize_or(Vec2::Y);
    let velocity = particle.position - particle.previous;
    let impact_displacement = (-velocity.dot(normal)).max(0.0);
    particle.position = surface_point + normal * skin;
    let normal_speed = velocity.dot(normal);
    let corrected_velocity = if normal_speed < 0.0 {
        velocity - normal * normal_speed
    } else {
        velocity
    };
    particle.previous = particle.position - corrected_velocity;
    ProjectionContact {
        normal,
        impact_displacement,
    }
}

#[derive(Clone, Copy, Debug)]
struct ProjectionContact {
    normal: Vec2,
    impact_displacement: f32,
}

#[cfg(test)]
fn resolve_particle_projection(
    particle: &mut Particle,
    surface_point: Vec2,
    is_inside: bool,
    skin: f32,
) -> Option<ProjectionContact> {
    resolve_particle_projection_with_normal(particle, surface_point, is_inside, None, skin)
}

fn resolve_particle_projection_with_normal(
    particle: &mut Particle,
    surface_point: Vec2,
    is_inside: bool,
    forced_normal: Option<Vec2>,
    skin: f32,
) -> Option<ProjectionContact> {
    let separation = if is_inside {
        surface_point - particle.position
    } else {
        particle.position - surface_point
    };
    let distance = separation.length();
    if !is_inside && distance > skin {
        return None;
    }
    let normal = forced_normal.unwrap_or_else(|| separation.normalize_or(Vec2::Y));
    let velocity = particle.position - particle.previous;
    let impact_displacement = (-velocity.dot(normal)).max(0.0);
    particle.position = surface_point + normal * skin;
    let normal_speed = velocity.dot(normal);
    let corrected_velocity = if normal_speed < 0.0 {
        velocity - normal * normal_speed
    } else {
        velocity
    };
    particle.previous = particle.position - corrected_velocity;
    Some(ProjectionContact {
        normal,
        impact_displacement,
    })
}

fn stable_inside_surface(point: Vec2, blob_center: Vec2, platform: Platform) -> (Vec2, Vec2) {
    let minimum = platform.center - platform.half_size;
    let maximum = platform.center + platform.half_size;
    let relative = blob_center - platform.center;
    let normalized_x = relative.x.abs() / platform.half_size.x.max(1.0);
    let normalized_y = relative.y.abs() / platform.half_size.y.max(1.0);
    if normalized_x > normalized_y {
        if relative.x < 0.0 {
            (
                Vec2::new(minimum.x, point.y.clamp(minimum.y, maximum.y)),
                Vec2::NEG_X,
            )
        } else {
            (
                Vec2::new(maximum.x, point.y.clamp(minimum.y, maximum.y)),
                Vec2::X,
            )
        }
    } else if relative.y < 0.0 {
        (
            Vec2::new(point.x.clamp(minimum.x, maximum.x), minimum.y),
            Vec2::NEG_Y,
        )
    } else {
        (
            Vec2::new(point.x.clamp(minimum.x, maximum.x), maximum.y),
            Vec2::Y,
        )
    }
}

/// Observes membrane/environment contacts through Avian without applying a
/// second collision response. This shadow mode provides evidence before the
/// legacy platform solver is replaced.
pub(super) fn sample_avian_contacts(
    spatial_query: SpatialQuery,
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    mut diagnostics: ResMut<AvianContactDiagnostics>,
    mut manifolds: ResMut<AvianContactManifolds>,
) {
    let filter = SpatialQueryFilter::from_mask(GameLayer::Environment);
    let mut particles = 0;
    let mut avian_contacts = 0;
    let mut legacy_contacts = 0;
    let mut agreements = 0;
    manifolds.by_blob.clear();

    for active_blob in &blobs.active {
        let probe_radius = 6.0 * active_blob.body.size_scale();
        let contacts = manifolds.by_blob.entry(active_blob.id).or_default();
        for (particle_index, particle) in active_blob.body.particles.iter().enumerate() {
            particles += 1;
            let projection = spatial_query.project_point(particle.position, false, &filter);
            let avian_contact = projection.as_ref().is_some_and(|projection| {
                projection.is_inside || projection.point.distance(particle.position) <= probe_radius
            });
            if let Some(projection) = projection.filter(|_| avian_contact) {
                let separation = if projection.is_inside {
                    projection.point - particle.position
                } else {
                    particle.position - projection.point
                };
                let distance = separation.length();
                contacts.push(AvianMembraneContact {
                    particle_index,
                    collider: projection.entity,
                    point: projection.point,
                    normal: separation.normalize_or(Vec2::Y),
                    depth: if projection.is_inside {
                        probe_radius + distance
                    } else {
                        (probe_radius - distance).max(0.0)
                    },
                });
            }
            let legacy_contact = level
                .platforms
                .iter()
                .any(|platform| point_near_platform(particle.position, probe_radius, platform));
            avian_contacts += avian_contact as usize;
            legacy_contacts += legacy_contact as usize;
            agreements += (avian_contact == legacy_contact) as usize;
        }
    }

    diagnostics.particles = particles;
    diagnostics.avian_contacts = avian_contacts;
    diagnostics.legacy_contacts = legacy_contacts;
    diagnostics.agreement = if particles == 0 {
        1.0
    } else {
        agreements as f32 / particles as f32
    };
    let selected_contacts = blobs
        .active
        .get(blobs.selected)
        .and_then(|blob| manifolds.by_blob.get(&blob.id))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    diagnostics.selected_surfaces = selected_contacts
        .iter()
        .map(|contact| contact.collider)
        .collect::<HashSet<_>>()
        .len();
    diagnostics.selected_particles = selected_contacts
        .iter()
        .map(|contact| contact.particle_index)
        .collect::<HashSet<_>>()
        .len();
    diagnostics.selected_ground_contacts = selected_contacts
        .iter()
        .filter(|contact| contact.normal.y > 0.55)
        .count();
    diagnostics.selected_max_depth = selected_contacts
        .iter()
        .map(|contact| contact.depth)
        .fold(0.0, f32::max);
    let minimum_x = selected_contacts
        .iter()
        .map(|contact| contact.point.x)
        .fold(f32::INFINITY, f32::min);
    let maximum_x = selected_contacts
        .iter()
        .map(|contact| contact.point.x)
        .fold(f32::NEG_INFINITY, f32::max);
    diagnostics.selected_contact_span = if selected_contacts.is_empty() {
        0.0
    } else {
        maximum_x - minimum_x
    };
}

fn point_near_platform(point: Vec2, radius: f32, platform: &Platform) -> bool {
    let minimum = platform.center - platform.half_size;
    let maximum = platform.center + platform.half_size;
    let closest = point.clamp(minimum, maximum);
    point.distance_squared(closest) <= radius * radius
}

fn platform(x: f32, y: f32, width: f32, height: f32) -> Platform {
    Platform {
        center: Vec2::new(x, y),
        half_size: Vec2::new(width, height) * 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_corner_contact_is_not_treated_as_full_body_impact() {
        assert_eq!(contact_patch_impact(&mut [1_000.0]), 680.0);
        assert!(contact_patch_impact(&mut [1_000.0, 900.0, 800.0]) > 900.0);
    }

    #[test]
    fn shared_fixture_edge_is_detected_as_internal() {
        let fixtures = vec![
            vec![
                Vec2::new(-10.0, -10.0),
                Vec2::new(0.0, -10.0),
                Vec2::new(0.0, 0.0),
                Vec2::new(-10.0, 0.0),
            ],
            vec![
                Vec2::new(0.0, -10.0),
                Vec2::new(10.0, -10.0),
                Vec2::new(10.0, 0.0),
                Vec2::new(0.0, 0.0),
            ],
        ];

        assert!(point_on_shared_fixture_edge(
            Vec2::new(0.0, -5.0),
            0,
            &fixtures
        ));
        assert!(!point_on_shared_fixture_edge(
            Vec2::new(-5.0, 0.0),
            0,
            &fixtures
        ));
    }

    #[test]
    fn prototype_loads_authored_objects_without_local_lanterns() {
        let level = Level::prototype();
        assert_eq!(level.nutrients.len(), 3);
        assert!(level.lights.is_empty());
        assert_eq!(level.expulsion_points.len(), 1);
        assert_eq!(level.hazards.len(), 1);
        assert_eq!(level.decorations.len(), 1);
    }

    #[test]
    fn every_test_route_uses_conservative_jump_gaps() {
        for scenario in 1..=6 {
            let (level, _) = Level::test_scenario(scenario);
            assert!(level.route.len() >= 2);
            for pair in level.route.windows(2) {
                let delta = pair[1] - pair[0];
                assert!(
                    delta.y <= 240.0,
                    "scenario {scenario} requires an excessive rise of {}",
                    delta.y
                );
                assert!(
                    delta.x.abs() <= 260.0,
                    "scenario {scenario} requires an excessive horizontal gap of {}",
                    delta.x.abs()
                );
                assert!(
                    route_segment_has_clear_arc(pair[0], pair[1], &level),
                    "scenario {scenario} has no clear blob-sized route from {:?} to {:?}",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    fn route_segment_has_clear_arc(start: Vec2, end: Vec2, level: &Level) -> bool {
        const CLEARANCE: f32 = 39.0;
        [30.0, 45.0, 65.0, 110.0, 160.0, 215.0, 260.0]
            .into_iter()
            .any(|arc_height| {
                (3..=14).all(|step| {
                    let t = step as f32 / 20.0;
                    let point = start.lerp(end, t) + Vec2::Y * arc_height * 4.0 * t * (1.0 - t);
                    !level
                        .platforms
                        .iter()
                        .any(|platform| point_inside_expanded_platform(point, *platform, CLEARANCE))
                        && !level.fixtures.iter().any(|vertices| {
                            point_inside_or_near_polygon(point, vertices, CLEARANCE)
                        })
                })
            })
    }

    fn point_inside_expanded_platform(point: Vec2, platform: Platform, clearance: f32) -> bool {
        let extent = platform.half_size + Vec2::splat(clearance);
        let delta = (point - platform.center).abs();
        delta.x < extent.x && delta.y < extent.y
    }

    fn point_inside_or_near_polygon(point: Vec2, vertices: &[Vec2], clearance: f32) -> bool {
        if vertices.len() < 3 {
            return false;
        }
        let inside = vertices
            .iter()
            .zip(vertices.iter().cycle().skip(1))
            .take(vertices.len())
            .fold(None, |sign: Option<f32>, (first, second)| {
                let cross = (*second - *first).perp_dot(point - *first);
                match sign {
                    None if cross.abs() > 0.001 => Some(cross.signum()),
                    Some(previous) if cross.signum() != previous && cross.abs() > 0.001 => {
                        Some(0.0)
                    }
                    value => value,
                }
            })
            .is_some_and(|sign| sign != 0.0);
        inside
            || vertices
                .iter()
                .zip(vertices.iter().cycle().skip(1))
                .take(vertices.len())
                .any(|(first, second)| {
                    let edge = *second - *first;
                    let t = ((point - *first).dot(edge) / edge.length_squared().max(0.001))
                        .clamp(0.0, 1.0);
                    point.distance(*first + edge * t) < clearance
                })
    }

    #[test]
    fn every_platform_gets_one_static_avian_collider_at_the_same_position() {
        let mut app = App::new();
        app.add_systems(Startup, setup_environment);
        app.update();

        let expected = Level::prototype();
        let mut query = app
            .world_mut()
            .query::<(&EnvironmentCollider, &RigidBody, &Transform)>();
        let colliders = query.iter(app.world()).collect::<Vec<_>>();
        let platform_colliders = colliders
            .into_iter()
            .filter(|(environment, _, _)| environment.platform_index.is_some())
            .collect::<Vec<_>>();
        assert_eq!(platform_colliders.len(), expected.platforms.len());
        for (environment, body, transform) in platform_colliders {
            assert_eq!(*body, RigidBody::Static);
            let platform_index = environment.platform_index.expect("platform collider");
            assert_eq!(
                transform.translation.truncate(),
                expected.platforms[platform_index].center
            );
        }
        let pilot_count = app
            .world_mut()
            .query_filtered::<Entity, With<AvianMigratedSurface>>()
            .iter(app.world())
            .count();
        assert_eq!(pilot_count, 6);
    }

    #[test]
    fn digit_zero_toggles_level_debug_overlay() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<LevelDebugOverlay>()
            .add_systems(Update, toggle_level_debug);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit0);

        app.update();

        let overlay = app.world().resource::<LevelDebugOverlay>();
        assert!(overlay.visible);
        assert!(overlay.camera_detached);
    }

    #[test]
    fn legacy_contact_probe_matches_rectangle_distance() {
        let platform = platform(10.0, 20.0, 100.0, 20.0);
        assert!(point_near_platform(Vec2::new(10.0, 34.0), 4.0, &platform));
        assert!(!point_near_platform(Vec2::new(10.0, 35.0), 4.0, &platform));
        assert!(point_near_platform(Vec2::new(60.0, 30.0), 0.0, &platform));
    }

    #[test]
    fn membrane_contact_keeps_particle_and_surface_geometry() {
        let contact = AvianMembraneContact {
            particle_index: 7,
            collider: Entity::PLACEHOLDER,
            point: Vec2::new(4.0, 8.0),
            normal: Vec2::Y,
            depth: 2.5,
        };
        assert_eq!(contact.particle_index, 7);
        assert_eq!(contact.point, Vec2::new(4.0, 8.0));
        assert_eq!(contact.normal, Vec2::Y);
        assert_eq!(contact.depth, 2.5);
    }

    #[test]
    fn pilot_projection_removes_inward_velocity_without_bounce() {
        let mut particle = Particle {
            position: Vec2::new(0.0, -2.0),
            previous: Vec2::new(0.0, 6.0),
        };
        let contact = resolve_particle_projection(&mut particle, Vec2::ZERO, true, 3.0).unwrap();
        assert_eq!(contact.normal, Vec2::Y);
        assert_eq!(particle.position, Vec2::new(0.0, 3.0));
        assert!(particle.position.y - particle.previous.y >= 0.0);
        assert_eq!(contact.impact_displacement, 8.0);
    }

    #[test]
    fn swept_contact_preserves_the_face_hit_from_below() {
        let mut particle = Particle {
            previous: Vec2::new(0.0, -20.0),
            position: Vec2::new(0.0, 20.0),
        };
        let contact = resolve_swept_particle(&mut particle, Vec2::ZERO, Vec2::NEG_Y, 3.0);
        assert_eq!(contact.normal, Vec2::NEG_Y);
        assert_eq!(particle.position, Vec2::new(0.0, -3.0));
        assert!(particle.position.y - particle.previous.y <= 0.0);
        assert_eq!(contact.impact_displacement, 40.0);
    }

    #[test]
    fn swept_contact_preserves_a_lateral_face() {
        let mut particle = Particle {
            previous: Vec2::new(-20.0, 0.0),
            position: Vec2::new(20.0, 0.0),
        };
        let contact = resolve_swept_particle(&mut particle, Vec2::ZERO, Vec2::NEG_X, 3.0);
        assert_eq!(contact.normal, Vec2::NEG_X);
        assert_eq!(particle.position, Vec2::new(-3.0, 0.0));
        assert!(particle.position.x - particle.previous.x <= 0.0);
    }

    #[test]
    fn embedded_point_uses_the_surface_facing_the_blob_center() {
        let platform = platform(0.0, 0.0, 100.0, 20.0);
        let (top_point, top_normal) =
            stable_inside_surface(Vec2::new(12.0, 0.0), Vec2::new(0.0, 40.0), platform);
        assert_eq!(top_point, Vec2::new(12.0, 10.0));
        assert_eq!(top_normal, Vec2::Y);

        let (bottom_point, bottom_normal) =
            stable_inside_surface(Vec2::new(-8.0, 0.0), Vec2::new(0.0, -40.0), platform);
        assert_eq!(bottom_point, Vec2::new(-8.0, -10.0));
        assert_eq!(bottom_normal, Vec2::NEG_Y);
    }
}
