//! Authored level layers and their parallax transform.

use super::*;

pub(crate) fn spawn_level_artwork(
    commands: &mut Commands,
    asset_server: Option<&AssetServer>,
    level: &Level,
) {
    let Some(asset_server) = asset_server else {
        return;
    };
    for layer in &level.visual_layers {
        spawn_artwork_layer(commands, asset_server, layer, false);
    }
    for layer in &level.decorations {
        spawn_artwork_layer(commands, asset_server, layer, true);
    }
}

fn spawn_artwork_layer(
    commands: &mut Commands,
    asset_server: &AssetServer,
    layer: &VisualLayer,
    foreground: bool,
) {
    let mut entity = commands.spawn((
        LevelArtwork,
        Sprite {
            image: asset_server.load(layer.image.clone()),
            custom_size: Some(layer.size),
            ..default()
        },
        Transform::from_translation(layer.position.extend(layer.depth)),
        ParallaxLayer::new(layer.position.extend(layer.depth), layer.parallax),
    ));
    if foreground {
        entity.insert(ForegroundArtwork);
    }
}

/// Applies parallax after the camera follows the active blob. A factor of one
/// preserves exact authored alignment.
pub(crate) fn update_parallax_layers(
    camera: Single<&Transform, With<GameCamera>>,
    mut layers: Query<(&ParallaxLayer, &mut Transform), Without<GameCamera>>,
) {
    let camera_position = camera.translation.truncate();
    for (layer, mut transform) in &mut layers {
        transform.translation = layer.origin + (camera_position * (1.0 - layer.factor)).extend(0.0);
    }
}
