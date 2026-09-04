//! Counterweight-driven plates and gates.

use super::*;

/// A gate translated by its linked counterweight plate.
#[derive(Component)]
pub(crate) struct CounterbalanceGate {
    pub(super) platform_index: usize,
    pub(super) closed_center: Vec2,
}

/// A plate that measures the blob mass resting on it.
#[derive(Component)]
pub(crate) struct CounterbalancePlate {
    pub(super) platform_index: usize,
    pub(super) closed_center: Vec2,
}

/// Opens a linked gate while enough blob mass rests on the plate.
///
/// Both the authored level geometry and its Avian transform are updated so
/// the custom soft body and rigid-body representation remain synchronized.
pub(crate) fn simulate_counterbalances(
    time: Res<Time<Fixed>>,
    mut blobs: ResMut<BlobWorld>,
    mut level: ResMut<Level>,
    // These entity sets are disjoint, allowing both transforms to be mutable.
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
            let rides_plate = (center.x - plate_surface.center.x).abs()
                <= plate_surface.half_size.x + radius * 0.3
                && center.y - radius <= plate_top + 5.0
                // Passing beneath the plate must not activate it.
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
        let current_gate = level.platforms[balance.gate_platform].center;
        let desired = closed + balance.open_offset * lift;
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
            if plate.platform_index != plate_index {
                continue;
            }

            let plate_target = plate.closed_center - balance.open_offset * lift;
            let previous_position = level.platforms[plate_index].center;
            let plate_position = previous_position.lerp(plate_target, blend);
            let plate_delta = plate_position - previous_position;
            level.platforms[plate_index].center = plate_position;
            transform.translation = plate_position.extend(0.0);

            // Avian cannot convey the custom membrane automatically. Riders
            // follow the moving support without receiving a launch impulse.
            for &index in &riders {
                for particle in &mut blobs.active[index].body.particles {
                    particle.position += plate_delta;
                    particle.previous += plate_delta;
                }
            }
        }
    }
}
