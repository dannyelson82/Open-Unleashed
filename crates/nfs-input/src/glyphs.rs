//! **PORT-DEVIATION (controller glyphs, `enhancements-plan.md` §1)** — the hints name the button of the device
//! used last: the keyboard's keys, or a pad's buttons by its family's own names (Xbox: A / B / X / Y, LB …;
//! PlayStation: Cross / Circle …, L1 …; Nintendo layout: B / A …). Text only, drawn in the game's font — no
//! console maker's artwork. The last device switches on a real press (a key, a pad button), not stick noise.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadFamily {
    Xbox,
    PlayStation,
    Nintendo,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastDevice {
    Keyboard,
    Pad(PadFamily),
}

/// A pad button by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    South,
    East,
    West,
    North,
    LeftShoulder,
    RightShoulder,
    LeftTrigger,
    RightTrigger,
    Back,
    Start,
    LeftStick,
    RightStick,
}

// 0 keyboard, 1 Xbox, 2 PlayStation, 3 Nintendo, 4 generic pad
static LAST: AtomicU8 = AtomicU8::new(if cfg!(target_os = "tvos") { 4 } else { 0 });

fn code(d: LastDevice) -> u8 {
    match d {
        LastDevice::Keyboard => 0,
        LastDevice::Pad(PadFamily::Xbox) => 1,
        LastDevice::Pad(PadFamily::PlayStation) => 2,
        LastDevice::Pad(PadFamily::Nintendo) => 3,
        LastDevice::Pad(PadFamily::Generic) => 4,
    }
}

/// The device the player used last (tvOS starts on "pad": it has no keyboard).
pub fn last() -> LastDevice {
    match LAST.load(Ordering::Relaxed) {
        0 => LastDevice::Keyboard,
        1 => LastDevice::Pad(PadFamily::Xbox),
        2 => LastDevice::Pad(PadFamily::PlayStation),
        3 => LastDevice::Pad(PadFamily::Nintendo),
        _ => LastDevice::Pad(PadFamily::Generic),
    }
}

pub fn note(d: LastDevice) {
    LAST.store(code(d), Ordering::Relaxed);
}

/// The family's name for a button.
pub fn name(f: PadFamily, b: Button) -> &'static str {
    use Button::*;
    match f {
        PadFamily::PlayStation => match b {
            South => "Cross",
            East => "Circle",
            West => "Square",
            North => "Triangle",
            LeftShoulder => "L1",
            RightShoulder => "R1",
            LeftTrigger => "L2",
            RightTrigger => "R2",
            Back => "Create",
            Start => "Options",
            LeftStick => "L3",
            RightStick => "R3",
        },
        PadFamily::Nintendo => match b {
            South => "B",
            East => "A",
            West => "Y",
            North => "X",
            LeftShoulder => "L",
            RightShoulder => "R",
            LeftTrigger => "ZL",
            RightTrigger => "ZR",
            Back => "-",
            Start => "+",
            LeftStick => "L stick",
            RightStick => "R stick",
        },
        PadFamily::Xbox | PadFamily::Generic => match b {
            South => "A",
            East => "B",
            West => "X",
            North => "Y",
            LeftShoulder => "LB",
            RightShoulder => "RB",
            LeftTrigger => "LT",
            RightTrigger => "RT",
            Back => "View",
            Start => "Menu",
            LeftStick => "LS",
            RightStick => "RS",
        },
    }
}

/// The name of a button on the pad used last (Xbox names when the keyboard was last).
pub fn last_name(b: Button) -> &'static str {
    match last() {
        LastDevice::Pad(f) => name(f, b),
        LastDevice::Keyboard => name(PadFamily::Xbox, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_family() {
        assert_eq!(name(PadFamily::Xbox, Button::South), "A");
        assert_eq!(name(PadFamily::PlayStation, Button::South), "Cross");
        assert_eq!(name(PadFamily::PlayStation, Button::Start), "Options");
        assert_eq!(name(PadFamily::Nintendo, Button::East), "A");
    }

    #[test]
    fn last_device_round_trip() {
        note(LastDevice::Pad(PadFamily::PlayStation));
        assert_eq!(last(), LastDevice::Pad(PadFamily::PlayStation));
        assert_eq!(last_name(Button::East), "Circle");
        note(LastDevice::Keyboard);
        assert_eq!(last(), LastDevice::Keyboard);
    }
}
