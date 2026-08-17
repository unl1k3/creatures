mod acid;
mod blob;
mod camera;
mod environment;
mod hud;
mod input;
mod rendering;
mod shield;
mod vitality;

use acid::{AcidWorld, draw_acid, fire_acid, simulate_acid};
use avian2d::collision::collider::contact_query::contact_manifolds;
use avian2d::prelude::PhysicsPlugins;
use avian2d::prelude::{Collider, ContactManifold};
use bevy::{
    app::AppExit,
    diagnostic::FrameTimeDiagnosticsPlugin,
    prelude::*,
    window::{ExitCondition, WindowPosition, WindowResolution},
};
use blob::{Blob, DEFAULT_CREATURE_SCALE, Platform, REFERENCE_RADIUS};
use camera::follow_camera;
#[cfg(test)]
use camera::selected_camera_target;
use environment::{
    AvianContactDiagnostics, Level, resolve_avian_environment, sample_avian_contacts,
    setup_environment, switch_test_scenario,
};
use hud::{arrange_auxiliary_windows, setup_legend, toggle_legend, update_metrics};
#[cfg(test)]
use input::next_selection;
use input::{cycle_selection, exit_on_escape, handle_blob_actions};
#[cfg(test)]
use rendering::blob_family_color;
use rendering::{draw_world, sync_blob_meshes};
use shield::{ShieldWorld, draw_shields, simulate_shields};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use vitality::{DeathCause, LifeState, Vitality, VitalityWorld, simulate_vitality};

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
                title: "Blob — X divide, E ricongiunge, R reset, TAB seleziona".into(),
                resolution: WindowResolution::new(900, 900),
                position: WindowPosition::At(IVec2::new(20, 30)),
                ..default()
            }),
            exit_condition: ExitCondition::OnPrimaryClosed,
            ..default()
        }))
        .add_plugins(PhysicsPlugins::default().with_length_unit(100.0))
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_systems(Startup, (setup, setup_environment, setup_legend).chain())
        .add_systems(
            FixedUpdate,
            (
                simulate_shields,
                simulate_blob,
                resolve_avian_environment,
                simulate_vitality,
                simulate_acid,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                exit_on_escape,
                arrange_auxiliary_windows,
                toggle_legend,
                switch_test_scenario,
                handle_blob_actions,
                fire_acid,
                cycle_selection,
                follow_camera,
                sample_avian_contacts,
                update_metrics,
                sync_blob_meshes,
                draw_world,
                draw_acid,
                draw_shields,
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
    commands.insert_resource(AcidWorld::new(seed.rotate_left(29)));
    commands.insert_resource(ShieldWorld::default());
    commands.insert_resource(VitalityWorld::default());
}

fn simulate_blob(
    time: Res<Time<Fixed>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    level: Res<Level>,
    shields: Res<ShieldWorld>,
    mut vitality: ResMut<VitalityWorld>,
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
        let alive = vitality.is_alive(active_blob.id);
        if !alive {
            active_blob.body.cancel_jump_charge();
        }
        let vigor = vitality.vigor(active_blob.id);
        let shield_extension = shields.extension(active_blob.id);
        let movement = if alive {
            rejoin_directions
                .as_ref()
                .map(|directions| directions[index])
                .unwrap_or(if is_selected {
                    horizontal as f32 * (1.0 - shield_extension * 0.58)
                } else {
                    0.0
                })
        } else {
            0.0
        };
        active_blob.body.step_with_vigor(
            time.delta_secs(),
            movement,
            rejoin_directions.is_none()
                && is_selected
                && alive
                && shield_extension < 0.05
                && keyboard.pressed(KeyCode::ArrowDown),
            level.platforms.get(4..).unwrap_or(&[]),
            &level.fixtures,
            vigor,
            alive,
            true,
        );
    }
    if let Some((children, parent)) = update_rejoining(&mut blobs, &level.platforms) {
        vitality.merge(children, parent);
    }
    resolve_blob_collisions_with_vitality(&mut blobs.active, &vitality);
}

fn reset_world_at(blobs: &mut BlobWorld, position: Vec2) {
    blobs.active = vec![ActiveBlob {
        id: 0,
        parent_id: None,
        body: Blob::new(position, INITIAL_RADIUS),
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

fn update_rejoining(blobs: &mut BlobWorld, platforms: &[Platform]) -> Option<([u64; 2], u64)> {
    let Some((first_index, second_index, parent_id)) = rejoin_pair_indices(blobs) else {
        return None;
    };
    let first_center = blobs.active[first_index].body.center();
    let second_center = blobs.active[second_index].body.center();
    if !path_is_clear(first_center, second_center, platforms) {
        return None;
    }
    let pair_scale = (blobs.active[first_index].body.size_scale()
        + blobs.active[second_index].body.size_scale())
        * 0.5;
    let surface_gap = blob_surface_gap(
        &blobs.active[first_index].body,
        &blobs.active[second_index].body,
    );
    if surface_gap <= 2.0 * pair_scale {
        let child_ids = [blobs.active[first_index].id, blobs.active[second_index].id];
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
        return Some((child_ids, parent_id));
    }
    None
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

#[cfg(test)]
fn resolve_blob_collisions(blobs: &mut [ActiveBlob]) {
    resolve_blob_collisions_impl(blobs, |_| (true, true));
}

fn resolve_blob_collisions_with_vitality(blobs: &mut [ActiveBlob], _vitality: &VitalityWorld) {
    resolve_blob_collisions_impl(blobs, |_| (true, true));
}

fn resolve_blob_collisions_impl(
    blobs: &mut [ActiveBlob],
    interaction: impl Fn(u64) -> (bool, bool),
) {
    for first_index in 0..blobs.len() {
        let (before_second, from_second) = blobs.split_at_mut(first_index + 1);
        let first_active = &mut before_second[first_index];
        let (first_alive, first_collides) = interaction(first_active.id);
        let first = &mut first_active.body;
        for second in from_second {
            let (second_alive, second_collides) = interaction(second.id);
            if !first_collides || !second_collides {
                continue;
            }
            let second = &mut second.body;
            let pair_scale = (first.size_scale() + second.size_scale()) * 0.5;
            let clearance = 1.5 * pair_scale;
            let Some((normal, contact_points, penetration)) =
                avian_blob_contacts(first, second, clearance)
            else {
                continue;
            };

            let first_mass = first.mass();
            let second_mass = second.mass();
            let total_mass = first_mass + second_mass;
            let contact_load = (penetration + 3.0 * pair_scale)
                .min(first.rest_radius.min(second.rest_radius) * 0.18);
            let point_count = contact_points.len().max(1) as f32;
            let actual_overlap = (penetration - clearance).max(0.0);
            for point in contact_points {
                let first_load = if first_alive {
                    contact_load / point_count
                } else {
                    actual_overlap * 0.30 / point_count
                };
                let second_load = if second_alive {
                    contact_load / point_count
                } else {
                    actual_overlap * 0.30 / point_count
                };
                if first_load > 0.001 {
                    first.apply_contact_patch(point, normal, first_load, !first_alive);
                }
                if second_load > 0.001 {
                    second.apply_contact_patch(point, -normal, second_load, !second_alive);
                }
            }

            let post_penetration = avian_blob_contacts(first, second, clearance)
                .map(|(_, _, penetration)| penetration)
                .unwrap_or(0.0);
            match (first_alive, second_alive) {
                (true, false) => first.translate(-normal * post_penetration),
                (false, true) => second.translate(normal * post_penetration),
                _ => {
                    first.translate(-normal * post_penetration * second_mass / total_mass);
                    second.translate(normal * post_penetration * first_mass / total_mass);
                }
            }

            // Convex contact normals can rotate slightly after a soft patch is
            // deformed. Close the tiny residual along the new centre axis so
            // the visible contours never remain interpenetrating.
            let final_delta = second.center() - first.center();
            let final_normal = final_delta.normalize_or(normal);
            let residual = (clearance - blob_surface_gap(first, second)).max(0.0);
            match (first_alive, second_alive) {
                (true, false) => first.translate(-final_normal * residual),
                (false, true) => second.translate(final_normal * residual),
                _ => {
                    first.translate(-final_normal * residual * second_mass / total_mass);
                    second.translate(final_normal * residual * first_mass / total_mass);
                }
            }

            // Blob-to-blob contact can support jump charging just like level
            // geometry. Only the upper body is grounded; side contacts do not
            // arm a jump.
            if normal.y > 0.55 && second_alive {
                second.grounded = true;
                second.record_support_normal(normal);
            } else if normal.y < -0.55 && first_alive {
                first.grounded = true;
                first.record_support_normal(-normal);
            }

            let relative_normal_speed = (second.velocity() - first.velocity()).dot(normal);
            if relative_normal_speed < 0.0 {
                match (first_alive, second_alive) {
                    (true, true) => {
                        first.add_velocity(normal * relative_normal_speed * 0.5);
                        second.add_velocity(-normal * relative_normal_speed * 0.5);
                    }
                    (true, false) => {
                        first.add_velocity(normal * relative_normal_speed);
                        second.damp_velocity(0.03);
                    }
                    (false, true) => {
                        second.add_velocity(-normal * relative_normal_speed);
                        first.damp_velocity(0.03);
                    }
                    (false, false) => {
                        first.damp_velocity(0.03);
                        second.damp_velocity(0.03);
                    }
                }
            }
        }
    }
}

fn avian_blob_contacts(
    first: &Blob,
    second: &Blob,
    prediction_distance: f32,
) -> Option<(Vec2, Vec<Vec2>, f32)> {
    let first_center = first.center();
    let second_center = second.center();
    let first_collider = Collider::convex_hull(
        first
            .particles
            .iter()
            .map(|particle| particle.position - first_center)
            .collect(),
    )?;
    let second_collider = Collider::convex_hull(
        second
            .particles
            .iter()
            .map(|particle| particle.position - second_center)
            .collect(),
    )?;
    let mut manifolds = Vec::<ContactManifold>::new();
    contact_manifolds(
        &first_collider,
        first_center,
        0.0,
        &second_collider,
        second_center,
        0.0,
        prediction_distance,
        &mut manifolds,
    );
    let manifold = manifolds
        .iter()
        .filter(|manifold| !manifold.points.is_empty())
        .max_by(|first, second| {
            let first_depth = first
                .points
                .iter()
                .map(|point| point.penetration)
                .fold(f32::NEG_INFINITY, f32::max);
            let second_depth = second
                .points
                .iter()
                .map(|point| point.penetration)
                .fold(f32::NEG_INFINITY, f32::max);
            first_depth.total_cmp(&second_depth)
        })?;
    let points = manifold
        .points
        .iter()
        .map(|point| point.point)
        .collect::<Vec<_>>();
    let correction = manifold
        .points
        .iter()
        .map(|point| point.penetration + prediction_distance)
        .fold(0.0, f32::max)
        .max(0.0);
    Some((manifold.normal, points, correction))
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
