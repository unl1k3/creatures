use super::*;
use crate::level_format::LightDefinition;
use crate::shield::shield_spine_fans;
use bevy::sprite::Anchor;
use bevy::{asset::RenderAssetUsages, mesh::Indices, render::render_resource::PrimitiveTopology};
use std::collections::HashSet;

#[derive(Component)]
pub(super) struct BlobMesh {
    blob_id: u64,
    parent_id: Option<u64>,
    selected: bool,
    life_state: LifeState,
    energy_band: u8,
}

#[derive(Component)]
pub(super) struct BlobOutlineMesh {
    blob_id: u64,
    selected: bool,
    life_state: LifeState,
}

#[derive(Component)]
pub(super) struct BlobVacuoleMesh {
    blob_id: u64,
}

#[derive(Component)]
pub(super) struct RouteMarker {
    scenario: u8,
    index: usize,
}

pub(super) fn sync_route_markers(
    mut commands: Commands,
    scenario: Res<TestScenario>,
    progress: Res<RouteProgress>,
    level: Res<Level>,
    debug_overlay: Res<LevelDebugOverlay>,
    markers: Query<(Entity, &RouteMarker)>,
) {
    if !debug_overlay.visible {
        for (entity, _) in &markers {
            commands.entity(entity).despawn();
        }
        return;
    }
    let mut existing = HashSet::new();
    for (entity, marker) in &markers {
        if marker.scenario != scenario.0
            || marker.index < progress.next
            || marker.index >= level.route.len()
        {
            commands.entity(entity).despawn();
        } else {
            existing.insert(marker.index);
        }
    }
    for index in progress.next..level.route.len() {
        if existing.contains(&index) {
            continue;
        }
        commands.spawn((
            RouteMarker {
                scenario: scenario.0,
                index,
            },
            Text2d::new(index.to_string()),
            TextFont {
                font_size: FontSize::Px((16.0 + index as f32 * 1.8).min(30.0)),
                ..default()
            },
            TextColor(Color::srgb(1.0, 0.82, 0.28)),
            Anchor::CENTER,
            Transform::from_translation(level.route[index].extend(0.35)),
        ));
    }
}

fn blob_vital_color(parent_id: Option<u64>, selected: bool, vitality: Vitality) -> Color {
    let base = blob_fill_color(parent_id, selected);
    let fade = 0.52 + vitality.energy * 0.48;
    let linear = base.to_srgba();
    Color::srgba(
        linear.red * fade,
        linear.green * fade,
        linear.blue * fade,
        linear.alpha,
    )
}

fn blob_outline_color(parent_id: Option<u64>, selected: bool, vitality: Vitality) -> Color {
    if !vitality.is_alive() {
        return Color::srgba(0.24, 0.28, 0.30, 0.88);
    }
    let (red, green, blue) = blob_family_rgb(parent_id);
    if selected {
        Color::srgba(
            (red * 1.18 + 0.16).min(1.0),
            (green * 1.18 + 0.16).min(1.0),
            (blue * 1.18 + 0.16).min(1.0),
            0.98,
        )
    } else {
        Color::srgba(red * 0.38, green * 0.38, blue * 0.38, 0.78)
    }
}

fn blob_family_rgb(parent_id: Option<u64>) -> (f32, f32, f32) {
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
    (red, green, blue)
}

#[cfg(test)]
pub(super) fn blob_family_color(parent_id: Option<u64>) -> Color {
    let (red, green, blue) = blob_family_rgb(parent_id);
    Color::srgba(red, green, blue, 0.9)
}

pub(super) fn blob_fill_color(parent_id: Option<u64>, selected: bool) -> Color {
    let (red, green, blue) = blob_family_rgb(parent_id);
    if selected {
        Color::srgba(
            (red * 1.12 + 0.10).min(1.0),
            (green * 1.12 + 0.10).min(1.0),
            (blue * 1.12 + 0.10).min(1.0),
            0.96,
        )
    } else {
        Color::srgba(red * 0.72, green * 0.72, blue * 0.72, 0.62)
    }
}

pub(super) fn sync_blob_meshes(
    mut commands: Commands,
    blobs: Res<BlobWorld>,
    level: Res<Level>,
    time: Res<Time>,
    vitality_world: Res<VitalityWorld>,
    nutrition: Res<NutritionWorld>,
    shields: Res<ShieldWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut rendered: Query<
        (
            Entity,
            &mut BlobMesh,
            &Mesh2d,
            &MeshMaterial2d<ColorMaterial>,
        ),
        (With<BlobMesh>, Without<BlobOutlineMesh>),
    >,
    mut outlines: Query<
        (
            Entity,
            &mut BlobOutlineMesh,
            &Mesh2d,
            &MeshMaterial2d<ColorMaterial>,
        ),
        (With<BlobOutlineMesh>, Without<BlobMesh>),
    >,
    mut vacuoles: Query<
        (Entity, &BlobVacuoleMesh, &Mesh2d),
        (
            With<BlobVacuoleMesh>,
            Without<BlobMesh>,
            Without<BlobOutlineMesh>,
        ),
    >,
) {
    let active_ids = blobs
        .active
        .iter()
        .map(|blob| blob.id)
        .collect::<HashSet<_>>();
    let mut rendered_ids = HashSet::new();

    for (entity, mut marker, mesh_handle, material_handle) in &mut rendered {
        let Some(active_blob) = blobs.active.iter().find(|blob| blob.id == marker.blob_id) else {
            commands.entity(entity).despawn();
            continue;
        };
        rendered_ids.insert(marker.blob_id);

        if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
            update_blob_mesh_with_load(
                &mut mesh,
                &active_blob.body,
                nutrition.internal_load(active_blob.id),
                &level.lights,
            );
        }
        let selected = blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.id == active_blob.id);
        let vitality = vitality_world.get(active_blob.id);
        let energy_band = (vitality.energy * 20.0).round() as u8;
        if marker.parent_id != active_blob.parent_id
            || marker.selected != selected
            || marker.life_state != vitality.state
            || marker.energy_band != energy_band
        {
            marker.parent_id = active_blob.parent_id;
            marker.selected = selected;
            marker.life_state = vitality.state;
            marker.energy_band = energy_band;
            if let Some(mut material) = materials.get_mut(&material_handle.0) {
                material.color = blob_vital_color(active_blob.parent_id, selected, vitality);
            }
        }
    }

    for active_blob in blobs
        .active
        .iter()
        .filter(|blob| active_ids.contains(&blob.id) && !rendered_ids.contains(&blob.id))
    {
        let selected = blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.id == active_blob.id);
        let mesh = meshes.add(create_blob_mesh_with_load(
            &active_blob.body,
            nutrition.internal_load(active_blob.id),
            &level.lights,
        ));
        let vitality = vitality_world.get(active_blob.id);
        let material = materials.add(ColorMaterial::from(blob_vital_color(
            active_blob.parent_id,
            selected,
            vitality,
        )));
        commands.spawn((
            BlobMesh {
                blob_id: active_blob.id,
                parent_id: active_blob.parent_id,
                selected,
                life_state: vitality.state,
                energy_band: (vitality.energy * 20.0).round() as u8,
            },
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_xyz(0.0, 0.0, -0.1),
        ));
    }

    let mut outlined_ids = HashSet::new();
    for (entity, mut marker, mesh_handle, _material_handle) in &mut outlines {
        let Some(active_blob) = blobs.active.iter().find(|blob| blob.id == marker.blob_id) else {
            commands.entity(entity).despawn();
            continue;
        };
        outlined_ids.insert(marker.blob_id);
        let selected = blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.id == active_blob.id);
        let vitality = vitality_world.get(active_blob.id);
        if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
            update_blob_outline_mesh(
                &mut mesh,
                &active_blob.body,
                nutrition.internal_load(active_blob.id),
                selected,
                active_blob.parent_id,
                vitality,
                &level.lights,
                active_blob.id,
                shields.extension(active_blob.id),
                shields.energy(active_blob.id),
                &level.platforms,
            );
        }
        if marker.selected != selected || marker.life_state != vitality.state {
            marker.selected = selected;
            marker.life_state = vitality.state;
        }
    }
    for active_blob in blobs
        .active
        .iter()
        .filter(|blob| !outlined_ids.contains(&blob.id))
    {
        let selected = blobs
            .active
            .get(blobs.selected)
            .is_some_and(|blob| blob.id == active_blob.id);
        let vitality = vitality_world.get(active_blob.id);
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        update_blob_outline_mesh(
            &mut mesh,
            &active_blob.body,
            nutrition.internal_load(active_blob.id),
            selected,
            active_blob.parent_id,
            vitality,
            &level.lights,
            active_blob.id,
            shields.extension(active_blob.id),
            shields.energy(active_blob.id),
            &level.platforms,
        );
        commands.spawn((
            BlobOutlineMesh {
                blob_id: active_blob.id,
                selected,
                life_state: vitality.state,
            },
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(ColorMaterial::from(Color::WHITE))),
            Transform::from_xyz(0.0, 0.0, -0.08),
        ));
    }

    let elapsed = time.elapsed_secs();
    let mut vacuole_ids = HashSet::new();
    for (entity, marker, mesh_handle) in &mut vacuoles {
        let Some(active_blob) = blobs.active.iter().find(|blob| blob.id == marker.blob_id) else {
            commands.entity(entity).despawn();
            continue;
        };
        vacuole_ids.insert(marker.blob_id);
        if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
            update_blob_vacuole_mesh(
                &mut mesh,
                active_blob,
                elapsed,
                vitality_world.get(active_blob.id).is_alive(),
                &level.lights,
            );
        }
    }
    for active_blob in blobs
        .active
        .iter()
        .filter(|blob| !vacuole_ids.contains(&blob.id))
    {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        update_blob_vacuole_mesh(
            &mut mesh,
            active_blob,
            elapsed,
            vitality_world.get(active_blob.id).is_alive(),
            &level.lights,
        );
        commands.spawn((
            BlobVacuoleMesh {
                blob_id: active_blob.id,
            },
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(ColorMaterial::from(Color::srgba(1.0, 1.0, 1.0, 0.66)))),
            Transform::from_xyz(0.0, 0.0, -0.06),
        ));
    }
}

#[cfg(test)]
pub(super) fn create_blob_mesh(blob: &Blob) -> Mesh {
    create_blob_mesh_with_load(blob, None, &[])
}

fn create_blob_mesh_with_load(
    blob: &Blob,
    load: Option<(Vec2, f32, f32, f32, usize, f32)>,
    lights: &[LightDefinition],
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    update_blob_mesh_with_load(&mut mesh, blob, load, lights);
    mesh
}

pub(super) fn charge_indicator_radius(blob: &Blob) -> f32 {
    let center = blob.center();
    let outermost = blob
        .particles
        .iter()
        .map(|particle| particle.position.distance(center))
        .fold(blob.rest_radius, f32::max);
    outermost + (5.0 * blob.size_scale()).max(2.5)
}

fn update_blob_mesh_with_load(
    mesh: &mut Mesh,
    blob: &Blob,
    load: Option<(Vec2, f32, f32, f32, usize, f32)>,
    lights: &[LightDefinition],
) {
    let center = blob.center();
    let membrane = rendered_membrane_points(blob, load);
    let mut positions = Vec::with_capacity(membrane.len() + 1);
    let mut uvs = Vec::with_capacity(membrane.len() + 1);
    let mut colors = Vec::with_capacity(membrane.len() + 1);
    positions.push([center.x, center.y, 0.0]);
    uvs.push([0.5, 0.5]);
    colors.push(blob_vertex_light(center, Vec2::Y, lights, true));
    for point in &membrane {
        positions.push([point.position.x, point.position.y, 0.0]);
        let local = (point.position - center) / (blob.rest_radius * 2.0);
        uvs.push([0.5 + local.x, 0.5 + local.y]);
        colors.push(blob_vertex_light(
            point.position,
            (point.position - center).normalize_or(Vec2::Y),
            lights,
            false,
        ));
    }

    let mut indices = Vec::with_capacity(membrane.len() * 3);
    let original = membrane
        .iter()
        .enumerate()
        .filter(|(_, point)| !point.appendage)
        .map(|(index, _)| index as u32 + 1)
        .collect::<Vec<_>>();
    for index in 0..original.len() {
        indices.extend_from_slice(&[0, original[index], original[(index + 1) % original.len()]]);
    }
    if let Some(first_appendage) = membrane.iter().position(|point| point.appendage) {
        let last_appendage = membrane
            .iter()
            .rposition(|point| point.appendage)
            .unwrap_or(first_appendage);
        let start = first_appendage.saturating_sub(1);
        let end = (last_appendage + 1) % membrane.len();
        let mut appendage = vec![start as u32 + 1];
        appendage.extend((first_appendage..=last_appendage).map(|index| index as u32 + 1));
        appendage.push(end as u32 + 1);
        triangulate_appendage(&appendage, &membrane, &mut indices);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}

/// Computes inexpensive 2D diffuse lighting for one mesh vertex. The ambient
/// term keeps the creature readable outside a lamp's radius, while nearby
/// lights tint only the membrane portions facing them.
fn blob_vertex_light(
    position: Vec2,
    outward_normal: Vec2,
    lights: &[LightDefinition],
    center_vertex: bool,
) -> [f32; 4] {
    let mut rgb = Vec3::new(0.38, 0.44, 0.48);
    for light in lights.iter().filter(|light| light.enabled) {
        let toward_light = light.position - position;
        let distance = toward_light.length();
        if distance >= light.radius {
            continue;
        }
        let radial = 1.0 - distance / light.radius;
        let facing = if center_vertex {
            0.42
        } else {
            0.12 + 0.88
                * outward_normal
                    .dot(toward_light.normalize_or_zero())
                    .max(0.0)
        };
        let contribution = radial * facing * light.intensity;
        rgb += Vec3::from_array(light.color) * contribution;
    }
    [rgb.x.min(1.0), rgb.y.min(1.0), rgb.z.min(1.0), 1.0]
}

fn update_blob_vacuole_mesh(
    mesh: &mut Mesh,
    active_blob: &ActiveBlob,
    elapsed: f32,
    alive: bool,
    lights: &[LightDefinition],
) {
    let blob = &active_blob.body;
    if !alive {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
        mesh.insert_indices(Indices::U32(Vec::new()));
        return;
    }

    const SEGMENTS: usize = 10;
    let count = ((blob.rest_radius / 10.0).round() as usize).clamp(3, 7);
    let center = blob.center();
    let average_motion = blob
        .particles
        .iter()
        .map(|particle| particle.position - particle.previous)
        .sum::<Vec2>()
        / blob.particles.len() as f32;
    let inertial_offset = (-average_motion * 3.2).clamp_length_max(blob.rest_radius * 0.16);
    let polygon = blob
        .particles
        .iter()
        .map(|particle| particle.position)
        .collect::<Vec<_>>();
    let mut positions = Vec::with_capacity(count * (SEGMENTS + 1));
    let mut colors = Vec::with_capacity(count * (SEGMENTS + 1));
    let mut indices = Vec::with_capacity(count * SEGMENTS * 3);

    for index in 0..count {
        let angle_seed = random_unit(active_blob.id, index, 0) * std::f32::consts::TAU;
        let radial_seed = 0.16 + random_unit(active_blob.id, index, 1) * 0.38;
        let phase = angle_seed + elapsed * (0.34 + random_unit(active_blob.id, index, 2) * 0.30);
        let base = Vec2::from_angle(angle_seed) * blob.rest_radius * radial_seed;
        let drift = Vec2::new(
            phase.sin() * blob.rest_radius * 0.055,
            (phase * 0.73).cos() * blob.rest_radius * 0.075,
        );
        let radius =
            (blob.rest_radius * (0.075 + random_unit(active_blob.id, index, 3) * 0.055)).max(1.5);
        let vacuole_center = fit_circle_inside_polygon(
            center,
            center + base + drift + inertial_offset,
            radius * 1.35,
            &polygon,
        );
        let first_vertex = positions.len() as u32;
        positions.push([vacuole_center.x, vacuole_center.y, 0.0]);
        let light = blob_vertex_light(vacuole_center, Vec2::Y, lights, true);
        let tint = vacuole_tint(active_blob.parent_id, index);
        let visible_light = [
            0.40 + light[0] * 0.60,
            0.44 + light[1] * 0.56,
            0.48 + light[2] * 0.52,
        ];
        colors.push([
            visible_light[0] * tint.x * 0.72,
            visible_light[1] * tint.y * 0.76,
            visible_light[2] * tint.z * 0.80,
            0.72,
        ]);

        let stretch = 0.82 + random_unit(active_blob.id, index, 4) * 0.34;
        let tilt = angle_seed * 1.7;
        for segment in 0..SEGMENTS {
            let angle = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let organic = 1.0 + 0.08 * (angle * 3.0 + phase).sin();
            let local = Vec2::from_angle(tilt).rotate(Vec2::new(
                angle.cos() * radius * stretch,
                angle.sin() * radius / stretch,
            )) * organic;
            positions.push([vacuole_center.x + local.x, vacuole_center.y + local.y, 0.0]);
            colors.push([
                visible_light[0] * tint.x,
                visible_light[1] * tint.y,
                visible_light[2] * tint.z,
                1.0,
            ]);
        }
        for segment in 0..SEGMENTS {
            indices.extend_from_slice(&[
                first_vertex,
                first_vertex + 1 + segment as u32,
                first_vertex + 1 + ((segment + 1) % SEGMENTS) as u32,
            ]);
        }
    }

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}

fn vacuole_tint(parent_id: Option<u64>, index: usize) -> Vec3 {
    let (red, green, blue) = blob_family_rgb(parent_id);
    let base = Vec3::new(red, green, blue);
    let white = Vec3::ONE;
    match index % 3 {
        // Soft complementary color: strongest contrast against the membrane.
        0 => (white - base).lerp(white, 0.34),
        // Rotated channels: distinct, while retaining the family palette.
        1 => Vec3::new(base.z, base.x, base.y).lerp(white, 0.30),
        // Pale family color: behaves like a liquid highlight.
        _ => base.lerp(white, 0.58),
    }
}

fn random_unit(blob_id: u64, index: usize, channel: u64) -> f32 {
    let mut value = blob_id
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((index as u64 + 1).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(channel.wrapping_mul(0x94D0_49BB_1331_11EB));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value as u32) as f32 / u32::MAX as f32
}

fn fit_circle_inside_polygon(center: Vec2, desired: Vec2, radius: f32, polygon: &[Vec2]) -> Vec2 {
    let mut candidate = desired;
    for _ in 0..12 {
        let inside = point_in_polygon(candidate, polygon);
        let clearance = polygon
            .iter()
            .copied()
            .zip(polygon.iter().copied().cycle().skip(1))
            .take(polygon.len())
            .map(|(start, end)| point_segment_distance(candidate, start, end))
            .fold(f32::INFINITY, f32::min);
        if inside && clearance >= radius {
            break;
        }
        candidate = center.lerp(candidate, 0.78);
    }
    candidate
}

fn point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    polygon
        .iter()
        .copied()
        .zip(polygon.iter().copied().cycle().skip(1))
        .take(polygon.len())
        .fold(false, |inside, (first, second)| {
            let crosses = (first.y > point.y) != (second.y > point.y)
                && point.x
                    < (second.x - first.x) * (point.y - first.y) / (second.y - first.y) + first.x;
            inside ^ crosses
        })
}

fn point_segment_distance(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let edge = end - start;
    let fraction = (point - start).dot(edge) / edge.length_squared().max(f32::EPSILON);
    point.distance(start + edge * fraction.clamp(0.0, 1.0))
}

fn update_blob_outline_mesh(
    mesh: &mut Mesh,
    blob: &Blob,
    load: Option<(Vec2, f32, f32, f32, usize, f32)>,
    selected: bool,
    parent_id: Option<u64>,
    vitality: Vitality,
    lights: &[LightDefinition],
    blob_id: u64,
    shield_extension: f32,
    shield_energy: f32,
    platforms: &[Platform],
) {
    let membrane = rendered_membrane_points(blob, load);
    let count = membrane.len();
    let thickness =
        (if selected { 7.0 } else { 5.2 } * blob.size_scale() * (1.0 + shield_extension * 0.52))
            .max(2.4);
    let transition_depth = thickness * 0.42;
    let mut positions = Vec::with_capacity(count * 3);
    let mut colors = Vec::with_capacity(count * 3);
    let mut inward_normals = Vec::with_capacity(count);
    let base = blob_outline_color(parent_id, selected, vitality).to_srgba();
    let base_rgb = Vec3::new(base.red, base.green, base.blue)
        .lerp(Vec3::new(0.34, 0.88, 1.0), shield_extension * 0.62);
    for point in &membrane {
        positions.push([point.position.x, point.position.y, 0.0]);
    }
    for index in 0..count {
        let previous = membrane[(index + count - 1) % count].position;
        let current = membrane[index].position;
        let next = membrane[(index + 1) % count].position;
        let first_inward = (current - previous).perp().normalize_or_zero();
        let second_inward = (next - current).perp().normalize_or_zero();
        let inward = (first_inward + second_inward)
            .normalize_or((blob.center() - current).normalize_or(Vec2::Y));
        inward_normals.push(inward);
        let transition = current + inward * transition_depth;
        positions.push([transition.x, transition.y, 0.0]);
    }
    for (index, point) in membrane.iter().enumerate() {
        let inner = point.position + inward_normals[index] * thickness;
        positions.push([inner.x, inner.y, 0.0]);
    }
    for (index, point) in membrane.iter().enumerate() {
        let illumination = blob_vertex_light(point.position, -inward_normals[index], lights, false);
        let energy = 0.72 + vitality.energy * 0.28;
        colors.push([
            (base_rgb.x * (0.62 + illumination[0] * 0.70) * energy).min(1.0),
            (base_rgb.y * (0.62 + illumination[1] * 0.70) * energy).min(1.0),
            (base_rgb.z * (0.62 + illumination[2] * 0.70) * energy).min(1.0),
            if selected { 0.98 } else { 0.90 },
        ]);
    }
    for _ in 0..count {
        colors.push([
            base_rgb.x * 0.76,
            base_rgb.y * 0.76,
            base_rgb.z * 0.76,
            0.88,
        ]);
    }
    for _ in 0..count {
        colors.push([
            base_rgb.x * 0.34,
            base_rgb.y * 0.34,
            base_rgb.z * 0.34,
            0.68,
        ]);
    }
    let mut indices = Vec::with_capacity(count * 12);
    for index in 0..count {
        let next = (index + 1) % count;
        indices.extend_from_slice(&[
            index as u32,
            next as u32,
            (count + next) as u32,
            index as u32,
            (count + next) as u32,
            (count + index) as u32,
            (count + index) as u32,
            (count + next) as u32,
            (count * 2 + next) as u32,
            (count + index) as u32,
            (count * 2 + next) as u32,
            (count * 2 + index) as u32,
        ]);
    }
    let contour = membrane
        .iter()
        .map(|point| point.position)
        .collect::<Vec<_>>();
    for (base_arc, tip) in shield_spine_fans(blob_id, blob, shield_extension, platforms, &contour) {
        let shield_brightness = 0.58 + shield_energy * 0.42;
        for edge in base_arc.windows(2) {
            let first = positions.len() as u32;
            positions.extend_from_slice(&[
                [edge[0].x, edge[0].y, 0.0],
                [tip.x, tip.y, 0.0],
                [edge[1].x, edge[1].y, 0.0],
            ]);
            colors.extend_from_slice(&[
                [0.22, 0.72, 0.82, 0.82],
                [
                    0.52 * shield_brightness,
                    0.96 * shield_brightness,
                    1.0 * shield_brightness,
                    0.96,
                ],
                [0.22, 0.72, 0.82, 0.82],
            ]);
            indices.extend_from_slice(&[first, first + 1, first + 2]);
        }
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
}

fn triangulate_appendage(
    polygon: &[u32],
    membrane: &[RenderedMembranePoint],
    indices: &mut Vec<u32>,
) {
    let point = |index: u32| membrane[index as usize - 1].position;
    let area = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(a, b)| point(*a).perp_dot(point(*b)))
        .sum::<f32>();
    let orientation = area.signum();
    let mut remaining = polygon.to_vec();
    while remaining.len() > 2 {
        let mut ear = None;
        for index in 0..remaining.len() {
            let previous = remaining[(index + remaining.len() - 1) % remaining.len()];
            let current = remaining[index];
            let next = remaining[(index + 1) % remaining.len()];
            let a = point(previous);
            let b = point(current);
            let c = point(next);
            if (b - a).perp_dot(c - b) * orientation <= 0.000_001 {
                continue;
            }
            let contains_point = remaining.iter().any(|candidate| {
                *candidate != previous
                    && *candidate != current
                    && *candidate != next
                    && point_in_triangle(point(*candidate), a, b, c)
            });
            if !contains_point {
                ear = Some((index, [previous, current, next]));
                break;
            }
        }
        let Some((index, triangle)) = ear else {
            // At the first frames the appendage can be almost flat. Complete
            // the remaining simple sliver instead of leaving a visible hole.
            for fan_index in 1..remaining.len().saturating_sub(1) {
                indices.extend_from_slice(&[
                    remaining[0],
                    remaining[fan_index],
                    remaining[fan_index + 1],
                ]);
            }
            return;
        };
        indices.extend_from_slice(&triangle);
        remaining.remove(index);
    }
}

fn point_in_triangle(point: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let first = (b - a).perp_dot(point - a);
    let second = (c - b).perp_dot(point - b);
    let third = (a - c).perp_dot(point - c);
    (first >= -0.000_01 && second >= -0.000_01 && third >= -0.000_01)
        || (first <= 0.000_01 && second <= 0.000_01 && third <= 0.000_01)
}

#[cfg(test)]
mod membrane_detail_tests {
    use super::*;

    #[test]
    fn internal_load_temporarily_adds_local_membrane_points() {
        let blob = Blob::new(Vec2::ZERO, 40.0);
        let normal_count = rendered_membrane_points(&blob, None).len();
        let detailed =
            rendered_membrane_points(&blob, Some((Vec2::new(20.0, 0.0), 12.0, 1.0, 0.37, 0, 0.5)));
        assert!(detailed.len() > normal_count);
        assert!(detailed.iter().any(|point| point.temporary));
        assert_eq!(detailed.iter().filter(|point| point.attachment).count(), 2);
        assert!(detailed.iter().any(|point| point.appendage));
        assert_eq!(rendered_membrane_points(&blob, None).len(), normal_count);
    }

    #[test]
    fn nascent_protrusion_is_fully_triangulated() {
        let blob = Blob::new(Vec2::ZERO, 40.0);
        for strength in [0.011, 0.025, 0.05, 0.10] {
            let load = Some((Vec2::new(72.0, 8.0), 5.0, strength, 0.61, 0, 0.5));
            let membrane = rendered_membrane_points(&blob, load);
            let mesh = create_blob_mesh_with_load(&blob, load, &[]);
            assert_eq!(
                mesh.indices().expect("mesh indices").len(),
                membrane.len() * 3,
                "incomplete triangulation at strength {strength}"
            );
        }
    }

    #[test]
    fn outline_is_a_closed_triangle_ring() {
        let blob = Blob::new(Vec2::ZERO, 40.0);
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        update_blob_outline_mesh(
            &mut mesh,
            &blob,
            None,
            true,
            None,
            Vitality::default(),
            &[],
            0,
            0.0,
            1.0,
            &[],
        );

        assert_eq!(
            mesh.indices().expect("outline indices").len(),
            blob.particles.len() * 12
        );
        assert_eq!(mesh.count_vertices(), blob.particles.len() * 3);
    }

    #[test]
    fn nearby_light_brightens_the_facing_membrane() {
        let light = LightDefinition {
            position: Vec2::new(20.0, 0.0),
            color: [1.0, 0.3, 0.1],
            radius: 100.0,
            intensity: 1.0,
            enabled: true,
        };
        let facing = blob_vertex_light(Vec2::ZERO, Vec2::X, &[light], false);
        let opposite = blob_vertex_light(Vec2::ZERO, Vec2::NEG_X, &[light], false);
        let outside = blob_vertex_light(Vec2::new(-100.0, 0.0), Vec2::X, &[light], false);

        assert!(facing[0] > opposite[0]);
        assert!(opposite[0] > outside[0]);
        assert!(facing[0] - facing[2] > opposite[0] - opposite[2]);
    }

    #[test]
    fn disabled_light_does_not_affect_the_blob() {
        let light = LightDefinition {
            position: Vec2::new(10.0, 0.0),
            color: [1.0, 1.0, 1.0],
            radius: 100.0,
            intensity: 2.0,
            enabled: false,
        };

        assert_eq!(
            blob_vertex_light(Vec2::ZERO, Vec2::X, &[light], false),
            blob_vertex_light(Vec2::ZERO, Vec2::X, &[], false)
        );
    }

    #[test]
    fn vacuole_palette_varies_with_index_and_blob_family() {
        assert_ne!(vacuole_tint(Some(2), 0), vacuole_tint(Some(2), 1));
        assert_ne!(vacuole_tint(Some(2), 1), vacuole_tint(Some(2), 2));
        assert_ne!(vacuole_tint(Some(2), 0), vacuole_tint(Some(3), 0));
    }
}

#[derive(Clone, Copy)]
struct RenderedMembranePoint {
    position: Vec2,
    temporary: bool,
    appendage: bool,
    attachment: bool,
}

fn rendered_membrane_points(
    blob: &Blob,
    load: Option<(Vec2, f32, f32, f32, usize, f32)>,
) -> Vec<RenderedMembranePoint> {
    let Some((load_position, load_radius, strength, variation, anchor_edge, anchor_t)) =
        load.filter(|(_, _, value, _, _, _)| *value > 0.01)
    else {
        return blob
            .particles
            .iter()
            .map(|particle| RenderedMembranePoint {
                position: particle.position,
                temporary: false,
                appendage: false,
                attachment: false,
            })
            .collect();
    };
    let count = blob.particles.len();
    let nearest_edge = anchor_edge % count;
    let center = blob.center();
    let load_direction = (load_position - center).normalize_or(Vec2::X);
    let mut points = Vec::with_capacity(count + 31);
    for index in 0..count {
        let second_anchor_edge = (nearest_edge + 1) % count;
        if index == second_anchor_edge {
            continue;
        }
        let start = blob.particles[index].position;
        let end_index = if index == nearest_edge {
            (index + 2) % count
        } else {
            (index + 1) % count
        };
        let end = blob.particles[end_index].position;
        points.push(RenderedMembranePoint {
            position: start,
            temporary: false,
            appendage: false,
            attachment: false,
        });
        if index != nearest_edge {
            continue;
        }
        let base = blob.particles[(nearest_edge + 1) % count].position;
        let tip = base.lerp(load_position, strength.clamp(0.0, 1.0));
        let length = base.distance(tip);
        let normal_axis = load_direction.perp();
        let secondary = (variation * 7.137).fract();
        let asymmetry = (anchor_t.clamp(0.0, 1.0) - 0.5) * 0.08;
        let start_attachment = start.lerp(base, 0.18 + asymmetry);
        let end_attachment = base.lerp(end, 0.82 + asymmetry);
        let attachment_tangent = (end_attachment - start_attachment).normalize_or(Vec2::X);
        let mut root_normal = attachment_tangent.perp();
        if root_normal.dot(base - center) < 0.0 {
            root_normal = -root_normal;
        }
        let control_a = base
            + root_normal * length * (0.30 + variation * 0.08)
            + attachment_tangent * length * (variation - 0.5) * 0.05;
        let control_b = base.lerp(tip, 0.72) + normal_axis * length * (secondary - 0.5) * 0.18;
        let width = (load_radius * (0.55 + strength * 0.45) * (0.88 + variation * 0.24))
            .min(start.distance(end) * 0.48)
            .max(0.5);
        let base_normal = root_normal.perp();
        // Follow the membrane's existing winding: the first side of the tube
        // must leave from `start`, otherwise the two base triangles cross.
        let winding_side = if (start - base).dot(base_normal) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let attachment_half_width = start_attachment.distance(end_attachment) * 0.5;
        points.push(RenderedMembranePoint {
            position: start_attachment,
            temporary: true,
            appendage: false,
            attachment: true,
        });
        const MINIMAL_PROFILE: &[f32] = &[0.24];
        const MEDIUM_PROFILE: &[f32] = &[0.20, 0.46, 0.70, 0.88, 0.975];
        const FULL_PROFILE: &[f32] = &[
            0.06, 0.14, 0.23, 0.32, 0.42, 0.52, 0.62, 0.71, 0.79, 0.86, 0.92, 0.97, 0.995,
        ];
        let profile = if strength < 0.16 {
            MINIMAL_PROFILE
        } else if strength < 0.38 {
            MEDIUM_PROFILE
        } else {
            FULL_PROFILE
        };
        let mut outline = Vec::with_capacity(profile.len() * 2 + 1);
        outline.extend(profile.iter().copied().map(|along| (along, 1.0)));
        outline.push((1.0, 0.0));
        outline.extend(profile.iter().rev().copied().map(|along| (along, -1.0)));
        for (along, side) in outline {
            let raw_side = side;
            let side = raw_side * winding_side;
            let inverse: f32 = 1.0 - along;
            let centerline = base * inverse.powi(3)
                + control_a * 3.0 * inverse.powi(2) * along
                + control_b * 3.0 * inverse * along.powi(2)
                + tip * along.powi(3);
            let tangent = ((control_a - base) * 3.0 * inverse.powi(2)
                + (control_b - control_a) * 6.0 * inverse * along
                + (tip - control_b) * 3.0 * along.powi(2))
            .normalize_or(load_direction);
            let normal = tangent.perp();
            let organic_wave = 1.0
                + (along * std::f32::consts::PI * (2.0 + variation * 1.4)
                    + variation * std::f32::consts::TAU)
                    .sin()
                    * (along * std::f32::consts::PI).sin()
                    * 0.075;
            let root_flare = 1.0 + (1.0 - along).powi(2) * (0.38 + variation * 0.12);
            let taper = (1.0_f32 - along * (0.76 + secondary * 0.10)).max(0.14);
            let rounded_tip = if along > 0.9 {
                ((1.0 - along) / 0.1).sqrt()
            } else {
                1.0
            };
            let profile_width = (width * root_flare * taper * organic_wave)
                .min(attachment_half_width * (1.0 - along * 0.58).max(0.18));
            let curved_position = centerline + normal * side * profile_width * rounded_tip;
            let attachment = if raw_side >= 0.0 {
                start_attachment
            } else {
                end_attachment
            };
            let root_blend = smoothstep01(along / 0.20);
            points.push(RenderedMembranePoint {
                position: attachment.lerp(curved_position, root_blend),
                temporary: true,
                appendage: true,
                attachment: false,
            });
        }
        points.push(RenderedMembranePoint {
            position: end_attachment,
            temporary: true,
            appendage: false,
            attachment: true,
        });
    }
    points
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

pub(super) fn draw_world(
    mut gizmos: Gizmos,
    blobs: Res<BlobWorld>,
    vitality_world: Res<VitalityWorld>,
    level: Res<Level>,
    debug_overlay: Res<LevelDebugOverlay>,
    route_progress: Res<RouteProgress>,
    nutrition: Res<NutritionWorld>,
) {
    // Laboratories without artwork retain their unobtrusive collision view.
    if !level.has_artwork() && !debug_overlay.visible {
        for platform in &level.platforms {
            gizmos.rect_2d(
                platform.center,
                platform.half_size * 2.0,
                Color::srgb(0.18, 0.27, 0.38),
            );
        }
        for fixture in &level.fixtures {
            gizmos.lineloop_2d(fixture.iter().copied(), Color::srgb(0.24, 0.38, 0.52));
        }
    }
    if debug_overlay.visible {
        // Draw three close contours to remain readable over detailed artwork.
        for platform in &level.platforms {
            for expansion in [-3.0, 0.0, 3.0] {
                gizmos.rect_2d(
                    platform.center,
                    platform.half_size * 2.0 + Vec2::splat(expansion),
                    Color::srgba(1.0, 0.28, 0.08, 0.96),
                );
            }
        }
        for fixture in &level.fixtures {
            for (first, second) in fixture
                .iter()
                .copied()
                .zip(fixture.iter().copied().cycle().skip(1))
                .take(fixture.len())
            {
                let normal = (second - first).perp().normalize_or_zero();
                for offset in [-2.0, 0.0, 2.0] {
                    gizmos.line_2d(
                        first + normal * offset,
                        second + normal * offset,
                        Color::srgba(1.0, 0.28, 0.08, 0.96),
                    );
                }
            }
        }
        for expansion in [-3.0, 0.0, 3.0] {
            gizmos.rect_2d(
                level.center(),
                level.size() + Vec2::splat(expansion),
                Color::srgba(0.12, 0.85, 1.0, 0.92),
            );
        }
        let marker_size = 14.0;
        for offset in [-2.0, 0.0, 2.0] {
            gizmos.line_2d(
                level.spawn_position + Vec2::new(-marker_size, offset),
                level.spawn_position + Vec2::new(marker_size, offset),
                Color::srgb(0.25, 1.0, 0.35),
            );
            gizmos.line_2d(
                level.spawn_position + Vec2::new(offset, -marker_size),
                level.spawn_position + Vec2::new(offset, marker_size),
                Color::srgb(0.25, 1.0, 0.35),
            );
        }
        for light in level.lights.iter().filter(|light| light.enabled) {
            let color = Color::srgba(
                light.color[0],
                light.color[1],
                light.color[2],
                (0.35 + light.intensity * 0.18).clamp(0.35, 0.9),
            );
            gizmos.circle_2d(light.position, light.radius, color);
            gizmos.circle_2d(light.position, 5.0, color);
        }
        for point in &level.expulsion_points {
            let length = (point.strength * 0.12).clamp(20.0, 80.0);
            let end = point.position + point.direction * length;
            gizmos.arrow_2d(point.position, end, Color::srgb(0.82, 0.35, 1.0));
        }
        for hazard in &level.hazards {
            for expansion in [-2.0, 0.0, 2.0] {
                gizmos.rect_2d(
                    hazard.position,
                    hazard.size + Vec2::splat(expansion),
                    Color::srgba(1.0, 0.08, 0.18, 0.94),
                );
            }
        }
    }
    if debug_overlay.visible {
        for (index, checkpoint) in level.route.iter().enumerate().skip(route_progress.next) {
            let radius = (7.0 + index as f32 * 1.5).min(20.0);
            gizmos.circle_2d(*checkpoint, radius, Color::srgba(1.0, 0.72, 0.18, 0.72));
        }
    }
    if !debug_overlay.visible {
        for hazard in &level.hazards {
            let left = hazard.position.x - hazard.size.x * 0.5;
            // Hazard volumes grow upward from their supporting surface.
            let surface_y = hazard.position.y - hazard.size.y * 0.5;
            let surface = (0..=12).map(|step| {
                let fraction = step as f32 / 12.0;
                Vec2::new(
                    left + hazard.size.x * fraction,
                    surface_y + (fraction * std::f32::consts::TAU * 2.0).sin() * 2.4,
                )
            });
            gizmos.linestrip_2d(surface, Color::srgba(0.64, 1.0, 0.06, 0.94));
            for offset in [0.2, 0.5, 0.78] {
                gizmos.circle_2d(
                    Vec2::new(left + hazard.size.x * offset, surface_y - 5.0),
                    2.5,
                    Color::srgba(0.72, 1.0, 0.08, 0.72),
                );
            }
        }
    }

    for active_blob in &blobs.active {
        let blob = &active_blob.body;
        let vitality = vitality_world.get(active_blob.id);
        let center = blob.center();
        let size_scale = blob.size_scale();
        if debug_overlay.visible {
            let membrane = rendered_membrane_points(blob, nutrition.internal_load(active_blob.id));
            for point in membrane.iter().filter(|point| point.temporary) {
                let radius = if point.attachment { 2.0 } else { 1.35 };
                let point_color = if point.attachment {
                    Color::srgba(1.0, 0.90, 0.42, 0.82)
                } else {
                    Color::srgba(1.0, 0.82, 0.30, 0.58)
                };
                gizmos.circle_2d(point.position, (radius * size_scale).max(0.72), point_color);
            }
            for particle in &blob.particles {
                gizmos.line_2d(
                    center,
                    particle.position,
                    Color::srgba(0.12, 0.55, 0.48, 0.42),
                );
            }
            if vitality.is_alive() {
                gizmos.circle_2d(center, 9.0 * size_scale, Color::srgb(0.72, 0.42, 0.95));
            }
        }

        if vitality.is_alive() && blob.charge > 0.0 {
            let radius = charge_indicator_radius(blob);
            let line_spacing = (1.8 * size_scale).max(0.9);
            gizmos.circle_2d(center, radius, Color::srgba(1.0, 0.72, 0.12, 0.20));
            for offset in [-line_spacing, 0.0, line_spacing] {
                gizmos.arc_2d(
                    center,
                    std::f32::consts::TAU * blob.charge,
                    radius + offset,
                    Color::srgba(1.0, 0.78, 0.16, 0.96),
                );
            }
            gizmos.circle_2d(
                center,
                (6.0 + 5.0 * blob.charge) * size_scale.max(0.45),
                Color::srgba(1.0, 0.62 + 0.24 * blob.charge, 0.12, 0.88),
            );
        }
    }
}
