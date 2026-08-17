use super::*;
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
    vitality_world: Res<VitalityWorld>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut rendered: Query<(
        Entity,
        &mut BlobMesh,
        &Mesh2d,
        &MeshMaterial2d<ColorMaterial>,
    )>,
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
            update_blob_mesh(&mut mesh, &active_blob.body);
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
        let mesh = meshes.add(create_blob_mesh(&active_blob.body));
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
}

pub(super) fn create_blob_mesh(blob: &Blob) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    update_blob_mesh(&mut mesh, blob);
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

fn update_blob_mesh(mesh: &mut Mesh, blob: &Blob) {
    let center = blob.center();
    let mut positions = Vec::with_capacity(blob.particles.len() + 1);
    let mut uvs = Vec::with_capacity(blob.particles.len() + 1);
    positions.push([center.x, center.y, 0.0]);
    uvs.push([0.5, 0.5]);
    for particle in &blob.particles {
        positions.push([particle.position.x, particle.position.y, 0.0]);
        let local = (particle.position - center) / (blob.rest_radius * 2.0);
        uvs.push([0.5 + local.x, 0.5 + local.y]);
    }

    let count = blob.particles.len() as u32;
    let mut indices = Vec::with_capacity(blob.particles.len() * 3);
    for index in 0..count {
        indices.extend_from_slice(&[0, index + 1, (index + 1) % count + 1]);
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
}

pub(super) fn draw_world(
    mut gizmos: Gizmos,
    blobs: Res<BlobWorld>,
    vitality_world: Res<VitalityWorld>,
    level: Res<Level>,
) {
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

    for active_blob in &blobs.active {
        let blob = &active_blob.body;
        let vitality = vitality_world.get(active_blob.id);
        let color = if vitality.is_alive() {
            blob_family_color(active_blob.parent_id)
        } else {
            Color::srgba(0.48, 0.52, 0.54, 0.96)
        };
        let outline = blob.particles.iter().map(|particle| particle.position);
        gizmos.lineloop_2d(outline, color);
        let center = blob.center();
        let size_scale = blob.size_scale();
        for particle in &blob.particles {
            gizmos.line_2d(
                center,
                particle.position,
                Color::srgba(0.12, 0.55, 0.48, 0.22),
            );
        }
        if vitality.is_alive() {
            gizmos.circle_2d(center, 9.0 * size_scale, Color::srgb(0.72, 0.42, 0.95));
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
