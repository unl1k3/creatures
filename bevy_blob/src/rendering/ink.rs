//! Ink-scene presentation controls: artwork visibility, atmosphere and
//! counterbalance visual synchronization.

use super::*;

type ForegroundVisibilityQuery<'w, 's> =
    Query<'w, 's, &'static mut Visibility, Or<(With<ForegroundArtwork>, With<InkForeground>)>>;

pub(crate) fn sync_counterbalance_visuals(
    level: Res<Level>,
    mut visuals: Query<(&CounterbalanceVisual, &mut Transform)>,
) {
    for (visual, mut transform) in &mut visuals {
        let Some(platform) = level.platforms.get(visual.platform_index) else {
            continue;
        };
        transform.translation.x = platform.center.x;
        transform.translation.y = platform.center.y;
    }
}

pub(crate) fn toggle_ink_style(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut ink_style: ResMut<InkStylePreview>,
    mut clear_color: ResMut<ClearColor>,
    mut artwork: Query<&mut Visibility, With<LevelArtwork>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyM) {
        return;
    }
    ink_style.enabled = !ink_style.enabled;
    clear_color.0 = if ink_style.enabled {
        game_palette::color(game_palette::IVORY)
    } else {
        game_palette::color(game_palette::NIGHT)
    };
    for mut visibility in &mut artwork {
        *visibility = if ink_style.enabled {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

/// G hides only decorative foreground artwork for collision testing.
pub(crate) fn toggle_foreground(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut foreground: ForegroundVisibilityQuery,
) {
    if !keyboard.just_pressed(KeyCode::KeyG) {
        return;
    }
    for mut visibility in &mut foreground {
        *visibility = match *visibility {
            Visibility::Hidden => Visibility::Inherited,
            _ => Visibility::Hidden,
        };
    }
}

/// Tints painted scenery by camera height without changing gameplay meshes.
pub(crate) fn sync_ink_atmosphere(
    ink_style: Res<InkStylePreview>,
    scenario: Res<TestScenario>,
    level: Res<Level>,
    camera: Single<&Transform, With<GameCamera>>,
    mut layers: Query<(&InkAtmosphereLayer, &mut Sprite)>,
) {
    if !ink_style.enabled || !supports_ink_background(scenario.0) {
        return;
    }
    let bottom = level.center().y - level.size().y * 0.5;
    let ascent = ((camera.translation.y - bottom) / level.size().y).clamp(0.0, 1.0);
    let ascent = ascent * ascent * (3.0 - 2.0 * ascent);
    for (layer, mut sprite) in &mut layers {
        sprite.color = ink_atmosphere_tint(ascent, layer.foreground);
    }
}

pub(super) fn ink_atmosphere_tint(ascent: f32, foreground: bool) -> Color {
    let (lower, upper) = if foreground {
        (Vec3::new(0.57, 0.58, 0.52), Vec3::new(0.47, 0.55, 0.59))
    } else {
        (Vec3::new(0.52, 0.55, 0.50), Vec3::new(0.42, 0.51, 0.57))
    };
    let tint = lower.lerp(upper, ascent);
    Color::srgb(tint.x, tint.y, tint.z)
}
