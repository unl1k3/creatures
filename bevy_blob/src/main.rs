mod blob;

use avian2d::prelude::PhysicsPlugins;
use bevy::{app::AppExit, prelude::*, window::WindowResolution};
use blob::{Blob, Platform};

const BLOB_START: Vec2 = Vec2::new(0.0, -280.0);

#[derive(Resource)]
struct Level {
    platforms: Vec<Platform>,
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.025, 0.035, 0.075)))
        .insert_resource(Time::<Fixed>::from_hz(120.0))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Blob verticale — A/D muovi, tieni GIÙ e rilascia per saltare".into(),
                resolution: WindowResolution::new(900, 900),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default())
        .add_systems(Startup, setup)
        .add_systems(FixedUpdate, simulate_blob)
        .add_systems(Update, (draw_world, follow_camera, exit_on_escape))
        .run();
}

fn exit_on_escape(keyboard: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keyboard.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.insert_resource(Blob::new(BLOB_START, 58.0));
    commands.insert_resource(Level {
        platforms: vec![
            platform(0.0, -370.0, 660.0, 38.0),
            platform(-220.0, -150.0, 300.0, 28.0),
            platform(210.0, 65.0, 300.0, 28.0),
            platform(-180.0, 290.0, 260.0, 28.0),
            platform(210.0, 510.0, 280.0, 28.0),
            platform(-170.0, 735.0, 300.0, 28.0),
        ],
    });
}

fn platform(x: f32, y: f32, width: f32, height: f32) -> Platform {
    Platform {
        center: Vec2::new(x, y),
        half_size: Vec2::new(width, height) * 0.5,
    }
}

fn simulate_blob(
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    level: Res<Level>,
    mut blob: ResMut<Blob>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        *blob = Blob::new(BLOB_START, 58.0);
        return;
    }

    let horizontal = (keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight))
        as i8
        - (keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft)) as i8;
    blob.step(
        time.delta_secs(),
        horizontal as f32,
        keyboard.pressed(KeyCode::ArrowDown),
        &level.platforms,
    );
}

fn draw_world(mut gizmos: Gizmos, blob: Res<Blob>, level: Res<Level>) {
    for platform in &level.platforms {
        gizmos.rect_2d(
            platform.center,
            platform.half_size * 2.0,
            Color::srgb(0.18, 0.27, 0.38),
        );
    }

    let outline = blob.particles.iter().map(|particle| particle.position);
    gizmos.lineloop_2d(outline, Color::srgb(0.30, 0.95, 0.72));
    let center = blob.center();
    for particle in &blob.particles {
        gizmos.line_2d(
            center,
            particle.position,
            Color::srgba(0.12, 0.55, 0.48, 0.22),
        );
    }
    gizmos.circle_2d(center, 13.0, Color::srgb(0.72, 0.42, 0.95));

    if blob.charge > 0.0 {
        gizmos.arc_2d(
            center,
            std::f32::consts::TAU * blob.charge,
            20.0,
            Color::srgb(1.0, 0.78, 0.24),
        );
    }
}

fn follow_camera(
    time: Res<Time>,
    blob: Res<Blob>,
    mut camera: Single<&mut Transform, With<Camera2d>>,
) {
    let target_y = blob.center().y.max(0.0);
    camera.translation.y += (target_y - camera.translation.y) * (5.0 * time.delta_secs()).min(1.0);
}
