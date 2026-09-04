use super::*;
use crate::environment::WastewaterEffects;
use crate::level_format::NutrientDefinition;
use crate::palette;
use avian2d::prelude::{Collider, LinearVelocity};
use bevy::ecs::system::SystemParam;

mod digestion;
mod feeding;
mod geometry;
mod membrane;
mod physics;
mod render;
mod state;

use digestion::{
    advance_digesting, advance_engulfing, advance_expelling, advance_probe_and_capture,
};
pub(super) use feeding::start_phagocytosis;
#[cfg(test)]
use geometry::circle_convex_penetration;
pub(super) use membrane::circle_blob_penetration;
#[cfg(test)]
use membrane::circle_intersects_blob_membrane;
use membrane::{
    circle_outside_blob_membrane, constrain_protrusion_load, membrane_anchor,
    membrane_lower_boundary, phagocytosis_path_clear,
};
use physics::{
    FreeNutrientFrame, free_nutrient_contact_radius, sync_free_nutrients_before_digestion,
    sync_nutrient_bodies_after_digestion,
};
pub(super) use physics::{NutrientPhysics, spawn_nutrient_bodies};
use render::update_nutrient_mesh;
#[cfg(test)]
use render::{append_nutrient_mesh, nutrient_palette};
pub(super) use render::{draw_nutrition, empty_nutrient_mesh};
use state::{ExploratoryProbe, Nutrient, NutrientState};
pub(super) use state::{NutrientRenderAssets, NutritionWorld};

const ENGULF_DURATION: f32 = 1.25;
const DIGESTION_DURATION: f32 = 6.0;
const EXPULSION_DURATION: f32 = 1.2;
const INTERNAL_WASTE_DRAG: f32 = 2.2;
// The procedural nutrient is slightly squashed, but its collision envelope is
// kept close to the rendered profile so it never looks embedded in a surface.
// The small skin also covers Avian's resting-contact tolerance.
const NUTRIENT_STRUCTURE_CONTACT_SCALE: f32 = 0.96;
const NUTRIENT_CONTACT_SKIN: f32 = 1.1;
const ENERGY_YIELD: f32 = 0.46;
const OBJECT_GRAVITY: f32 = 900.0;
const PHAGOCYTOSIS_REACH: f32 = 44.0;

pub(super) fn setup_nutrition(
    mut commands: Commands,
    level: Res<Level>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let mut nutrition = NutritionWorld::default();
    nutrition.reset_from_definitions(&level.nutrients);
    let slots = nutrition.nutrients.len().max(1);
    let mut nutrient_mesh = empty_nutrient_mesh();
    update_nutrient_mesh(&mut nutrient_mesh, &nutrition.nutrients, slots, 0.0, &[]);
    commands.insert_resource(nutrition);
    spawn_nutrient_bodies(&mut commands, &level.nutrients);
    let mesh = meshes.add(nutrient_mesh);
    commands.spawn((
        Mesh2d(mesh.clone()),
        // Vertex alpha shows nutrients through the translucent blob membrane.
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        Transform::from_xyz(0.0, 0.0, -0.06),
    ));
    commands.insert_resource(NutrientRenderAssets { mesh, slots });
}

/// Resources that drive one fixed-step update of the feeding state machine.
#[derive(SystemParam)]
pub(super) struct NutritionSimulationParams<'w> {
    time: Res<'w, Time<Fixed>>,
    keyboard: Res<'w, ButtonInput<KeyCode>>,
    blobs: Res<'w, BlobWorld>,
    level: Res<'w, Level>,
    vitality: ResMut<'w, VitalityWorld>,
    nutrition: ResMut<'w, NutritionWorld>,
    wastewater_effects: ResMut<'w, WastewaterEffects>,
}

/// Avian components belonging to free nutrients and digested residues.
#[derive(SystemParam)]
pub(super) struct NutrientBodyParams<'w, 's> {
    bodies: Query<
        'w,
        's,
        (
            &'static NutrientPhysics,
            &'static mut Transform,
            &'static mut LinearVelocity,
            &'static mut Collider,
        ),
    >,
}

pub(super) fn simulate_nutrition(
    simulation: NutritionSimulationParams,
    mut nutrient_bodies: NutrientBodyParams,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    let NutritionSimulationParams {
        time,
        keyboard,
        blobs,
        level,
        mut vitality,
        mut nutrition,
        mut wastewater_effects,
    } = simulation;
    let dt = time.delta_secs();
    let elapsed = time.elapsed_secs();
    let rolling_command = movement_command(&keyboard);
    sync_free_nutrients_before_digestion(
        FreeNutrientFrame {
            dt,
            elapsed,
            blobs: &blobs,
            level: &level,
        },
        &mut nutrition,
        &mut sound_events,
        &mut wastewater_effects,
        &mut nutrient_bodies.bodies,
    );
    advance_probe_and_capture(
        dt,
        &keyboard,
        rolling_command,
        &blobs,
        &level,
        &vitality,
        &mut nutrition,
    );

    let mut interrupted_probe = None;
    for nutrient in &mut nutrition.nutrients {
        match nutrient.state {
            NutrientState::Available { velocity } => {
                nutrient.state = NutrientState::Available { velocity };
            }
            NutrientState::Engulfing { .. } => {
                if let Some(probe) = advance_engulfing(
                    nutrient,
                    dt,
                    rolling_command,
                    &blobs,
                    &level,
                    &vitality,
                    &mut sound_events,
                ) {
                    interrupted_probe = Some(probe);
                }
            }
            NutrientState::Digesting { .. } => {
                advance_digesting(nutrient, dt, &blobs, &mut vitality, &mut sound_events)
            }
            NutrientState::Expelling { .. } => advance_expelling(nutrient, dt, &blobs),
            NutrientState::Waste { velocity } => {
                nutrient.state = NutrientState::Waste { velocity };
            }
        }
    }
    sync_nutrient_bodies_after_digestion(&nutrition, &mut nutrient_bodies.bodies);
    if let Some(probe) = interrupted_probe {
        nutrition.probe = Some(probe);
    }
}

fn movement_command(keyboard: &ButtonInput<KeyCode>) -> bool {
    keyboard.pressed(KeyCode::ArrowLeft)
        || keyboard.pressed(KeyCode::ArrowRight)
        || keyboard.pressed(KeyCode::KeyA)
        || keyboard.pressed(KeyCode::KeyD)
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn living_host<'a>(
    blobs: &'a BlobWorld,
    vitality: &VitalityWorld,
    id: u64,
) -> Option<&'a ActiveBlob> {
    blobs
        .active
        .iter()
        .find(|blob| blob.id == id && vitality.is_alive(id))
}

fn host_side(blob_id: u64) -> f32 {
    if blob_id & 1 == 0 { 1.0 } else { -1.0 }
}

#[cfg(test)]
mod tests;
