//! Explicit Bevy scheduling phases for the prototype.
//!
//! Keeping these phases outside `main.rs` makes system ordering discoverable
//! without mixing it with application setup or gameplay code.

use bevy::prelude::*;

/// Ordered fixed-timestep phases for physics-affecting work.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FixedGameSet {
    Actuation,
    Motion,
    Contacts,
    Consequences,
}

/// Ordered per-frame phases for input, visuals, and audio dispatch.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FrameSet {
    Input,
    Gameplay,
    Camera,
    Ambient,
    Presentation,
    Audio,
}

pub(crate) trait GameScheduleAppExt {
    fn configure_game_schedules(&mut self) -> &mut Self;
}

impl GameScheduleAppExt for App {
    fn configure_game_schedules(&mut self) -> &mut Self {
        self.configure_sets(
            FixedUpdate,
            (
                FixedGameSet::Actuation,
                FixedGameSet::Motion,
                FixedGameSet::Contacts,
                FixedGameSet::Consequences,
            )
                .chain(),
        )
        .configure_sets(
            Update,
            (
                FrameSet::Input,
                FrameSet::Gameplay,
                FrameSet::Camera,
                FrameSet::Ambient,
                FrameSet::Presentation,
                FrameSet::Audio,
            )
                .chain(),
        )
    }
}
