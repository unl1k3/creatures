//! Blob-world state and the gameplay rules that operate on a family of blobs.
//!
//! Keeping splitting, rejoining, collision resolution and recovery together
//! makes the gameplay loop independent from application startup and rendering.

use super::*;

mod collisions;
mod placement;
mod rejoin;
mod split;
pub(crate) use collisions::{blob_surface_gap, resolve_blob_collisions_with_vitality};
#[cfg(test)]
pub(crate) use collisions::{
    resolve_blob_collisions, resolve_blob_collisions_impl, support_extent,
};
pub(crate) use placement::{path_is_clear, place_blob_clear};
#[cfg(test)]
pub(crate) use rejoin::rejoin_pair_indices;
pub(crate) use rejoin::{
    advance_rejoin_timeout, rejoin_roll_directions, start_selected_rejoin, update_rejoining,
};
#[cfg(test)]
pub(crate) use split::split_selected;
pub(crate) use split::{SplitRng, split_selected_in_level};

#[cfg(feature = "dev-tools")]
pub(crate) const BLOB_START: Vec2 = Vec2::new(0.0, -280.0);
pub(crate) const INITIAL_RADIUS: f32 = REFERENCE_RADIUS * DEFAULT_CREATURE_SCALE;
pub(crate) const MAX_ACTIVE_BLOBS: usize = 4;
pub(crate) const REJOIN_TIMEOUT: f32 = 4.0;
const BLOB_CONTACT_PREDICTION_CLEARANCE: f32 = 1.5;
pub(crate) const BLOB_CONTACT_VISUAL_CLEARANCE: f32 = 0.0;
const BLOB_CONTACT_MAX_CORRECTION: f32 = 4.0;
const BLOB_CONTACT_MAX_TRANSFER_SPEED: f32 = 4.0;

pub(crate) struct ActiveBlob {
    pub(crate) id: u64,
    pub(crate) parent_id: Option<u64>,
    pub(crate) body: Blob,
}

#[derive(Resource)]
pub(crate) struct BlobWorld {
    pub(crate) active: Vec<ActiveBlob>,
    pub(crate) selected: usize,
    pub(crate) rejoin_parent: Option<u64>,
    pub(crate) rejoin_elapsed: f32,
    pub(crate) parent_links: HashMap<u64, Option<u64>>,
    pub(crate) next_id: u64,
}

pub(crate) fn enforce_blob_safety_bounds(level: Res<Level>, mut blobs: ResMut<BlobWorld>) {
    let Some(bounds) = level.safety_bounds else {
        return;
    };
    for active_blob in &mut blobs.active {
        if active_blob
            .body
            .contain_within_safety_bounds(bounds.min, bounds.max)
        {
            active_blob.body.cancel_jump_charge();
            active_blob.body.stabilize_after_external_projection();
        }
    }
}

/// Returns true only for an actual containment, not for the shallow overlap
/// that can occur while a soft membrane is resting on its contact skin.
pub(crate) fn reset_world_at(blobs: &mut BlobWorld, position: Vec2) {
    blobs.active = vec![ActiveBlob {
        id: 0,
        parent_id: None,
        body: Blob::new(position, INITIAL_RADIUS),
    }];
    blobs.selected = 0;
    blobs.rejoin_parent = None;
    blobs.rejoin_elapsed = 0.0;
    blobs.parent_links.clear();
    blobs.next_id = 1;
}
