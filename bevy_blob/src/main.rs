mod blob;

use avian2d::prelude::PhysicsPlugins;
use bevy::{app::AppExit, prelude::*, window::WindowResolution};
use blob::{Blob, DEFAULT_CREATURE_SCALE, Platform, REFERENCE_RADIUS};
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

fn handle_blob_actions(
    keyboard: Res<ButtonInput<KeyCode>>,
    fixed_time: Res<Time<Fixed>>,
    mut blobs: ResMut<BlobWorld>,
    mut split_rng: ResMut<SplitRng>,
) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        if !start_selected_rejoin(&mut blobs) && blobs.active.len() == 1 {
            reset_world(&mut blobs);
        }
        return;
    }

    if keyboard.just_pressed(KeyCode::KeyX)
        && blobs.active.len() < MAX_ACTIVE_BLOBS
        && blobs.rejoin_parent.is_none()
        && blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.body.can_split())
    {
        split_selected(&mut blobs, &mut split_rng, fixed_time.delta_secs());
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

fn blob_family_color(parent_id: Option<u64>) -> Color {
    const FAMILY_COLORS: [(f32, f32, f32); 6] = [
        (0.30, 0.82, 0.72),
        (0.42, 0.68, 1.00),
        (0.88, 0.48, 0.82),
        (1.00, 0.58, 0.34),
        (0.62, 0.82, 0.34),
        (0.65, 0.52, 0.96),
    ];
    let family_index = parent_id
        .map(|id| (id as usize).wrapping_mul(5).wrapping_add(1) % FAMILY_COLORS.len())
        .unwrap_or(0);
    let (red, green, blue) = FAMILY_COLORS[family_index];
    Color::srgba(red, green, blue, 0.88)
}

fn draw_world(mut gizmos: Gizmos, blobs: Res<BlobWorld>, level: Res<Level>) {
    for platform in &level.platforms {
        gizmos.rect_2d(
            platform.center,
            platform.half_size * 2.0,
            Color::srgb(0.18, 0.27, 0.38),
        );
    }

    for (index, active_blob) in blobs.active.iter().enumerate() {
        let blob = &active_blob.body;
        let is_selected = index == blobs.selected;
        let color = blob_family_color(active_blob.parent_id);
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
    let Some(target) = selected_camera_target(&blobs) else {
        return;
    };
    let response = (5.0 * time.delta_secs()).min(1.0);
    camera.translation.x += (target.x - camera.translation.x) * response;
    camera.translation.y += (target.y - camera.translation.y) * response;
}

fn selected_camera_target(blobs: &BlobWorld) -> Option<Vec2> {
    blobs
        .active
        .get(blobs.selected)
        .map(|blob| blob.body.center())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(id: u64, parent_id: Option<u64>, body: Blob) -> ActiveBlob {
        ActiveBlob {
            id,
            parent_id,
            body,
        }
    }

    fn sibling_world(first: Blob, second: Blob, rejoining: bool) -> BlobWorld {
        BlobWorld {
            active: vec![active(1, Some(0), first), active(2, Some(0), second)],
            selected: 0,
            rejoin_parent: rejoining.then_some(0),
            rejoin_elapsed: 0.0,
            parent_links: HashMap::from([(0, None)]),
            next_id: 3,
        }
    }

    #[test]
    fn active_blobs_are_separated_when_they_overlap() {
        let mut blobs = vec![
            active(0, None, Blob::new(Vec2::ZERO, 30.0)),
            active(1, None, Blob::new(Vec2::ZERO, 30.0)),
        ];
        resolve_blob_collisions(&mut blobs);
        let distance = blobs[0].body.center().distance(blobs[1].body.center());
        let scaled_gap = 1.5 * blobs[0].body.size_scale();
        assert!(
            distance >= blobs[0].body.rest_radius + blobs[1].body.rest_radius + scaled_gap - 0.01
        );
    }

    #[test]
    fn collision_uses_deformed_outline_instead_of_rest_radius() {
        let mut first = Blob::new(Vec2::new(-34.0, 0.0), 30.0);
        let mut second = Blob::new(Vec2::new(34.0, 0.0), 30.0);
        // Push the facing membrane points beyond their nominal radii while the
        // two rest circles remain separated.
        first.particles[0].position.x += 12.0;
        first.particles[0].previous.x += 12.0;
        let leftmost = second.particles.len() / 2;
        second.particles[leftmost].position.x -= 12.0;
        second.particles[leftmost].previous.x -= 12.0;
        let center_of_mass_before = (first.center() * first.mass()
            + second.center() * second.mass())
            / (first.mass() + second.mass());
        let mut blobs = vec![active(0, None, first), active(1, None, second)];

        assert!(blob_surface_gap(&blobs[0].body, &blobs[1].body) < 0.0);
        resolve_blob_collisions(&mut blobs);

        let expected_gap = 1.5 * (blobs[0].body.size_scale() + blobs[1].body.size_scale()) * 0.5;
        assert!(blob_surface_gap(&blobs[0].body, &blobs[1].body) >= expected_gap - 0.01);
        let center_of_mass_after = (blobs[0].body.center() * blobs[0].body.mass()
            + blobs[1].body.center() * blobs[1].body.mass())
            / (blobs[0].body.mass() + blobs[1].body.mass());
        assert!(center_of_mass_after.distance(center_of_mass_before) < 0.0001);
    }

    #[test]
    fn tab_selection_wraps_between_two_blobs() {
        assert_eq!(next_selection(0, 2), 1);
        assert_eq!(next_selection(1, 2), 0);
    }

    #[test]
    fn camera_target_is_the_selected_blob() {
        let world = BlobWorld {
            active: vec![
                active(1, Some(0), Blob::new(Vec2::new(-80.0, 20.0), 20.0)),
                active(2, Some(0), Blob::new(Vec2::new(90.0, 160.0), 20.0)),
            ],
            selected: 1,
            rejoin_parent: None,
            rejoin_elapsed: 0.0,
            parent_links: HashMap::from([(0, None)]),
            next_id: 3,
        };
        assert!(
            selected_camera_target(&world)
                .unwrap()
                .distance(Vec2::new(90.0, 160.0))
                < 0.0001
        );
    }

    #[test]
    fn siblings_share_a_color_and_other_families_do_not() {
        assert_eq!(blob_family_color(Some(4)), blob_family_color(Some(4)));
        assert_ne!(blob_family_color(Some(4)), blob_family_color(Some(5)));
        assert_ne!(blob_family_color(None), blob_family_color(Some(4)));
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
        let mut world = sibling_world(first, second, true);

        update_rejoining(&mut world, &[]);
        assert_eq!(world.active.len(), 1);
        assert!(world.rejoin_parent.is_none());
        assert_eq!(world.active[0].id, 0);
    }

    #[test]
    fn separated_children_roll_before_they_can_merge() {
        let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
        let [first, second] = parent.split_pair(1.0 / 120.0);
        let mut world = sibling_world(first, second, true);

        let directions = rejoin_roll_directions(&world, &[]).unwrap();
        assert_eq!(directions, vec![1.0, -1.0]);
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
        let mut world = sibling_world(first, second, false);

        update_rejoining(&mut world, &[]);
        assert_eq!(world.active.len(), 2);
    }

    #[test]
    fn unsuccessful_rejoin_stops_after_timeout() {
        let parent = Blob::new(Vec2::ZERO, INITIAL_RADIUS);
        let [first, second] = parent.split_pair(1.0 / 120.0);
        let mut world = sibling_world(first, second, true);

        advance_rejoin_timeout(&mut world, REJOIN_TIMEOUT - 0.1);
        assert_eq!(world.rejoin_parent, Some(0));
        advance_rejoin_timeout(&mut world, 0.11);
        assert!(world.rejoin_parent.is_none());
        assert_eq!(world.rejoin_elapsed, 0.0);
        assert!(rejoin_roll_directions(&world, &[]).is_none());
    }

    #[test]
    fn selected_blob_can_split_again_and_merge_up_the_lineage() {
        let mut world = BlobWorld {
            active: vec![active(0, None, Blob::new(Vec2::ZERO, INITIAL_RADIUS))],
            selected: 0,
            rejoin_parent: None,
            rejoin_elapsed: 0.0,
            parent_links: HashMap::new(),
            next_id: 1,
        };
        let mut rng = SplitRng(0x1234_5678);
        let dt = 1.0 / 120.0;

        split_selected(&mut world, &mut rng, dt);
        assert_eq!(world.active.len(), 2);
        let root_sibling_id = world.active[1].id;

        // The selected first child is divided again, producing three leaves.
        split_selected(&mut world, &mut rng, dt);
        assert_eq!(world.active.len(), 3);
        let inner_parent = world.active[0].parent_id.unwrap();
        assert_eq!(world.active[1].parent_id, Some(inner_parent));
        assert_eq!(world.active[2].id, root_sibling_id);

        // Merge the deepest siblings first.
        assert!(start_selected_rejoin(&mut world));
        touch_rejoin_pair(&mut world);
        update_rejoining(&mut world, &[]);
        assert_eq!(world.active.len(), 2);
        assert_eq!(world.active[0].id, inner_parent);

        // The reconstructed parent can now merge with its own sibling.
        world.selected = 0;
        assert!(start_selected_rejoin(&mut world));
        touch_rejoin_pair(&mut world);
        update_rejoining(&mut world, &[]);
        assert_eq!(world.active.len(), 1);
        assert_eq!(world.active[0].id, 0);
        assert_eq!(world.active[0].parent_id, None);
    }

    fn touch_rejoin_pair(world: &mut BlobWorld) {
        let (first_index, second_index, _) = rejoin_pair_indices(world).unwrap();
        let midpoint = (world.active[first_index].body.center()
            + world.active[second_index].body.center())
            * 0.5;
        let first_radius = world.active[first_index].body.rest_radius;
        let second_radius = world.active[second_index].body.rest_radius;
        let first_offset =
            midpoint - world.active[first_index].body.center() + Vec2::NEG_X * first_radius;
        let second_offset =
            midpoint - world.active[second_index].body.center() + Vec2::X * second_radius;
        world.active[first_index].body.translate(first_offset);
        world.active[second_index].body.translate(second_offset);
    }
}
