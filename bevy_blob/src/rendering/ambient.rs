use super::InkStylePreview;
use crate::BlobSoundEvent;
use crate::camera::GameCamera;
use crate::environment::{Level, TestScenario, WastewaterEffects};
use crate::level_format::{BubbleSettingsDefinition, WastewaterAreaDefinition};
use crate::palette;
use crate::rendering::light_dynamic_rgba;
use bevy::prelude::*;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};

mod bubbles;
mod drops;
mod wastewater;
pub(crate) use bubbles::{simulate_wastewater_bubbles, simulate_wastewater_impacts};
use drops::create_teardrop_mesh;
pub(crate) use drops::{
    AmbientDropState, AmbientSplashParticle, simulate_ambient_drops, trigger_drop_shower,
};
use wastewater::create_bubble_mesh;
use wastewater::organic_splash_random;
pub(crate) use wastewater::{
    WastewaterBubble, WastewaterBubbleState, WastewaterEffectMaterials, WastewaterState,
    simulate_wastewater,
};

#[derive(Resource)]
pub(crate) struct AmbientDropAssets {
    mesh: Handle<Mesh>,
    material: Handle<ColorMaterial>,
    bubble_mesh: Handle<Mesh>,
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
