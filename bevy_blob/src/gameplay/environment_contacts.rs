//! Physical exchanges between blobs and dynamic level elements.

use super::*;
use crate::blob::WastewaterContact;
use crate::vitality::WASTEWATER_DAMAGE_PER_SECOND;
use avian2d::prelude::LinearVelocity;
use bevy::ecs::system::SystemParam;

/// Avian-owned nutrient bodies that can exchange contact impulses with blobs.
#[derive(SystemParam)]
pub(crate) struct NutrientContactParams<'w, 's> {
    pub(super) bodies: Query<
        'w,
        's,
        (
            &'static NutrientPhysics,
            &'static mut Transform,
            &'static mut LinearVelocity,
        ),
    >,
}

pub(super) struct WastewaterFrame {
    pub(super) elapsed: f32,
    pub(super) dt: f32,
    pub(super) movement: f32,
    pub(super) shield_extension: f32,
    pub(super) alive: bool,
}

/// Separates free Avian nutrients from the soft membrane after contact.
pub(super) fn resolve_nutrient_contacts(
    active_blob: &mut ActiveBlob,
    nutrition: &NutritionWorld,
    nutrient_bodies: &mut Query<(
        &'static NutrientPhysics,
        &'static mut Transform,
        &'static mut LinearVelocity,
    )>,
) {
    for (nutrient, mut transform, mut velocity) in nutrient_bodies {
        if !nutrition.is_free_index(nutrient.index) {
            continue;
        }
        let center = transform.translation.truncate();
        let Some(radius) = nutrition.collision_radius(nutrient.index) else {
            continue;
        };
        let Some((depth, normal)) = circle_blob_penetration(center, radius, &active_blob.body)
        else {
            continue;
        };
        // Avian owns the nutrient: release that body, then apply only a small
        // equal reaction to the membrane instead of translating the whole blob.
        transform.translation += (normal * (depth + 0.15)).extend(0.0);
        let inward = velocity.0.dot(normal);
        if inward < 0.0 {
            velocity.0 -= normal * inward;
        }
        active_blob.body.add_velocity(-normal * depth * 0.10);
    }
}

/// Applies liquid physics, damage and entry effects, returning contact data
/// needed by the independent audio feedback module.
pub(super) fn apply_wastewater_contact(
    active_blob: &mut ActiveBlob,
    level: &Level,
    vitality: &mut VitalityWorld,
    effects: &mut WastewaterEffects,
    frame: WastewaterFrame,
) -> Option<WastewaterContact> {
    let (area_index, area, contact) = level
        .wastewater_areas
        .iter()
        .copied()
        .enumerate()
        .find_map(|(area_index, area)| {
            let center = active_blob.body.center();
            area.contains_x(center.x).then(|| {
                let surface_y = area.surface_y(center.x, frame.elapsed);
                let bottom_y = area.position.y - area.size.y * 0.5;
                active_blob
                    .body
                    .apply_wastewater_forces_with_spine_drag(
                        surface_y,
                        bottom_y,
                        frame.dt,
                        frame.shield_extension,
                        frame.movement,
                    )
                    .map(|contact| (area_index, area, contact))
            })?
        })?;

    let immune = area
        .immune_family
        .is_some_and(|family| crate::palette::blob_family_index(active_blob.parent_id) == family);
    if frame.alive && !immune {
        vitality.damage(
            active_blob.id,
            WASTEWATER_DAMAGE_PER_SECOND * contact.submerged_fraction * frame.dt,
        );
    }
    if contact.entered {
        let impact_strength = (contact.entry_speed / 430.0).clamp(0.45, 1.45);
        effects.emit(
            area_index,
            Vec2::new(active_blob.body.center().x, contact.surface_y),
            active_blob.body.rest_radius * (0.46 + contact.submerged_fraction * 0.42),
            impact_strength,
        );
    }
    Some(contact)
}
