//! Stateful timing and deterministic random sequence for ambient rain.

use bevy::prelude::Resource;

use super::*;

#[derive(Component)]
pub(crate) struct AmbientDrop {
    pub(super) position: Vec2,
    pub(super) velocity: Vec2,
    pub(super) gravity: f32,
    pub(super) radius: f32,
    /// World-space contact height, projected into the emitter layer each frame.
    pub(super) terminal_world_y: f32,
    pub(super) splash_on_impact: bool,
    pub(super) depth: f32,
    /// A whole fall remains in one parallax plane so it stays vertical.
    pub(super) parallax: f32,
}

impl AmbientDrop {
    pub(super) fn terminal_y(&self, camera_position: Vec2) -> f32 {
        self.terminal_world_y - camera_position.y * (1.0 - self.parallax)
    }
}

#[derive(Component)]
pub(crate) struct AmbientLightTint {
    pub(super) material: Handle<ColorMaterial>,
}

#[derive(Component)]
pub(crate) struct AmbientSplashParticle {
    pub(super) position: Vec2,
    pub(super) velocity: Vec2,
    pub(super) gravity: f32,
    pub(super) remaining: f32,
    pub(super) duration: f32,
    pub(super) radius: f32,
    pub(super) depth: f32,
    pub(super) parallax: f32,
}

#[derive(Resource)]
pub(crate) struct AmbientDropState {
    pub(super) scenario: Option<u8>,
    pub(super) normal_delay: f32,
    random: u64,
    pub(super) rain_enabled: bool,
    pub(super) rain_delay: f32,
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
    pub(super) fn unit_random(&mut self) -> f32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        (self.random as u32) as f32 / u32::MAX as f32
    }
}

/// Toggles the visual rain test. It bypasses authored emitters but remains
/// within the physical side walls of the active level.
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
        let horizontal = state.unit_random() - 0.5;
        let height = state.unit_random() - 0.5;
        let speed = state.unit_random();
        let position = Vec2::new(
            (rain_left + (rain_right - rain_left) * fraction + horizontal * 34.0)
                .clamp(rain_left, rain_right),
            start_y + height * 58.0,
        );
        let surface = first_surface_below(position, &level);
        let terminal_y = surface.unwrap_or(level.center().y - level.size().y * 0.5 - 14.0);
        let radius = 1.7 + speed * 1.8;
        let material = materials.add(ColorMaterial::from(palette::color(light_dynamic_rgba(
            palette::AMBIENT_DROP,
            position,
            &level.lights,
        ))));
        commands.spawn((
            AmbientDrop {
                position,
                velocity: Vec2::new(horizontal * 86.0, -40.0 - speed * 165.0),
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

pub(super) fn create_teardrop_mesh() -> Mesh {
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

/// Shared top edge for sparse ambient rain and the V storm.
pub(super) fn rain_start_y(camera_position: Vec2, level: &Level) -> f32 {
    const VIEW_TOP_OFFSET: f32 = 390.0;
    (camera_position.y + VIEW_TOP_OFFSET).min(level.center().y + level.size().y * 0.5 - 28.0)
}

/// Keeps procedurally generated rain inside the physical side walls.
pub(super) fn rain_horizontal_bounds(level: &Level) -> (f32, f32) {
    let bounds = level.safety_bounds.map_or(
        (
            level.center().x - level.size().x * 0.5,
            level.center().x + level.size().x * 0.5,
        ),
        |bounds| (bounds.min.x, bounds.max.x),
    );
    let left = bounds.0 + 42.0;
    (left, (bounds.1 - 42.0).max(left + 1.0))
}

pub(super) fn parallax_offset(camera_position: Vec2, factor: f32) -> Vec2 {
    camera_position * (1.0 - factor)
}

pub(super) fn spawn_dry_surface_splash(
    commands: &mut Commands,
    assets: &AmbientDropAssets,
    impact: Vec2,
    source_radius: f32,
    depth: f32,
    parallax: f32,
    camera_position: Vec2,
) {
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
