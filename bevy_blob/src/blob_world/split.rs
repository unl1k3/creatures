//! Deterministic blob splitting and family-tree updates.

use super::*;

#[derive(Resource)]
pub(crate) struct SplitRng(pub(crate) u64);

impl SplitRng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(crate) fn split_choice(&mut self, particle_count: usize) -> (usize, bool) {
        let ratio = 0.37 + (self.next() % 10) as f32 * 0.01;
        let smaller_count = ((particle_count as f32 * ratio).round() as usize)
            .clamp(6, particle_count.saturating_sub(6));
        let smaller_on_left = self.next() & 1 == 0;
        (smaller_count, smaller_on_left)
    }
}

#[cfg(test)]
pub(crate) fn split_selected(blobs: &mut BlobWorld, rng: &mut SplitRng, dt: f32) {
    let _ = split_selected_in_level(blobs, rng, dt, &[], &[]);
}

pub(crate) fn split_selected_in_level(
    blobs: &mut BlobWorld,
    rng: &mut SplitRng,
    dt: f32,
    platforms: &[Platform],
    fixtures: &[Vec<Vec2>],
) -> bool {
    if blobs.active.is_empty() || blobs.active.len() >= MAX_ACTIVE_BLOBS {
        return false;
    }
    let index = blobs.selected.min(blobs.active.len() - 1);
    if !blobs.active[index].body.can_split() {
        return false;
    }
    let parent_body = &blobs.active[index].body;
    let (smaller_count, smaller_on_left) = rng.split_choice(parent_body.particles.len());
    let [mut first_body, mut second_body] =
        parent_body.split_pair_uneven(dt, smaller_count, smaller_on_left);
    // Never replace a valid parent with children already embedded in level
    // geometry. This is most visible next to the thin wall of scenario 8.
    if !place_blob_clear(&mut first_body, platforms, fixtures)
        || !place_blob_clear(&mut second_body, platforms, fixtures)
    {
        return false;
    }

    let parent = blobs.active.remove(index);
    blobs.parent_links.insert(parent.id, parent.parent_id);
    let first_id = blobs.next_id;
    let second_id = blobs.next_id + 1;
    blobs.next_id += 2;
    blobs.active.insert(
        index,
        ActiveBlob {
            id: first_id,
            parent_id: Some(parent.id),
            body: first_body,
        },
    );
    blobs.active.insert(
        index + 1,
        ActiveBlob {
            id: second_id,
            parent_id: Some(parent.id),
            body: second_body,
        },
    );
    blobs.selected = index;
    blobs.rejoin_elapsed = 0.0;
    true
}
