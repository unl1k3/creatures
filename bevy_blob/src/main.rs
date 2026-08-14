mod blob;
mod camera;
mod input;
mod rendering;

use avian2d::prelude::PhysicsPlugins;
use bevy::{app::AppExit, prelude::*, window::WindowResolution};
use blob::{Blob, DEFAULT_CREATURE_SCALE, Platform, REFERENCE_RADIUS};
use camera::follow_camera;
#[cfg(test)]
use camera::selected_camera_target;
#[cfg(test)]
use input::next_selection;
use input::{cycle_selection, exit_on_escape, handle_blob_actions};
#[cfg(test)]
use rendering::blob_family_color;
use rendering::draw_world;
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};

const BLOB_START: Vec2 = Vec2::new(0.0, -280.0);
const INITIAL_RADIUS: f32 = REFERENCE_RADIUS * DEFAULT_CREATURE_SCALE;
const MAX_ACTIVE_BLOBS: usize = 4;
const REJOIN_TIMEOUT: f32 = 4.0;

struct ActiveBlob {
    id: u64,
    parent_id: Option<u64>,
    body: Blob,
}

#[derive(Resource)]
struct BlobWorld {
    active: Vec<ActiveBlob>,
    selected: usize,
    rejoin_parent: Option<u64>,
    rejoin_elapsed: f32,
    parent_links: HashMap<u64, Option<u64>>,
    next_id: u64,
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

    fn split_choice(&mut self, particle_count: usize) -> (usize, bool) {
        let ratio = 0.37 + (self.next() % 10) as f32 * 0.01;
        let smaller_count = ((particle_count as f32 * ratio).round() as usize)
            .clamp(6, particle_count.saturating_sub(6));
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
            (
                exit_on_escape,
                handle_blob_actions,
                cycle_selection,
                follow_camera,
                draw_world,
            )
                .chain(),
        )
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.insert_resource(BlobWorld {
        active: vec![ActiveBlob {
            id: 0,
            parent_id: None,
            body: Blob::new(BLOB_START, INITIAL_RADIUS),
        }],
        selected: 0,
        rejoin_parent: None,
        rejoin_elapsed: 0.0,
        parent_links: HashMap::new(),
        next_id: 1,
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
            platform(-250.0, -150.0, 260.0, 28.0),
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
) {
    advance_rejoin_timeout(&mut blobs, time.delta_secs());

    let horizontal = (keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight))
        as i8
        - (keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft)) as i8;
    let rejoin_directions = rejoin_roll_directions(&blobs, &level.platforms);
    let selected = blobs.selected;
    for (index, active_blob) in blobs.active.iter_mut().enumerate() {
        let is_selected = index == selected;
        let movement = rejoin_directions
            .as_ref()
            .map(|directions| directions[index])
            .unwrap_or(if is_selected { horizontal as f32 } else { 0.0 });
        active_blob.body.step(
            time.delta_secs(),
            movement,
            rejoin_directions.is_none() && is_selected && keyboard.pressed(KeyCode::ArrowDown),
            &level.platforms,
        );
    }
    update_rejoining(&mut blobs, &level.platforms);
    resolve_blob_collisions(&mut blobs.active);
}

fn reset_world(blobs: &mut BlobWorld) {
    blobs.active = vec![ActiveBlob {
        id: 0,
        parent_id: None,
        body: Blob::new(BLOB_START, INITIAL_RADIUS),
    }];
    blobs.selected = 0;
    blobs.rejoin_parent = None;
    blobs.rejoin_elapsed = 0.0;
    blobs.parent_links.clear();
    blobs.next_id = 1;
}

fn split_selected(blobs: &mut BlobWorld, rng: &mut SplitRng, dt: f32) {
    if blobs.active.is_empty() || blobs.active.len() >= MAX_ACTIVE_BLOBS {
        return;
    }
    let index = blobs.selected.min(blobs.active.len() - 1);
    if !blobs.active[index].body.can_split() {
        return;
    }
    let parent = blobs.active.remove(index);
    let (smaller_count, smaller_on_left) = rng.split_choice(parent.body.particles.len());
    let [first_body, second_body] =
        parent
            .body
            .split_pair_uneven(dt, smaller_count, smaller_on_left);
    blobs.parent_links.insert(parent.id, parent.parent_id);
    let first_id = blobs.next_id;
    let second_id = blobs.next_id + 1;
    blobs.next_id += 2;
    blobs.active.insert(
        index,
        ActiveBlob {
            id: first_id,
            parent_id: Some(parent.id),
            body: first_body,
        },
    );
    blobs.active.insert(
        index + 1,
        ActiveBlob {
            id: second_id,
            parent_id: Some(parent.id),
            body: second_body,
        },
    );
    blobs.selected = index;
    blobs.rejoin_elapsed = 0.0;
}

fn start_selected_rejoin(blobs: &mut BlobWorld) -> bool {
    let Some(selected) = blobs.active.get(blobs.selected) else {
        return false;
    };
    let Some(parent_id) = selected.parent_id else {
        return false;
    };
    if blobs
        .active
        .iter()
        .filter(|blob| blob.parent_id == Some(parent_id))
        .count()
        != 2
    {
        return false;
    }
    blobs.rejoin_parent = Some(parent_id);
    blobs.rejoin_elapsed = 0.0;
    true
}

fn advance_rejoin_timeout(blobs: &mut BlobWorld, dt: f32) {
    if blobs.rejoin_parent.is_none() {
        blobs.rejoin_elapsed = 0.0;
        return;
    }
    blobs.rejoin_elapsed += dt;
    if blobs.rejoin_elapsed >= REJOIN_TIMEOUT {
        blobs.rejoin_parent = None;
        blobs.rejoin_elapsed = 0.0;
    }
}

fn rejoin_pair_indices(blobs: &BlobWorld) -> Option<(usize, usize, u64)> {
    let parent_id = blobs.rejoin_parent?;
    let mut indices = blobs
        .active
        .iter()
        .enumerate()
        .filter_map(|(index, blob)| (blob.parent_id == Some(parent_id)).then_some(index));
    let first = indices.next()?;
    let second = indices.next()?;
    indices
        .next()
        .is_none()
        .then_some((first, second, parent_id))
}

fn rejoin_roll_directions(blobs: &BlobWorld, platforms: &[Platform]) -> Option<Vec<f32>> {
    let (first_index, second_index, _) = rejoin_pair_indices(blobs)?;
    let first_center = blobs.active[first_index].body.center();
    let second_center = blobs.active[second_index].body.center();
    if !path_is_clear(first_center, second_center, platforms) {
        return None;
    }
    let horizontal_delta = second_center.x - first_center.x;
    let direction = if horizontal_delta.abs() > 1.0 {
        horizontal_delta.signum()
    } else {
        0.0
    };
    let mut directions = vec![0.0; blobs.active.len()];
    directions[first_index] = direction;
    directions[second_index] = -direction;
    Some(directions)
}

fn update_rejoining(blobs: &mut BlobWorld, platforms: &[Platform]) {
    let Some((first_index, second_index, parent_id)) = rejoin_pair_indices(blobs) else {
        return;
    };
    let first_center = blobs.active[first_index].body.center();
    let second_center = blobs.active[second_index].body.center();
    if !path_is_clear(first_center, second_center, platforms) {
        return;
    }
    let pair_scale = (blobs.active[first_index].body.size_scale()
        + blobs.active[second_index].body.size_scale())
        * 0.5;
    let surface_gap = blob_surface_gap(
        &blobs.active[first_index].body,
        &blobs.active[second_index].body,
    );
    if surface_gap <= 2.0 * pair_scale {
        let merged = Blob::merge_pair(
            &blobs.active[first_index].body,
            &blobs.active[second_index].body,
        );
        let grandparent = blobs.parent_links.remove(&parent_id).flatten();
        let insert_index = first_index.min(second_index);
        blobs.active.remove(first_index.max(second_index));
        blobs.active.remove(insert_index);
        blobs.active.insert(
            insert_index,
            ActiveBlob {
                id: parent_id,
                parent_id: grandparent,
                body: merged,
            },
        );
        blobs.selected = insert_index;
        blobs.rejoin_parent = None;
        blobs.rejoin_elapsed = 0.0;
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

fn resolve_blob_collisions(blobs: &mut [ActiveBlob]) {
    for first_index in 0..blobs.len() {
        let (before_second, from_second) = blobs.split_at_mut(first_index + 1);
        let first = &mut before_second[first_index].body;
        for second in from_second {
            let second = &mut second.body;
            let delta = second.center() - first.center();
            let distance = delta.length();
            let normal = if distance > 0.001 {
                delta / distance
            } else {
                Vec2::X
            };
            let pair_scale = (first.size_scale() + second.size_scale()) * 0.5;
            let first_extent = support_extent(first, normal);
            let second_extent = support_extent(second, -normal);
            let required_distance = first_extent + second_extent + 1.5 * pair_scale;
            if distance >= required_distance {
                continue;
            }

            // Weight separation inversely by physical mass: the smaller blob
            // yields more, regardless of its rendering/solver resolution.
            let first_mass = first.mass();
            let second_mass = second.mass();
            let total_mass = first_mass + second_mass;
            let penetration = required_distance - distance;
            first.translate(-normal * penetration * second_mass / total_mass);
            second.translate(normal * penetration * first_mass / total_mass);

            let relative_normal_speed = (second.velocity() - first.velocity()).dot(normal);
            if relative_normal_speed < 0.0 {
                first.add_velocity(normal * relative_normal_speed * 0.5);
                second.add_velocity(-normal * relative_normal_speed * 0.5);
            }
        }
    }
}

fn support_extent(blob: &Blob, direction: Vec2) -> f32 {
    let center = blob.center();
    blob.particles
        .iter()
        .map(|particle| (particle.position - center).dot(direction))
        .fold(0.0, f32::max)
}

fn blob_surface_gap(first: &Blob, second: &Blob) -> f32 {
    let delta = second.center() - first.center();
    let distance = delta.length();
    let normal = if distance > 0.001 {
        delta / distance
    } else {
        Vec2::X
    };
    distance - support_extent(first, normal) - support_extent(second, -normal)
}

include!("game_tests.rs");
