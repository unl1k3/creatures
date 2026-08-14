mod blob;

use avian2d::prelude::PhysicsPlugins;
use bevy::{app::AppExit, prelude::*, window::WindowResolution};
use blob::{Blob, Platform, REFERENCE_RADIUS};
use std::time::{SystemTime, UNIX_EPOCH};

const BLOB_START: Vec2 = Vec2::new(0.0, -280.0);
const CREATURE_SCALE: f32 = 0.65;
const INITIAL_RADIUS: f32 = REFERENCE_RADIUS * CREATURE_SCALE;

#[derive(Resource)]
struct BlobWorld {
    active: Vec<Blob>,
    selected: usize,
    rejoining: bool,
    /// Inactive snapshot retained for the future merge operation.
    split_parent: Option<Blob>,
}

#[derive(Resource)]
struct Level {
    platforms: Vec<Platform>,
}

#[derive(Resource)]
struct SplitRng(u64);

impl SplitRng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn split_choice(&mut self) -> (usize, bool) {
        let smaller_count = 9 + (self.next() % 3) as usize;
        let smaller_on_left = self.next() & 1 == 0;
        (smaller_count, smaller_on_left)
    }
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.025, 0.035, 0.075)))
        .insert_resource(Time::<Fixed>::from_hz(120.0))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Blob — X divide, R ricongiunge, TAB seleziona".into(),
                resolution: WindowResolution::new(900, 900),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default())
        .add_systems(Startup, setup)
        .add_systems(FixedUpdate, simulate_blob)
        .add_systems(
            Update,
            (exit_on_escape, cycle_selection, follow_camera, draw_world).chain(),
        )
        .run();
}

fn exit_on_escape(keyboard: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    if keyboard.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
}

fn cycle_selection(keyboard: Res<ButtonInput<KeyCode>>, mut blobs: ResMut<BlobWorld>) {
    if keyboard.just_pressed(KeyCode::Tab) && blobs.active.len() > 1 {
        blobs.selected = next_selection(blobs.selected, blobs.active.len());
    }
}

fn next_selection(current: usize, blob_count: usize) -> usize {
    if blob_count == 0 {
        0
    } else {
        (current + 1) % blob_count
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.insert_resource(BlobWorld {
        active: vec![Blob::new(BLOB_START, INITIAL_RADIUS)],
        selected: 0,
        rejoining: false,
        split_parent: None,
    });
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15)
        .max(1);
    commands.insert_resource(SplitRng(seed));
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
    mut blobs: ResMut<BlobWorld>,
    mut split_rng: ResMut<SplitRng>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        if blobs.active.len() == 2 {
            blobs.rejoining = true;
        } else {
            blobs.active = vec![Blob::new(BLOB_START, INITIAL_RADIUS)];
            blobs.selected = 0;
            blobs.rejoining = false;
            blobs.split_parent = None;
        }
        return;
    }

    if keyboard.just_pressed(KeyCode::KeyX) && blobs.active.len() == 1 {
        let parent = blobs.active.remove(0);
        let (smaller_count, smaller_on_left) = split_rng.split_choice();
        let children = parent.split_pair_uneven(time.delta_secs(), smaller_count, smaller_on_left);
        blobs.split_parent = Some(parent);
        blobs.active.extend(children);
        blobs.selected = 0;
        blobs.rejoining = false;
    }

    let horizontal = (keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight))
        as i8
        - (keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft)) as i8;
    let rejoin_directions = rejoin_roll_directions(&blobs, &level.platforms);
    let selected = blobs.selected;
    for (index, blob) in blobs.active.iter_mut().enumerate() {
        let is_selected = index == selected;
        let movement = rejoin_directions
            .map(|directions| directions[index])
            .unwrap_or(if is_selected { horizontal as f32 } else { 0.0 });
        blob.step(
            time.delta_secs(),
            movement,
            rejoin_directions.is_none() && is_selected && keyboard.pressed(KeyCode::ArrowDown),
            &level.platforms,
        );
    }
    update_rejoining(&mut blobs, &level.platforms);
    resolve_blob_collisions(&mut blobs.active);
}

fn rejoin_roll_directions(blobs: &BlobWorld, platforms: &[Platform]) -> Option<[f32; 2]> {
    if !blobs.rejoining || blobs.active.len() != 2 {
        return None;
    }
    let first_center = blobs.active[0].center();
    let second_center = blobs.active[1].center();
    if !path_is_clear(first_center, second_center, platforms) {
        return None;
    }
    let horizontal_delta = second_center.x - first_center.x;
    let direction = if horizontal_delta.abs() > 1.0 {
        horizontal_delta.signum()
    } else {
        0.0
    };
    Some([direction, -direction])
}

fn update_rejoining(blobs: &mut BlobWorld, platforms: &[Platform]) {
    if !blobs.rejoining || blobs.active.len() != 2 {
        return;
    }
    let first_center = blobs.active[0].center();
    let second_center = blobs.active[1].center();
    if !path_is_clear(first_center, second_center, platforms) {
        return;
    }
    let pair_scale = (blobs.active[0].size_scale() + blobs.active[1].size_scale()) * 0.5;
    let surface_gap = first_center.distance(second_center)
        - blobs.active[0].rest_radius
        - blobs.active[1].rest_radius;
    if surface_gap <= 2.0 * pair_scale {
        let merged = Blob::merge_pair(&blobs.active[0], &blobs.active[1]);
        blobs.active = vec![merged];
        blobs.selected = 0;
        blobs.rejoining = false;
        blobs.split_parent = None;
    }
}

fn path_is_clear(start: Vec2, end: Vec2, platforms: &[Platform]) -> bool {
    !platforms
        .iter()
        .any(|platform| segment_intersects_aabb(start, end, platform))
}

fn segment_intersects_aabb(start: Vec2, end: Vec2, platform: &Platform) -> bool {
    let minimum = platform.center - platform.half_size;
    let maximum = platform.center + platform.half_size;
    let direction = end - start;
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;

    for (origin, delta, min_axis, max_axis) in [
        (start.x, direction.x, minimum.x, maximum.x),
        (start.y, direction.y, minimum.y, maximum.y),
    ] {
        if delta.abs() < 0.0001 {
            if origin < min_axis || origin > max_axis {
                return false;
            }
            continue;
        }
        let first = (min_axis - origin) / delta;
        let second = (max_axis - origin) / delta;
        near = near.max(first.min(second));
        far = far.min(first.max(second));
        if near > far {
            return false;
        }
    }
    far >= 0.0 && near <= 1.0
}

fn resolve_blob_collisions(blobs: &mut [Blob]) {
    for first_index in 0..blobs.len() {
        let (before_second, from_second) = blobs.split_at_mut(first_index + 1);
        let first = &mut before_second[first_index];
        for second in from_second {
            let delta = second.center() - first.center();
            let distance = delta.length();
            let normal = if distance > 0.001 {
                delta / distance
            } else {
                Vec2::X
            };
            let pair_scale = (first.size_scale() + second.size_scale()) * 0.5;
            let minimum_distance = first.rest_radius + second.rest_radius + 1.5 * pair_scale;
            if distance >= minimum_distance {
                continue;
            }

            let correction = normal * (minimum_distance - distance) * 0.5;
            first.translate(-correction);
            second.translate(correction);

            let relative_normal_speed = (second.velocity() - first.velocity()).dot(normal);
            if relative_normal_speed < 0.0 {
                first.add_velocity(normal * relative_normal_speed * 0.5);
                second.add_velocity(-normal * relative_normal_speed * 0.5);
            }
        }
    }
}

fn draw_world(mut gizmos: Gizmos, blobs: Res<BlobWorld>, level: Res<Level>) {
    for platform in &level.platforms {
        gizmos.rect_2d(
            platform.center,
            platform.half_size * 2.0,
            Color::srgb(0.18, 0.27, 0.38),
        );
    }

    for (index, blob) in blobs.active.iter().enumerate() {
        let is_selected = index == blobs.selected;
        let color = if is_selected {
            Color::srgb(1.0, 0.86, 0.18)
        } else {
            Color::srgba(0.30, 0.72, 0.68, 0.62)
        };
        let outline = blob.particles.iter().map(|particle| particle.position);
        gizmos.lineloop_2d(outline, color);
        let center = blob.center();
        if is_selected {
            let outer_outline = blob
                .particles
                .iter()
                .map(|particle| center + (particle.position - center) * 1.045);
            gizmos.lineloop_2d(outer_outline, Color::srgba(1.0, 0.72, 0.08, 0.72));
        }
        let size_scale = blob.size_scale();
        for particle in &blob.particles {
            gizmos.line_2d(
                center,
                particle.position,
                Color::srgba(0.12, 0.55, 0.48, 0.22),
            );
        }
        gizmos.circle_2d(center, 9.0 * size_scale, Color::srgb(0.72, 0.42, 0.95));

        if blob.charge > 0.0 {
            gizmos.arc_2d(
                center,
                std::f32::consts::TAU * blob.charge,
                16.0 * size_scale,
                Color::srgb(1.0, 0.78, 0.24),
            );
        }
    }
}

fn follow_camera(
    time: Res<Time>,
    blobs: Res<BlobWorld>,
    mut camera: Single<&mut Transform, With<Camera2d>>,
) {
    let target_y = blobs
        .active
        .iter()
        .map(Blob::center)
        .map(|center| center.y)
        .fold(0.0, f32::max);
    camera.translation.y += (target_y - camera.translation.y) * (5.0 * time.delta_secs()).min(1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_blobs_are_separated_when_they_overlap() {
        let mut blobs = vec![Blob::new(Vec2::ZERO, 30.0), Blob::new(Vec2::ZERO, 30.0)];
        resolve_blob_collisions(&mut blobs);
        let distance = blobs[0].center().distance(blobs[1].center());
        let scaled_gap = 1.5 * blobs[0].size_scale();
        assert!(distance >= blobs[0].rest_radius + blobs[1].rest_radius + scaled_gap - 0.01);
    }

    #[test]
    fn tab_selection_wraps_between_two_blobs() {
        assert_eq!(next_selection(0, 2), 1);
        assert_eq!(next_selection(1, 2), 0);
    }

    #[test]
    fn platform_blocks_rejoining_line_of_sight() {
        let wall = Platform {
            center: Vec2::ZERO,
            half_size: Vec2::new(5.0, 80.0),
        };
        assert!(!path_is_clear(
            Vec2::new(-50.0, 0.0),
            Vec2::new(50.0, 0.0),
            &[wall]
        ));
        assert!(path_is_clear(
            Vec2::new(-50.0, 100.0),
            Vec2::new(50.0, 100.0),
            &[wall]
        ));
    }

    #[test]
    fn touching_children_merge_into_one_blob() {
        let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
        let [mut first, mut second] = parent.split_pair(1.0 / 120.0);
        let midpoint = (first.center() + second.center()) * 0.5;
        first.translate(midpoint - first.center() + Vec2::NEG_X * first.rest_radius);
        second.translate(midpoint - second.center() + Vec2::X * second.rest_radius);
        let mut world = BlobWorld {
            active: vec![first, second],
            selected: 0,
            rejoining: true,
            split_parent: Some(parent),
        };

        update_rejoining(&mut world, &[]);
        assert_eq!(world.active.len(), 1);
        assert!(!world.rejoining);
        assert!(world.split_parent.is_none());
    }

    #[test]
    fn separated_children_roll_before_they_can_merge() {
        let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
        let [first, second] = parent.split_pair(1.0 / 120.0);
        let mut world = BlobWorld {
            active: vec![first, second],
            selected: 0,
            rejoining: true,
            split_parent: Some(parent),
        };

        let directions = rejoin_roll_directions(&world, &[]).unwrap();
        assert_eq!(directions, [1.0, -1.0]);
        update_rejoining(&mut world, &[]);
        assert_eq!(world.active.len(), 2);
    }

    #[test]
    fn touching_children_do_not_merge_until_rejoining_is_enabled() {
        let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
        let [mut first, mut second] = parent.split_pair(1.0 / 120.0);
        let midpoint = (first.center() + second.center()) * 0.5;
        first.translate(midpoint - first.center() + Vec2::NEG_X * first.rest_radius);
        second.translate(midpoint - second.center() + Vec2::X * second.rest_radius);
        let mut world = BlobWorld {
            active: vec![first, second],
            selected: 0,
            rejoining: false,
            split_parent: Some(parent),
        };

        update_rejoining(&mut world, &[]);
        assert_eq!(world.active.len(), 2);
    }
}
