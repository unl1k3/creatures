//! Persistent state for wastewater surfaces and their procedural bubbles.

use crate::level_format::WastewaterAreaDefinition;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::HashMap;

use super::*;

mod mesh;

pub(super) use mesh::create_bubble_mesh;
use mesh::{create_wastewater_mesh, update_wastewater_positions};

#[derive(Resource, Default)]
pub(crate) struct WastewaterEffectMaterials(pub(super) HashMap<[u8; 3], Handle<ColorMaterial>>);

type WastewaterSurfaces<'w, 's> = Query<'w, 's, (Entity, &'static WastewaterSurface)>;

/// Resources and surface entities required for one wastewater render update.
#[derive(SystemParam)]
pub(crate) struct WastewaterSimulationParams<'w, 's> {
    time: Res<'w, Time>,
    scenario: Res<'w, TestScenario>,
    level: Res<'w, Level>,
    effects: Res<'w, WastewaterEffects>,
    state: ResMut<'w, WastewaterState>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<ColorMaterial>>,
    surfaces: WastewaterSurfaces<'w, 's>,
}

pub(crate) fn simulate_wastewater(mut commands: Commands, params: WastewaterSimulationParams) {
    let WastewaterSimulationParams {
        time,
        scenario,
        level,
        effects,
        mut state,
        mut meshes,
        mut materials,
        surfaces,
    } = params;
    let visible = !level.wastewater_areas.is_empty();
    if state.scenario != Some(scenario.0) || state.visible != visible {
        for (entity, _) in &surfaces {
            commands.entity(entity).despawn();
        }
        if visible {
            for (index, definition) in level.wastewater_areas.iter().copied().enumerate() {
                let phase_offset = index as f32 * 1.73;
                let material = materials.add(ColorMaterial::default());
                for occlusion_layer in [false, true] {
                    let mesh = meshes.add(create_wastewater_mesh(
                        definition,
                        index,
                        time.elapsed_secs() + phase_offset,
                        occlusion_layer,
                        &level.lights,
                    ));
                    commands.spawn((
                        WastewaterSurface {
                            area_index: index,
                            definition,
                            mesh: mesh.clone(),
                            phase_offset,
                            occlusion_layer,
                        },
                        Mesh2d(mesh),
                        MeshMaterial2d(material.clone()),
                        Transform::from_translation(
                            definition.position.extend(
                                definition.depth + if occlusion_layer { 0.085 } else { 0.0 },
                            ),
                        ),
                    ));
                }
            }
        }
        state.scenario = Some(scenario.0);
        state.visible = visible;
    }
    if visible {
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
}

/// Deterministic variation keeps splash shapes organic without visual jitter.
pub(super) fn organic_splash_random(variation: f32, particle: usize, channel: u32) -> f32 {
    let mut value = variation.to_bits() as u64
        ^ (particle as u64).wrapping_mul(0x9e37_79b9)
        ^ (channel as u64).wrapping_mul(0x85eb_ca6b);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    (value as u32) as f32 / u32::MAX as f32
}

#[derive(Component)]
pub(crate) struct WastewaterSurface {
    pub(super) area_index: usize,
    pub(super) definition: WastewaterAreaDefinition,
    pub(super) mesh: Handle<Mesh>,
    pub(super) phase_offset: f32,
    pub(super) occlusion_layer: bool,
}

#[derive(Component)]
pub(crate) struct WastewaterBubble {
    pub(super) area_index: usize,
    pub(super) area: WastewaterAreaDefinition,
    pub(super) rise_speed: f32,
    pub(super) base_radius: f32,
    pub(super) sway_phase: f32,
}

#[derive(Resource, Default)]
pub(crate) struct WastewaterState {
    pub(super) scenario: Option<u8>,
    pub(super) visible: bool,
}

#[derive(Resource)]
pub(crate) struct WastewaterBubbleState {
    pub(super) scenario: Option<u8>,
    pub(super) timers: Vec<f32>,
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
    pub(super) fn unit_random(&mut self) -> f32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 7;
        self.random ^= self.random << 17;
        (self.random as u32) as f32 / u32::MAX as f32
    }

    pub(super) fn range(&mut self, range: [f32; 2]) -> f32 {
        range[0] + (range[1] - range[0]) * self.unit_random()
    }
}
