//! Actions sharing the pointer's FIFO and Picking cycle barriers.

use std::time::{Duration, Instant};

use bevy::picking::pointer::{PointerAction, PointerButton};
use bevy::prelude::Vec2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HoldKind {
    Timed,
    Automatic,
}

pub(super) enum Action {
    Input(PointerAction),
    Hold {
        button: PointerButton,
        duration: Duration,
        kind: HoldKind,
    },
    Wait {
        duration: Duration,
        deadline: Option<Instant>,
    },
    Drag {
        button: PointerButton,
        start: Vec2,
        end: Vec2,
        frames: u32,
        phase: DragPhase,
    },
}

#[derive(Clone, Copy)]
pub(super) enum DragPhase {
    Locate,
    Press,
    Move(u32),
    Release,
}

impl Action {
    pub(super) fn timed_button(&self) -> Option<PointerButton> {
        match self {
            Self::Hold {
                button,
                kind: HoldKind::Timed,
                ..
            } => Some(*button),
            _ => None,
        }
    }

    pub(super) fn needs_free_pointer(&self) -> bool {
        matches!(
            self,
            Self::Hold {
                kind: HoldKind::Automatic,
                ..
            } | Self::Drag {
                phase: DragPhase::Locate,
                ..
            }
        )
    }
}
