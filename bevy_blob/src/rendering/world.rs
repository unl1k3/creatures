//! Gizmo-based world, mechanism, and debug visualization.

use super::*;
use bevy::ecs::system::SystemParam;

/// Read-only gameplay state consumed by gizmo-based world rendering.
#[derive(SystemParam)]
pub(crate) struct WorldDrawingState<'w> {
    blobs: Res<'w, BlobWorld>,
    vitality: Res<'w, VitalityWorld>,
    level: Res<'w, Level>,
    debug_overlay: Res<'w, LevelDebugOverlay>,
    route_progress: Res<'w, RouteProgress>,
    nutrition: Res<'w, NutritionWorld>,
    ink_style: Res<'w, InkStylePreview>,
}

/// Draws a thin, slightly twisted ink rope instead of a single debug line.
fn draw_ink_rope(gizmos: &mut Gizmos, start: Vec2, end: Vec2, ink: Color) {
    let span = end - start;
    let length = span.length();
    if length <= f32::EPSILON {
        return;
    }
    let direction = span / length;
    let normal = Vec2::new(-direction.y, direction.x);
    let edge = normal * 1.35;
    gizmos.line_2d(start + edge, end + edge, ink);
    gizmos.line_2d(start - edge, end - edge, ink);

    // Short alternating ties keep the cable organic while remaining readable
    // at the small in-game scale.
    let ties = (length / 18.0).floor() as usize;
    for index in 1..ties {
        let center = start + direction * (index as f32 * 18.0);
        let slant = if index % 2 == 0 {
            direction
        } else {
            -direction
        };
        gizmos.line_2d(
            center - edge - slant * 1.8,
            center + edge + slant * 1.8,
            ink,
        );
    }
}

pub(crate) fn draw_world(mut gizmos: Gizmos, state: WorldDrawingState) {
    let WorldDrawingState {
        blobs,
        vitality: vitality_world,
        level,
        debug_overlay,
        route_progress,
        nutrition,
        ink_style,
    } = state;
    // Counterbalances are rendered as ink mechanisms, not as debug volumes.
    for balance in &level.counterbalances {
        if let (Some(plate), Some(gate)) = (
            level.platforms.get(balance.plate_platform),
            level.platforms.get(balance.gate_platform),
        ) {
            // Fixed pulleys sit above the two moving ends. The cable sections
            // then make the equal and opposite travel of plate and gate clear.
            // Higher than the gate's fully open top edge, so the door never
            // visually crosses a pulley during its upward stroke.
            let pulley_height = 145.0;
            let left_pulley = Vec2::new(plate.center.x, pulley_height);
            let right_pulley = Vec2::new(gate.center.x, pulley_height);
            let plate_anchor = plate.center + Vec2::Y * plate.half_size.y;
            let gate_anchor = gate.center + Vec2::Y * gate.half_size.y;
            let ink_at = |position| {
                game_palette::color(light_dynamic_rgba(
                    game_palette::INK,
                    position,
                    &level.lights,
                ))
            };
            draw_ink_rope(
                &mut gizmos,
                plate_anchor,
                left_pulley,
                ink_at((plate_anchor + left_pulley) * 0.5),
            );
            draw_ink_rope(
                &mut gizmos,
                left_pulley,
                right_pulley,
                ink_at((left_pulley + right_pulley) * 0.5),
            );
            draw_ink_rope(
                &mut gizmos,
                right_pulley,
                gate_anchor,
                ink_at((right_pulley + gate_anchor) * 0.5),
            );
            for pulley in [left_pulley, right_pulley] {
                let cable = ink_at(pulley);
                // A small hanging bracket and two imperfect-looking rings
                // read as hand-drawn hardware over the ivory level artwork.
                gizmos.line_2d(pulley + Vec2::Y * 10.0, pulley + Vec2::Y * 23.0, cable);
                gizmos.line_2d(
                    pulley + Vec2::new(-8.0, 23.0),
                    pulley + Vec2::new(8.0, 23.0),
                    cable,
                );
                gizmos.circle_2d(pulley, 11.0, cable);
                gizmos.circle_2d(pulley, 4.0, cable);
                gizmos.line_2d(
                    pulley + Vec2::new(-7.0, -7.0),
                    pulley + Vec2::new(7.0, 7.0),
                    cable,
                );
                gizmos.line_2d(
                    pulley + Vec2::new(-7.0, 7.0),
                    pulley + Vec2::new(7.0, -7.0),
                    cable,
                );
            }
        }
    }
    // Laboratories without artwork retain their unobtrusive collision view.
    if !ink_style.enabled && !level.has_artwork() && !debug_overlay.visible {
        for platform in &level.platforms {
            gizmos.rect_2d(
                platform.center,
                platform.half_size * 2.0,
                game_palette::color(game_palette::LAB_PLATFORM),
            );
        }
        for fixture in &level.fixtures {
            gizmos.lineloop_2d(
                fixture.iter().copied(),
                game_palette::color(game_palette::LAB_FIXTURE),
            );
        }
    }
    if debug_overlay.visible {
        // Draw three close contours to remain readable over detailed artwork.
        for platform in &level.platforms {
            for expansion in [-3.0, 0.0, 3.0] {
                gizmos.rect_2d(
                    platform.center,
                    platform.half_size * 2.0 + Vec2::splat(expansion),
                    game_palette::color(game_palette::DEBUG_PLATFORM),
                );
            }
        }
        for fixture in &level.fixtures {
            for (first, second) in fixture
                .iter()
                .copied()
                .zip(fixture.iter().copied().cycle().skip(1))
                .take(fixture.len())
            {
                let normal = (second - first).perp().normalize_or_zero();
                for offset in [-2.0, 0.0, 2.0] {
                    gizmos.line_2d(
                        first + normal * offset,
                        second + normal * offset,
                        game_palette::color(game_palette::DEBUG_PLATFORM),
                    );
                }
            }
        }
        for expansion in [-3.0, 0.0, 3.0] {
            gizmos.rect_2d(
                level.center(),
                level.size() + Vec2::splat(expansion),
                game_palette::color(game_palette::DEBUG_BOUNDS),
            );
        }
        let marker_size = 14.0;
        for offset in [-2.0, 0.0, 2.0] {
            gizmos.line_2d(
                level.spawn_position + Vec2::new(-marker_size, offset),
                level.spawn_position + Vec2::new(marker_size, offset),
                game_palette::color(game_palette::DEBUG_SPAWN),
            );
            gizmos.line_2d(
                level.spawn_position + Vec2::new(offset, -marker_size),
                level.spawn_position + Vec2::new(offset, marker_size),
                game_palette::color(game_palette::DEBUG_SPAWN),
            );
        }
        for light in level.lights.iter().filter(|light| light.enabled) {
            let color = Color::srgba(
                light.color[0],
                light.color[1],
                light.color[2],
                (0.35 + light.intensity * 0.18).clamp(0.35, 0.9),
            );
            gizmos.circle_2d(light.position, light.radius, color);
            gizmos.circle_2d(light.position, 5.0, color);
        }
        for point in &level.expulsion_points {
            let length = (point.strength * 0.12).clamp(20.0, 80.0);
            let end = point.position + point.direction * length;
            gizmos.arrow_2d(
                point.position,
                end,
                game_palette::color(game_palette::DEBUG_EXPULSION),
            );
        }
        for hazard in &level.hazards {
            for expansion in [-2.0, 0.0, 2.0] {
                gizmos.rect_2d(
                    hazard.position,
                    hazard.size + Vec2::splat(expansion),
                    game_palette::color(game_palette::DEBUG_HAZARD),
                );
            }
        }
    }
    if debug_overlay.visible {
        for (index, checkpoint) in level.route.iter().enumerate().skip(route_progress.next) {
            let radius = (7.0 + index as f32 * 1.5).min(20.0);
            gizmos.circle_2d(
                *checkpoint,
                radius,
                game_palette::color(game_palette::DEBUG_ROUTE),
            );
        }
    }
    if !debug_overlay.visible {
        for hazard in &level.hazards {
            let left = hazard.position.x - hazard.size.x * 0.5;
            // Hazard volumes grow upward from their supporting surface.
            let surface_y = hazard.position.y - hazard.size.y * 0.5;
            let surface = (0..=12).map(|step| {
                let fraction = step as f32 / 12.0;
                Vec2::new(
                    left + hazard.size.x * fraction,
                    surface_y + (fraction * std::f32::consts::TAU * 2.0).sin() * 2.4,
                )
            });
            gizmos.linestrip_2d(surface, game_palette::color(game_palette::HAZARD_SURFACE));
            for offset in [0.2, 0.5, 0.78] {
                gizmos.circle_2d(
                    Vec2::new(left + hazard.size.x * offset, surface_y - 5.0),
                    2.5,
                    game_palette::color(game_palette::HAZARD_BUBBLE),
                );
            }
        }
    }

    for active_blob in &blobs.active {
        let blob = &active_blob.body;
        let vitality = vitality_world.get(active_blob.id);
        let center = blob.center();
        let size_scale = blob.size_scale();
        if debug_overlay.visible {
            let membrane = rendered_membrane_points(blob, nutrition.internal_load(active_blob.id));
            for point in membrane.iter().filter(|point| point.temporary) {
                let radius = if point.attachment { 2.0 } else { 1.35 };
                let point_color = if point.attachment {
                    game_palette::color(game_palette::DEBUG_ATTACHMENT)
                } else {
                    game_palette::color(game_palette::DEBUG_PARTICLE)
                };
                gizmos.circle_2d(point.position, (radius * size_scale).max(0.72), point_color);
            }
            for particle in &blob.particles {
                gizmos.line_2d(
                    center,
                    particle.position,
                    game_palette::color(game_palette::DEBUG_SPRING),
                );
            }
            if vitality.is_alive() {
                gizmos.circle_2d(
                    center,
                    9.0 * size_scale,
                    game_palette::color(game_palette::DEBUG_CENTER),
                );
            }
        }

        if vitality.is_alive() && blob.charge > 0.0 {
            let radius = charge_indicator_radius(blob);
            let line_spacing = (1.8 * size_scale).max(0.9);
            gizmos.circle_2d(
                center,
                radius,
                game_palette::color(game_palette::CHARGE_GLOW),
            );
            for offset in [-line_spacing, 0.0, line_spacing] {
                gizmos.arc_2d(
                    center,
                    std::f32::consts::TAU * blob.charge,
                    radius + offset,
                    game_palette::color(game_palette::CHARGE_ARC),
                );
            }
        }
    }
}
