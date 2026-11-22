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
        // PORT-DEVIATION: no trigger effect stays on under the pause menu
        self.haptics_off();
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
        let now = self.gamepads.racing_pad_count();
        let before = PADS_SEEN.swap(now, Ordering::Relaxed);
        if before != usize::MAX && now < before {
            self.request_pause("controller disconnected");
        }
    }
}

/// When the audio route was last checked.
static ROUTE_CHECKED: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
/// A platform route notice asks for a check now (tvOS: the app shell's AVAudioSession route change).
pub(crate) static ROUTE_FORCE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The platform says the old route went away (AirPods taken out / disconnected).
pub(crate) static ROUTE_OLD_GONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Headphones by name (the Mac has no route kind through cpal): AirPods, Beats, "Headphones", "Buds", Bluetooth.
fn is_headphones(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["airpods", "beats", "headphone", "buds", "bluetooth"].iter().any(|k| n.contains(k))
}

impl App {
    /// `airpods-plan.md` A/B/E, once a frame: every 2 s (or at once on a platform notice) the audio output follows
    /// the default device with the same mixer state; when headphones go away mid-race, the race pauses.
    pub(crate) fn audio_route_tick(&mut self) {
        let force = ROUTE_FORCE.swap(false, Ordering::AcqRel);
        let due = {
            let mut last = ROUTE_CHECKED.lock().unwrap();
            let due = force || last.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(2));
            if due {
                *last = Some(std::time::Instant::now());
            }
            due
        };
        if !due {
            return;
        }
        let Some(a) = self.snd.audio.as_mut() else { return };
        let Some(change) = a.route_check(force) else { return };
        let gone = ROUTE_OLD_GONE.swap(false, Ordering::AcqRel) || (is_headphones(&change.from) && !is_headphones(&change.to));
        if gone && nfs_game::extras::get().audio.pause_on_headphones_gone {
            if !self.request_pause("headphones disconnected") && self.fe.race_game_type == Some(game_type::NETWORK) {
                tracing::info!("headphones disconnected during a network race: no pause there (the original's rules)");
            }
        }
    }
}

/// tvOS route notices (`tvos.rs`).
#[cfg_attr(not(target_os = "tvos"), allow(dead_code))]
pub(crate) fn route_force() {
    ROUTE_FORCE.store(true, Ordering::Release);
}

#[cfg_attr(not(target_os = "tvos"), allow(dead_code))]
pub(crate) fn route_old_gone() {
    ROUTE_OLD_GONE.store(true, Ordering::Release);
}
