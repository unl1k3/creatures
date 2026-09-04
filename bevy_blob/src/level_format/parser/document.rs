//! Serde representation of the authored level JSON.

use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LevelDocument {
    pub(super) version: u32,
    pub(super) name: String,
    pub(super) size: Point,
    #[serde(default)]
    pub(super) center: Point,
    #[serde(default)]
    pub(super) safety_bounds: Option<SafetyBoundsDocument>,
    pub(super) spawn: Point,
    #[serde(default)]
    pub(super) colliders: Vec<ColliderDocument>,
    #[serde(default)]
    pub(super) route: Vec<Point>,
    #[serde(default)]
    pub(super) visual_layers: Vec<VisualLayerDocument>,
    #[serde(default)]
    pub(super) nutrients: Vec<NutrientDocument>,
    #[serde(default)]
    pub(super) expulsion_points: Vec<ExpulsionPointDocument>,
    #[serde(default)]
    pub(super) hazards: Vec<HazardDocument>,
    #[serde(default)]
    pub(super) chains: Vec<ChainDocument>,
    #[serde(default)]
    pub(super) decorations: Vec<VisualLayerDocument>,
    #[serde(default)]
    pub(super) wastewater_areas: Vec<WastewaterAreaDocument>,
    #[serde(default)]
    pub(super) counterbalances: Vec<CounterbalanceDocument>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CounterbalanceDocument {
    pub(super) minimum_radius: f32,
    pub(super) plate_platform: usize,
    pub(super) gate_platform: usize,
    pub(super) open_offset: Point,
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Point {
    pub(super) x: f32,
    pub(super) y: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SafetyBoundsDocument {
    pub(super) min: Point,
    pub(super) max: Point,
}

impl From<Point> for Vec2 {
    fn from(point: Point) -> Self {
        Self::new(point.x, point.y)
    }
}

#[derive(Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ColliderDocument {
    Rectangle {
        id: String,
        position: Point,
        size: Point,
        #[serde(default)]
        surface: SurfaceDocument,
    },
    Polygon {
        id: String,
        points: Vec<Point>,
    },
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SurfaceDocument {
    #[default]
    Stone,
    Ice,
    Glue,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct VisualLayerDocument {
    pub(super) image: String,
    pub(super) position: Point,
    pub(super) size: Point,
    pub(super) depth: f32,
    #[serde(default = "default_parallax")]
    pub(super) parallax: f32,
}

fn default_parallax() -> f32 {
    1.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WastewaterAreaDocument {
    pub(super) position: Point,
    pub(super) size: Point,
    pub(super) color: [f32; 4],
    pub(super) wave_height: f32,
    pub(super) wave_speed: f32,
    pub(super) depth: f32,
    #[serde(default)]
    pub(super) bubbles: Option<BubbleSettingsDocument>,
    #[serde(default)]
    pub(super) immune_family: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BubbleSettingsDocument {
    pub(super) interval: [f32; 2],
    pub(super) radius: [f32; 2],
    pub(super) rise_speed: [f32; 2],
    pub(super) max_active: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NutrientDocument {
    pub(super) position: Point,
    pub(super) radius: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExpulsionPointDocument {
    pub(super) position: Point,
    pub(super) direction: Point,
    pub(super) strength: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HazardDocument {
    pub(super) position: Point,
    pub(super) size: Point,
    pub(super) damage_per_second: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ChainDocument {
    pub(super) id: String,
    pub(super) anchor: Point,
    pub(super) links: usize,
    pub(super) link_radius: f32,
    pub(super) spacing: f32,
}
