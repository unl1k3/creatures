//! Optional visual-motion preview for inspecting the soft body in isolation.

use bevy::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Self-contained movement loop: alternating roll, tiny hop, rest.
#[derive(Resource, Default)]
pub(crate) struct BlobDancePreview {
    enabled: bool,
    elapsed: f32,
    tiny_hop_pending: bool,
    origin_x: Option<f32>,
    direction: f32,
    last_cycle: u64,
    random_state: u64,
}

impl BlobDancePreview {
    pub(crate) fn advance(&mut self, delta_seconds: f32) {
        if !self.enabled {
            return;
        }

        const CYCLE_SECONDS: f32 = 2.45;
        const HOP_PHASE: f32 = 0.98;
        let previous_phase = self.elapsed.rem_euclid(CYCLE_SECONDS);
        self.elapsed += delta_seconds;
        let current_phase = self.elapsed.rem_euclid(CYCLE_SECONDS);
        let cycle = (self.elapsed / CYCLE_SECONDS).floor() as u64;
        if cycle != self.last_cycle {
            self.last_cycle = cycle;
            self.direction = if self.next_random() & 1 == 0 {
                1.0
            } else {
                -1.0
            };
        }
        // Queue one hop per cycle. It is consumed only after the selected
        // blob has a supporting surface, so it can never become an air jump.
        if (previous_phase < HOP_PHASE && current_phase >= HOP_PHASE)
            || (current_phase < previous_phase && current_phase >= HOP_PHASE)
        {
            self.tiny_hop_pending = true;
        }
    }

    /// Horizontal intent only: each short excursion is pulled back towards
    /// the point at which the preview was enabled, avoiding visual drift.
    pub(crate) fn movement_intent(&mut self, center_x: f32) -> Option<f32> {
        if !self.enabled {
            return None;
        }
        const CYCLE_SECONDS: f32 = 2.45;
        const MAX_EXCURSION: f32 = 18.0;
        let origin_x = *self.origin_x.get_or_insert(center_x);
        let offset = center_x - origin_x;
        let phase = self.elapsed.rem_euclid(CYCLE_SECONDS);
        let movement = match phase {
            // Short, varied outward roll. It immediately eases if the blob
            // has already travelled far enough away from its origin.
            0.0..0.92 if offset * self.direction < MAX_EXCURSION => self.direction * 0.48,
            0.0..0.92 => -self.direction * 0.30,
            // The tiny hop occurs here; the body remains horizontally quiet.
            0.92..1.28 => 0.0,
            // Return with a proportional correction so inertia cannot build
            // up over many dance cycles.
            1.28..2.16 if offset.abs() > 1.5 => -offset.signum() * 0.58,
            _ => 0.0,
        };
        Some(movement)
    }

    pub(crate) fn take_tiny_hop(&mut self) -> bool {
        let hop = self.enabled && self.tiny_hop_pending;
        self.tiny_hop_pending = false;
        hop
    }

    fn toggle(&mut self) {
        self.enabled = !self.enabled;
        self.elapsed = 0.0;
        self.tiny_hop_pending = false;
        self.origin_x = None;
        self.last_cycle = 0;
        // A local generator avoids adding a dependency solely for this
        // optional preview. The activation time changes the initial seed,
        // while each following cycle still varies its direction.
        self.random_state = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0x9e37_79b9_7f4a_7c15, |elapsed| elapsed.as_nanos() as u64)
            ^ 0x9e37_79b9_7f4a_7c15;
        self.direction = if self.next_random() & 1 == 0 {
            1.0
        } else {
            -1.0
        };
    }

    fn next_random(&mut self) -> u64 {
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 7;
        self.random_state ^= self.random_state << 17;
        self.random_state
    }
}

pub(crate) fn toggle_blob_dance(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut dance: ResMut<BlobDancePreview>,
) {
    if keyboard.just_pressed(KeyCode::KeyT) {
        dance.toggle();
    }
}
