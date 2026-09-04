//! Avian integration and safety correction for free nutrients and residues.

use super::geometry::{circle_aabb_penetration, circle_convex_penetration};
use super::membrane::circle_blob_penetration;
use super::*;
use crate::environment::GameLayer;
use avian2d::dynamics::ccd::SweptCcd;
use avian2d::prelude::{Collider, CollisionLayers, GravityScale, LinearVelocity, RigidBody};

#[derive(Component)]
pub(crate) struct NutrientPhysics {
    pub(crate) index: usize,
}

/// Read-only inputs shared by every free nutrient during one fixed step.
pub(super) struct FreeNutrientFrame<'a> {
    pub(super) dt: f32,
    pub(super) elapsed: f32,
    pub(super) blobs: &'a BlobWorld,
    pub(super) level: &'a Level,
}

/// Rebuilds the Avian bodies that own free nutrient and residue motion.
/// Scenario changes call this after replacing the JSON-derived definitions.
pub(crate) fn spawn_nutrient_bodies(commands: &mut Commands, definitions: &[NutrientDefinition]) {
    for (index, definition) in definitions.iter().enumerate() {
        commands.spawn((
            NutrientPhysics { index },
            RigidBody::Dynamic,
            Collider::circle(nutrient_contact_radius(definition.radius)),
            GravityScale(1.0),
            CollisionLayers::new(
                [GameLayer::Projectile],
                [GameLayer::Environment, GameLayer::Projectile],
            ),
            LinearVelocity::ZERO,
            // Expulsion can create short, high-speed trajectories. Swept CCD
            // prevents the free nutrient from tunnelling through thin walls.
            SweptCcd::default(),
            Transform::from_xyz(definition.position.x, definition.position.y, 0.0),
        ));
    }
}

pub(super) fn free_nutrient_contact_radius(nutrient: &Nutrient) -> f32 {
    nutrient_contact_radius(nutrient.radius)
}

fn nutrient_contact_radius(radius: f32) -> f32 {
    radius * NUTRIENT_STRUCTURE_CONTACT_SCALE + NUTRIENT_CONTACT_SKIN
}

/// Wastewater is a volume, not a solid collider. Avian owns the nutrient's
/// position while this force supplies buoyancy and viscous drag inside it.
pub(super) fn apply_avian_nutrient_water_forces(
    position: Vec2,
    radius: f32,
    velocity: &mut LinearVelocity,
    dt: f32,
    elapsed: f32,
    level: &Level,
) -> Option<(usize, f32)> {
    for (area_index, area) in level.wastewater_areas.iter().enumerate() {
        if !area.contains_x(position.x) {
            continue;
        }
        let surface = area.surface_y(position.x, elapsed);
        let bottom = area.position.y - area.size.y * 0.5;
        if position.y + radius < bottom || position.y - radius > surface {
            continue;
        }
        let submerged = ((surface - (position.y - radius)) / (radius * 2.0)).clamp(0.0, 1.0);
        velocity.0.y += 1_150.0 * submerged * 1.28 * dt;
        velocity.0 *= (-5.8 * submerged * dt).exp();
        return Some((area_index, surface));
    }
    None
}

/// Resolves the exceptional case where a soft blob presses a dynamic nutrient
/// through an Avian contact in a single simulation step. Level geometry wins
/// over membrane contact; the nutrient is never allowed to remain inside
/// either kind of solid.
pub(super) fn resolve_free_nutrient_penetration(
    position: &mut Vec2,
    velocity: &mut LinearVelocity,
    radius: f32,
    level: &Level,
    blobs: &BlobWorld,
) -> f32 {
    let mut strongest_structure_impact = 0.0_f32;
    for _ in 0..4 {
        let correction = level
            .platforms
            .iter()
            .find_map(|platform| {
                circle_aabb_penetration(*position, radius, platform.center, platform.half_size)
                    .map(|(depth, normal)| (depth, normal, true))
            })
            .or_else(|| {
                level.fixtures.iter().find_map(|fixture| {
                    circle_convex_penetration(*position, radius, fixture)
                        .map(|(depth, normal)| (depth, normal, true))
                })
            })
            .or_else(|| {
                blobs.active.iter().find_map(|blob| {
                    circle_blob_penetration(*position, radius, &blob.body)
                        .map(|(depth, normal)| (depth, normal, false))
                })
            });
        let Some((depth, normal, is_structure)) = correction else {
            break;
        };
        if is_structure {
            strongest_structure_impact =
                strongest_structure_impact.max((-velocity.0.dot(normal)).max(0.0));
        }
        *position += normal * (depth + 0.35);
        let inward_speed = velocity.0.dot(normal);
        if inward_speed < 0.0 {
            velocity.0 -= normal * inward_speed;
        }
    }
    strongest_structure_impact
}

/// Synchronizes Avian's already-solved free-body motion into the biological
/// state and applies water forces for the following physics step.
pub(super) fn sync_free_nutrients_before_digestion(
    frame: FreeNutrientFrame<'_>,
    nutrition: &mut NutritionWorld,
    sound_events: &mut MessageWriter<BlobSoundEvent>,
    wastewater_effects: &mut WastewaterEffects,
    physics_nutrients: &mut Query<(
        &NutrientPhysics,
        &mut Transform,
        &mut LinearVelocity,
        &mut Collider,
    )>,
) {
    for (physics, mut transform, mut velocity, _) in physics_nutrients.iter_mut() {
        let Some(nutrient) = nutrition.nutrients.get_mut(physics.index) else {
            continue;
        };
        if !matches!(
            nutrient.state,
            NutrientState::Available { .. } | NutrientState::Waste { .. }
        ) {
            continue;
        }

        // Avian handles ordinary rigid-body contacts. This compact post-solve
        // pass is a safety net for a nutrient squeezed by the soft membrane.
        let contact_radius = free_nutrient_contact_radius(nutrient);
        let mut position = transform.translation.truncate();
        let structure_impact = resolve_free_nutrient_penetration(
            &mut position,
            &mut velocity,
            contact_radius,
            frame.level,
            frame.blobs,
        );
        if structure_impact >= 110.0 {
            sound_events.write(BlobSoundEvent::NutrientImpact {
                strength: (structure_impact / 480.0).clamp(0.0, 1.0),
            });
        }
        transform.translation = position.extend(transform.translation.z);
        nutrient.position = transform.translation.truncate();
        store_free_velocity(nutrient, velocity.0);

        let entry_speed = (-velocity.0.y).max(0.0);
        let surface = apply_avian_nutrient_water_forces(
            nutrient.position,
            contact_radius,
            &mut velocity,
            frame.dt,
            frame.elapsed,
            frame.level,
        );
        if let Some((area_index, surface_y)) = surface {
            if !nutrient.was_submerged {
                wastewater_effects.emit(
                    area_index,
                    Vec2::new(nutrient.position.x, surface_y),
                    nutrient.radius,
                    (entry_speed / 180.0).clamp(0.35, 1.25),
                );
                sound_events.write(BlobSoundEvent::NutrientWater {
                    strength: (entry_speed / 240.0).clamp(0.0, 1.0),
                });
            }
            nutrient.was_submerged = true;
        } else {
            nutrient.was_submerged = false;
        }
        store_free_velocity(nutrient, velocity.0);
    }
}

/// Returns captured nutrients to their internally controlled positions and
/// hands residues back to Avian once they leave the membrane.
pub(super) fn sync_nutrient_bodies_after_digestion(
    nutrition: &NutritionWorld,
    physics_nutrients: &mut Query<(
        &NutrientPhysics,
        &mut Transform,
        &mut LinearVelocity,
        &mut Collider,
    )>,
) {
    for (physics, mut transform, mut velocity, mut collider) in physics_nutrients.iter_mut() {
        let Some(nutrient) = nutrition.nutrients.get(physics.index) else {
            continue;
        };
        match nutrient.state {
            NutrientState::Engulfing { .. } | NutrientState::Digesting { .. } => {
                transform.translation = nutrient.position.extend(0.0);
                velocity.0 = Vec2::ZERO;
            }
            NutrientState::Expelling {
                velocity: outgoing, ..
            } => {
                transform.translation = nutrient.position.extend(0.0);
                velocity.0 = outgoing;
            }
            NutrientState::Waste { velocity: outgoing } => {
                if transform
                    .translation
                    .truncate()
                    .distance_squared(nutrient.position)
                    > 0.01
                {
                    transform.translation = nutrient.position.extend(0.0);
                    velocity.0 = outgoing;
                }
                *collider = Collider::circle(free_nutrient_contact_radius(nutrient));
            }
            NutrientState::Available { .. } => {}
        }
    }
}

fn store_free_velocity(nutrient: &mut Nutrient, velocity: Vec2) {
    match &mut nutrient.state {
        NutrientState::Available { velocity: stored }
        | NutrientState::Waste { velocity: stored } => {
            *stored = velocity;
        }
        _ => {}
    }
}
