use crate::blob::Platform;
use bevy::prelude::Vec2;
use std::{error::Error, fmt};

mod parser;
pub(super) use parser::parse_level;

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
    pub(super) ice_platforms: Vec<usize>,
    pub(super) glue_platforms: Vec<usize>,
    pub(super) nutrients: Vec<NutrientDefinition>,
    pub(super) expulsion_points: Vec<ExpulsionPointDefinition>,
    pub(super) hazards: Vec<HazardDefinition>,
    pub(super) chains: Vec<ChainDefinition>,
    pub(super) decorations: Vec<VisualLayer>,
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
    pub(super) intensity: f32,
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

#[derive(Clone, Copy, Debug)]
pub(super) struct CounterbalanceDefinition {
    pub(super) minimum_radius: f32,
    pub(super) plate_platform: usize,
    pub(super) gate_platform: usize,
    pub(super) open_offset: Vec2,
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
