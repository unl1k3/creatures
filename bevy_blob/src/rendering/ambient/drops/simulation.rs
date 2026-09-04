//! Falling-drop simulation, splashes and vertical surface queries.

use super::spawn::{spawn_dry_surface_splash, spawn_sparse_drop};
use super::*;
use bevy::ecs::system::SystemParam;

type DropBodies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut AmbientDrop,
        &'static mut Transform,
        &'static AmbientLightTint,
    ),
    (Without<AmbientSplashParticle>, Without<GameCamera>),
>;

type SplashBodies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut AmbientSplashParticle,
        &'static mut Transform,
    ),
    (Without<AmbientDrop>, Without<GameCamera>),
>;

#[derive(SystemParam)]
pub(crate) struct AmbientDropSimulation<'w, 's> {
    time: Res<'w, Time>,
    ink_style: Res<'w, InkStylePreview>,
    scenario: Res<'w, TestScenario>,
    level: Res<'w, Level>,
    camera: Single<'w, 's, &'static Transform, With<GameCamera>>,
    assets: Res<'w, AmbientDropAssets>,
    state: ResMut<'w, AmbientDropState>,
    drops: DropBodies<'w, 's>,
    splashes: SplashBodies<'w, 's>,
    materials: ResMut<'w, Assets<ColorMaterial>>,
}

struct FallingDropContext<'a> {
    assets: &'a AmbientDropAssets,
    level: &'a Level,
    camera_position: Vec2,
    dt: f32,
}

pub(crate) fn simulate_ambient_drops(
    simulation: AmbientDropSimulation,
    mut commands: Commands,
    mut sound_events: MessageWriter<BlobSoundEvent>,
    mut sound_cooldown: Local<f32>,
) {
    let AmbientDropSimulation {
        time,
        ink_style,
        scenario,
        level,
        camera,
        assets,
        mut state,
        mut drops,
        mut splashes,
        mut materials,
    } = simulation;
    let dt = time.delta_secs().min(1.0 / 20.0);
    *sound_cooldown = (*sound_cooldown - dt).max(0.0);
    if !ink_style.enabled || !matches!(scenario.0, 0 | 1) {
        for (entity, _, _, _) in &mut drops {
            commands.entity(entity).despawn();
        }
        for (entity, _, _) in &mut splashes {
            commands.entity(entity).despawn();
        }
        state.scenario = None;
        state.normal_delay = 0.0;
        return;
    }

    update_falling_drops(
        &mut commands,
        &mut drops,
        &mut materials,
        FallingDropContext {
            assets: &assets,
            level: &level,
            camera_position: camera.translation.truncate(),
            dt,
        },
        &mut sound_events,
        &mut sound_cooldown,
    );
    update_splash_particles(
        &mut commands,
        &mut splashes,
        camera.translation.truncate(),
        dt,
    );

    if state.scenario != Some(scenario.0) {
        state.scenario = Some(scenario.0);
        state.normal_delay = 0.6 + state.unit_random() * 1.4;
    }
    state.normal_delay -= dt;
    if state.normal_delay > 0.0 {
        return;
    }
    state.normal_delay += 1.45 + state.unit_random() * 2.35;
    spawn_sparse_drop(
        &mut commands,
        &mut materials,
        &assets,
        &level,
        camera.translation.truncate(),
        &mut state,
    );
}

fn update_falling_drops(
    commands: &mut Commands,
    drops: &mut DropBodies,
    materials: &mut Assets<ColorMaterial>,
    context: FallingDropContext<'_>,
    sound_events: &mut MessageWriter<BlobSoundEvent>,
    sound_cooldown: &mut f32,
) {
    for (entity, mut drop, mut transform, tint) in drops {
        drop.velocity.y -= drop.gravity * context.dt;
        let velocity = drop.velocity;
        drop.position += velocity * context.dt;
        transform.translation = (drop.position
            + parallax_offset(context.camera_position, drop.parallax))
        .extend(drop.depth);
        let speed_stretch = 1.0 + (-drop.velocity.y / 360.0).clamp(0.0, 0.65);
        transform.scale.y = drop.radius * speed_stretch;
        if let Some(mut material) = materials.get_mut(&tint.material) {
            material.color = palette::color(light_dynamic_rgba(
                palette::AMBIENT_DROP,
                drop.position,
                &context.level.lights,
            ));
        }
        let terminal_y = drop.terminal_y(context.camera_position);
        if drop.position.y - drop.radius * speed_stretch <= terminal_y {
            if drop.splash_on_impact {
                spawn_dry_surface_splash(
                    commands,
                    context.assets,
                    Vec2::new(drop.position.x, terminal_y),
                    drop.radius,
                    drop.depth,
                    drop.parallax,
                    context.camera_position,
                );
                if *sound_cooldown <= 0.0 {
                    sound_events.write(BlobSoundEvent::AmbientDrop);
                    *sound_cooldown = 0.36;
                }
            }
            commands.entity(entity).despawn();
        }
    }
}

fn update_splash_particles(
    commands: &mut Commands,
    splashes: &mut SplashBodies,
    camera_position: Vec2,
    dt: f32,
) {
    for (entity, mut particle, mut transform) in splashes {
        particle.remaining -= dt;
        if particle.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        particle.velocity.y -= particle.gravity * dt;
        let velocity = particle.velocity;
        particle.position += velocity * dt;
        transform.translation = (particle.position
            + parallax_offset(camera_position, particle.parallax))
        .extend(particle.depth);
        let life = (particle.remaining / particle.duration).clamp(0.0, 1.0);
        let scale = particle.radius * life.sqrt();
        transform.scale = Vec3::new(scale, scale * 1.35, 1.0);
    }
}

pub(super) fn first_surface_below(origin: Vec2, level: &Level) -> Option<f32> {
    let mut best: Option<f32> = None;
    let mut consider = |height: f32| {
        if height < origin.y && best.is_none_or(|current| height > current) {
            best = Some(height);
        }
    };
    for platform in &level.platforms {
        if (origin.x - platform.center.x).abs() <= platform.half_size.x {
            consider(platform.center.y + platform.half_size.y);
        }
    }
    for polygon in &level.fixtures {
        for (start, end) in polygon
            .iter()
            .copied()
            .zip(polygon.iter().copied().cycle().skip(1))
            .take(polygon.len())
        {
            let delta_x = end.x - start.x;
            if delta_x.abs() <= f32::EPSILON {
                continue;
            }
            let fraction = (origin.x - start.x) / delta_x;
            if (0.0..=1.0).contains(&fraction) {
                consider(start.y + (end.y - start.y) * fraction);
            }
        }
    }
    for area in &level.wastewater_areas {
        if (origin.x - area.position.x).abs() <= area.size.x * 0.5 {
            consider(area.position.y + area.size.y * 0.5);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{blob::Platform, level_format::WastewaterAreaDefinition};

    #[test]
    fn drop_ends_on_the_first_physical_surface_below_its_outlet() {
        let level = Level::from_test_geometry(
            vec![
                Platform {
                    center: Vec2::new(0.0, -100.0),
                    half_size: Vec2::new(80.0, 10.0),
                },
                Platform {
                    center: Vec2::new(0.0, -220.0),
                    half_size: Vec2::new(80.0, 10.0),
                },
            ],
            Vec::new(),
        );
        assert_eq!(
            first_surface_below(Vec2::new(0.0, 50.0), &level),
            Some(-90.0)
        );
    }

    #[test]
    fn wastewater_surface_collects_drops_without_a_platform_below() {
        let mut level = Level::from_test_geometry(Vec::new(), Vec::new());
        level.wastewater_areas.push(WastewaterAreaDefinition {
            position: Vec2::new(0.0, -200.0),
            size: Vec2::new(400.0, 80.0),
            color: palette::DEFAULT_WASTEWATER,
            wave_height: 4.0,
            wave_speed: 0.3,
            depth: -0.12,
            bubbles: None,
            immune_family: None,
        });
        assert_eq!(
            first_surface_below(Vec2::new(100.0, 50.0), &level),
            Some(-160.0)
        );
        assert_eq!(first_surface_below(Vec2::new(250.0, 50.0), &level), None);
    }
}
