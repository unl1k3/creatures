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
        let external_level = match index {
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
        if let Some(source) = external_level {
            return Self::from_embedded_regression(source);
        }
        match index {
            2 => (
                Self {
                    _name: "Supports lab".into(),
                    size: Vec2::new(760.0, 900.0),
                    center: Vec2::ZERO,
                    safety_bounds: None,
                    platforms: vec![
                        platform(0.0, -370.0, 760.0, 38.0),
                        platform(-245.0, -265.0, 70.0, 170.0),
                        platform(-105.0, -315.0, 105.0, 70.0),
                        platform(10.0, -270.0, 105.0, 160.0),
                        platform(170.0, -225.0, 105.0, 250.0),
                        platform(295.0, 55.0, 120.0, 28.0),
                    ],
                    fixtures: Vec::new(),
                    spawn_position: Vec2::new(-320.0, -285.0),
                    route: vec![
                        Vec2::new(-320.0, -285.0),
                        Vec2::new(-245.0, -140.0),
                        Vec2::new(-105.0, -240.0),
                        Vec2::new(10.0, -150.0),
                        Vec2::new(170.0, -60.0),
                        Vec2::new(295.0, 110.0),
                    ],
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
                },
                Vec2::new(-320.0, -285.0),
            ),
            3 => (
                Self {
                    _name: "Curves lab".into(),
                    size: Vec2::new(1000.0, 900.0),
                    center: Vec2::ZERO,
                    safety_bounds: None,
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(350.0, 0.0, 105.0, 24.0),
                        platform(470.0, 145.0, 80.0, 24.0),
                    ],
                    fixtures: {
                        let mut fixtures = vec![vec![
                            Vec2::new(-340.0, -370.0),
                            Vec2::new(80.0, -370.0),
                            Vec2::new(80.0, -280.0),
                        ]];
                        fixtures.push(semicircle_fixture(Vec2::new(220.0, -250.0), 105.0, 28.0));
                        fixtures.extend(wave_fixtures(-330.0, 330.0, 285.0, 220.0, 9));
                        fixtures
                    },
                    // Fall onto the shared vertex between two upper wave
                    // segments so the problematic contact is reproducible.
                    spawn_position: Vec2::new(36.67, 430.0),
                    route: vec![
                        Vec2::new(-300.0, -285.0),
                        Vec2::new(-150.0, -270.0),
                        Vec2::new(20.0, -220.0),
                        Vec2::new(220.0, -105.0),
                        Vec2::new(350.0, 55.0),
                        Vec2::new(470.0, 200.0),
                        Vec2::new(320.0, 330.0),
                        Vec2::new(120.0, 330.0),
                        Vec2::new(-80.0, 330.0),
                        Vec2::new(-260.0, 320.0),
                    ],
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
                },
                Vec2::new(36.67, 430.0),
            ),
            4 => (
                Self {
                    _name: "Low passage lab".into(),
                    size: Vec2::new(760.0, 900.0),
                    center: Vec2::ZERO,
                    safety_bounds: None,
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(-210.0, -285.0, 28.0, 190.0),
                        platform(10.0, -285.0, 28.0, 190.0),
                        platform(-100.0, -365.0, 248.0, 28.0),
                        platform(235.0, -250.0, 250.0, 28.0),
                    ],
                    fixtures: Vec::new(),
                    spawn_position: Vec2::new(-100.0, -245.0),
                    route: vec![
                        Vec2::new(-100.0, -245.0),
                        Vec2::new(-25.0, -145.0),
                        Vec2::new(80.0, -310.0),
                        Vec2::new(220.0, -310.0),
                        Vec2::new(355.0, -310.0),
                    ],
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
                },
                Vec2::new(-100.0, -245.0),
            ),
            5 => (
                Self {
                    _name: "Impact lab".into(),
                    size: Vec2::new(900.0, 1100.0),
                    center: Vec2::ZERO,
                    safety_bounds: None,
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(-185.0, -245.0, 125.0, 24.0),
                        platform(20.0, -105.0, 105.0, 24.0),
                        platform(245.0, 45.0, 105.0, 24.0),
                        platform(20.0, 185.0, 115.0, 24.0),
                        platform(-220.0, 335.0, 95.0, 24.0),
                        platform(-40.0, 475.0, 110.0, 24.0),
                        platform(130.0, 600.0, 120.0, 24.0),
                        platform(245.0, 470.0, 26.0, 260.0),
                        platform(365.0, 470.0, 26.0, 260.0),
                    ],
                    fixtures: Vec::new(),
                    spawn_position: Vec2::new(-300.0, -285.0),
                    route: vec![
                        Vec2::new(-300.0, -285.0),
                        Vec2::new(-185.0, -190.0),
                        Vec2::new(20.0, -50.0),
                        Vec2::new(245.0, 100.0),
                        Vec2::new(20.0, 240.0),
                        Vec2::new(-220.0, 390.0),
                        Vec2::new(-40.0, 530.0),
                        Vec2::new(130.0, 655.0),
                        Vec2::new(305.0, 650.0),
                    ],
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
                },
                Vec2::new(-300.0, -285.0),
            ),
            6 => (
                Self {
                    _name: "Split bridge lab".into(),
                    size: Vec2::new(760.0, 900.0),
                    center: Vec2::ZERO,
                    safety_bounds: None,
                    platforms: vec![
                        platform(0.0, -390.0, 760.0, 38.0),
                        platform(270.0, -40.0, 105.0, 24.0),
                        platform(155.0, 115.0, 130.0, 24.0),
                        platform(-45.0, 115.0, 130.0, 24.0),
                    ],
                    fixtures: v_valley_fixtures(Vec2::new(0.0, -180.0), 300.0, 120.0),
                    spawn_position: Vec2::new(0.0, -125.0),
                    route: vec![
                        Vec2::new(0.0, -125.0),
                        Vec2::new(145.0, -15.0),
                        Vec2::new(270.0, 15.0),
                        Vec2::new(155.0, 170.0),
                        Vec2::new(-45.0, 170.0),
                    ],
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
                },
                Vec2::new(0.0, -125.0),
            ),
            _ => (Self::prototype(), BLOB_START),
        }
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

#[cfg(feature = "dev-tools")]
fn semicircle_fixture(center: Vec2, radius: f32, depth: f32) -> Vec<Vec2> {
    let mut vertices = vec![center + Vec2::new(-radius, -depth)];
    for step in 0..=16 {
        let x = -radius + radius * 2.0 * step as f32 / 16.0;
        let y = (radius * radius - x * x).max(0.0).sqrt();
        vertices.push(center + Vec2::new(x, y));
    }
    vertices.push(center + Vec2::new(radius, -depth));
    vertices
}

#[cfg(feature = "dev-tools")]
fn wave_fixtures(
    minimum_x: f32,
    maximum_x: f32,
    baseline: f32,
    bottom: f32,
    segments: usize,
) -> Vec<Vec<Vec2>> {
    (0..segments)
        .map(|segment| {
            let fraction_a = segment as f32 / segments as f32;
            let fraction_b = (segment + 1) as f32 / segments as f32;
            let x_a = minimum_x + (maximum_x - minimum_x) * fraction_a;
            let x_b = minimum_x + (maximum_x - minimum_x) * fraction_b;
            let y_a = baseline + (fraction_a * std::f32::consts::TAU * 1.5).sin() * 48.0;
            let y_b = baseline + (fraction_b * std::f32::consts::TAU * 1.5).sin() * 48.0;
            vec![
                Vec2::new(x_a, bottom),
                Vec2::new(x_b, bottom),
                Vec2::new(x_b, y_b),
                Vec2::new(x_a, y_a),
            ]
        })
        .collect()
}

#[cfg(feature = "dev-tools")]
fn v_valley_fixtures(center: Vec2, width: f32, depth: f32) -> Vec<Vec<Vec2>> {
    let half = width * 0.5;
    vec![
        vec![
            center + Vec2::new(-half, -depth),
            center + Vec2::new(0.0, -depth),
            center,
            center + Vec2::new(-half, depth),
        ],
        vec![
            center + Vec2::new(0.0, -depth),
            center + Vec2::new(half, -depth),
            center + Vec2::new(half, depth),
            center,
        ],
    ]
}

#[cfg(any(feature = "dev-tools", test))]
pub(super) fn platform(x: f32, y: f32, width: f32, height: f32) -> Platform {
    Platform {
        center: Vec2::new(x, y),
        half_size: Vec2::new(width, height) * 0.5,
    }
}
