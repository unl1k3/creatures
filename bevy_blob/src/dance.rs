//! Optional choreography preview for inspecting the soft body in isolation.

use bevy::{audio::Volume, prelude::*};

const DANCE_COUNT: u64 = 3;
/// The bundled loop is 120 BPM, so one eight-beat phrase lasts four seconds.
const DANCE_BPM: f32 = 120.0;
const BEAT_SECONDS: f32 = 60.0 / DANCE_BPM;
const ROUTINE_SECONDS: f32 = 8.0 * BEAT_SECONDS;
const MUSIC_LOOP_SECONDS: f32 = ROUTINE_SECONDS * DANCE_COUNT as f32;
const MAX_EXCURSION: f32 = 7.0;

/// A temporary membrane extension used exclusively by the renderer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DanceTentacleCue {
    pub(crate) side: f32,
    pub(crate) extension: f32,
    pub(crate) wave: f32,
}

/// Repeating collection of small, readable dances for animation review.
#[derive(Resource, Default)]
pub(crate) struct BlobDancePreview {
    enabled: bool,
    elapsed: f32,
    origin_x: Option<f32>,
    routine: u64,
    tiny_hop_pending: bool,
    last_hop_phase: bool,
}

/// Looping music that exists only while the dance preview is active.
#[derive(Component)]
pub(crate) struct DanceMusic;

impl BlobDancePreview {
    pub(crate) fn advance(&mut self, delta_seconds: f32) {
        if !self.enabled {
            return;
        }

        self.set_elapsed(self.elapsed + delta_seconds);
    }

    /// Makes the choreography follow the audio device clock rather than the
    /// physics clock. This keeps visual accents on the audible downbeats.
    pub(crate) fn synchronize_to_music(&mut self, music_seconds: Option<f32>) {
        if let Some(seconds) = music_seconds.filter(|_| self.enabled) {
            self.set_elapsed(seconds.rem_euclid(MUSIC_LOOP_SECONDS));
        }
    }

    fn set_elapsed(&mut self, elapsed: f32) {
        let previous_phase = self.phase();
        self.elapsed = elapsed;
        let phase = self.phase();
        self.routine = (self.elapsed / ROUTINE_SECONDS).floor() as u64 % DANCE_COUNT;
        let hop_phase = self.routine == 2 && (1.50..1.63).contains(&phase);
        if hop_phase && !self.last_hop_phase {
            self.tiny_hop_pending = true;
        }
        self.last_hop_phase = hop_phase;
        if phase < previous_phase {
            self.origin_x = None;
        }
    }

    /// Horizontal intent only; the choreography sways around its start point.
    pub(crate) fn movement_intent(&mut self, center_x: f32) -> Option<f32> {
        if !self.enabled {
            return None;
        }

        let origin_x = *self.origin_x.get_or_insert(center_x);
        let offset = center_x - origin_x;
        let beat = self.beat_phase();
        // These are body leans, not travel commands. Every target passes
        // through the origin before reversing direction.
        let target_offset = match self.routine {
            0 => (beat * std::f32::consts::FRAC_PI_4).sin() * 3.8,
            1 => (beat * std::f32::consts::FRAC_PI_2).sin() * 3.1,
            _ => (beat * std::f32::consts::FRAC_PI_4).sin() * 2.4,
        };
        let correction = (target_offset - offset) * 0.040;
        let intent = if offset.abs() > MAX_EXCURSION {
            -offset.signum() * 0.14
        } else {
            correction.clamp(-0.12, 0.12)
        };
        Some(intent)
    }

    /// The current render-only tentacle, if the active routine calls for it.
    pub(crate) fn tentacle_cue(&self) -> Option<DanceTentacleCue> {
        if !self.enabled {
            return None;
        }
        let beat = self.beat_phase();
        match self.routine {
            0 if beat < 2.6 => Some(tentacle(-1.0, beat / 2.6, beat * 2.5)),
            0 if (3.0..5.8).contains(&beat) => Some(tentacle(1.0, (beat - 3.0) / 2.8, beat * 2.5)),
            1 if beat < 2.2 => Some(tentacle(-1.0, beat / 2.2, beat * 3.5)),
            1 if (2.0..4.1).contains(&beat) => Some(tentacle(1.0, (beat - 2.0) / 2.1, beat * 3.5)),
            1 if (4.0..6.2).contains(&beat) => Some(tentacle(-1.0, (beat - 4.0) / 2.2, beat * 3.5)),
            2 if beat < 1.8 => Some(tentacle(-1.0, beat / 1.8, beat * 3.0)),
            2 if (2.6..4.8).contains(&beat) => Some(tentacle(1.0, (beat - 2.6) / 2.2, beat * 3.0)),
            _ => None,
        }
    }

    pub(crate) fn take_tiny_hop(&mut self) -> bool {
        let hop = self.enabled && self.tiny_hop_pending;
        self.tiny_hop_pending = false;
        hop
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn phase(&self) -> f32 {
        self.elapsed.rem_euclid(ROUTINE_SECONDS)
    }

    fn beat_phase(&self) -> f32 {
        self.phase() / BEAT_SECONDS
    }

    fn toggle(&mut self) {
        self.enabled = !self.enabled;
        self.elapsed = 0.0;
        self.origin_x = None;
        self.routine = 0;
        self.tiny_hop_pending = false;
        self.last_hop_phase = false;
    }
}

fn tentacle(side: f32, progress: f32, wave: f32) -> DanceTentacleCue {
    let progress = progress.clamp(0.0, 1.0);
    DanceTentacleCue {
        side,
        extension: (progress * std::f32::consts::PI).sin().powf(0.62),
        wave,
    }
}

pub(crate) fn toggle_blob_dance(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut dance: ResMut<BlobDancePreview>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
    music: Query<Entity, With<DanceMusic>>,
) {
    if keyboard.just_pressed(KeyCode::KeyT) {
        dance.toggle();
        if dance.is_enabled() {
            commands.spawn((
                DanceMusic,
                AudioPlayer::new(asset_server.load("audio/music/blob-dance.wav")),
                PlaybackSettings::LOOP.with_volume(Volume::Linear(0.24)),
            ));
        } else {
            for entity in &music {
                commands.entity(entity).despawn();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_routine_presents_left_then_right_tentacle() {
        let mut dance = BlobDancePreview {
            enabled: true,
            ..default()
        };
        dance.elapsed = 0.35;
        assert!(dance.tentacle_cue().is_some_and(|cue| cue.side < 0.0));
        dance.elapsed = 2.10;
        assert!(dance.tentacle_cue().is_some_and(|cue| cue.side > 0.0));
    }

    #[test]
    fn first_routine_overlaps_arms_with_body_motion() {
        let mut dance = BlobDancePreview {
            enabled: true,
            ..default()
        };
        dance.elapsed = 0.75;
        assert!(dance.tentacle_cue().is_some_and(|cue| cue.side < 0.0));
        assert!(
            dance
                .movement_intent(0.0)
                .is_some_and(|intent| intent > 0.0)
        );

        dance.elapsed = 1.55;
        assert!(dance.tentacle_cue().is_some_and(|cue| cue.side > 0.0));
        assert!(
            dance
                .movement_intent(0.0)
                .is_some_and(|intent| intent > 0.0)
        );
    }

    #[test]
    fn music_clock_places_the_right_arm_on_the_fourth_downbeat() {
        let mut dance = BlobDancePreview {
            enabled: true,
            ..default()
        };
        dance.synchronize_to_music(Some(1.5));
        assert!(dance.tentacle_cue().is_some_and(|cue| cue.side > 0.0));
        assert!(
            dance
                .movement_intent(0.0)
                .is_some_and(|intent| intent > 0.0)
        );
    }

    #[test]
    fn dance_sway_is_limited_and_returns_to_the_origin() {
        let mut dance = BlobDancePreview {
            enabled: true,
            ..default()
        };
        dance.elapsed = 1.0;
        let first_push = dance.movement_intent(0.0).unwrap();
        assert!((0.0..=0.12).contains(&first_push));

        let return_push = dance.movement_intent(MAX_EXCURSION + 1.0).unwrap();
        assert!(return_push < 0.0);
    }

    #[test]
    fn routines_rotate_after_each_phrase() {
        let mut dance = BlobDancePreview {
            enabled: true,
            ..default()
        };
        dance.advance(ROUTINE_SECONDS + 0.01);
        assert_eq!(dance.routine, 1);
    }
}
