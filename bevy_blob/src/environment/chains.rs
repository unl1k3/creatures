//! Physical hanging chains and their ink presentation.

use super::{GameLayer, Level};
use crate::BlobSoundEvent;
use crate::BlobWorld;
use crate::level_format::LightDefinition;
use crate::palette as game_palette;
use crate::rendering::light_dynamic_rgba;
use avian2d::prelude::{
    AngularDamping, Collider, CollisionLayers, JointCollisionDisabled, LinearDamping,
    LinearVelocity, MassPropertiesBundle, RevoluteJoint, RigidBody,
};
use bevy::{
    asset::RenderAssetUsages,
    prelude::MeshMaterial2d,
    prelude::*,
    render::{mesh::Indices, render_resource::PrimitiveTopology},
};

#[derive(Component)]
pub(crate) struct LevelChain;

#[derive(Component)]
pub(crate) struct ChainAnchor {
    chain_index: usize,
}

#[derive(Component)]
pub(crate) struct ChainLink {
    radius: f32,
    chain_index: usize,
    link_index: usize,
}

/// Each physical chain element owns a material so its ink darkens and warms
/// independently while swinging through the authored lantern pools.
#[derive(Component)]
pub(crate) struct ChainLightMaterial(Handle<ColorMaterial>);

pub(crate) fn spawn_level_chains(
    commands: &mut Commands,
    level: &Level,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    for (chain_index, chain) in level.chains.iter().enumerate() {
        let mesh = meshes.add(ellipse_ring_mesh(
            Vec2::new(chain.link_radius * 0.56, chain.link_radius * 0.95),
            Vec2::new(chain.link_radius * 0.88, chain.link_radius * 1.28),
            16,
        ));
        let anchor_material = materials.add(ColorMaterial::from(game_palette::color(
            light_dynamic_rgba(game_palette::INK, chain.anchor, &level.lights),
        )));
        let anchor = commands
            .spawn((
                Name::new(format!("Chain anchor: {}", chain.id)),
                LevelChain,
                ChainAnchor { chain_index },
                RigidBody::Kinematic,
                Mesh2d(meshes.add(ring_mesh(1.5, 5.5, 12))),
                MeshMaterial2d(anchor_material.clone()),
                ChainLightMaterial(anchor_material),
                Transform::from_translation(chain.anchor.extend(0.0)),
            ))
            .id();
        let mut previous = anchor;
        for link_index in 0..chain.links {
            let position = chain.anchor - Vec2::Y * chain.spacing * (link_index + 1) as f32;
            let material = materials.add(ColorMaterial::from(game_palette::color(
                light_dynamic_rgba(game_palette::INK, position, &level.lights),
            )));
            let link = commands
                .spawn((
                    Name::new(format!("Chain link {}: {link_index}", chain.id)),
                    LevelChain,
                    ChainLink {
                        radius: chain.link_radius,
                        chain_index,
                        link_index,
                    },
                    RigidBody::Dynamic,
                    Collider::circle(chain.link_radius),
                    MassPropertiesBundle::from_shape(&Circle::new(chain.link_radius), 0.7),
                    LinearDamping(1.1),
                    AngularDamping(1.8),
                    CollisionLayers::new(
                        [GameLayer::Projectile],
                        [GameLayer::Environment, GameLayer::Projectile],
                    ),
                    Mesh2d(mesh.clone()),
                    MeshMaterial2d(material.clone()),
                    ChainLightMaterial(material),
                    Transform::from_translation(position.extend(0.12)).with_rotation(
                        Quat::from_rotation_z(if link_index % 2 == 0 { 0.0 } else { 0.35 }),
                    ),
                ))
                .id();
            commands.spawn((
                LevelChain,
                RevoluteJoint::new(previous, link)
                    .with_local_anchor2(Vec2::Y * chain.spacing)
                    .with_point_compliance(0.000_01),
                JointCollisionDisabled,
            ));
            previous = link;
        }
    }
}

/// Updates chain ink from the current physical positions. The links are Avian
/// bodies, so their light must be sampled after the physics step rather than
/// from their JSON spawn coordinates.
pub(crate) fn sync_chain_lighting(
    level: Res<Level>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    chains: Query<(&Transform, &ChainLightMaterial)>,
) {
    for (transform, material_handle) in &chains {
        let Some(mut material) = materials.get_mut(&material_handle.0) else {
            continue;
        };
        material.color = game_palette::color(light_dynamic_rgba(
            game_palette::INK,
            transform.translation.truncate(),
            &level.lights,
        ));
    }
}

/// Thin bridge marks show the alternating links that are seen edge-on.
pub(crate) fn draw_level_chains(
    mut gizmos: Gizmos,
    level: Res<Level>,
    links: Query<(&Transform, &ChainLink)>,
    anchors: Query<(&Transform, &ChainAnchor)>,
) {
    let mut ordered = links
        .iter()
        .map(|(transform, link)| {
            (
                link.chain_index,
                link.link_index,
                transform.translation.truncate(),
            )
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(chain, link, _)| (*chain, *link));
    for pair in ordered.windows(2) {
        let [
            (first_chain, first_link, first),
            (second_chain, second_link, second),
        ] = pair
        else {
            continue;
        };
        if first_chain != second_chain || *second_link != *first_link + 1 {
            continue;
        }
        draw_chain_stroke(&mut gizmos, *first, *second, &level.lights);
    }
    for (anchor_transform, anchor) in &anchors {
        let anchor_position = anchor_transform.translation.truncate();
        if let Some((_, _, first_link)) = ordered.iter().find(|(chain_index, link_index, _)| {
            *chain_index == anchor.chain_index && *link_index == 0
        }) {
            draw_chain_stroke(&mut gizmos, anchor_position, *first_link, &level.lights);
        }
    }
}

fn draw_chain_stroke(gizmos: &mut Gizmos, start: Vec2, end: Vec2, lights: &[LightDefinition]) {
    let direction = (end - start).normalize_or(Vec2::NEG_Y);
    let normal = Vec2::new(-direction.y, direction.x);
    let start = start + direction * 4.0;
    let end = end - direction * 4.0;
    let ink = game_palette::color(light_dynamic_rgba(
        game_palette::INK,
        (start + end) * 0.5,
        lights,
    ));
    for offset in [-1.0, 0.0, 1.0] {
        gizmos.line_2d(start + normal * offset, end + normal * offset, ink);
    }
}

fn ring_mesh(inner_radius: f32, outer_radius: f32, segments: usize) -> Mesh {
    ellipse_ring_mesh(
        Vec2::splat(inner_radius),
        Vec2::splat(outer_radius),
        segments,
    )
}

fn ellipse_ring_mesh(inner_radius: Vec2, outer_radius: Vec2, segments: usize) -> Mesh {
    let segments = segments.max(3);
    let mut positions = Vec::with_capacity(segments * 2);
    for index in 0..segments {
        let angle = index as f32 / segments as f32 * std::f32::consts::TAU;
        positions.push([
            outer_radius.x * angle.cos(),
            outer_radius.y * angle.sin(),
            0.0,
        ]);
        positions.push([
            inner_radius.x * angle.cos(),
            inner_radius.y * angle.sin(),
            0.0,
        ]);
    }
    let mut indices = Vec::with_capacity(segments * 6);
    for index in 0..segments {
        let next = (index + 1) % segments;
        let outer = (index * 2) as u32;
        let inner = outer + 1;
        let next_outer = (next * 2) as u32;
        let next_inner = next_outer + 1;
        indices.extend_from_slice(&[outer, next_outer, inner, inner, next_outer, next_inner]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Couples the blob's custom membrane solver to Avian chain links. Each link
/// resolves only its closest membrane point, preventing a hanging chain from
/// behaving like a continuous rigid wall.
pub(crate) fn resolve_blob_chain_contacts(
    time: Res<Time<Fixed>>,
    mut blobs: ResMut<BlobWorld>,
    mut links: Query<(&Transform, &ChainLink, &mut LinearVelocity)>,
    mut sound_events: MessageWriter<BlobSoundEvent>,
) {
    for (transform, link, mut velocity) in &mut links {
        let link_position = transform.translation.truncate();
        for active_blob in &mut blobs.active {
            let blob_center = active_blob.body.center();
            let blob_velocity = active_blob
                .body
                .particles
                .iter()
                .map(|particle| particle.position - particle.previous)
                .sum::<Vec2>()
                / active_blob.body.particles.len().max(1) as f32;
            let average_radius = active_blob
                .body
                .particles
                .iter()
                .map(|particle| particle.position.distance(blob_center))
                .sum::<f32>()
                / active_blob.body.particles.len().max(1) as f32;
            let volume_radius = average_radius.max(active_blob.body.rest_radius * 0.70);
            let center_offset = link_position - blob_center;
            let center_distance = center_offset.length();
            // A link inside the body receives pressure from the blob volume,
            // rather than waiting until it reaches one membrane particle.
            if center_distance < volume_radius {
                let volume_normal = center_offset.normalize_or(Vec2::Y);
                let depth = volume_radius - center_distance;
                // Transfer both the body's travel and its volumetric pressure.
                // This is intentionally stronger than a membrane-only hit:
                // a chain should visibly yield when a blob presses through it.
                **velocity += blob_velocity * 0.48 + volume_normal * (depth * 5.2).min(220.0);
            }
            let skin = 2.0 * active_blob.body.size_scale();
            let minimum_distance = link.radius + skin;
            let Some((particle_index, distance)) = active_blob
                .body
                .particles
                .iter()
                .enumerate()
                .map(|(index, particle)| (index, particle.position.distance(link_position)))
                .min_by(|(_, first), (_, second)| first.total_cmp(second))
            else {
                continue;
            };
            if distance >= minimum_distance {
                continue;
            }
            let particle = &mut active_blob.body.particles[particle_index];
            let normal = (particle.position - link_position).normalize_or(Vec2::Y);
            let incoming = particle.position - particle.previous;
            let penetration = minimum_distance - distance;
            // A soft partial correction lets the membrane fold around a link.
            let correction = normal * penetration * 0.42;
            particle.position += correction;
            particle.previous += correction * 0.35;
            let impact = (-incoming.dot(normal)).max(0.0);
            **velocity -= normal * (impact * 0.55 + penetration * 5.0);
            let impact_speed = impact / time.delta_secs().max(0.000_001);
            if impact_speed >= 95.0 {
                sound_events.write(BlobSoundEvent::ChainImpact {
                    strength: (impact_speed / 420.0).clamp(0.0, 1.0),
                });
            }
        }
    }
}
