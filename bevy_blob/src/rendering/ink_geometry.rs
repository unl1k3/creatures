//! Ink-textured visual geometry for authored platforms and fixtures.

use super::*;

/// Builds the visible collision silhouettes from the level's authored
/// platforms and convex fixtures. It never creates or changes physics bodies.
pub(super) fn spawn_ink_level_geometry(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    level: &Level,
    scenario: u8,
) {
    // Platform geometry is shared by every level, so the paper-and-ink tile
    // is shared too: laboratory and regression scenarios stay visually
    // comparable with the playable sewer scene.
    let brick_texture = asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                // Ink lines are intentionally high contrast. Nearest sampling
                // keeps a line black rather than blending it to a flickering
                // grey while the camera follows the blob.
                mag_filter: ImageFilterMode::Nearest,
                min_filter: ImageFilterMode::Nearest,
                ..default()
            });
        })
        .load("levels/sewer_01/art/ink/platform-bricks.png");
    for (platform_index, platform) in level.platforms.iter().enumerate() {
        spawn_ink_platform(
            commands,
            meshes,
            materials,
            scenario,
            platform_index,
            platform,
            level.ice_platforms.contains(&platform_index),
            level.glue_platforms.contains(&platform_index),
            &brick_texture,
            &level.lights,
            level
                .counterbalances
                .iter()
                .any(|balance| {
                    balance.gate_platform == platform_index
                        || balance.plate_platform == platform_index
                })
                .then_some(platform_index),
        );
    }
    let fixture_material = materials.add(ColorMaterial {
        texture: Some(brick_texture.clone()),
        ..default()
    });
    for fixture in &level.fixtures {
        if fixture.len() < 3 {
            continue;
        }
        // The physics uses these authored vertices. Grow only the visible
        // contour by the blob's contact skin so the membrane meets the drawn
        // surface without changing Avian's stable collision geometry.
        let visual_fixture = offset_convex_polygon(fixture, PLATFORM_VISUAL_CONTACT_OFFSET);
        let positions = visual_fixture
            .iter()
            .map(|point| [point.x, point.y, 0.0])
            .collect::<Vec<_>>();
        // Fixture vertices are already in world space. Their UV coordinates
        // therefore share the same global origin as every rectangular slab.
        let texture_coordinates = visual_fixture
            .iter()
            .map(|point| {
                [
                    point.x / BRICK_TILE_WORLD_WIDTH,
                    point.y / BRICK_TILE_WORLD_HEIGHT,
                ]
            })
            .collect::<Vec<_>>();
        let vertex_colors = visual_fixture
            .iter()
            .map(|point| scenery_vertex_light(*point, &level.lights))
            .collect::<Vec<_>>();
        let mut indices = Vec::with_capacity((fixture.len() - 2) * 3);
        for index in 1..fixture.len() - 1 {
            indices.extend_from_slice(&[0, index as u32, index as u32 + 1]);
        }
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, texture_coordinates);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vertex_colors);
        mesh.insert_indices(Indices::U32(indices));
        commands.spawn((
            InkPreviewShape { scenario },
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(fixture_material.clone()),
            Transform::from_xyz(0.0, 0.0, FIXTURE_TEXTURE_DEPTH),
        ));
    }
}

/// Expands the visible contour without changing the collision polygon. This
/// hides the deliberate contact clearance used by the blob solver.
fn offset_convex_polygon(vertices: &[Vec2], distance: f32) -> Vec<Vec2> {
    if vertices.len() < 3 || distance <= 0.0 {
        return vertices.to_vec();
    }
    let signed_area = vertices
        .iter()
        .copied()
        .zip(vertices.iter().copied().cycle().skip(1))
        .take(vertices.len())
        .map(|(first, second)| first.perp_dot(second))
        .sum::<f32>();
    let orientation = signed_area.signum();
    if orientation == 0.0 {
        return vertices.to_vec();
    }

    (0..vertices.len())
        .map(|index| {
            let previous = vertices[(index + vertices.len() - 1) % vertices.len()];
            let current = vertices[index];
            let next = vertices[(index + 1) % vertices.len()];
            let previous_outward = -(current - previous).perp().normalize_or_zero() * orientation;
            let next_outward = -(next - current).perp().normalize_or_zero() * orientation;
            let bisector = (previous_outward + next_outward).normalize_or(previous_outward);
            let alignment = bisector.dot(previous_outward).abs().max(0.35);
            current + bisector * (distance / alignment).min(distance * 2.5)
        })
        .collect()
}

fn spawn_ink_platform(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    scenario: u8,
    platform_index: usize,
    platform: &Platform,
    is_ice: bool,
    is_glue: bool,
    brick_texture: &Handle<Image>,
    lights: &[LightDefinition],
    counterbalance_platform: Option<usize>,
) {
    // Level 1's ordinary horizontal platforms are 28 world units tall. The
    // artwork's 40-pixel tile is calibrated to their visible inner height so
    // it displays exactly its authored two rows there. All other rectangles
    // reuse this fixed scale instead of stretching the drawing.
    // The contact solver retains a small gap around the physical rectangle.
    // Grow only the artwork by that documented skin to hide the numerical
    // clearance while keeping the Avian collider untouched.
    let visual_center = platform.center;
    let visual_half_size = platform.half_size + Vec2::splat(PLATFORM_VISUAL_CONTACT_OFFSET);
    let size = visual_half_size * 2.0;
    // Repeat the texture in UV space over a mesh that is exactly the size of
    // the rectangle. Mesh bounds clip incomplete edge bricks rather than
    // compressing them, and a thicker structure naturally exposes more rows.
    let brick_world_size = Vec2::new(BRICK_TILE_WORLD_WIDTH, BRICK_TILE_WORLD_HEIGHT);
    // `Rectangle` uses image-style UVs: V grows downward while world Y grows
    // upward. Account for that inversion here; otherwise every rectangle gets
    // a different vertical brick phase despite sharing world coordinates.
    let texture_scale = Vec2::new(size.x / brick_world_size.x, -size.y / brick_world_size.y);
    let visual_min = visual_center - size * 0.5;
    let visual_max = visual_center + size * 0.5;
    let texture_origin = Vec2::new(
        visual_min.x / brick_world_size.x,
        visual_max.y / brick_world_size.y,
    );
    let texture_material = materials.add(ColorMaterial {
        texture: Some(brick_texture.clone()),
        color: if is_ice {
            game_palette::color(game_palette::ICE_SURFACE)
        } else if is_glue {
            game_palette::color(game_palette::GLUE_SURFACE)
        } else {
            Color::WHITE
        },
        // Anchoring UV zero in world space keeps the pattern continuous where
        // two independently-authored structures meet or overlap.
        uv_transform: Affine2::from_scale_angle_translation(texture_scale, 0.0, texture_origin),
        ..default()
    });
    let mesh = lit_rectangle_mesh(size, visual_center, lights);
    let mut fill = commands.spawn((
        InkPreviewShape { scenario },
        Mesh2d(meshes.add(mesh)),
        MeshMaterial2d(texture_material),
        Transform::from_translation(
            visual_center
                .extend(PLATFORM_TEXTURE_DEPTH + platform_index as f32 * PLATFORM_DEPTH_STEP),
        ),
    ));
    if let Some(platform_index) = counterbalance_platform {
        fill.insert(CounterbalanceVisual { platform_index });
    }
}

/// Rectangle mesh with a light sample at each corner. Texture coordinates stay
/// in the regular 0..1 range because the material owns the shared world-space
/// brick phase through its UV transform.
fn lit_rectangle_mesh(size: Vec2, center: Vec2, lights: &[LightDefinition]) -> Mesh {
    let half = size * 0.5;
    let local_positions = [
        Vec2::new(-half.x, -half.y),
        Vec2::new(half.x, -half.y),
        Vec2::new(half.x, half.y),
        Vec2::new(-half.x, half.y),
    ];
    let positions = local_positions
        .iter()
        .map(|point| [point.x, point.y, 0.0])
        .collect::<Vec<_>>();
    let colors = local_positions
        .iter()
        .map(|point| scenery_vertex_light(center + *point, lights))
        .collect::<Vec<_>>();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    mesh
}
