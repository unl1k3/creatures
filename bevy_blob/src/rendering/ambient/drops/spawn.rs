//! Construction of ambient drops and dry-surface splash particles.

use super::simulation::first_surface_below;
use super::*;

struct DropSpawnSpec {
    position: Vec2,
    velocity: Vec2,
    gravity: f32,
    radius: f32,
    terminal_world_y: f32,
    splash_on_impact: bool,
}

pub(super) fn spawn_sparse_drop(
    commands: &mut Commands,
    materials: &mut Assets<ColorMaterial>,
    assets: &AmbientDropAssets,
    level: &Level,
    camera_position: Vec2,
    state: &mut AmbientDropState,
) {
    let (rain_left, rain_right) = rain_horizontal_bounds(level);
    let radius = 2.2 + state.unit_random() * 1.25;
    let position = Vec2::new(
        rain_left + (rain_right - rain_left) * state.unit_random(),
        rain_start_y(camera_position, level),
    );
    let surface = first_surface_below(position, level);
    spawn_drop(
        commands,
        materials,
        assets,
        level,
        DropSpawnSpec {
            position,
            velocity: Vec2::new(0.0, -24.0 - state.unit_random() * 42.0),
            gravity: 370.0 + state.unit_random() * 110.0,
            radius,
            terminal_world_y: surface
                .unwrap_or_else(|| level.center().y - level.size().y * 0.5 - radius * 4.0),
            splash_on_impact: surface.is_some(),
        },
    );
}

pub(super) fn spawn_drop_shower(
    commands: &mut Commands,
    materials: &mut Assets<ColorMaterial>,
    assets: &AmbientDropAssets,
    level: &Level,
    camera_position: Vec2,
    state: &mut AmbientDropState,
) {
    const DROP_COUNT: usize = 12;
    let start_y = rain_start_y(camera_position, level);
    let (rain_left, rain_right) = rain_horizontal_bounds(level);
    for index in 0..DROP_COUNT {
        let fraction = (index as f32 + 0.5) / DROP_COUNT as f32;
        let horizontal = state.unit_random() - 0.5;
        let height = state.unit_random() - 0.5;
        let speed = state.unit_random();
        let position = Vec2::new(
            (rain_left + (rain_right - rain_left) * fraction + horizontal * 34.0)
                .clamp(rain_left, rain_right),
            start_y + height * 58.0,
        );
        let surface = first_surface_below(position, level);
        spawn_drop(
            commands,
            materials,
            assets,
            level,
            DropSpawnSpec {
                position,
                velocity: Vec2::new(horizontal * 86.0, -40.0 - speed * 165.0),
                gravity: 380.0 + state.unit_random() * 190.0,
                radius: 1.7 + speed * 1.8,
                terminal_world_y: surface.unwrap_or(level.center().y - level.size().y * 0.5 - 14.0),
                splash_on_impact: surface.is_some(),
            },
        );
    }
}

fn spawn_drop(
    commands: &mut Commands,
    materials: &mut Assets<ColorMaterial>,
    assets: &AmbientDropAssets,
    level: &Level,
    spec: DropSpawnSpec,
) {
    const DEPTH: f32 = -4.8;
    let material = materials.add(ColorMaterial::from(palette::color(light_dynamic_rgba(
        palette::AMBIENT_DROP,
        spec.position,
        &level.lights,
    ))));
    commands.spawn((
        AmbientDrop {
            position: spec.position,
            velocity: spec.velocity,
            gravity: spec.gravity,
            radius: spec.radius,
            terminal_world_y: spec.terminal_world_y,
            splash_on_impact: spec.splash_on_impact,
            depth: DEPTH,
            parallax: 1.0,
        },
        Mesh2d(assets.mesh.clone()),
        MeshMaterial2d(material.clone()),
        AmbientLightTint { material },
        Transform {
            translation: spec.position.extend(DEPTH),
            scale: Vec3::new(spec.radius * 1.35, spec.radius, 1.0),
            ..default()
        },
    ));
}

pub(super) fn spawn_dry_surface_splash(
    commands: &mut Commands,
    assets: &AmbientDropAssets,
    impact: Vec2,
    source_radius: f32,
    depth: f32,
    parallax: f32,
    camera_position: Vec2,
) {
    const SPLASH_DEPTH: f32 = 0.11;
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
                position: impact + Vec2::Y * radius,
                velocity,
                gravity: 300.0,
                remaining: duration,
                duration,
                radius,
                depth,
                parallax,
            },
            Mesh2d(assets.mesh.clone()),
            MeshMaterial2d(assets.material.clone()),
            Transform {
                translation: (impact
                    + Vec2::Y * radius
                    + parallax_offset(camera_position, parallax))
                .extend(depth + SPLASH_DEPTH),
                scale: Vec3::new(radius, radius * 1.35, 1.0),
                ..default()
            },
        ));
    }
}
