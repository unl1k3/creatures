//! Lifecycle synchronization for blob render entities.

use super::*;
use bevy::ecs::system::SystemParam;
use std::collections::HashSet;

type BlobBodies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut BlobMesh,
        &'static Mesh2d,
        &'static MeshMaterial2d<ColorMaterial>,
    ),
    (With<BlobMesh>, Without<BlobOutlineMesh>),
>;

type BlobOutlines<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut BlobOutlineMesh,
        &'static Mesh2d,
        &'static MeshMaterial2d<ColorMaterial>,
    ),
    (With<BlobOutlineMesh>, Without<BlobMesh>),
>;

type BlobVacuoles<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static BlobVacuoleMesh, &'static Mesh2d),
    (
        With<BlobVacuoleMesh>,
        Without<BlobMesh>,
        Without<BlobOutlineMesh>,
    ),
>;

/// Read-only biological state and mutable render assets for blob visuals.
#[derive(SystemParam)]
pub(crate) struct BlobRenderParams<'w> {
    blobs: Res<'w, BlobWorld>,
    level: Res<'w, Level>,
    time: Res<'w, Time>,
    vitality: Res<'w, VitalityWorld>,
    nutrition: Res<'w, NutritionWorld>,
    shields: Res<'w, ShieldWorld>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<ColorMaterial>>,
}

/// The three independent entity layers composing each rendered blob.
#[derive(SystemParam)]
pub(crate) struct BlobRenderEntities<'w, 's> {
    bodies: BlobBodies<'w, 's>,
    outlines: BlobOutlines<'w, 's>,
    vacuoles: BlobVacuoles<'w, 's>,
}

pub(crate) fn sync_blob_meshes(
    mut commands: Commands,
    params: BlobRenderParams,
    entities: BlobRenderEntities,
) {
    let BlobRenderParams {
        blobs,
        level,
        time,
        vitality,
        nutrition,
        shields,
        mut meshes,
        mut materials,
    } = params;
    let BlobRenderEntities {
        mut bodies,
        mut outlines,
        mut vacuoles,
    } = entities;
    let active_ids = blobs
        .active
        .iter()
        .map(|blob| blob.id)
        .collect::<HashSet<_>>();
    let mut rendered_ids = HashSet::new();

    for (entity, mut marker, mesh_handle, material_handle) in &mut bodies {
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
        let vitality = vitality.get(active_blob.id);
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
        let vitality = vitality.get(active_blob.id);
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
        let vitality = vitality.get(active_blob.id);
        if let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) {
            update_blob_outline_mesh(
                &mut mesh,
                MembraneRenderContext {
                    blob: &active_blob.body,
                    load: nutrition.internal_load(active_blob.id),
                    selected,
                    parent_id: active_blob.parent_id,
                    vitality,
                    lights: &level.lights,
                    blob_id: active_blob.id,
                    shield_extension: shields.extension(active_blob.id),
                    shield_energy: shields.energy(active_blob.id),
                    platforms: &level.platforms,
                },
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
        let vitality = vitality.get(active_blob.id);
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        update_blob_outline_mesh(
            &mut mesh,
            MembraneRenderContext {
                blob: &active_blob.body,
                load: nutrition.internal_load(active_blob.id),
                selected,
                parent_id: active_blob.parent_id,
                vitality,
                lights: &level.lights,
                blob_id: active_blob.id,
                shield_extension: shields.extension(active_blob.id),
                shield_energy: shields.energy(active_blob.id),
                platforms: &level.platforms,
            },
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
                vitality.get(active_blob.id).is_alive(),
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
            vitality.get(active_blob.id).is_alive(),
            &level.lights,
        );
        commands.spawn((
            BlobVacuoleMesh {
                blob_id: active_blob.id,
            },
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(ColorMaterial::from(game_palette::color(
                game_palette::TRANSLUCENT_WHITE,
            )))),
            Transform::from_xyz(0.0, 0.0, -0.06),
        ));
    }
}
