use super::InkStylePreview;
use crate::environment::{Level, TestScenario};
use crate::level_format::{BubbleSettingsDefinition, WastewaterAreaDefinition};
use bevy::prelude::*;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};

#[derive(Resource)]
pub(crate) struct AmbientDropAssets {
    mesh: Handle<Mesh>,
    material: Handle<ColorMaterial>,
    bubble_mesh: Handle<Mesh>,
    bubble_material: Handle<ColorMaterial>,
}

#[derive(Resource, Default)]
pub(crate) struct AmbientDropState {
    scenario: Option<u8>,
    timers: Vec<f32>,
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
    definition: WastewaterAreaDefinition,
    mesh: Handle<Mesh>,
    phase_offset: f32,
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
    velocity: Vec2,
    gravity: f32,
    radius: f32,
    terminal_y: f32,
    splash_on_impact: bool,
}

#[derive(Component)]
pub(crate) struct AmbientSplashParticle {
    velocity: Vec2,
    gravity: f32,
    remaining: f32,
    duration: f32,
    radius: f32,
}

pub(crate) fn setup_ambient_drop_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(AmbientDropAssets {
        mesh: meshes.add(create_teardrop_mesh()),
        material: materials.add(ColorMaterial::from(Color::srgb(0.02, 0.72, 0.82))),
        bubble_mesh: meshes.add(create_bubble_mesh()),
        bubble_material: materials.add(ColorMaterial::from(Color::WHITE)),
    });
    commands.insert_resource(AmbientDropState::default());
    commands.insert_resource(WastewaterState::default());
    commands.insert_resource(WastewaterBubbleState::default());
}

pub(crate) fn simulate_wastewater(
    mut commands: Commands,
    time: Res<Time>,
    ink_style: Res<InkStylePreview>,
    scenario: Res<TestScenario>,
    level: Res<Level>,
    mut state: ResMut<WastewaterState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    surfaces: Query<(Entity, &WastewaterSurface)>,
) {
    let visible = ink_style.enabled && matches!(scenario.0, 0 | 1);
    let needs_rebuild = state.scenario != Some(scenario.0) || state.visible != visible;
    if needs_rebuild {
        for (entity, _) in &surfaces {
            commands.entity(entity).despawn();
        }
        if visible {
            for (index, definition) in level.wastewater_areas.iter().copied().enumerate() {
                let phase_offset = index as f32 * 1.73;
                let mesh = meshes.add(create_wastewater_mesh(
                    definition,
                    time.elapsed_secs() + phase_offset,
                ));
                let material = materials.add(ColorMaterial::from(Color::WHITE));
                commands.spawn((
                    WastewaterSurface {
                        definition,
                        mesh: mesh.clone(),
                        phase_offset,
                    },
                    Mesh2d(mesh),
                    MeshMaterial2d(material),
                    Transform::from_translation(definition.position.extend(definition.depth)),
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
                time.elapsed_secs() + surface.phase_offset,
            );
        }
    }
}

pub(crate) fn simulate_wastewater_bubbles(
    mut commands: Commands,
    time: Res<Time>,
    ink_style: Res<InkStylePreview>,
    scenario: Res<TestScenario>,
    level: Res<Level>,
    assets: Res<AmbientDropAssets>,
    mut state: ResMut<WastewaterBubbleState>,
    mut bubbles: Query<(Entity, &WastewaterBubble, &mut Transform)>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    let visible = ink_style.enabled && matches!(scenario.0, 0 | 1);
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
        let local_x = transform.translation.x - bubble.area.position.x;
        let surface_y = bubble.area.position.y
            + bubble.area.size.y * 0.5
            + wastewater_wave(local_x, time.elapsed_secs(), bubble.area);
        let bottom_y = bubble.area.position.y - bubble.area.size.y * 0.5;
        let ascent =
            ((transform.translation.y - bottom_y) / (surface_y - bottom_y)).clamp(0.0, 1.0);
        let radius = bubble.base_radius * (1.0 + ascent * 0.38);
        transform.scale = Vec3::new(radius * (1.0 + ascent * 0.08), radius, 1.0);

        if transform.translation.y + radius >= surface_y {
            spawn_bubble_burst(
                &mut commands,
                &assets,
                Vec2::new(transform.translation.x, surface_y),
                radius,
            );
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
            spawn_wastewater_bubble(
                &mut commands,
                &assets,
                &mut state,
                area_index,
                area,
                settings,
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
        MeshMaterial2d(assets.bubble_material.clone()),
        Transform {
            translation: Vec3::new(x, y, area.depth + 0.006),
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
) {
    let velocities = [
        Vec2::new(-34.0, 36.0),
        Vec2::new(-17.0, 49.0),
        Vec2::new(2.0, 55.0),
        Vec2::new(21.0, 47.0),
        Vec2::new(38.0, 32.0),
    ];
    for (index, velocity) in velocities.into_iter().enumerate() {
        let duration = 0.24 + index as f32 * 0.012;
        let radius = source_radius * (0.20 + index as f32 * 0.025);
        commands.spawn((
            AmbientSplashParticle {
                velocity,
                gravity: 245.0,
                remaining: duration,
                duration,
                radius,
            },
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(assets.bubble_material.clone()),
            Transform {
                translation: (impact + Vec2::Y * radius).extend(-0.108),
                scale: Vec3::new(radius, radius * 1.25, 1.0),
                ..default()
            },
        ));
    }
}

pub(crate) fn simulate_ambient_drops(
    mut commands: Commands,
    time: Res<Time>,
    ink_style: Res<InkStylePreview>,
    scenario: Res<TestScenario>,
    level: Res<Level>,
    assets: Res<AmbientDropAssets>,
    mut state: ResMut<AmbientDropState>,
    mut drops: Query<(Entity, &mut AmbientDrop, &mut Transform), Without<AmbientSplashParticle>>,
    mut splashes: Query<(Entity, &mut AmbientSplashParticle, &mut Transform), Without<AmbientDrop>>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    if !ink_style.enabled || !matches!(scenario.0, 0 | 1) {
        for (entity, _, _) in &mut drops {
            commands.entity(entity).despawn();
        }
        for (entity, _, _) in &mut splashes {
            commands.entity(entity).despawn();
        }
        state.scenario = None;
        state.timers.clear();
        return;
    }

    for (entity, mut drop, mut transform) in &mut drops {
        drop.velocity.y -= drop.gravity * dt;
        transform.translation += (drop.velocity * dt).extend(0.0);
        let speed_stretch = 1.0 + (-drop.velocity.y / 360.0).clamp(0.0, 0.65);
        transform.scale.y = drop.radius * speed_stretch;
        if transform.translation.y - drop.radius * speed_stretch <= drop.terminal_y {
            if drop.splash_on_impact {
                spawn_dry_surface_splash(
                    &mut commands,
                    &assets,
                    Vec2::new(transform.translation.x, drop.terminal_y),
                    drop.radius,
                );
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
        transform.translation += (particle.velocity * dt).extend(0.0);
        let life = (particle.remaining / particle.duration).clamp(0.0, 1.0);
        let scale = particle.radius * life.sqrt();
        transform.scale = Vec3::new(scale, scale * 1.35, 1.0);
    }

    if state.scenario != Some(scenario.0) || state.timers.len() != level.drop_emitters.len() {
        state.scenario = Some(scenario.0);
        state.timers = level
            .drop_emitters
            .iter()
            .map(|emitter| -emitter.initial_delay)
            .collect();
    }

    for (index, emitter) in level.drop_emitters.iter().enumerate() {
        state.timers[index] += dt;
        if state.timers[index] < 0.0 {
            continue;
        }
        state.timers[index] -= emitter.interval;
        let surface = first_surface_below(emitter.position, &level);
        let terminal_y = surface
            .unwrap_or_else(|| level.center().y - level.size().y * 0.5 - emitter.radius * 4.0);
        commands.spawn((
            AmbientDrop {
                velocity: Vec2::ZERO,
                gravity: emitter.gravity,
                radius: emitter.radius,
                terminal_y,
                splash_on_impact: surface.is_some(),
            },
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(assets.material.clone()),
            Transform {
                translation: emitter.position.extend(emitter.depth),
                scale: Vec3::new(emitter.radius * 1.35, emitter.radius, 1.0),
                ..default()
            },
        ));
    }
}

fn spawn_dry_surface_splash(
    commands: &mut Commands,
    assets: &AmbientDropAssets,
    impact: Vec2,
    source_radius: f32,
) {
    // Above platform artwork (-0.13..-0.105), but still behind the blob fill (-0.1).
    const SPLASH_DEPTH: f32 = -0.102;
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
                velocity,
                gravity: 300.0,
                remaining: duration,
                duration,
                radius,
            },
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(assets.material.clone()),
            Transform {
                translation: (impact + Vec2::Y * radius).extend(SPLASH_DEPTH),
                scale: Vec3::new(radius, radius * 1.35, 1.0),
                ..default()
            },
        ));
    }
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
    colors.push([0.78, 0.92, 0.52, 0.10]);
    for index in 0..SIDES {
        let angle = index as f32 / SIDES as f32 * std::f32::consts::TAU;
        positions.push([angle.cos(), angle.sin(), 0.0]);
        colors.push([0.12, 0.20, 0.03, 0.68]);
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
const WASTEWATER_ROWS: usize = 3;

fn create_wastewater_mesh(definition: WastewaterAreaDefinition, elapsed: f32) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        wastewater_positions(definition, elapsed),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, wastewater_colors(definition));

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
    elapsed: f32,
) {
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        wastewater_positions(definition, elapsed),
    );
}

fn wastewater_positions(definition: WastewaterAreaDefinition, elapsed: f32) -> Vec<[f32; 3]> {
    let half_size = definition.size * 0.5;
    let mut positions = Vec::with_capacity((WASTEWATER_SEGMENTS + 1) * WASTEWATER_ROWS);
    for row in 0..WASTEWATER_ROWS {
        for column in 0..=WASTEWATER_SEGMENTS {
            let fraction = column as f32 / WASTEWATER_SEGMENTS as f32;
            let x = -half_size.x + definition.size.x * fraction;
            let surface = wastewater_wave(x, elapsed, definition);
            let y = match row {
                0 => half_size.y + surface,
                1 => half_size.y - 16.0 + surface * 0.32,
                _ => -half_size.y,
            };
            positions.push([x, y, 0.0]);
        }
    }
    positions
}

fn wastewater_wave(x: f32, elapsed: f32, definition: WastewaterAreaDefinition) -> f32 {
    let travel = elapsed * definition.wave_speed;
    let broad_wave = (x * 0.014 + travel * 0.72).sin() * 0.42;
    let opposing_wave = (x * 0.031 - travel * 1.24 + 1.9).sin() * 0.27;
    let short_ripple = (x * 0.072 + travel * 1.83 + 4.2).sin() * 0.18;
    let moving_pulse = {
        let center = ((travel * 115.0 + definition.size.x * 0.5).rem_euclid(definition.size.x))
            - definition.size.x * 0.5;
        let distance = (x - center).abs();
        (1.0 - distance / 105.0).max(0.0).powi(2) * 0.34
    };
    definition.wave_height * (broad_wave + opposing_wave + short_ripple + moving_pulse)
}

fn wastewater_colors(definition: WastewaterAreaDefinition) -> Vec<[f32; 4]> {
    let [red, green, blue, alpha] = definition.color;
    let shades = [
        [
            (red * 1.28).min(1.0),
            (green * 1.22).min(1.0),
            (blue * 0.82).min(1.0),
            alpha,
        ],
        [red, green, blue, alpha * 0.92],
        [red * 0.45, green * 0.48, blue * 0.38, alpha * 0.96],
    ];
    let mut colors = Vec::with_capacity((WASTEWATER_SEGMENTS + 1) * WASTEWATER_ROWS);
    for shade in shades {
        colors.extend(std::iter::repeat_n(shade, WASTEWATER_SEGMENTS + 1));
    }
    colors
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
    use crate::{blob::Platform, environment::Level};
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
            color: [0.4, 0.5, 0.1, 0.8],
            wave_height: 4.0,
            wave_speed: 0.3,
            depth: -0.12,
            bubbles: None,
        });

        assert_eq!(
            first_surface_below(Vec2::new(100.0, 50.0), &level),
            Some(-160.0)
        );
        assert_eq!(first_surface_below(Vec2::new(250.0, 50.0), &level), None);
    }
}
