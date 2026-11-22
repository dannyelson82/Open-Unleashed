//! **PORT-DEVIATION (Extras › Controller › Trigger effects)** — a DualSense's adaptive triggers on Apple platforms,
//! through the GameController framework (`GCDualSenseAdaptiveTrigger`, tvOS 14.5+ / macOS 11.3+): SDL drives the pad's
//! rumble but exposes no trigger modes there. Every connected DualSense gets the same modes. A mode is only sent when
//! it changes (each call is a Bluetooth packet); [`reset`] turns both triggers off (menus, pause, background, exit).
//! Not tested on a device yet (none was available).

use objc2_game_controller::{GCController, GCDualSenseAdaptiveTrigger, GCDualSenseGamepad};
use std::sync::Mutex;

/// What one trigger does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TriggerMode {
    Off,
    /// resistance from `start` (0…1 of the travel), `strength` 0…1
    Resistance { start: f32, strength: f32 },
    /// a vibration from `start`: `amplitude` 0…1, `hz` (mapped onto the pad's 0…1 frequency range)
    Vibration { start: f32, amplitude: f32, hz: f32 },
}

impl TriggerMode {
    /// Rounded so that a value drifting by a hair does not re-send the mode.
    fn quantised(self) -> Self {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 20.0).round() / 20.0;
        match self {
            TriggerMode::Resistance { strength, .. } if q(strength) == 0.0 => TriggerMode::Off,
            TriggerMode::Vibration { amplitude, .. } if q(amplitude) == 0.0 => TriggerMode::Off,
            TriggerMode::Resistance { start, strength } => TriggerMode::Resistance { start: q(start), strength: q(strength) },
            TriggerMode::Vibration { start, amplitude, hz } => TriggerMode::Vibration { start: q(start), amplitude: q(amplitude), hz: (hz / 5.0).round() * 5.0 },
            TriggerMode::Off => TriggerMode::Off,
        }
    }
}

/// The highest vibration the frequency scale's 1.0 stands for (the DualSense's trigger motor tops out around here).
const MAX_HZ: f32 = 40.0;

static SENT: Mutex<Option<(TriggerMode, TriggerMode)>> = Mutex::new(None);

fn apply(t: &GCDualSenseAdaptiveTrigger, m: TriggerMode) {
    // SAFETY: plain Objective-C setters on a live trigger object
    unsafe {
        match m {
            TriggerMode::Off => t.setModeOff(),
            TriggerMode::Resistance { start, strength } => t.setModeFeedbackWithStartPosition_resistiveStrength(start, strength),
            TriggerMode::Vibration { start, amplitude, hz } => t.setModeVibrationWithStartPosition_amplitude_frequency(start, amplitude, (hz / MAX_HZ).clamp(0.0, 1.0)),
        }
    }
}

/// Set both triggers on every connected DualSense (no call when nothing changed). Returns whether one is connected.
pub fn set(left: TriggerMode, right: TriggerMode) -> bool {
    let (left, right) = (left.quantised(), right.quantised());
    let Ok(mut sent) = SENT.lock() else { return false };
    // SAFETY: GameController's class method, on the thread the game runs on
    let controllers = unsafe { GCController::controllers() };
    let pads: Vec<_> = controllers
        .iter()
        .filter_map(|c| {
            // SAFETY: an accessor of a live controller
            let g = unsafe { c.extendedGamepad() }?;
            g.downcast::<GCDualSenseGamepad>().ok()
        })
        .collect();
    if pads.is_empty() {
        *sent = None;
        return false;
    }
    if *sent == Some((left, right)) {
        return true;
    }
    for p in &pads {
        // SAFETY: accessors of a live DualSense
        unsafe {
            apply(&p.leftTrigger(), left);
            apply(&p.rightTrigger(), right);
        }
    }
    tracing::debug!(?left, ?right, pads = pads.len(), "DualSense adaptive triggers");
    *sent = Some((left, right));
    true
}

/// Both triggers off (menus, pause, background, disconnect, exit).
pub fn reset() {
    set(TriggerMode::Off, TriggerMode::Off);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_are_quantised_and_zero_strength_is_off() {
        assert_eq!(TriggerMode::Resistance { start: 0.1, strength: 0.01 }.quantised(), TriggerMode::Off);
        assert_eq!(TriggerMode::Vibration { start: 0.2, amplitude: 0.0, hz: 14.0 }.quantised(), TriggerMode::Off);
        assert_eq!(TriggerMode::Resistance { start: 0.12, strength: 0.26 }.quantised(), TriggerMode::Resistance { start: 0.1, strength: 0.25 });
        assert_eq!(TriggerMode::Vibration { start: 0.2, amplitude: 0.61, hz: 14.0 }.quantised(), TriggerMode::Vibration { start: 0.2, amplitude: 0.6, hz: 15.0 });
    }
}
