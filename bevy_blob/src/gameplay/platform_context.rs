//! Builds the collision view consumed by one blob simulation step.

use super::*;

/// Platforms and material indices after applying transient gameplay exclusions.
pub(super) struct CollisionPlatforms {
    pub(super) platforms: Vec<Platform>,
    pub(super) ice_indices: Vec<usize>,
    pub(super) glue_indices: Vec<usize>,
}

/// Produces a compact collision slice while preserving material classification.
///
/// A returning counterweight plate is visually moving but must not become a
/// second collision surface beneath a blob that has already jumped away.
pub(super) fn build_collision_platforms(level: &Level, blobs: &BlobWorld) -> CollisionPlatforms {
    let excluded_counterweight_plates: Vec<usize> = level
        .counterbalances
        .iter()
        .filter_map(|balance| {
            let plate = level.platforms[balance.plate_platform];
            let has_rider = blobs.active.iter().any(|blob| {
                let center = blob.body.center();
                let radius = blob.body.rest_radius;
                (center.x - plate.center.x).abs() <= plate.half_size.x + radius * 0.3
                    && center.y - radius <= plate.center.y + plate.half_size.y + 5.0
                    && center.y >= plate.center.y
            });
            let has_airborne_blob_above = blobs.active.iter().any(|blob| {
                let center = blob.body.center();
                (center.x - plate.center.x).abs() <= plate.half_size.x + blob.body.rest_radius * 0.3
                    && center.y > plate.center.y
            });
            (!has_rider && has_airborne_blob_above).then_some(balance.plate_platform)
        })
        .collect();

    let mut platforms = Vec::with_capacity(level.platforms.len());
    let mut ice_indices = Vec::new();
    let mut glue_indices = Vec::new();
    for (level_index, platform) in level.platforms.iter().copied().enumerate() {
        if excluded_counterweight_plates.contains(&level_index) {
            continue;
        }
        let collision_index = platforms.len();
        platforms.push(platform);
        if level.ice_platforms.contains(&level_index) {
            ice_indices.push(collision_index);
        }
        if level.glue_platforms.contains(&level_index) {
            glue_indices.push(collision_index);
        }
    }

    CollisionPlatforms {
        platforms,
        ice_indices,
        glue_indices,
    }
}
