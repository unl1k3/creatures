//! JSON document schema, conversion, and validation for authored levels.

use super::*;

mod document;
mod validation;

use document::*;
use validation::*;

pub(crate) fn parse_level(source: &str) -> Result<ParsedLevel, LevelFormatError> {
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
    let mut ice_platforms = Vec::new();
    let mut glue_platforms = Vec::new();
    let mut fixtures = Vec::new();
    for collider in document.colliders {
        match collider {
            ColliderDocument::Rectangle {
                id,
                position,
                size,
                surface,
            } => {
                validate_text("collider id", &id)?;
                let platform_index = platforms.len();
                platforms.push(Platform {
                    center: finite_point(&format!("collider '{id}' position"), position)?,
                    half_size: positive_size(&format!("collider '{id}' size"), size)? * 0.5,
                });
                if matches!(surface, SurfaceDocument::Ice) {
                    ice_platforms.push(platform_index);
                }
                if matches!(surface, SurfaceDocument::Glue) {
                    glue_platforms.push(platform_index);
                }
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
        ice_platforms,
        glue_platforms,
        nutrients,
        expulsion_points,
        hazards,
        chains,
        decorations,
        wastewater_areas,
        counterbalances,
    })
}
