//! **PLATFORM GLUE** — pausing a race for reasons outside the game: a controller disconnecting (TVOS.md
//! Stage 5.5), the app going to the background (Stage 6.3), headphones going away (`airpods-plan.md` E).
//! One implementation, shared with the Pause key: the same state the original's pause sets (0x4151E3).

use crate::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Pads connected at the last hot-plug (`usize::MAX` = not yet known).
static PADS_SEEN: AtomicUsize = AtomicUsize::new(usize::MAX);

impl App {
    /// The single-player pause, exactly as the Pause key opens it: the menu, the orbit camera from 0
    /// (0x4151E3 → 0x461740), the first row, no question up.
    pub(crate) fn enter_pause(&mut self) {
        self.paused = true;
        self.orbit = Orbit::default();
        self.pause_sel = 0;
        self.pause_confirm = None;
    }

    /// Pause the race if one is running that can pause: single player (a network race has no pause in the
    /// original — the caller shows a notice instead), not already paused, no result page or HUD dialog up,
    /// not a replay playing back, not photo mode. Returns whether it paused. Never resumes by itself.
    pub(crate) fn request_pause(&mut self, reason: &str) -> bool {
        if self.vehicle.is_none() || self.paused || self.photo.active {
            return false;
        }
        if self.fe.race_game_type == Some(game_type::NETWORK) {
            tracing::info!(reason, "pause request in a network race: the original has no pause there");
            return false;
        }
        if self.results.is_some() || self.hud_dialog_up() {
            return false;
        }
        if self.replay.stream.as_ref().is_some_and(|r| matches!(r.mode, nfs_game::replay::Mode::Playing | nfs_game::replay::Mode::Exhausted)) {
            return false;
        }
        tracing::info!(reason, "race paused");
        self.enter_pause();
        true
    }

    /// After a pad hot-plug: the race pauses when the last pad went away mid-race (the keyboard / Siri Remote
    /// can't race on tvOS; on the Mac the keyboard could, but a pad vanishing mid-corner is still worth a pause).
    pub(crate) fn pad_hotplug_pause(&mut self) {
        let now = self.gamepads.records().len();
        let before = PADS_SEEN.swap(now, Ordering::Relaxed);
        if before != usize::MAX && now < before {
            self.request_pause("controller disconnected");
        }
    }
}
