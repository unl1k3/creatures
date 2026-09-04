//! Procedural wastewater bubbles, bursts, and impact splashes.

use super::*;
use bevy::ecs::system::SystemParam;

type BubbleBodies<'w, 's> =
    Query<'w, 's, (Entity, &'static WastewaterBubble, &'static mut Transform)>;

/// Runtime state and rendering assets used by the wastewater bubble system.
#[derive(SystemParam)]
pub(crate) struct BubbleSimulation<'w, 's> {
    time: Res<'w, Time>,
    scenario: Res<'w, TestScenario>,
    level: Res<'w, Level>,
    assets: Res<'w, AmbientDropAssets>,
    effect_materials: ResMut<'w, WastewaterEffectMaterials>,
    materials: ResMut<'w, Assets<ColorMaterial>>,
    effects: ResMut<'w, WastewaterEffects>,
    state: ResMut<'w, WastewaterBubbleState>,
    bubbles: BubbleBodies<'w, 's>,
}

pub(crate) fn simulate_wastewater_impacts(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<AmbientDropAssets>,
    level: Res<Level>,
    mut effect_materials: ResMut<WastewaterEffectMaterials>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut effects: ResMut<WastewaterEffects>,
) {
    effects.advance(time.delta_secs().min(1.0 / 20.0));
    for impact in std::mem::take(&mut effects.pending) {
        // Keep droplets in the readable range established for bubble bursts.
        // Object size and impact energy affect the wave, not particle size.
        let splash_radius = (impact.source_radius * 0.55).clamp(4.5, 7.5);
        let color = level
            .wastewater_areas
            .get(impact.area_index)
            .map(|area| area.color)
            .unwrap_or(palette::DEFAULT_WASTEWATER_RUNTIME);
        let material = wastewater_effect_material(color, &mut effect_materials, &mut materials);
        spawn_bubble_burst(
            &mut commands,
            &assets,
            impact.position,
            splash_radius,
            impact.variation,
            material,
        );
    }
}

pub(crate) fn simulate_wastewater_bubbles(
    mut commands: Commands,
    simulation: BubbleSimulation,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    let BubbleSimulation {
        time,
        scenario,
        level,
        assets,
        mut effect_materials,
        mut materials,
        mut effects,
        mut state,
        mut bubbles,
    } = simulation;
    let dt = time.delta_secs().min(1.0 / 20.0);
    let visible = !level.wastewater_areas.is_empty();
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
        let surface_y = bubble
            .area
            .surface_y(transform.translation.x, time.elapsed_secs());
        let bottom_y = bubble.area.position.y - bubble.area.size.y * 0.5;
        let ascent =
            ((transform.translation.y - bottom_y) / (surface_y - bottom_y)).clamp(0.0, 1.0);
        let radius = bubble.base_radius * (1.0 + ascent * 0.38);
        transform.scale = Vec3::new(radius * (1.0 + ascent * 0.08), radius, 1.0);

        if transform.translation.y + radius >= surface_y {
            let impact = Vec2::new(transform.translation.x, surface_y);
            let variation = effects.emit_ripple(bubble.area_index, impact, radius * 0.72, 0.45);
            let material = wastewater_effect_material(
                bubble.area.color,
                &mut effect_materials,
                &mut materials,
            );
            spawn_bubble_burst(&mut commands, &assets, impact, radius, variation, material);
            // This event is intentionally adjacent to the surface burst and
            // despawn: every bubble that visibly pops owns one matching cue.
            // Bubble intervals in the JSON already control the overall rate.
            sound_events.write(BlobSoundEvent::AmbientBubble);
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
            let material =
                wastewater_effect_material(area.color, &mut effect_materials, &mut materials);
            spawn_wastewater_bubble(
                &mut commands,
                &assets,
                &mut state,
                area_index,
                area,
                settings,
                material,
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
    material: Handle<ColorMaterial>,
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
        MeshMaterial2d(material),
        Transform {
            // Explicitly above the front wastewater layer. The previous
            // near-identical depths made transparent sorting unreliable.
            translation: Vec3::new(x, y, 0.10),
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
    variation: f32,
    material: Handle<ColorMaterial>,
) {
    let count = 3 + (variation * 4.0).floor() as usize;
    for index in 0..count {
        let fraction = (index as f32 + 0.5) / count as f32;
        let direction_variation = organic_splash_random(variation, index, 0);
        let speed_variation = organic_splash_random(variation, index, 1);
        let size_variation = organic_splash_random(variation, index, 2);
        let angle =
            0.48 + fraction * (std::f32::consts::PI - 0.96) + (direction_variation - 0.5) * 0.34;
        let velocity = Vec2::from_angle(angle) * (34.0 + speed_variation * 34.0);
        let duration = 0.26 + organic_splash_random(variation, index, 3) * 0.15;
        let radius = source_radius * (0.20 + size_variation * 0.16);
        commands.spawn((
            AmbientSplashParticle {
                position: impact + Vec2::Y * radius,
                velocity,
                gravity: 245.0,
                remaining: duration,
                duration,
                radius,
                depth: 0.11,
                parallax: 1.0,
            },
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(material.clone()),
            Transform {
                translation: (impact + Vec2::Y * radius).extend(0.11),
                scale: Vec3::new(radius, radius * 1.25, 1.0),
                ..default()
            },
        ));
    }
}

fn wastewater_effect_material(
    water: [f32; 4],
    cache: &mut WastewaterEffectMaterials,
    materials: &mut Assets<ColorMaterial>,
) -> Handle<ColorMaterial> {
    let key = [
        (water[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (water[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (water[2].clamp(0.0, 1.0) * 255.0).round() as u8,
    ];
    cache
        .0
        .entry(key)
        .or_insert_with(|| {
            let mut tint = palette::mix(water, palette::IVORY, 0.26);
            // A translucent base selects Bevy's alpha-blended material path,
            // preserving the bubble mesh's internal alpha gradient.
            tint[3] = 0.92;
            materials.add(ColorMaterial::from(palette::color(tint)))
        })
        .clone()
}
