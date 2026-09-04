//! Level loading, conversion, and development-only scenario geometry.

use super::*;

impl Level {
    pub(super) fn prototype() -> Self {
        let parsed = parse_level(include_str!("../../assets/levels/sewer_01/level.json"))
            .expect("embedded sewer_01 level must be valid");
        Self::from_parsed(parsed)
    }

    fn from_parsed(parsed: ParsedLevel) -> Self {
        Self {
            _name: parsed.name,
            size: parsed.size,
            center: parsed.center,
            safety_bounds: parsed.safety_bounds,
            platforms: parsed.platforms,
            fixtures: parsed.fixtures,
            spawn_position: parsed.spawn,
            route: parsed.route,
            visual_layers: parsed.visual_layers,
            ice_platforms: parsed.ice_platforms,
            glue_platforms: parsed.glue_platforms,
            nutrients: parsed.nutrients,
            lights: Vec::new(),
            expulsion_points: parsed.expulsion_points,
            hazards: parsed.hazards,
            chains: parsed.chains,
            decorations: parsed.decorations,
            wastewater_areas: parsed.wastewater_areas,
            counterbalances: parsed.counterbalances,
        }
    }

    pub(crate) fn has_artwork(&self) -> bool {
        !self.visual_layers.is_empty()
    }

    pub(crate) fn size(&self) -> Vec2 {
        self.size
    }

    pub(crate) fn center(&self) -> Vec2 {
        self.center
    }

    #[cfg(test)]
    pub(crate) fn from_test_geometry(platforms: Vec<Platform>, fixtures: Vec<Vec<Vec2>>) -> Self {
        Self {
            _name: "Test level".into(),
            size: Vec2::splat(1000.0),
            center: Vec2::ZERO,
            safety_bounds: None,
            platforms,
            fixtures,
            spawn_position: Vec2::ZERO,
            route: Vec::new(),
            visual_layers: Vec::new(),
            ice_platforms: Vec::new(),
            glue_platforms: Vec::new(),
            decorations: Vec::new(),
            wastewater_areas: Vec::new(),
            nutrients: Vec::new(),
            lights: Vec::new(),
            expulsion_points: Vec::new(),
            hazards: Vec::new(),
            chains: Vec::new(),
            counterbalances: Vec::new(),
        }
    }

    #[cfg(feature = "dev-tools")]
    pub(crate) fn test_scenario(index: u8) -> (Self, Vec2) {
        let source = match index {
            2 => Some(include_str!("../../assets/levels/supports_lab/level.json")),
            3 => Some(include_str!("../../assets/levels/curves_lab/level.json")),
            4 => Some(include_str!(
                "../../assets/levels/low_passage_lab/level.json"
            )),
            5 => Some(include_str!("../../assets/levels/impact_lab/level.json")),
            6 => Some(include_str!(
                "../../assets/levels/split_bridge_lab/level.json"
            )),
            7 => Some(include_str!(
                "../../assets/levels/regression_fragment_seams/level.json"
            )),
            8 => Some(include_str!(
                "../../assets/levels/regression_nutrient_wall/level.json"
            )),
            9 => Some(include_str!(
                "../../assets/levels/regression_coral_basin/level.json"
            )),
            _ => None,
        };
        source.map_or_else(
            || (Self::prototype(), BLOB_START),
            Self::from_embedded_regression,
        )
    }

    #[cfg(feature = "dev-tools")]
    fn from_embedded_regression(source: &str) -> (Self, Vec2) {
        let level = Self::from_parsed(
            parse_level(source).expect("embedded regression level must be valid"),
        );
        let spawn = level.spawn_position;
        (level, spawn)
    }
}

#[cfg(test)]
pub(super) fn platform(x: f32, y: f32, width: f32, height: f32) -> Platform {
    Platform {
        center: Vec2::new(x, y),
        half_size: Vec2::new(width, height) * 0.5,
    }
}
