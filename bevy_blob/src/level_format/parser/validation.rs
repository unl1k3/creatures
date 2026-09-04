//! Reusable validation and conversion helpers for level fields.

use super::*;

pub(super) fn parse_bubble_settings(
    area_index: usize,
    settings: BubbleSettingsDocument,
) -> Result<BubbleSettingsDefinition, LevelFormatError> {
    let interval = positive_range(
        &format!("wastewater area {area_index} bubble interval"),
        settings.interval,
    )?;
    let radius = positive_range(
        &format!("wastewater area {area_index} bubble radius"),
        settings.radius,
    )?;
    let rise_speed = positive_range(
        &format!("wastewater area {area_index} bubble rise_speed"),
        settings.rise_speed,
    )?;
    if settings.max_active == 0 {
        return Err(LevelFormatError(format!(
            "wastewater area {area_index} bubble max_active must be positive"
        )));
    }
    Ok(BubbleSettingsDefinition {
        interval,
        radius,
        rise_speed,
        max_active: settings.max_active,
    })
}

pub(super) fn positive_range(label: &str, range: [f32; 2]) -> Result<[f32; 2], LevelFormatError> {
    let minimum = positive_number(&format!("{label} minimum"), range[0])?;
    let maximum = positive_number(&format!("{label} maximum"), range[1])?;
    if maximum < minimum {
        Err(LevelFormatError(format!(
            "{label} maximum cannot be smaller than minimum"
        )))
    } else {
        Ok([minimum, maximum])
    }
}

pub(super) fn parse_visual_layers(
    layers: Vec<VisualLayerDocument>,
    label: &str,
) -> Result<Vec<VisualLayer>, LevelFormatError> {
    layers
        .into_iter()
        .enumerate()
        .map(|(index, layer)| {
            validate_text(&format!("{label} {index} image"), &layer.image)?;
            Ok(VisualLayer {
                image: layer.image,
                position: finite_point(&format!("{label} {index} position"), layer.position)?,
                size: positive_size(&format!("{label} {index} size"), layer.size)?,
                depth: finite_number(&format!("{label} {index} depth"), layer.depth)?,
                parallax: finite_number(&format!("{label} {index} parallax"), layer.parallax)?,
            })
        })
        .collect()
}

pub(super) fn validate_text(label: &str, value: &str) -> Result<(), LevelFormatError> {
    if value.trim().is_empty() {
        Err(LevelFormatError(format!("{label} cannot be empty")))
    } else {
        Ok(())
    }
}

pub(super) fn finite_number(label: &str, value: f32) -> Result<f32, LevelFormatError> {
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| LevelFormatError(format!("{label} must be finite")))
}

pub(super) fn positive_number(label: &str, value: f32) -> Result<f32, LevelFormatError> {
    let value = finite_number(label, value)?;
    if value <= 0.0 {
        Err(LevelFormatError(format!("{label} must be positive")))
    } else {
        Ok(value)
    }
}

pub(super) fn finite_point(label: &str, point: Point) -> Result<Vec2, LevelFormatError> {
    Ok(Vec2::new(
        finite_number(&format!("{label}.x"), point.x)?,
        finite_number(&format!("{label}.y"), point.y)?,
    ))
}

pub(super) fn positive_size(label: &str, point: Point) -> Result<Vec2, LevelFormatError> {
    let size = finite_point(label, point)?;
    if size.x <= 0.0 || size.y <= 0.0 {
        Err(LevelFormatError(format!(
            "{label} must have positive dimensions"
        )))
    } else {
        Ok(size)
    }
}
