use super::InkStylePreview;
use crate::BlobSoundEvent;
use crate::camera::GameCamera;
use crate::environment::{Level, TestScenario, WastewaterEffects};
use crate::level_format::{BubbleSettingsDefinition, WastewaterAreaDefinition};
use crate::palette;
use crate::rendering::light_dynamic_rgba;
use bevy::prelude::*;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};
use std::collections::HashMap;

#[derive(Resource)]
pub(crate) struct AmbientDropAssets {
    mesh: Handle<Mesh>,
    material: Handle<ColorMaterial>,
    bubble_mesh: Handle<Mesh>,
}

#[derive(Resource, Default)]
pub(crate) struct WastewaterEffectMaterials(HashMap<[u8; 3], Handle<ColorMaterial>>);

#[derive(Resource)]
pub(crate) struct AmbientDropState {
    scenario: Option<u8>,
    normal_delay: f32,
    random: u64,
    rain_enabled: bool,
    rain_delay: f32,
}

impl Default for AmbientDropState {
    fn default() -> Self {
        Self {
            scenario: None,
            normal_delay: 0.0,
            random: 0xa2d4_7c81_39ef_165b,
            rain_enabled: false,
            rain_delay: 0.0,
        }
    }
}

impl AmbientDropState {
    fn unit_random(&mut self) -> f32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        (self.random as u32) as f32 / u32::MAX as f32
    }
}

#[derive(Resource, Default)]
pub(crate) struct WastewaterState {
    scenario: Option<u8>,
    visible: bool,
}

#[derive(Resource)]
pub(crate) struct WastewaterBubbleState {
    scenario: Option<u8>,
    timers: Vec<f32>,
    random: u64,
}

impl Default for WastewaterBubbleState {
    fn default() -> Self {
        Self {
            scenario: None,
            timers: Vec::new(),
            random: 0x8d26_4f3b_71c9_a5e1,
        }
    }
}

impl WastewaterBubbleState {
    fn unit_random(&mut self) -> f32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        (self.random as u32) as f32 / u32::MAX as f32
    }

    fn range(&mut self, range: [f32; 2]) -> f32 {
        range[0] + (range[1] - range[0]) * self.unit_random()
    }
}

#[derive(Component)]
pub(crate) struct WastewaterSurface {
    area_index: usize,
    definition: WastewaterAreaDefinition,
    mesh: Handle<Mesh>,
    phase_offset: f32,
    occlusion_layer: bool,
}

#[derive(Component)]
pub(crate) struct WastewaterBubble {
    area_index: usize,
    area: WastewaterAreaDefinition,
    rise_speed: f32,
    base_radius: f32,
    sway_phase: f32,
}

#[derive(Component)]
pub(crate) struct AmbientDrop {
    position: Vec2,
    velocity: Vec2,
    gravity: f32,
    radius: f32,
    /// World-space contact height. It is projected into the emitter's layer
    /// every frame so the visual impact stays on the physical surface.
    terminal_world_y: f32,
    splash_on_impact: bool,
    depth: f32,
    /// The complete fall remains in the outlet's parallax plane. Mixing
    /// factors during the fall would make a vertical drop appear diagonal.
    parallax: f32,
}

impl AmbientDrop {
    fn terminal_y(&self, camera_position: Vec2) -> f32 {
        self.terminal_world_y - camera_position.y * (1.0 - self.parallax)
    }
}

#[derive(Component)]
pub(crate) struct AmbientLightTint {
    material: Handle<ColorMaterial>,
}

#[derive(Component)]
pub(crate) struct AmbientSplashParticle {
    position: Vec2,
    velocity: Vec2,
    gravity: f32,
    remaining: f32,
    duration: f32,
    radius: f32,
    depth: f32,
    parallax: f32,
}

pub(crate) fn setup_ambient_drop_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(AmbientDropAssets {
        mesh: meshes.add(create_teardrop_mesh()),
        material: materials.add(ColorMaterial::from(palette::color(palette::AMBIENT_DROP))),
        bubble_mesh: meshes.add(create_bubble_mesh()),
        // The bubble mesh and its internal highlight rely on vertex alpha.
    });
    commands.insert_resource(WastewaterEffectMaterials::default());
    commands.insert_resource(AmbientDropState::default());
    commands.insert_resource(WastewaterState::default());
    commands.insert_resource(WastewaterBubbleState::default());
}

pub(crate) fn simulate_wastewater(
    mut commands: Commands,
    time: Res<Time>,
    scenario: Res<TestScenario>,
    level: Res<Level>,
    effects: Res<WastewaterEffects>,
    mut state: ResMut<WastewaterState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    surfaces: Query<(Entity, &WastewaterSurface)>,
) {
    // Water is gameplay geometry and must remain visible in every laboratory,
    // not only in the ink-art preview of the main level.
    let visible = !level.wastewater_areas.is_empty();
    let needs_rebuild = state.scenario != Some(scenario.0) || state.visible != visible;
    if needs_rebuild {
        for (entity, _) in &surfaces {
            commands.entity(entity).despawn();
        }
        if visible {
            for (index, definition) in level.wastewater_areas.iter().copied().enumerate() {
                let phase_offset = index as f32 * 1.73;
                let rear_mesh = meshes.add(create_wastewater_mesh(
                    definition,
                    index,
                    time.elapsed_secs() + phase_offset,
                    false,
                    &level.lights,
                ));
                // `ColorMaterial::from(Color::WHITE)` selects opaque mode and
                // discards vertex transparency. The default material uses
                // alpha blending, required by both wastewater layers.
                let material = materials.add(ColorMaterial::default());
                commands.spawn((
                    WastewaterSurface {
                        area_index: index,
                        definition,
                        mesh: rear_mesh.clone(),
                        phase_offset,
                        occlusion_layer: false,
                    },
                    Mesh2d(rear_mesh),
                    MeshMaterial2d(material.clone()),
                    Transform::from_translation(definition.position.extend(definition.depth)),
                ));
                let front_mesh = meshes.add(create_wastewater_mesh(
                    definition,
                    index,
                    time.elapsed_secs() + phase_offset,
                    true,
                    &level.lights,
                ));
                commands.spawn((
                    WastewaterSurface {
                        area_index: index,
                        definition,
                        mesh: front_mesh.clone(),
                        phase_offset,
                        occlusion_layer: true,
                    },
                    Mesh2d(front_mesh),
                    MeshMaterial2d(material),
                    // In front of blobs and objects, while bubbles and surface
                    // splashes use a still higher layer.
                    Transform::from_translation(
                        definition.position.extend(definition.depth + 0.085),
                    ),
                ));
            }
        }
        state.scenario = Some(scenario.0);
        state.visible = visible;
    }

    if !visible {
        return;
    }
    for (_, surface) in &surfaces {
        if let Some(mut mesh) = meshes.get_mut(&surface.mesh) {
            update_wastewater_positions(
                &mut mesh,
                surface.definition,
                surface.area_index,
                time.elapsed_secs() + surface.phase_offset,
                &effects,
                &level.lights,
                surface.occlusion_layer,
            );
        }
    }
}

pub(crate) fn simulate_wastewater_impacts(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<AmbientDropAssets>,
    level: Res<Level>,
    mut effect_materials: ResMut<WastewaterEffectMaterials>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut effects: ResMut<WastewaterEffects>,
) {
    effects.advance(time.delta_secs().min(1.0 / 20.0));
    for impact in std::mem::take(&mut effects.pending) {
        // Keep droplets in the readable range established for bubble bursts.
        // Object size and impact energy affect the wave, not particle size.
        let splash_radius = (impact.source_radius * 0.55).clamp(4.5, 7.5);
        let color = level
            .wastewater_areas
            .get(impact.area_index)
            .map(|area| area.color)
            .unwrap_or(palette::DEFAULT_WASTEWATER_RUNTIME);
        let material = wastewater_effect_material(color, &mut effect_materials, &mut materials);
        spawn_bubble_burst(
            &mut commands,
            &assets,
            impact.position,
            splash_radius,
            impact.variation,
            material,
        );
    }
}

pub(crate) fn simulate_wastewater_bubbles(
    mut commands: Commands,
    time: Res<Time>,
    scenario: Res<TestScenario>,
    level: Res<Level>,
    assets: Res<AmbientDropAssets>,
    mut effect_materials: ResMut<WastewaterEffectMaterials>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut effects: ResMut<WastewaterEffects>,
    mut state: ResMut<WastewaterBubbleState>,
    mut bubbles: Query<(Entity, &WastewaterBubble, &mut Transform)>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    let visible = !level.wastewater_areas.is_empty();
    if !visible {
        for (entity, _, _) in &mut bubbles {
            commands.entity(entity).despawn();
        }
        state.scenario = None;
        state.timers.clear();
        return;
    }

    if state.scenario != Some(scenario.0) || state.timers.len() != level.wastewater_areas.len() {
        for (entity, _, _) in &mut bubbles {
            commands.entity(entity).despawn();
        }
        state.scenario = Some(scenario.0);
        state.timers = (0..level.wastewater_areas.len())
            .map(|index| 0.25 + index as f32 * 0.31)
            .collect();
    }

    for (entity, bubble, mut transform) in &mut bubbles {
        transform.translation.y += bubble.rise_speed * dt;
        transform.translation.x += (time.elapsed_secs() * 2.1 + bubble.sway_phase).sin() * 7.0 * dt;
        let surface_y = bubble
            .area
            .surface_y(transform.translation.x, time.elapsed_secs());
        let bottom_y = bubble.area.position.y - bubble.area.size.y * 0.5;
        let ascent =
            ((transform.translation.y - bottom_y) / (surface_y - bottom_y)).clamp(0.0, 1.0);
        let radius = bubble.base_radius * (1.0 + ascent * 0.38);
        transform.scale = Vec3::new(radius * (1.0 + ascent * 0.08), radius, 1.0);

        if transform.translation.y + radius >= surface_y {
            let impact = Vec2::new(transform.translation.x, surface_y);
            let variation = effects.emit_ripple(bubble.area_index, impact, radius * 0.72, 0.45);
            let material = wastewater_effect_material(
                bubble.area.color,
                &mut effect_materials,
                &mut materials,
            );
            spawn_bubble_burst(&mut commands, &assets, impact, radius, variation, material);
            // This event is intentionally adjacent to the surface burst and
            // despawn: every bubble that visibly pops owns one matching cue.
            // Bubble intervals in the JSON already control the overall rate.
            sound_events.write(BlobSoundEvent::AmbientBubble);
            commands.entity(entity).despawn();
        }
    }

    for (area_index, area) in level.wastewater_areas.iter().copied().enumerate() {
        let Some(settings) = area.bubbles else {
            continue;
        };
        state.timers[area_index] -= dt;
        if state.timers[area_index] > 0.0 {
            continue;
        }
        let active = bubbles
            .iter()
            .filter(|(_, bubble, _)| bubble.area_index == area_index)
            .count();
        state.timers[area_index] = state.range(settings.interval);
        if active < settings.max_active {
            let material =
                wastewater_effect_material(area.color, &mut effect_materials, &mut materials);
            spawn_wastewater_bubble(
                &mut commands,
                &assets,
                &mut state,
                area_index,
                area,
                settings,
                material,
            );
        }
    }
}

fn spawn_wastewater_bubble(
    commands: &mut Commands,
    assets: &AmbientDropAssets,
    state: &mut WastewaterBubbleState,
    area_index: usize,
    area: WastewaterAreaDefinition,
    settings: BubbleSettingsDefinition,
    material: Handle<ColorMaterial>,
) {
    let radius = state.range(settings.radius);
    let margin = radius * 2.0 + 8.0;
    let usable_width = (area.size.x - margin * 2.0).max(1.0);
    let x = area.position.x - area.size.x * 0.5 + margin + usable_width * state.unit_random();
    let y = area.position.y - area.size.y * 0.5 + radius;
    commands.spawn((
        WastewaterBubble {
            area_index,
            area,
            rise_speed: state.range(settings.rise_speed),
            base_radius: radius,
            sway_phase: state.unit_random() * std::f32::consts::TAU,
        },
        Mesh2d(assets.bubble_mesh.clone()),
        MeshMaterial2d(material),
        Transform {
            // Explicitly above the front wastewater layer. The previous
            // near-identical depths made transparent sorting unreliable.
            translation: Vec3::new(x, y, 0.10),
            scale: Vec3::splat(radius),
            ..default()
        },
    ));
}

fn spawn_bubble_burst(
    commands: &mut Commands,
    assets: &AmbientDropAssets,
    impact: Vec2,
    source_radius: f32,
    variation: f32,
    material: Handle<ColorMaterial>,
) {
    let count = 3 + (variation * 4.0).floor() as usize;
    for index in 0..count {
        let fraction = (index as f32 + 0.5) / count as f32;
        let direction_variation = organic_splash_random(variation, index, 0);
        let speed_variation = organic_splash_random(variation, index, 1);
        let size_variation = organic_splash_random(variation, index, 2);
        let angle =
            0.48 + fraction * (std::f32::consts::PI - 0.96) + (direction_variation - 0.5) * 0.34;
        let velocity = Vec2::from_angle(angle) * (34.0 + speed_variation * 34.0);
        let duration = 0.26 + organic_splash_random(variation, index, 3) * 0.15;
        let radius = source_radius * (0.20 + size_variation * 0.16);
        commands.spawn((
            AmbientSplashParticle {
                position: impact + Vec2::Y * radius,
                velocity,
                gravity: 245.0,
                remaining: duration,
                duration,
                radius,
                depth: 0.11,
                parallax: 1.0,
            },
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(material.clone()),
            Transform {
                translation: (impact + Vec2::Y * radius).extend(0.11),
                scale: Vec3::new(radius, radius * 1.25, 1.0),
                ..default()
            },
        ));
    }
}

fn wastewater_effect_material(
    water: [f32; 4],
    cache: &mut WastewaterEffectMaterials,
    materials: &mut Assets<ColorMaterial>,
) -> Handle<ColorMaterial> {
    let key = [
        (water[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (water[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (water[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ];
    cache
        .0
        .entry(key)
        .or_insert_with(|| {
            let mut tint = palette::mix(water, palette::IVORY, 0.26);
            // A translucent base selects Bevy's alpha-blended material path,
            // preserving the bubble mesh's internal alpha gradient.
            tint[3] = 0.92;
            materials.add(ColorMaterial::from(palette::color(tint)))
        })
        .clone()
}

fn organic_splash_random(variation: f32, particle: usize, channel: u32) -> f32 {
    let mut value = variation.to_bits() as u64
        ^ (particle as u64).wrapping_mul(0x9e37_79b9)
        ^ (channel as u64).wrapping_mul(0x85eb_ca6b);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    (value as u32) as f32 / u32::MAX as f32
}

pub(crate) fn simulate_ambient_drops(
    mut commands: Commands,
    time: Res<Time>,
    ink_style: Res<InkStylePreview>,
    scenario: Res<TestScenario>,
    level: Res<Level>,
    camera: Single<&Transform, With<GameCamera>>,
    assets: Res<AmbientDropAssets>,
    mut state: ResMut<AmbientDropState>,
    mut drops: Query<
        (Entity, &mut AmbientDrop, &mut Transform, &AmbientLightTint),
        (Without<AmbientSplashParticle>, Without<GameCamera>),
    >,
    mut splashes: Query<
        (Entity, &mut AmbientSplashParticle, &mut Transform),
        (Without<AmbientDrop>, Without<GameCamera>),
    >,
    mut sound_events: MessageWriter<BlobSoundEvent>,
    mut sound_cooldown: Local<f32>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    *sound_cooldown = (*sound_cooldown - dt).max(0.0);
    if !ink_style.enabled || !matches!(scenario.0, 0 | 1) {
        for (entity, _, _, _) in &mut drops {
            commands.entity(entity).despawn();
        }
        for (entity, _, _) in &mut splashes {
            commands.entity(entity).despawn();
        }
        state.scenario = None;
        state.normal_delay = 0.0;
        return;
    }

    for (entity, mut drop, mut transform, tint) in &mut drops {
        drop.velocity.y -= drop.gravity * dt;
        let velocity = drop.velocity;
        drop.position += velocity * dt;
        let camera_position = camera.translation.truncate();
        transform.translation =
            (drop.position + parallax_offset(camera_position, drop.parallax)).extend(drop.depth);
        let speed_stretch = 1.0 + (-drop.velocity.y / 360.0).clamp(0.0, 0.65);
        transform.scale.y = drop.radius * speed_stretch;
        if let Some(mut material) = materials.get_mut(&tint.material) {
            material.color = palette::color(light_dynamic_rgba(
                palette::AMBIENT_DROP,
                drop.position,
                &level.lights,
            ));
        }
        let terminal_y = drop.terminal_y(camera_position);
        if drop.position.y - drop.radius * speed_stretch <= terminal_y {
            if drop.splash_on_impact {
                spawn_dry_surface_splash(
                    &mut commands,
                    &assets,
                    Vec2::new(drop.position.x, terminal_y),
                    drop.radius,
                    drop.depth,
                    drop.parallax,
                    camera_position,
                );
                if *sound_cooldown <= 0.0 {
                    sound_events.write(BlobSoundEvent::AmbientDrop);
                    // The recorded drop lasts about a third of a second;
                    // avoid layering a second one before it finishes.
                    *sound_cooldown = 0.36;
                }
            }
            commands.entity(entity).despawn();
        }
    }

    for (entity, mut particle, mut transform) in &mut splashes {
        particle.remaining -= dt;
        if particle.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        particle.velocity.y -= particle.gravity * dt;
        let velocity = particle.velocity;
        particle.position += velocity * dt;
        transform.translation = (particle.position
            + parallax_offset(camera.translation.truncate(), particle.parallax))
        .extend(particle.depth);
        let life = (particle.remaining / particle.duration).clamp(0.0, 1.0);
        let scale = particle.radius * life.sqrt();
        transform.scale = Vec3::new(scale, scale * 1.35, 1.0);
    }

    if state.scenario != Some(scenario.0) {
        state.scenario = Some(scenario.0);
        state.normal_delay = 0.6 + state.unit_random() * 1.4;
    }

    state.normal_delay -= dt;
    if state.normal_delay > 0.0 {
        return;
    }
    // Sparse rain is entirely procedural: each drop receives a new time,
    // horizontal coordinate, size and fall acceleration. JSON levels contain
    // no invisible rain positions to maintain.
    state.normal_delay += 1.45 + state.unit_random() * 2.35;
    let camera_position = camera.translation.truncate();
    let (rain_left, rain_right) = rain_horizontal_bounds(&level);
    let radius = 2.2 + state.unit_random() * 1.25;
    let position = Vec2::new(
        rain_left + (rain_right - rain_left) * state.unit_random(),
        rain_start_y(camera_position, &level),
    );
    let surface = first_surface_below(position, &level);
    let terminal_world_y =
        surface.unwrap_or_else(|| level.center().y - level.size().y * 0.5 - radius * 4.0);
    let material = materials.add(ColorMaterial::from(palette::color(light_dynamic_rgba(
        palette::AMBIENT_DROP,
        position,
        &level.lights,
    ))));
    commands.spawn((
        AmbientDrop {
            velocity: Vec2::new(0.0, -24.0 - state.unit_random() * 42.0),
            position,
            gravity: 370.0 + state.unit_random() * 110.0,
            radius,
            terminal_world_y,
            splash_on_impact: surface.is_some(),
            depth: -4.8,
            parallax: 1.0,
        },
        Mesh2d(assets.mesh.clone()),
        MeshMaterial2d(material.clone()),
        AmbientLightTint { material },
        Transform {
            translation: position.extend(-4.8),
            scale: Vec3::new(radius * 1.35, radius, 1.0),
            ..default()
        },
    ));
}

/// Toggles a visual rain test. The drops stay inside the playable side walls
/// and deliberately bypass the authored level emitters.
pub(crate) fn trigger_drop_shower(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    camera: Single<&Transform, With<GameCamera>>,
    level: Res<Level>,
    assets: Res<AmbientDropAssets>,
    mut state: ResMut<AmbientDropState>,
    mut commands: Commands,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    if keyboard.just_pressed(KeyCode::KeyV) {
        state.rain_enabled = !state.rain_enabled;
        state.rain_delay = 0.0;
    }
    if !state.rain_enabled {
        return;
    }

    state.rain_delay -= time.delta_secs();
    if state.rain_delay > 0.0 {
        return;
    }
    state.rain_delay += 0.38;

    const DROP_COUNT: usize = 12;
    let camera_position = camera.translation.truncate();
    let start_y = rain_start_y(camera_position, &level);
    let (rain_left, rain_right) = rain_horizontal_bounds(&level);

    for index in 0..DROP_COUNT {
        let fraction = (index as f32 + 0.5) / DROP_COUNT as f32;
        let horizontal_variation = state.unit_random() - 0.5;
        let height_variation = state.unit_random() - 0.5;
        let speed_variation = state.unit_random();
        let position = Vec2::new(
            (rain_left + (rain_right - rain_left) * fraction + horizontal_variation * 34.0)
                .clamp(rain_left, rain_right),
            start_y + height_variation * 58.0,
        );
        let surface = first_surface_below(position, &level);
        let terminal_y = surface.unwrap_or(level.center().y - level.size().y * 0.5 - 14.0);
        let radius = 1.7 + speed_variation * 1.8;
        let material = materials.add(ColorMaterial::from(palette::color(light_dynamic_rgba(
            palette::AMBIENT_DROP,
            position,
            &level.lights,
        ))));
        commands.spawn((
            AmbientDrop {
                position,
                velocity: Vec2::new(horizontal_variation * 86.0, -40.0 - speed_variation * 165.0),
                gravity: 380.0 + state.unit_random() * 190.0,
                radius,
                terminal_world_y: terminal_y,
                splash_on_impact: surface.is_some(),
                depth: -4.8,
                parallax: 1.0,
            },
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(material.clone()),
            AmbientLightTint { material },
            Transform {
                translation: position.extend(-4.8),
                scale: Vec3::new(radius * 1.35, radius, 1.0),
                ..default()
            },
        ));
    }
}

/// Shared top edge for sparse ambient rain and the V storm. This keeps both
/// modes visually coherent while their density and size stay distinct.
fn rain_start_y(camera_position: Vec2, level: &Level) -> f32 {
    const VIEW_TOP_OFFSET: f32 = 390.0;
    (camera_position.y + VIEW_TOP_OFFSET).min(level.center().y + level.size().y * 0.5 - 28.0)
}

/// The side walls are physical boundaries, not rain sources. Keep every drop
/// inside them so it never appears to enter from behind the foreground art.
fn rain_horizontal_bounds(level: &Level) -> (f32, f32) {
    let horizontal_bounds = level.safety_bounds.map_or(
        (
            level.center().x - level.size().x * 0.5,
            level.center().x + level.size().x * 0.5,
        ),
        |bounds| (bounds.min.x, bounds.max.x),
    );
    let left = horizontal_bounds.0 + 42.0;
    let right = (horizontal_bounds.1 - 42.0).max(left + 1.0);
    (left, right)
}

fn spawn_dry_surface_splash(
    commands: &mut Commands,
    assets: &AmbientDropAssets,
    impact: Vec2,
    source_radius: f32,
    depth: f32,
    parallax: f32,
    camera_position: Vec2,
) {
    // Above platform artwork (-0.13..-0.105), but still behind the blob fill (-0.1).
    const SPLASH_DEPTH: f32 = 0.11;
    let velocities = [
        Vec2::new(-62.0, 45.0),
        Vec2::new(-43.0, 62.0),
        Vec2::new(-20.0, 76.0),
        Vec2::new(5.0, 82.0),
        Vec2::new(29.0, 72.0),
        Vec2::new(51.0, 57.0),
        Vec2::new(67.0, 40.0),
    ];
    for (index, velocity) in velocities.into_iter().enumerate() {
        let duration = 0.34 + index as f32 * 0.018;
        let radius = source_radius * (0.34 + index as f32 * 0.022);
        commands.spawn((
            AmbientSplashParticle {
                position: impact + Vec2::Y * radius,
                velocity,
                gravity: 300.0,
                remaining: duration,
                duration,
                radius,
                depth,
                parallax,
            },
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(assets.material.clone()),
            Transform {
                translation: (impact
                    + Vec2::Y * radius
                    + parallax_offset(camera_position, parallax))
                .extend(depth + SPLASH_DEPTH),
                scale: Vec3::new(radius, radius * 1.35, 1.0),
                ..default()
            },
        ));
    }
}

fn parallax_offset(camera_position: Vec2, factor: f32) -> Vec2 {
    camera_position * (1.0 - factor)
}

fn create_teardrop_mesh() -> Mesh {
    let outline = [
        Vec2::new(0.0, 1.35),
        Vec2::new(-0.30, 0.72),
        Vec2::new(-0.58, 0.20),
        Vec2::new(-0.68, -0.35),
        Vec2::new(-0.48, -0.78),
        Vec2::new(0.0, -1.0),
        Vec2::new(0.48, -0.78),
        Vec2::new(0.68, -0.35),
        Vec2::new(0.58, 0.20),
        Vec2::new(0.30, 0.72),
    ];
    let mut positions = Vec::with_capacity(outline.len() + 1);
    positions.push([0.0, 0.0, 0.0]);
    positions.extend(outline.map(|point| [point.x, point.y, 0.0]));
    let mut indices = Vec::with_capacity(outline.len() * 3);
    for index in 0..outline.len() {
        indices.extend_from_slice(&[
            0,
            index as u32 + 1,
            (index as u32 + 1) % outline.len() as u32 + 1,
        ]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn create_bubble_mesh() -> Mesh {
    const SIDES: usize = 24;
    let mut positions = Vec::with_capacity(SIDES + 1);
    let mut colors = Vec::with_capacity(SIDES + 1);
    positions.push([0.0, 0.0, 0.0]);
    colors.push(palette::BUBBLE_CENTER);
    for index in 0..SIDES {
        let angle = index as f32 / SIDES as f32 * std::f32::consts::TAU;
        positions.push([angle.cos(), angle.sin(), 0.0]);
        colors.push(palette::BUBBLE_EDGE);
    }
    let mut indices = Vec::with_capacity(SIDES * 3);
    for index in 0..SIDES {
        indices.extend_from_slice(&[0, index as u32 + 1, (index as u32 + 1) % SIDES as u32 + 1]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

const WASTEWATER_SEGMENTS: usize = 64;
const WASTEWATER_ROWS: usize = 4;
// The rear layer gives the basin its coloured volume. The front layer only
// filters immersed objects, so it must stay substantially more transparent.
const WASTEWATER_REAR_ALPHA_SCALE: [f32; WASTEWATER_ROWS] = [0.26, 0.34, 0.29, 0.32];
const WASTEWATER_FRONT_ALPHA: [f32; WASTEWATER_ROWS] = [0.20, 0.08, 0.12, 0.17];

fn create_wastewater_mesh(
    definition: WastewaterAreaDefinition,
    area_index: usize,
    elapsed: f32,
    occlusion_layer: bool,
    lights: &[crate::level_format::LightDefinition],
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    let positions = wastewater_positions(definition, area_index, elapsed, None);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        wastewater_colors(definition, occlusion_layer, &positions, lights, elapsed),
    );

    let row_width = WASTEWATER_SEGMENTS + 1;
    let mut indices = Vec::with_capacity(WASTEWATER_SEGMENTS * (WASTEWATER_ROWS - 1) * 6);
    for row in 0..WASTEWATER_ROWS - 1 {
        for column in 0..WASTEWATER_SEGMENTS {
            let top_left = (row * row_width + column) as u32;
            let top_right = top_left + 1;
            let bottom_left = top_left + row_width as u32;
            let bottom_right = bottom_left + 1;
            indices.extend_from_slice(&[
                top_left,
                bottom_left,
                top_right,
                top_right,
                bottom_left,
                bottom_right,
            ]);
        }
    }
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn update_wastewater_positions(
    mesh: &mut Mesh,
    definition: WastewaterAreaDefinition,
    area_index: usize,
    elapsed: f32,
    effects: &WastewaterEffects,
    lights: &[crate::level_format::LightDefinition],
    occlusion_layer: bool,
) {
    let positions = wastewater_positions(definition, area_index, elapsed, Some(effects));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
    // Lighting follows the wavy surface as it moves, so shallow bright bands
    // do not remain frozen in world space after an impact ripple.
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        wastewater_colors(definition, occlusion_layer, &positions, lights, elapsed),
    );
}

fn wastewater_positions(
    definition: WastewaterAreaDefinition,
    area_index: usize,
    elapsed: f32,
    effects: Option<&WastewaterEffects>,
) -> Vec<[f32; 3]> {
    let half_size = definition.size * 0.5;
    let mut positions = Vec::with_capacity((WASTEWATER_SEGMENTS + 1) * WASTEWATER_ROWS);
    for row in 0..WASTEWATER_ROWS {
        for column in 0..=WASTEWATER_SEGMENTS {
            let fraction = column as f32 / WASTEWATER_SEGMENTS as f32;
            let x = -half_size.x + definition.size.x * fraction;
            let world_x = definition.position.x + x;
            let surface = definition.wave_offset(x, elapsed)
                + effects.map_or(0.0, |effects| effects.surface_offset(area_index, world_x));
            let y = match row {
                // A thin, uneven scum rim breaks the hard rectangular edge
                // and visually seats the animated water in the basin.
                0 => half_size.y + surface + wastewater_shore_rim(x, elapsed),
                1 => half_size.y + surface,
                2 => half_size.y - 16.0 + surface * 0.32,
                _ => -half_size.y,
            };
            positions.push([x, y, 0.0]);
        }
    }
    positions
}

fn wastewater_shore_rim(local_x: f32, elapsed: f32) -> f32 {
    let broad = (local_x * 0.021 + elapsed * 0.34).sin() * 0.72;
    let fine = (local_x * 0.071 - elapsed * 0.52).sin() * 0.34;
    2.6 + broad + fine
}

fn wastewater_colors(
    definition: WastewaterAreaDefinition,
    occlusion_layer: bool,
    positions: &[[f32; 3]],
    lights: &[crate::level_format::LightDefinition],
    elapsed: f32,
) -> Vec<[f32; 4]> {
    let [red, green, blue, alpha] = definition.color;
    let alphas = if occlusion_layer {
        // Objects remain visible below the surface, increasingly filtered by
        // murky water with depth instead of being cut away completely. The
        // old values made the foreground layer almost opaque by itself.
        WASTEWATER_FRONT_ALPHA
    } else {
        WASTEWATER_REAR_ALPHA_SCALE.map(|scale| alpha * scale)
    };
    let shades = [
        // Dark, low-saturation rim: it reads as foam, grease and debris
        // instead of a clean vector line over the water.
        [
            (red * 0.42).min(1.0),
            (green * 0.50).min(1.0),
            (blue * 0.28).min(1.0),
            alphas[0],
        ],
        [
            (red * 1.28).min(1.0),
            (green * 1.22).min(1.0),
            (blue * 0.82).min(1.0),
            alphas[1],
        ],
        [red, green, blue, alphas[2]],
        [red * 0.45, green * 0.48, blue * 0.38, alphas[3]],
    ];
    positions
        .iter()
        .enumerate()
        .map(|(index, position)| {
            let shade = shades[index / (WASTEWATER_SEGMENTS + 1)];
            let row = index / (WASTEWATER_SEGMENTS + 1);
            let world_position = definition.position + Vec2::new(position[0], position[1]);
            let mut color = light_dynamic_rgba(shade, world_position, lights);
            // Reflections belong only to the two moving surface rows. Their
            // broken horizontal bands drift independently under each lantern
            // rather than painting a uniform bright stripe across the basin.
            if row <= 1 {
                for (light_index, light) in
                    lights.iter().enumerate().filter(|(_, light)| light.enabled)
                {
                    let lateral = (1.0
                        - (world_position.x - light.position.x).abs() / light.radius)
                        .clamp(0.0, 1.0);
                    let vertical = (1.0
                        - (world_position.y - light.position.y).abs() / (light.radius * 1.35))
                        .clamp(0.0, 1.0);
                    let phase = world_position.x * (0.075 + light_index as f32 * 0.004)
                        - elapsed * (2.1 + light_index as f32 * 0.11)
                        + light_index as f32 * 1.37;
                    let shimmer = (phase.sin() * 0.5 + 0.5).powi(5)
                        * lateral.powi(2)
                        * vertical
                        * light.intensity
                        * 0.18;
                    color[0] = (color[0] + light.color[0] * shimmer).min(1.0);
                    color[1] = (color[1] + light.color[1] * shimmer).min(1.0);
                    color[2] = (color[2] + light.color[2] * shimmer).min(1.0);
                }
            }
            color
        })
        .collect()
}

fn first_surface_below(origin: Vec2, level: &Level) -> Option<f32> {
    let mut best: Option<f32> = None;
    let mut consider = |height: f32| {
        if height < origin.y && best.is_none_or(|current| height > current) {
            best = Some(height);
        }
    };

    for platform in &level.platforms {
        if (origin.x - platform.center.x).abs() <= platform.half_size.x {
            consider(platform.center.y + platform.half_size.y);
        }
    }
    for polygon in &level.fixtures {
        for (start, end) in polygon
            .iter()
            .copied()
            .zip(polygon.iter().copied().cycle().skip(1))
            .take(polygon.len())
        {
            let delta_x = end.x - start.x;
            if delta_x.abs() <= f32::EPSILON {
                continue;
            }
            let fraction = (origin.x - start.x) / delta_x;
            if (0.0..=1.0).contains(&fraction) {
                consider(start.y + (end.y - start.y) * fraction);
            }
        }
    }
    for area in &level.wastewater_areas {
        if (origin.x - area.position.x).abs() <= area.size.x * 0.5 {
            consider(area.position.y + area.size.y * 0.5);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::{WastewaterAreaDefinition, first_surface_below};
    use crate::{blob::Platform, environment::Level, palette};
    use bevy::prelude::*;

    #[test]
    fn drop_ends_on_the_first_physical_surface_below_its_outlet() {
        let level = Level::from_test_geometry(
            vec![
                Platform {
                    center: Vec2::new(0.0, -100.0),
                    half_size: Vec2::new(80.0, 10.0),
                },
                Platform {
                    center: Vec2::new(0.0, -220.0),
                    half_size: Vec2::new(80.0, 10.0),
                },
            ],
            Vec::new(),
        );
        assert_eq!(
            first_surface_below(Vec2::new(0.0, 50.0), &level),
            Some(-90.0)
        );
    }

    #[test]
    fn wastewater_surface_collects_drops_without_a_platform_below() {
        let mut level = Level::from_test_geometry(Vec::new(), Vec::new());
        level.wastewater_areas.push(WastewaterAreaDefinition {
            position: Vec2::new(0.0, -200.0),
            size: Vec2::new(400.0, 80.0),
            color: palette::DEFAULT_WASTEWATER,
            wave_height: 4.0,
            wave_speed: 0.3,
            depth: -0.12,
            bubbles: None,
            immune_family: None,
        });

        assert_eq!(
            first_surface_below(Vec2::new(100.0, 50.0), &level),
            Some(-160.0)
        );
        assert_eq!(first_surface_below(Vec2::new(250.0, 50.0), &level), None);
    }
}
