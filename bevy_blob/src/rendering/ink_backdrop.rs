//! Authored sewer backdrop, foreground, fog, and parallax decoration.

use super::*;

/// Scenario 0 is the startup instance of level 1; pressing 1 assigns it
/// explicitly while development tools are enabled.
pub(super) fn supports_ink_background(scenario: u8) -> bool {
    matches!(scenario, 0 | 1)
}

/// Spawns only the authored illustration layers. Collision shapes remain
/// independent and are created by the level systems.
pub(super) fn spawn_ink_backdrop(
    commands: &mut Commands,
    asset_server: &AssetServer,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    level: &Level,
    scenario: u8,
) {
    if !supports_ink_background(scenario) {
        return;
    }

    // Keep a single authored backdrop. Its slow parallax keeps it in view
    // through the initial vertical extension without visibly repeating or
    // stretching the illustration.
    const INK_SECTOR_HEIGHT: f32 = 1500.0;
    // The source foreground is split into two anchored bands. This preserves
    // the artwork's scale while leaving the extended middle of the level open.
    const INK_FOREGROUND_SOURCE_HEIGHT: f32 = 1150.0;
    const INK_FOREGROUND_BOTTOM_PIXELS: f32 = 330.0;
    const INK_FOREGROUND_TOP_WIDTH: f32 = 1940.0;
    const INK_FOREGROUND_TOP_HEIGHT_PIXELS: f32 = 810.0;
    const INK_FOREGROUND_BOTTOM_HEIGHT: f32 =
        INK_SECTOR_HEIGHT * INK_FOREGROUND_BOTTOM_PIXELS / INK_FOREGROUND_SOURCE_HEIGHT;
    let ink_foreground_top_height =
        INK_FOREGROUND_TOP_HEIGHT_PIXELS * level.size().x / INK_FOREGROUND_TOP_WIDTH;
    let bottom = level.center().y - level.size().y * 0.5;
    let background_origin = Vec3::new(level.center().x, bottom + 750.0, -20.0);
    commands.spawn((
        InkPreviewShape { scenario },
        Sprite {
            image: asset_server.load("levels/sewer_01/art/ink/background.png"),
            custom_size: Some(Vec2::new(level.size().x, INK_SECTOR_HEIGHT)),
            // The illustration supplies the room's darkness; lantern glows
            // below restore local warmth at authored light points.
            color: super::ink_atmosphere_tint(0.0, false),
            ..default()
        },
        InkAtmosphereLayer { foreground: false },
        Transform::from_translation(background_origin),
        ParallaxLayer::new(background_origin, 0.10),
    ));
    spawn_upper_sewer_atmosphere(commands, meshes, materials, scenario);
    let foreground_layers = [
        (
            "levels/sewer_01/art/ink/foreground-bottom.png",
            Vec2::new(level.size().x, INK_FOREGROUND_BOTTOM_HEIGHT),
            bottom + INK_FOREGROUND_BOTTOM_HEIGHT * 0.5,
        ),
        (
            "levels/sewer_01/art/ink/foreground-top-finished.png",
            Vec2::new(level.size().x, ink_foreground_top_height),
            bottom + level.size().y - ink_foreground_top_height * 0.5,
        ),
    ];

    for (image_path, foreground_size, y) in foreground_layers {
        commands.spawn((
            InkPreviewShape { scenario },
            InkForeground,
            Sprite {
                image: asset_server.load(image_path),
                custom_size: Some(foreground_size),
                color: super::ink_atmosphere_tint(0.0, true),
                ..default()
            },
            InkAtmosphereLayer { foreground: true },
            // Foreground pipes and debris are an occlusion layer: gameplay
            // structures remain visible through their central opening, while
            // overlapping portions correctly pass behind this artwork.
            Transform::from_translation(Vec3::new(level.center().x, y, 2.5)),
        ));
    }
}

pub(super) fn spawn_upper_sewer_atmosphere(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    scenario: u8,
) {
    const FOG_BANDS: &[(Vec2, f32, f32, f32)] = &[
        (Vec2::new(-210.0, 960.0), 900.0, 108.0, 0.17),
        (Vec2::new(250.0, 1_280.0), 1_060.0, 146.0, 0.25),
        (Vec2::new(-160.0, 1_650.0), 980.0, 122.0, 0.20),
        (Vec2::new(180.0, 2_035.0), 1_140.0, 168.0, 0.30),
    ];
    let fog_material = materials.add(ColorMaterial::default());
    for (index, (center, width, height, parallax)) in FOG_BANDS.iter().enumerate() {
        commands.spawn((
            InkPreviewShape { scenario },
            InkFogBand,
            Mesh2d(meshes.add(create_ink_fog_band(*center, *width, *height, index as f32))),
            MeshMaterial2d(fog_material.clone()),
            Transform::from_xyz(0.0, 0.0, -18.8),
            ParallaxLayer::new(Vec3::new(0.0, 0.0, -18.8), *parallax),
        ));
    }
    commands.spawn((
        InkPreviewShape { scenario },
        Mesh2d(meshes.add(create_upper_infrastructure_mesh())),
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        Transform::from_xyz(0.0, 0.0, -18.65),
        ParallaxLayer::new(Vec3::new(0.0, 0.0, -18.65), 0.22),
    ));
}

/// A soft three-row ribbon with an irregular ink-like edge. Vertex alpha lets
/// it dissolve at both edges instead of reading as a translucent rectangle.
fn create_ink_fog_band(center: Vec2, width: f32, height: f32, seed: f32) -> Mesh {
    const SEGMENTS: usize = 18;
    let mut positions = Vec::with_capacity((SEGMENTS + 1) * 3);
    let mut colors = Vec::with_capacity((SEGMENTS + 1) * 3);
    let mut indices = Vec::with_capacity(SEGMENTS * 12);
    for index in 0..=SEGMENTS {
        let t = index as f32 / SEGMENTS as f32;
        let x = center.x + (t - 0.5) * width;
        let uneven = (t * 15.7 + seed * 2.13).sin() * height * 0.10
            + (t * 29.1 + seed).cos() * height * 0.045;
        let fade = (std::f32::consts::PI * t).sin().powi(2);
        for (vertical, alpha) in [(0.5, 0.0), (0.0, 1.0), (-0.5, 0.0)] {
            positions.push([x, center.y + vertical * height + uneven, 0.0]);
            colors.push([
                game_palette::SEWER_FOG[0],
                game_palette::SEWER_FOG[1],
                game_palette::SEWER_FOG[2],
                game_palette::SEWER_FOG[3] * fade * alpha,
            ]);
        }
    }
    for index in 0..SEGMENTS {
        let base = (index * 3) as u32;
        let next = base + 3;
        indices.extend_from_slice(&[
            base,
            next,
            base + 1,
            base + 1,
            next,
            next + 1,
            base + 1,
            next + 1,
            base + 2,
            base + 2,
            next + 1,
            next + 2,
        ]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Distant pipes, clamps and grated passages provide new visual landmarks in
/// the upper shafts. They deliberately remain subtle and non-interactive.
fn create_upper_infrastructure_mesh() -> Mesh {
    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    let color = game_palette::DISTANT_INFRASTRUCTURE;
    let mut add_rect = |center: Vec2, size: Vec2, opacity: f32| {
        let first = positions.len() as u32;
        let half = size * 0.5;
        positions.extend_from_slice(&[
            [center.x - half.x, center.y - half.y, 0.0],
            [center.x + half.x, center.y - half.y, 0.0],
            [center.x + half.x, center.y + half.y, 0.0],
            [center.x - half.x, center.y + half.y, 0.0],
        ]);
        colors.extend(std::iter::repeat_n(
            [color[0], color[1], color[2], color[3] * opacity],
            4,
        ));
        indices.extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
    };

    // Tall service pipes with irregularly spaced clamps.
    for (x, bottom, height) in [(-605.0, 1_010.0, 1_150.0), (620.0, 1_180.0, 1_050.0)] {
        add_rect(
            Vec2::new(x, bottom + height * 0.5),
            Vec2::new(18.0, height),
            0.82,
        );
        for offset in [110.0, 330.0, 570.0, 820.0] {
            if offset < height {
                add_rect(Vec2::new(x, bottom + offset), Vec2::new(42.0, 13.0), 0.96);
            }
        }
    }
    // Cross pipes and narrow grate marks prevent the extended top room from
    // becoming visually empty while preserving its open playable silhouette.
    add_rect(Vec2::new(-330.0, 1_870.0), Vec2::new(430.0, 16.0), 0.70);
    add_rect(Vec2::new(360.0, 1_420.0), Vec2::new(360.0, 14.0), 0.60);
    for x in [-190.0, -172.0, -154.0, 480.0, 498.0, 516.0] {
        let y = if x < 0.0 { 2_120.0 } else { 1_690.0 };
        add_rect(Vec2::new(x, y), Vec2::new(5.0, 96.0), 0.72);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
