//! Blob-world state and the gameplay rules that operate on a family of blobs.
//!
//! Keeping splitting, rejoining, collision resolution and recovery together
//! makes the gameplay loop independent from application startup and rendering.

use super::*;

pub(crate) const BLOB_START: Vec2 = Vec2::new(0.0, -280.0);
pub(crate) const INITIAL_RADIUS: f32 = REFERENCE_RADIUS * DEFAULT_CREATURE_SCALE;
pub(crate) const MAX_ACTIVE_BLOBS: usize = 4;
pub(crate) const REJOIN_TIMEOUT: f32 = 4.0;
const BLOB_CONTACT_PREDICTION_CLEARANCE: f32 = 1.5;
pub(crate) const BLOB_CONTACT_VISUAL_CLEARANCE: f32 = 0.0;
const BLOB_CONTACT_MAX_CORRECTION: f32 = 4.0;
const BLOB_CONTACT_MAX_TRANSFER_SPEED: f32 = 4.0;

pub(crate) struct ActiveBlob {
    pub(crate) id: u64,
    pub(crate) parent_id: Option<u64>,
    pub(crate) body: Blob,
}

#[derive(Resource)]
pub(crate) struct BlobWorld {
    pub(crate) active: Vec<ActiveBlob>,
    pub(crate) selected: usize,
    pub(crate) rejoin_parent: Option<u64>,
    pub(crate) rejoin_elapsed: f32,
    pub(crate) parent_links: HashMap<u64, Option<u64>>,
    pub(crate) next_id: u64,
}

#[derive(Resource)]
pub(crate) struct SplitRng(pub(crate) u64);

impl SplitRng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(crate) fn split_choice(&mut self, particle_count: usize) -> (usize, bool) {
        let ratio = 0.37 + (self.next() % 10) as f32 * 0.01;
        let smaller_count = ((particle_count as f32 * ratio).round() as usize)
            .clamp(6, particle_count.saturating_sub(6));
        let smaller_on_left = self.next() & 1 == 0;
        (smaller_count, smaller_on_left)
    }
}

pub(crate) fn enforce_blob_safety_bounds(level: Res<Level>, mut blobs: ResMut<BlobWorld>) {
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
pub(crate) fn reset_world_at(blobs: &mut BlobWorld, position: Vec2) {
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
pub(crate) fn split_selected(blobs: &mut BlobWorld, rng: &mut SplitRng, dt: f32) {
    let _ = split_selected_in_level(blobs, rng, dt, &[], &[]);
}

pub(crate) fn split_selected_in_level(
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

pub(crate) fn start_selected_rejoin(blobs: &mut BlobWorld) -> bool {
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

pub(crate) fn advance_rejoin_timeout(blobs: &mut BlobWorld, dt: f32) {
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

pub(crate) fn rejoin_pair_indices(blobs: &BlobWorld) -> Option<(usize, usize, u64)> {
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

pub(crate) fn rejoin_roll_directions(
    blobs: &BlobWorld,
    platforms: &[Platform],
) -> Option<Vec<f32>> {
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

pub(crate) fn update_rejoining(
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

pub(crate) fn place_blob_clear(
    blob: &mut Blob,
    platforms: &[Platform],
    fixtures: &[Vec<Vec2>],
) -> bool {
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

pub(crate) fn merge_circle_aabb_penetration(
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

pub(crate) fn merge_circle_convex_penetration(
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

pub(crate) fn path_is_clear(start: Vec2, end: Vec2, platforms: &[Platform]) -> bool {
    !platforms
        .iter()
        .any(|platform| segment_intersects_aabb(start, end, platform))
}

pub(crate) fn segment_intersects_aabb(start: Vec2, end: Vec2, platform: &Platform) -> bool {
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
pub(crate) fn resolve_blob_collisions(blobs: &mut [ActiveBlob]) {
    resolve_blob_collisions_impl(blobs, |_| (true, true));
}

pub(crate) fn resolve_blob_collisions_with_vitality(
    blobs: &mut [ActiveBlob],
    _vitality: &VitalityWorld,
) {
    resolve_blob_collisions_impl(blobs, |_| (true, true));
}

pub(crate) fn resolve_blob_collisions_impl(
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

pub(crate) fn avian_blob_contacts(
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

pub(crate) fn support_extent(blob: &Blob, direction: Vec2) -> f32 {
    let center = blob.center();
    blob.particles
        .iter()
        .map(|particle| (particle.position - center).dot(direction))
        .fold(0.0, f32::max)
}

pub(crate) fn blob_surface_gap(first: &Blob, second: &Blob) -> f32 {
    let delta = second.center() - first.center();
    let distance = delta.length();
    let normal = if distance > 0.001 {
        delta / distance
    } else {
        Vec2::X
    };
    distance - support_extent(first, normal) - support_extent(second, -normal)
}
