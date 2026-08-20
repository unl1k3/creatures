use crate::blob::Platform;
use bevy::prelude::Vec2;
use serde::Deserialize;
use std::{error::Error, fmt};

pub(super) const LEVEL_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub(super) struct ParsedLevel {
    pub(super) name: String,
    pub(super) size: Vec2,
    pub(super) center: Vec2,
    pub(super) spawn: Vec2,
    pub(super) platforms: Vec<Platform>,
    pub(super) fixtures: Vec<Vec<Vec2>>,
    pub(super) route: Vec<Vec2>,
    pub(super) visual_layers: Vec<VisualLayer>,
    pub(super) nutrients: Vec<NutrientDefinition>,
    pub(super) lights: Vec<LightDefinition>,
    pub(super) expulsion_points: Vec<ExpulsionPointDefinition>,
    pub(super) hazards: Vec<HazardDefinition>,
    pub(super) decorations: Vec<VisualLayer>,
}

#[derive(Clone, Debug)]
pub(super) struct VisualLayer {
    pub(super) image: String,
    pub(super) position: Vec2,
    pub(super) size: Vec2,
    pub(super) depth: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NutrientDefinition {
    pub(super) position: Vec2,
    pub(super) radius: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct LightDefinition {
    pub(super) position: Vec2,
    pub(super) color: [f32; 3],
    pub(super) radius: f32,
    pub(super) intensity: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ExpulsionPointDefinition {
    pub(super) position: Vec2,
    pub(super) direction: Vec2,
    pub(super) strength: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct HazardDefinition {
    pub(super) position: Vec2,
    pub(super) size: Vec2,
    pub(super) damage_per_second: f32,
}

#[derive(Debug)]
pub(super) struct LevelFormatError(String);

impl fmt::Display for LevelFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for LevelFormatError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LevelDocument {
    version: u32,
    name: String,
    size: Point,
    #[serde(default)]
    center: Point,
    spawn: Point,
    #[serde(default)]
    colliders: Vec<ColliderDocument>,
    #[serde(default)]
    route: Vec<Point>,
    #[serde(default)]
    visual_layers: Vec<VisualLayerDocument>,
    #[serde(default)]
    nutrients: Vec<NutrientDocument>,
    #[serde(default)]
    lights: Vec<LightDocument>,
    #[serde(default)]
    expulsion_points: Vec<ExpulsionPointDocument>,
    #[serde(default)]
    hazards: Vec<HazardDocument>,
    #[serde(default)]
    decorations: Vec<VisualLayerDocument>,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Point {
    x: f32,
    y: f32,
}

impl From<Point> for Vec2 {
    fn from(point: Point) -> Self {
        Self::new(point.x, point.y)
    }
}

#[derive(Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case", deny_unknown_fields)]
enum ColliderDocument {
    Rectangle {
        id: String,
        position: Point,
        size: Point,
    },
    Polygon {
        id: String,
        points: Vec<Point>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VisualLayerDocument {
    image: String,
    position: Point,
    size: Point,
    depth: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NutrientDocument {
    position: Point,
    radius: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LightDocument {
    position: Point,
    color: [f32; 3],
    radius: f32,
    intensity: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpulsionPointDocument {
    position: Point,
    direction: Point,
    strength: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HazardDocument {
    position: Point,
    size: Point,
    damage_per_second: f32,
}

pub(super) fn parse_level(source: &str) -> Result<ParsedLevel, LevelFormatError> {
    let document: LevelDocument = serde_json::from_str(source)
        .map_err(|error| LevelFormatError(format!("invalid level JSON: {error}")))?;
    if document.version != LEVEL_FORMAT_VERSION {
        return Err(LevelFormatError(format!(
            "unsupported level version {}; expected {LEVEL_FORMAT_VERSION}",
            document.version
        )));
    }
    validate_text("level name", &document.name)?;
    let size = positive_size("level size", document.size)?;
    let center = finite_point("level center", document.center)?;
    let spawn = finite_point("spawn", document.spawn)?;
    let mut platforms = Vec::new();
    let mut fixtures = Vec::new();
    for collider in document.colliders {
        match collider {
            ColliderDocument::Rectangle { id, position, size } => {
                validate_text("collider id", &id)?;
                platforms.push(Platform {
                    center: finite_point(&format!("collider '{id}' position"), position)?,
                    half_size: positive_size(&format!("collider '{id}' size"), size)? * 0.5,
                });
            }
            ColliderDocument::Polygon { id, points } => {
                validate_text("collider id", &id)?;
                if points.len() < 3 {
                    return Err(LevelFormatError(format!(
                        "polygon collider '{id}' needs at least three points"
                    )));
                }
                fixtures.push(
                    points
                        .into_iter()
                        .enumerate()
                        .map(|(index, point)| {
                            finite_point(&format!("collider '{id}' point {index}"), point)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
        }
    }
    if platforms.is_empty() && fixtures.is_empty() {
        return Err(LevelFormatError(
            "a level needs at least one collider".into(),
        ));
    }
    let route = document
        .route
        .into_iter()
        .enumerate()
        .map(|(index, point)| finite_point(&format!("route point {index}"), point))
        .collect::<Result<Vec<_>, _>>()?;
    let visual_layers = document
        .visual_layers
        .into_iter()
        .enumerate()
        .map(|(index, layer)| {
            validate_text(&format!("visual layer {index} image"), &layer.image)?;
            Ok(VisualLayer {
                image: layer.image,
                position: finite_point(&format!("visual layer {index} position"), layer.position)?,
                size: positive_size(&format!("visual layer {index} size"), layer.size)?,
                depth: finite_number(&format!("visual layer {index} depth"), layer.depth)?,
            })
        })
        .collect::<Result<Vec<_>, LevelFormatError>>()?;
    let decorations = parse_visual_layers(document.decorations, "decoration")?;
    let nutrients = document
        .nutrients
        .into_iter()
        .enumerate()
        .map(|(index, nutrient)| {
            Ok(NutrientDefinition {
                position: finite_point(&format!("nutrient {index} position"), nutrient.position)?,
                radius: positive_number(&format!("nutrient {index} radius"), nutrient.radius)?,
            })
        })
        .collect::<Result<Vec<_>, LevelFormatError>>()?;
    let lights = document
        .lights
        .into_iter()
        .enumerate()
        .map(|(index, light)| {
            if light
                .color
                .iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
            {
                return Err(LevelFormatError(format!(
                    "light {index} color channels must be between 0 and 1"
                )));
            }
            Ok(LightDefinition {
                position: finite_point(&format!("light {index} position"), light.position)?,
                color: light.color,
                radius: positive_number(&format!("light {index} radius"), light.radius)?,
                intensity: positive_number(&format!("light {index} intensity"), light.intensity)?,
            })
        })
        .collect::<Result<Vec<_>, LevelFormatError>>()?;
    let expulsion_points = document
        .expulsion_points
        .into_iter()
        .enumerate()
        .map(|(index, point)| {
            let direction = finite_point(
                &format!("expulsion point {index} direction"),
                point.direction,
            )?;
            if direction.length_squared() < 0.0001 {
                return Err(LevelFormatError(format!(
                    "expulsion point {index} direction cannot be zero"
                )));
            }
            Ok(ExpulsionPointDefinition {
                position: finite_point(
                    &format!("expulsion point {index} position"),
                    point.position,
                )?,
                direction: direction.normalize(),
                strength: positive_number(
                    &format!("expulsion point {index} strength"),
                    point.strength,
                )?,
            })
        })
        .collect::<Result<Vec<_>, LevelFormatError>>()?;
    let hazards = document
        .hazards
        .into_iter()
        .enumerate()
        .map(|(index, hazard)| {
            Ok(HazardDefinition {
                position: finite_point(&format!("hazard {index} position"), hazard.position)?,
                size: positive_size(&format!("hazard {index} size"), hazard.size)?,
                damage_per_second: positive_number(
                    &format!("hazard {index} damage_per_second"),
                    hazard.damage_per_second,
                )?,
            })
        })
        .collect::<Result<Vec<_>, LevelFormatError>>()?;

    Ok(ParsedLevel {
        name: document.name,
        size,
        center,
        spawn,
        platforms,
        fixtures,
        route,
        visual_layers,
        nutrients,
        lights,
        expulsion_points,
        hazards,
        decorations,
    })
}

fn parse_visual_layers(
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
            })
        })
        .collect()
}

fn validate_text(label: &str, value: &str) -> Result<(), LevelFormatError> {
    if value.trim().is_empty() {
        Err(LevelFormatError(format!("{label} cannot be empty")))
    } else {
        Ok(())
    }
}

fn finite_number(label: &str, value: f32) -> Result<f32, LevelFormatError> {
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| LevelFormatError(format!("{label} must be finite")))
}

fn positive_number(label: &str, value: f32) -> Result<f32, LevelFormatError> {
    let value = finite_number(label, value)?;
    if value <= 0.0 {
        Err(LevelFormatError(format!("{label} must be positive")))
    } else {
        Ok(value)
    }
}

fn finite_point(label: &str, point: Point) -> Result<Vec2, LevelFormatError> {
    Ok(Vec2::new(
        finite_number(&format!("{label}.x"), point.x)?,
        finite_number(&format!("{label}.y"), point.y)?,
    ))
}

fn positive_size(label: &str, point: Point) -> Result<Vec2, LevelFormatError> {
    let size = finite_point(label, point)?;
    if size.x <= 0.0 || size.y <= 0.0 {
        Err(LevelFormatError(format!(
            "{label} must have positive dimensions"
        )))
    } else {
        Ok(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rectangles_polygons_and_visual_layers() {
        let level = parse_level(
            r#"{
                "version": 1,
                "name": "Test",
                "size": { "x": 1000, "y": 800 },
                "spawn": { "x": 10, "y": 20 },
                "colliders": [
                    { "shape": "rectangle", "id": "floor", "position": { "x": 0, "y": -20 }, "size": { "x": 200, "y": 20 } },
                    { "shape": "polygon", "id": "ramp", "points": [{ "x": 0, "y": 0 }, { "x": 50, "y": 0 }, { "x": 50, "y": 30 }] }
                ],
                "visual_layers": [{ "image": "levels/test/background.png", "position": { "x": 0, "y": 0 }, "size": { "x": 1000, "y": 800 }, "depth": -20 }]
            }"#,
        )
        .expect("valid level");

        assert_eq!(level.platforms.len(), 1);
        assert_eq!(level.fixtures.len(), 1);
        assert_eq!(level.visual_layers.len(), 1);
    }

    #[test]
    fn rejects_unknown_versions_and_invalid_geometry() {
        let invalid = r#"{
            "version": 2,
            "name": "Broken",
            "size": { "x": 0, "y": 800 },
            "spawn": { "x": 0, "y": 0 },
            "colliders": []
        }"#;
        assert!(parse_level(invalid).is_err());
    }
}
