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
    pub(super) safety_bounds: Option<SafetyBoundsDefinition>,
    pub(super) spawn: Vec2,
    pub(super) platforms: Vec<Platform>,
    pub(super) fixtures: Vec<Vec<Vec2>>,
    pub(super) route: Vec<Vec2>,
    pub(super) visual_layers: Vec<VisualLayer>,
    pub(super) nutrients: Vec<NutrientDefinition>,
    pub(super) lights: Vec<LightDefinition>,
    pub(super) expulsion_points: Vec<ExpulsionPointDefinition>,
    pub(super) hazards: Vec<HazardDefinition>,
    pub(super) chains: Vec<ChainDefinition>,
    pub(super) decorations: Vec<VisualLayer>,
    pub(super) drop_emitters: Vec<DropEmitterDefinition>,
    pub(super) wastewater_areas: Vec<WastewaterAreaDefinition>,
    pub(super) counterbalances: Vec<CounterbalanceDefinition>,
}

/// Last-resort containment, intentionally separate from playable colliders.
#[derive(Clone, Copy, Debug)]
pub(super) struct SafetyBoundsDefinition {
    pub(super) min: Vec2,
    pub(super) max: Vec2,
}

#[derive(Clone, Debug)]
pub(super) struct VisualLayer {
    pub(super) image: String,
    pub(super) position: Vec2,
    pub(super) size: Vec2,
    pub(super) depth: f32,
    /// Screen movement relative to the camera: 1.0 is world-locked, lower
    /// values recede into the distance and values above 1.0 are foreground.
    pub(super) parallax: f32,
}

#[derive(Clone, Debug)]
pub(super) struct DropEmitterDefinition {
    pub(super) position: Vec2,
    pub(super) interval: f32,
    pub(super) initial_delay: f32,
    pub(super) radius: f32,
    pub(super) gravity: f32,
    pub(super) depth: f32,
    /// Rendering depth factor; 1.0 keeps the drop in world space.
    pub(super) parallax: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WastewaterAreaDefinition {
    pub(super) position: Vec2,
    pub(super) size: Vec2,
    pub(super) color: [f32; 4],
    pub(super) wave_height: f32,
    pub(super) wave_speed: f32,
    pub(super) depth: f32,
    pub(super) bubbles: Option<BubbleSettingsDefinition>,
    pub(super) immune_family: Option<usize>,
}

impl WastewaterAreaDefinition {
    pub(super) fn contains_x(self, world_x: f32) -> bool {
        (world_x - self.position.x).abs() <= self.size.x * 0.5
    }

    pub(super) fn surface_y(self, world_x: f32, elapsed: f32) -> f32 {
        self.position.y + self.size.y * 0.5 + self.wave_offset(world_x - self.position.x, elapsed)
    }

    pub(super) fn wave_offset(self, local_x: f32, elapsed: f32) -> f32 {
        let travel = elapsed * self.wave_speed;
        let broad_wave = (local_x * 0.014 + travel * 0.72).sin() * 0.42;
        let opposing_wave = (local_x * 0.031 - travel * 1.24 + 1.9).sin() * 0.27;
        let short_ripple = (local_x * 0.072 + travel * 1.83 + 4.2).sin() * 0.18;
        let pulse_center =
            ((travel * 115.0 + self.size.x * 0.5).rem_euclid(self.size.x)) - self.size.x * 0.5;
        let pulse_distance = (local_x - pulse_center).abs();
        let moving_pulse = (1.0 - pulse_distance / 105.0).max(0.0).powi(2) * 0.34;
        self.wave_height * (broad_wave + opposing_wave + short_ripple + moving_pulse)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct BubbleSettingsDefinition {
    pub(super) interval: [f32; 2],
    pub(super) radius: [f32; 2],
    pub(super) rise_speed: [f32; 2],
    pub(super) max_active: usize,
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
    /// Authored radius retained while runtime lighting profiles scale a
    /// lamp's effective reach consistently across every level.
    pub(super) base_radius: f32,
    pub(super) intensity: f32,
    /// Authored intensity retained while runtime lantern pulses update the
    /// current `intensity` field.
    pub(super) base_intensity: f32,
    pub(super) enabled: bool,
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

#[derive(Clone, Debug)]
pub(super) struct ChainDefinition {
    pub(super) id: String,
    pub(super) anchor: Vec2,
    pub(super) links: usize,
    pub(super) link_radius: f32,
    pub(super) spacing: f32,
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
    #[serde(default)]
    safety_bounds: Option<SafetyBoundsDocument>,
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
    chains: Vec<ChainDocument>,
    #[serde(default)]
    decorations: Vec<VisualLayerDocument>,
    #[serde(default)]
    drop_emitters: Vec<DropEmitterDocument>,
    #[serde(default)]
    wastewater_areas: Vec<WastewaterAreaDocument>,
    #[serde(default)]
    counterbalances: Vec<CounterbalanceDocument>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CounterbalanceDocument {
    minimum_radius: f32,
    plate_platform: usize,
    gate_platform: usize,
    open_offset: Point,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CounterbalanceDefinition {
    pub(super) minimum_radius: f32,
    pub(super) plate_platform: usize,
    pub(super) gate_platform: usize,
    pub(super) open_offset: Vec2,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Point {
    x: f32,
    y: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SafetyBoundsDocument {
    min: Point,
    max: Point,
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
    #[serde(default = "default_parallax")]
    parallax: f32,
}

fn default_parallax() -> f32 {
    1.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DropEmitterDocument {
    position: Point,
    interval: f32,
    #[serde(default)]
    initial_delay: f32,
    radius: f32,
    gravity: f32,
    depth: f32,
    #[serde(default = "default_parallax")]
    parallax: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WastewaterAreaDocument {
    position: Point,
    size: Point,
    color: [f32; 4],
    wave_height: f32,
    wave_speed: f32,
    depth: f32,
    #[serde(default)]
    bubbles: Option<BubbleSettingsDocument>,
    #[serde(default)]
    immune_family: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BubbleSettingsDocument {
    interval: [f32; 2],
    radius: [f32; 2],
    rise_speed: [f32; 2],
    max_active: usize,
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
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

const fn enabled_by_default() -> bool {
    true
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChainDocument {
    id: String,
    anchor: Point,
    links: usize,
    link_radius: f32,
    spacing: f32,
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
    let safety_bounds = document
        .safety_bounds
        .map(|bounds| {
            let min = finite_point("safety bounds min", bounds.min)?;
            let max = finite_point("safety bounds max", bounds.max)?;
            if min.x >= max.x || min.y >= max.y {
                return Err(LevelFormatError(
                    "safety bounds min must be strictly below max on both axes".into(),
                ));
            }
            Ok(SafetyBoundsDefinition { min, max })
        })
        .transpose()?;
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
                parallax: finite_number(&format!("visual layer {index} parallax"), layer.parallax)?,
            })
        })
        .collect::<Result<Vec<_>, LevelFormatError>>()?;
    let decorations = parse_visual_layers(document.decorations, "decoration")?;
    let drop_emitters = document
        .drop_emitters
        .into_iter()
        .enumerate()
        .map(|(index, emitter)| {
            let initial_delay = finite_number(
                &format!("drop emitter {index} initial_delay"),
                emitter.initial_delay,
            )?;
            if initial_delay < 0.0 {
                return Err(LevelFormatError(format!(
                    "drop emitter {index} initial_delay cannot be negative"
                )));
            }
            Ok(DropEmitterDefinition {
                position: finite_point(
                    &format!("drop emitter {index} position"),
                    emitter.position,
                )?,
                interval: positive_number(
                    &format!("drop emitter {index} interval"),
                    emitter.interval,
                )?,
                initial_delay,
                radius: positive_number(&format!("drop emitter {index} radius"), emitter.radius)?,
                gravity: positive_number(
                    &format!("drop emitter {index} gravity"),
                    emitter.gravity,
                )?,
                depth: finite_number(&format!("drop emitter {index} depth"), emitter.depth)?,
                parallax: finite_number(
                    &format!("drop emitter {index} parallax"),
                    emitter.parallax,
                )?,
            })
        })
        .collect::<Result<Vec<_>, LevelFormatError>>()?;
    let wastewater_areas = document
        .wastewater_areas
        .into_iter()
        .enumerate()
        .map(|(index, area)| {
            if area
                .color
                .iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
            {
                return Err(LevelFormatError(format!(
                    "wastewater area {index} color channels must be between 0 and 1"
                )));
            }
            if let Some(family) = area.immune_family
                && family >= crate::palette::BLOB_FAMILIES.len()
            {
                return Err(LevelFormatError(format!(
                    "wastewater area {index} immune_family must reference a blob family"
                )));
            }
            Ok(WastewaterAreaDefinition {
                position: finite_point(
                    &format!("wastewater area {index} position"),
                    area.position,
                )?,
                size: positive_size(&format!("wastewater area {index} size"), area.size)?,
                color: area.color,
                wave_height: positive_number(
                    &format!("wastewater area {index} wave_height"),
                    area.wave_height,
                )?,
                wave_speed: positive_number(
                    &format!("wastewater area {index} wave_speed"),
                    area.wave_speed,
                )?,
                depth: finite_number(&format!("wastewater area {index} depth"), area.depth)?,
                bubbles: area
                    .bubbles
                    .map(|bubbles| parse_bubble_settings(index, bubbles))
                    .transpose()?,
                immune_family: area.immune_family,
            })
        })
        .collect::<Result<Vec<_>, LevelFormatError>>()?;
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
    let chains = document
        .chains
        .into_iter()
        .enumerate()
        .map(|(index, chain)| {
            if !(2..=24).contains(&chain.links) {
                return Err(LevelFormatError(format!(
                    "chain {index} links must be between 2 and 24"
                )));
            }
            Ok(ChainDefinition {
                id: {
                    validate_text(&format!("chain {index} id"), &chain.id)?;
                    chain.id
                },
                anchor: finite_point(&format!("chain {index} anchor"), chain.anchor)?,
                links: chain.links,
                link_radius: positive_number(
                    &format!("chain {index} link_radius"),
                    chain.link_radius,
                )?,
                spacing: positive_number(&format!("chain {index} spacing"), chain.spacing)?,
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
            let intensity = positive_number(&format!("light {index} intensity"), light.intensity)?;
            let radius = positive_number(&format!("light {index} radius"), light.radius)?;
            Ok(LightDefinition {
                position: finite_point(&format!("light {index} position"), light.position)?,
                color: light.color,
                radius,
                base_radius: radius,
                intensity,
                base_intensity: intensity,
                enabled: light.enabled,
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
    let counterbalances = document
        .counterbalances
        .into_iter()
        .enumerate()
        .map(|(index, balance)| {
            if balance.plate_platform >= platforms.len() || balance.gate_platform >= platforms.len()
            {
                return Err(LevelFormatError(format!(
                    "counterbalance {index} platforms must reference rectangle colliders"
                )));
            }
            Ok(CounterbalanceDefinition {
                minimum_radius: positive_number(
                    &format!("counterbalance {index} minimum_radius"),
                    balance.minimum_radius,
                )?,
                plate_platform: balance.plate_platform,
                gate_platform: balance.gate_platform,
                open_offset: finite_point(
                    &format!("counterbalance {index} open_offset"),
                    balance.open_offset,
                )?,
            })
        })
        .collect::<Result<Vec<_>, LevelFormatError>>()?;

    Ok(ParsedLevel {
        name: document.name,
        size,
        center,
        safety_bounds,
        spawn,
        platforms,
        fixtures,
        route,
        visual_layers,
        nutrients,
        lights,
        expulsion_points,
        hazards,
        chains,
        decorations,
        drop_emitters,
        wastewater_areas,
        counterbalances,
    })
}

fn parse_bubble_settings(
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

fn positive_range(label: &str, range: [f32; 2]) -> Result<[f32; 2], LevelFormatError> {
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
                parallax: finite_number(&format!("{label} {index} parallax"), layer.parallax)?,
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
                "visual_layers": [{ "image": "levels/test/background.png", "position": { "x": 0, "y": 0 }, "size": { "x": 1000, "y": 800 }, "depth": -20 }],
                "drop_emitters": [{
                    "position": { "x": 20, "y": 30 },
                    "interval": 2.5,
                    "initial_delay": 0.4,
                    "radius": 3,
                    "gravity": 420,
                    "depth": -5
                }],
                "chains": [{
                    "id": "test_chain",
                    "anchor": { "x": 40, "y": 90 },
                    "links": 6,
                    "link_radius": 7,
                    "spacing": 15
                }],
                "wastewater_areas": [{
                    "position": { "x": 0, "y": -350 },
                    "size": { "x": 900, "y": 100 },
                    "color": [0.42, 0.58, 0.08, 0.8],
                    "wave_height": 4,
                    "wave_speed": 0.35,
                    "depth": -4,
                    "immune_family": 1,
                    "bubbles": {
                        "interval": [0.7, 2.4],
                        "radius": [2, 7],
                        "rise_speed": [22, 48],
                        "max_active": 12
                    }
                }]
            }"#,
        )
        .expect("valid level");

        assert_eq!(level.platforms.len(), 1);
        assert_eq!(level.fixtures.len(), 1);
        assert_eq!(level.visual_layers.len(), 1);
        assert_eq!(level.drop_emitters.len(), 1);
        assert_eq!(level.chains.len(), 1);
        assert_eq!(level.chains[0].id, "test_chain");
        assert_eq!(level.wastewater_areas.len(), 1);
        assert_eq!(
            level.wastewater_areas[0]
                .bubbles
                .expect("bubble settings")
                .max_active,
            12
        );
        assert_eq!(level.drop_emitters[0].gravity, 420.0);
        assert_eq!(level.wastewater_areas[0].immune_family, Some(1));
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
