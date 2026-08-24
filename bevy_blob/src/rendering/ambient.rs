use super::InkStylePreview;
use crate::environment::{Level, TestScenario};
use bevy::prelude::*;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};

#[derive(Resource)]
pub(crate) struct AmbientDropAssets {
    mesh: Handle<Mesh>,
    material: Handle<ColorMaterial>,
}

#[derive(Resource, Default)]
pub(crate) struct AmbientDropState {
    scenario: Option<u8>,
    timers: Vec<f32>,
}

#[derive(Component)]
pub(crate) struct AmbientDrop {
    velocity: Vec2,
    gravity: f32,
    radius: f32,
    terminal_y: f32,
    splash_on_impact: bool,
}

#[derive(Component)]
pub(crate) struct AmbientSplashParticle {
    velocity: Vec2,
    gravity: f32,
    remaining: f32,
    duration: f32,
    radius: f32,
}

pub(crate) fn setup_ambient_drop_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(AmbientDropAssets {
        mesh: meshes.add(create_teardrop_mesh()),
        material: materials.add(ColorMaterial::from(Color::srgb(0.02, 0.72, 0.82))),
    });
    commands.insert_resource(AmbientDropState::default());
}

pub(crate) fn simulate_ambient_drops(
    mut commands: Commands,
    time: Res<Time>,
    ink_style: Res<InkStylePreview>,
    scenario: Res<TestScenario>,
    level: Res<Level>,
    assets: Res<AmbientDropAssets>,
    mut state: ResMut<AmbientDropState>,
    mut drops: Query<(Entity, &mut AmbientDrop, &mut Transform), Without<AmbientSplashParticle>>,
    mut splashes: Query<(Entity, &mut AmbientSplashParticle, &mut Transform), Without<AmbientDrop>>,
) {
    let dt = time.delta_secs().min(1.0 / 20.0);
    if !ink_style.enabled || !matches!(scenario.0, 0 | 1) {
        for (entity, _, _) in &mut drops {
            commands.entity(entity).despawn();
        }
        for (entity, _, _) in &mut splashes {
            commands.entity(entity).despawn();
        }
        state.scenario = None;
        state.timers.clear();
        return;
    }

    for (entity, mut drop, mut transform) in &mut drops {
        drop.velocity.y -= drop.gravity * dt;
        transform.translation += (drop.velocity * dt).extend(0.0);
        let speed_stretch = 1.0 + (-drop.velocity.y / 360.0).clamp(0.0, 0.65);
        transform.scale.y = drop.radius * speed_stretch;
        if transform.translation.y - drop.radius * speed_stretch <= drop.terminal_y {
            if drop.splash_on_impact {
                spawn_dry_surface_splash(
                    &mut commands,
                    &assets,
                    Vec2::new(transform.translation.x, drop.terminal_y),
                    drop.radius,
                );
            }
            commands.entity(entity).despawn();
        }
    }

    for (entity, mut particle, mut transform) in &mut splashes {
        particle.remaining -= dt;
        if particle.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        particle.velocity.y -= particle.gravity * dt;
        transform.translation += (particle.velocity * dt).extend(0.0);
        let life = (particle.remaining / particle.duration).clamp(0.0, 1.0);
        let scale = particle.radius * life.sqrt();
        transform.scale = Vec3::new(scale, scale * 1.35, 1.0);
    }

    if state.scenario != Some(scenario.0) || state.timers.len() != level.drop_emitters.len() {
        state.scenario = Some(scenario.0);
        state.timers = level
            .drop_emitters
            .iter()
            .map(|emitter| -emitter.initial_delay)
            .collect();
    }

    for (index, emitter) in level.drop_emitters.iter().enumerate() {
        state.timers[index] += dt;
        if state.timers[index] < 0.0 {
            continue;
        }
        state.timers[index] -= emitter.interval;
        let surface = first_surface_below(emitter.position, &level);
        let terminal_y = surface
            .unwrap_or_else(|| level.center().y - level.size().y * 0.5 - emitter.radius * 4.0);
        commands.spawn((
            AmbientDrop {
                velocity: Vec2::ZERO,
                gravity: emitter.gravity,
                radius: emitter.radius,
                terminal_y,
                splash_on_impact: surface.is_some(),
            },
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(assets.material.clone()),
            Transform {
                translation: emitter.position.extend(emitter.depth),
                scale: Vec3::new(emitter.radius * 1.35, emitter.radius, 1.0),
                ..default()
            },
        ));
    }
}

fn spawn_dry_surface_splash(
    commands: &mut Commands,
    assets: &AmbientDropAssets,
    impact: Vec2,
    source_radius: f32,
) {
    // Above platform artwork (-0.13..-0.105), but still behind the blob fill (-0.1).
    const SPLASH_DEPTH: f32 = -0.102;
    let velocities = [
        Vec2::new(-62.0, 45.0),
        Vec2::new(-43.0, 62.0),
        Vec2::new(-20.0, 76.0),
        Vec2::new(5.0, 82.0),
        Vec2::new(29.0, 72.0),
        Vec2::new(51.0, 57.0),
        Vec2::new(67.0, 40.0),
    ];
    for (index, velocity) in velocities.into_iter().enumerate() {
        let duration = 0.34 + index as f32 * 0.018;
        let radius = source_radius * (0.34 + index as f32 * 0.022);
        commands.spawn((
            AmbientSplashParticle {
                velocity,
                gravity: 300.0,
                remaining: duration,
                duration,
                radius,
            },
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(assets.material.clone()),
            Transform {
                translation: (impact + Vec2::Y * radius).extend(SPLASH_DEPTH),
                scale: Vec3::new(radius, radius * 1.35, 1.0),
                ..default()
            },
        ));
    }
}

fn create_teardrop_mesh() -> Mesh {
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

fn first_surface_below(origin: Vec2, level: &Level) -> Option<f32> {
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
    best
}

#[cfg(test)]
mod tests {
    use super::first_surface_below;
    use crate::{blob::Platform, environment::Level};
    use bevy::prelude::*;

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
}
