//! Stateful timing and deterministic random sequence for ambient rain.

use bevy::ecs::system::SystemParam;
use bevy::prelude::Resource;

use super::*;

mod simulation;
mod spawn;

pub(crate) use simulation::simulate_ambient_drops;
use spawn::spawn_drop_shower;

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

/// Input and resources used by the manually enabled heavy-rain preview.
#[derive(SystemParam)]
pub(crate) struct DropShowerParams<'w, 's> {
    keyboard: Res<'w, ButtonInput<KeyCode>>,
    time: Res<'w, Time>,
    camera: Single<'w, 's, &'static Transform, With<GameCamera>>,
    level: Res<'w, Level>,
    assets: Res<'w, AmbientDropAssets>,
    state: ResMut<'w, AmbientDropState>,
    materials: ResMut<'w, Assets<ColorMaterial>>,
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
pub(crate) fn trigger_drop_shower(params: DropShowerParams, mut commands: Commands) {
    let DropShowerParams {
        keyboard,
        time,
        camera,
        level,
        assets,
        mut state,
        mut materials,
    } = params;
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

    spawn_drop_shower(
        &mut commands,
        &mut materials,
        &assets,
        &level,
        camera.translation.truncate(),
        &mut state,
    );
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
