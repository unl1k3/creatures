mod acid;
mod blob;
mod camera;
mod environment;
mod hud;
mod input;
mod level_format;
mod nutrition;
mod palette;
mod rendering;
mod shield;
mod vitality;

use acid::{AcidWorld, draw_acid, fire_acid, simulate_acid};
use avian2d::collision::collider::contact_query::contact_manifolds;
use avian2d::prelude::LinearVelocity;
use avian2d::prelude::PhysicsPlugins;
use avian2d::prelude::{Collider, ContactManifold, Gravity};
use bevy::{
    app::AppExit,
    audio::Volume,
    diagnostic::FrameTimeDiagnosticsPlugin,
    prelude::*,
    window::{ExitCondition, WindowPosition, WindowResolution},
};
use blob::{Blob, DEFAULT_CREATURE_SCALE, Platform, REFERENCE_RADIUS};
#[cfg(test)]
use camera::selected_camera_target;
use camera::{GameCamera, follow_camera};
use environment::{
    AvianContactDiagnostics, Level, LevelDebugOverlay, RouteProgress, TestScenario,
    WastewaterEffects, advance_route_progress, draw_level_chains, resolve_avian_environment,
    resolve_blob_chain_contacts, sample_avian_contacts, setup_environment,
    simulate_counterbalances, simulate_level_hazards, switch_test_scenario, toggle_level_debug,
    update_parallax_layers,
};
use hud::{arrange_auxiliary_windows, setup_legend, toggle_legend, update_metrics};
#[cfg(test)]
use input::next_selection;
use input::{cycle_selection, exit_on_escape, handle_blob_actions, toggle_pause};
use nutrition::{
    NutrientPhysics, NutritionWorld, circle_blob_penetration, draw_nutrition, setup_nutrition,
    simulate_nutrition, spawn_nutrient_bodies, start_phagocytosis,
};
#[cfg(test)]
use rendering::blob_family_color;
use rendering::{
    InkStylePreview, draw_world, setup_ambient_drop_assets, simulate_ambient_drops,
    simulate_wastewater, simulate_wastewater_bubbles, simulate_wastewater_impacts,
    sync_blob_meshes, sync_counterbalance_visuals, sync_ink_preview, sync_route_markers,
    toggle_foreground, toggle_ink_style, trigger_drop_shower,
};
use shield::{ShieldWorld, simulate_shields, spider_climb_anchor_direction};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use vitality::{
    DeathCause, LifeState, Vitality, VitalityWorld, WASTEWATER_DAMAGE_PER_SECOND, simulate_vitality,
};

const BLOB_START: Vec2 = Vec2::new(0.0, -280.0);
const INITIAL_RADIUS: f32 = REFERENCE_RADIUS * DEFAULT_CREATURE_SCALE;
const MAX_ACTIVE_BLOBS: usize = 4;
const REJOIN_TIMEOUT: f32 = 4.0;
const BLOB_CONTACT_PREDICTION_CLEARANCE: f32 = 1.5;
const BLOB_CONTACT_VISUAL_CLEARANCE: f32 = 0.0;
const BLOB_CONTACT_MAX_CORRECTION: f32 = 4.0;
const BLOB_CONTACT_MAX_TRANSFER_SPEED: f32 = 4.0;

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
        .insert_resource(ClearColor(palette::color(palette::IVORY)))
        .insert_resource(Time::<Fixed>::from_hz(120.0))
        .init_resource::<InkStylePreview>()
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
        .insert_resource(Gravity(Vec2::new(0.0, -1_150.0)))
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_systems(
            Startup,
            (
                setup_environment,
                setup,
                setup_ambient_music,
                setup_nutrition,
                setup_ambient_drop_assets,
                setup_legend,
            )
                .chain(),
        )
        .add_systems(
            FixedUpdate,
            (
                simulate_shields,
                simulate_counterbalances,
                simulate_blob,
                resolve_blob_chain_contacts,
                resolve_avian_environment,
                enforce_blob_safety_bounds,
                simulate_level_hazards,
                simulate_vitality,
                simulate_nutrition,
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
                toggle_level_debug,
                toggle_ink_style,
                switch_test_scenario,
                handle_blob_actions,
                start_phagocytosis,
                fire_acid,
                cycle_selection,
                (follow_camera, update_parallax_layers).chain(),
                advance_route_progress,
                sample_avian_contacts,
                update_metrics,
                (
                    trigger_drop_shower,
                    simulate_ambient_drops,
                    simulate_wastewater_impacts,
                    simulate_wastewater,
                    simulate_wastewater_bubbles,
                    sync_blob_meshes,
                )
                    .chain(),
                sync_ink_preview,
                sync_route_markers,
                draw_world,
                draw_acid,
                draw_nutrition,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                toggle_pause,
                toggle_foreground,
                draw_level_chains,
                // Ink platforms may be rebuilt when the scenario changes;
                // update movable visual layers only after that rebuild.
                sync_counterbalance_visuals.after(sync_ink_preview),
            ),
        )
        .run();
}

/// Starts the authored sewer ambience once for the whole application.
fn setup_ambient_music(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        AudioPlayer::new(asset_server.load("audio/music/underworld-echoes.mp3")),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.22)),
    ));
}

fn setup(mut commands: Commands, level: Res<Level>) {
    commands.spawn((Camera2d, GameCamera));
    commands.insert_resource(BlobWorld {
        active: vec![ActiveBlob {
            id: 0,
            parent_id: None,
            body: Blob::new(level.spawn_position, INITIAL_RADIUS),
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
    nutrition: Res<NutritionWorld>,
    mut vitality: ResMut<VitalityWorld>,
    mut blobs: ResMut<BlobWorld>,
    mut wastewater_effects: ResMut<WastewaterEffects>,
    mut nutrient_bodies: Query<(&NutrientPhysics, &mut Transform, &mut LinearVelocity)>,
) {
    advance_rejoin_timeout(&mut blobs, time.delta_secs());

    let horizontal = (keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight))
        as i8
        - (keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft)) as i8;
    // A returning counterweight plate may pass through a blob that has just
    // jumped away from it. It is visually moving, but must not become a
    // second moving collision surface underneath the airborne soft body.
    let airborne_counterweight_plates: Vec<usize> = level
        .counterbalances
        .iter()
        .filter_map(|balance| {
            let plate = level.platforms[balance.plate_platform];
            let has_rider = blobs.active.iter().any(|blob| {
                let center = blob.body.center();
                let radius = blob.body.rest_radius;
                (center.x - plate.center.x).abs() <= plate.half_size.x + radius * 0.3
                    && center.y - radius <= plate.center.y + plate.half_size.y + 5.0
                    && center.y >= plate.center.y
            });
            let has_airborne_blob_above = blobs.active.iter().any(|blob| {
                let center = blob.body.center();
                (center.x - plate.center.x).abs() <= plate.half_size.x + blob.body.rest_radius * 0.3
                    && center.y > plate.center.y
            });
            (!has_rider && has_airborne_blob_above).then_some(balance.plate_platform)
        })
        .collect();
    let collision_platforms: Vec<Platform> = level
        .platforms
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, platform)| {
            (!airborne_counterweight_plates.contains(&index)).then_some(platform)
        })
        .collect();
    let rejoin_directions = rejoin_roll_directions(&blobs, &collision_platforms);
    let selected = blobs.selected;
    for (index, active_blob) in blobs.active.iter_mut().enumerate() {
        let is_selected = index == selected;
        let alive = vitality.is_alive(active_blob.id);
        let protrusion_active = nutrition.has_external_protrusion(active_blob.id);
        if !alive || protrusion_active {
            active_blob.body.cancel_jump_charge();
        }
        let vigor = vitality.vigor(active_blob.id) * nutrition.capability_factor(active_blob.id);
        if let Some((position, radius, strength)) = nutrition.physical_load(active_blob.id) {
            active_blob
                .body
                .apply_internal_bulge(position, radius, strength);
        }
        let shield_extension = shields.extension(active_blob.id);
        let movement = if alive {
            rejoin_directions
                .as_ref()
                .map(|directions| directions[index])
                .unwrap_or(if is_selected {
                    horizontal as f32
                        * (1.0 - shield_extension * 0.58)
                        * if nutrition.has_external_protrusion(active_blob.id) {
                            0.0
                        } else {
                            1.0
                        }
                } else {
                    0.0
                })
        } else {
            0.0
        };
        let spider_anchor = spider_climb_anchor_direction(
            active_blob.id,
            &active_blob.body,
            shield_extension,
            &collision_platforms,
            &level.fixtures,
        );
        active_blob
            .body
            .set_spider_cling(spider_anchor.map(|anchor| (anchor.direction, anchor.wall_top)));
        active_blob.body.step_with_vigor(
            time.delta_secs(),
            movement,
            rejoin_directions.is_none()
                && is_selected
                && alive
                && !protrusion_active
                && shield_extension < 0.05
                && keyboard.pressed(KeyCode::ArrowDown),
            &collision_platforms,
            &level.fixtures,
            vigor,
            alive,
            true,
        );
        for (nutrient, mut transform, mut velocity) in &mut nutrient_bodies {
            if !nutrition.is_free_index(nutrient.index) {
                continue;
            }
            let center = transform.translation.truncate();
            let Some(radius) = nutrition.collision_radius(nutrient.index) else {
                continue;
            };
            let Some((depth, normal)) = circle_blob_penetration(center, radius, &active_blob.body)
            else {
                continue;
            };
            // The nutrient is owned by Avian: release it from the membrane by
            // moving its physics body, then give the soft body a small equal
            // reaction without translating the whole blob rigidly.
            transform.translation += (normal * (depth + 0.15)).extend(0.0);
            let inward = velocity.0.dot(normal);
            if inward < 0.0 {
                velocity.0 -= normal * inward;
            }
            active_blob.body.add_velocity(-normal * depth * 0.10);
        }
        let water_contact =
            level
                .wastewater_areas
                .iter()
                .copied()
                .enumerate()
                .find_map(|(area_index, area)| {
                    let center = active_blob.body.center();
                    area.contains_x(center.x).then(|| {
                        let surface_y = area.surface_y(center.x, time.elapsed_secs());
                        let bottom_y = area.position.y - area.size.y * 0.5;
                        active_blob
                            .body
                            .apply_wastewater_forces_with_spine_drag(
                                surface_y,
                                bottom_y,
                                time.delta_secs(),
                                shield_extension,
                                movement,
                            )
                            .map(|contact| (area_index, area, contact))
                    })?
                });
        if let Some((area_index, area, contact)) = water_contact {
            let immune = area.immune_family.is_some_and(|family| {
                crate::palette::blob_family_index(active_blob.parent_id) == family
            });
            if alive && !immune {
                vitality.damage(
                    active_blob.id,
                    WASTEWATER_DAMAGE_PER_SECOND * contact.submerged_fraction * time.delta_secs(),
                );
            }
            if contact.entered {
                let impact_strength = (contact.entry_speed / 430.0).clamp(0.45, 1.45);
                wastewater_effects.emit(
                    area_index,
                    Vec2::new(active_blob.body.center().x, contact.surface_y),
                    active_blob.body.rest_radius * (0.46 + contact.submerged_fraction * 0.42),
                    impact_strength,
                );
            }
        }
    }
    if let Some((children, parent)) =
        update_rejoining(&mut blobs, &level.platforms, &level.fixtures)
    {
        vitality.merge(children, parent);
    }
    resolve_blob_collisions_with_vitality(&mut blobs.active, &vitality);
}

fn enforce_blob_safety_bounds(level: Res<Level>, mut blobs: ResMut<BlobWorld>) {
    let Some(bounds) = level.safety_bounds else {
        return;
    };
    for active_blob in &mut blobs.active {
        if active_blob
            .body
            .contain_within_safety_bounds(bounds.min, bounds.max)
        {
            active_blob.body.cancel_jump_charge();
            active_blob.body.stabilize_after_external_projection();
        }
    }
}

/// Returns true only for an actual containment, not for the shallow overlap
/// that can occur while a soft membrane is resting on its contact skin.
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

#[cfg(test)]
fn split_selected(blobs: &mut BlobWorld, rng: &mut SplitRng, dt: f32) {
    let _ = split_selected_in_level(blobs, rng, dt, &[], &[]);
}

fn split_selected_in_level(
    blobs: &mut BlobWorld,
    rng: &mut SplitRng,
    dt: f32,
    platforms: &[Platform],
    fixtures: &[Vec<Vec2>],
) -> bool {
    if blobs.active.is_empty() || blobs.active.len() >= MAX_ACTIVE_BLOBS {
        return false;
    }
    let index = blobs.selected.min(blobs.active.len() - 1);
    if !blobs.active[index].body.can_split() {
        return false;
    }
    let parent_body = &blobs.active[index].body;
    let (smaller_count, smaller_on_left) = rng.split_choice(parent_body.particles.len());
    let [mut first_body, mut second_body] =
        parent_body.split_pair_uneven(dt, smaller_count, smaller_on_left);
    // Never replace a valid parent with children already embedded in level
    // geometry. This is most visible next to the thin wall of scenario 8.
    if !place_blob_clear(&mut first_body, platforms, fixtures)
        || !place_blob_clear(&mut second_body, platforms, fixtures)
    {
        return false;
    }

    let parent = blobs.active.remove(index);
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
    true
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

fn update_rejoining(
    blobs: &mut BlobWorld,
    platforms: &[Platform],
    fixtures: &[Vec<Vec2>],
) -> Option<([u64; 2], u64)> {
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
        let mut merged = Blob::merge_pair(
            &blobs.active[first_index].body,
            &blobs.active[second_index].body,
        );
        if !place_blob_clear(&mut merged, platforms, fixtures) {
            return None;
        }
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

fn place_blob_clear(blob: &mut Blob, platforms: &[Platform], fixtures: &[Vec<Vec2>]) -> bool {
    let initial_center = blob.center();
    let clearance_radius = blob.rest_radius + 3.0 * blob.size_scale();
    for _ in 0..16 {
        let center = blob.center();
        let correction = platforms
            .iter()
            .find_map(|platform| merge_circle_aabb_penetration(center, clearance_radius, platform))
            .or_else(|| {
                fixtures.iter().find_map(|vertices| {
                    merge_circle_convex_penetration(center, clearance_radius, vertices)
                })
            });
        let Some((depth, normal)) = correction else {
            return blob.center().distance(initial_center) <= blob.rest_radius * 1.1;
        };
        blob.translate(normal * (depth + 0.5));
    }
    false
}

fn merge_circle_aabb_penetration(
    center: Vec2,
    radius: f32,
    platform: &Platform,
) -> Option<(f32, Vec2)> {
    let local = center - platform.center;
    let closest = local.clamp(-platform.half_size, platform.half_size);
    let delta = local - closest;
    let distance = delta.length();
    if distance > 0.001 {
        return (distance < radius).then(|| (radius - distance, delta / distance));
    }
    let x_clearance = platform.half_size.x - local.x.abs();
    let y_clearance = platform.half_size.y - local.y.abs();
    if x_clearance < y_clearance {
        let side = if local.x >= 0.0 { 1.0 } else { -1.0 };
        Some((radius + x_clearance, Vec2::new(side, 0.0)))
    } else {
        let side = if local.y >= 0.0 { 1.0 } else { -1.0 };
        Some((radius + y_clearance, Vec2::new(0.0, side)))
    }
}

fn merge_circle_convex_penetration(
    center: Vec2,
    radius: f32,
    vertices: &[Vec2],
) -> Option<(f32, Vec2)> {
    if vertices.len() < 3 {
        return None;
    }
    let orientation = vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .map(|(first, second)| first.perp_dot(*second))
        .sum::<f32>()
        .signum();
    if orientation == 0.0 {
        return None;
    }
    let mut inside = true;
    let mut nearest = (f32::INFINITY, Vec2::Y, Vec2::Y);
    for (first, second) in vertices.iter().zip(vertices.iter().cycle().skip(1)) {
        let edge = *second - *first;
        inside &= edge.perp_dot(center - *first) * orientation >= 0.0;
        let t = ((center - *first).dot(edge) / edge.length_squared().max(0.001)).clamp(0.0, 1.0);
        let delta = center - (*first + edge * t);
        if delta.length() < nearest.0 {
            let outward = -edge.perp() * orientation / edge.length().max(0.001);
            nearest = (delta.length(), outward, delta.normalize_or(outward));
        }
    }
    if inside {
        Some((radius + nearest.0, nearest.1))
    } else if nearest.0 < radius {
        Some((radius - nearest.0, nearest.2))
    } else {
        None
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
    let crowded = blobs.len() > 2;
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
            // Keep a generous predictive skin for stable continuous contact,
            // but do not expose that entire skin as a visible gap between the
            // rendered membranes.
            let prediction_clearance = BLOB_CONTACT_PREDICTION_CLEARANCE * pair_scale;
            let visual_clearance = BLOB_CONTACT_VISUAL_CLEARANCE * pair_scale;
            let Some((normal, contact_points, penetration)) =
                avian_blob_contacts(first, second, prediction_clearance)
            else {
                continue;
            };

            // A predictive manifold only says that contact is imminent. Do
            // not deform the membranes or cancel their closing velocity until
            // their visible contours have actually reached one another.
            if blob_surface_gap(first, second) > visual_clearance {
                continue;
            }

            let first_mass = first.mass();
            let second_mass = second.mass();
            let total_mass = first_mass + second_mass;
            let contact_load = if crowded {
                (penetration + 1.5 * pair_scale)
                    .min(first.rest_radius.min(second.rest_radius) * 0.12)
            } else {
                (penetration + 3.0 * pair_scale)
                    .min(first.rest_radius.min(second.rest_radius) * 0.18)
            };
            let point_count = contact_points.len().max(1) as f32;
            let actual_overlap = (penetration - prediction_clearance).max(0.0);
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

            let predicted_post_correction =
                avian_blob_contacts(first, second, prediction_clearance)
                    .map(|(_, _, penetration)| penetration)
                    .unwrap_or(0.0);
            let mut post_penetration =
                (predicted_post_correction - (prediction_clearance - visual_clearance)).max(0.0);
            if crowded {
                post_penetration = post_penetration.min(BLOB_CONTACT_MAX_CORRECTION * pair_scale);
            }
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
            let residual = (visual_clearance - blob_surface_gap(first, second)).max(0.0);
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

            let mut relative_normal_speed = (second.velocity() - first.velocity()).dot(normal);
            if crowded {
                relative_normal_speed =
                    relative_normal_speed.max(-BLOB_CONTACT_MAX_TRANSFER_SPEED * pair_scale);
            }
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
