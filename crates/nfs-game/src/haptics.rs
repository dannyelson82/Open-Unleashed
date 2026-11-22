//! **PORT-DEVIATION (Extras › Controller › Rumble: Enhanced, `enhancements-plan.md` §5)** — pad haptics beside the
//! original's force feedback (`nfs_physics::force_feedback`, mapped onto a pad's two motors in
//! `open_unleashed::app::force_feedback`, D24). Pure: the race loop hands it the car's state once a game tick and
//! sends what it returns; nothing here touches the simulation, so replays and network races are unaffected.
//!
//! * **Body motors** (on top of the original's levels, never below them): a surface texture by the body tracker's
//!   surface class (the original's own road rule decides *whether* a class rumbles; the texture is a guess until the
//!   classes are named on the device — the frame overlay shows the current one), the engine only in the top 15 % of
//!   the revs, the rev limiter as sharp buzz pulses, a thump per gear change (stronger down), and the original's
//!   collision jolt left as it is.
//! * **Trigger motors** (Xbox pads, `SDL_RumbleGamepadTriggers`): left = ABS chatter / a locked wheel, right =
//!   wheelspin and the limiter.
//! * **Adaptive triggers** (DualSense): left (brake) a light progressive resistance that turns into a vibration under
//!   ABS and stiffens on a locked wheel without it; right (throttle) a very light resistance, a vibration with
//!   wheelspin and limiter bursts.
//!
//! Every effect is scaled by Extras' "Trigger effects" (triggers) and the original's `FF.*` levels (body).

/// One game tick of the player's car, as the haptics read it.
#[derive(Debug, Clone, Copy, Default)]
pub struct HapticIn {
    pub speed_kmh: f32,
    /// the body tracker's surface nibble (`car+0x464`), the original road rule's input
    pub surface: u32,
    pub rpm: f32,
    pub redline: f32,
    /// the rev limiter has the ignition cut this tick
    pub limiter: bool,
    pub gear: usize,
    /// any wheel locked under braking
    pub locked: bool,
    /// the ABS assist is pulsing the brake (Extras › Driving assists › ABS)
    pub abs: bool,
    /// the driven wheels' slip beyond grip, 0 (none) … 1 (spinning)
    pub wheelspin: f32,
    pub throttle: f32,
    pub brake: f32,
    pub airborne: bool,
}

/// What a DualSense trigger does (GameController's `GCDualSenseAdaptiveTrigger` modes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TriggerFx {
    Off,
    /// resistance from `start` (0…1 of the travel) on, `strength` 0…1
    Resistance { start: f32, strength: f32 },
    /// a vibration from `start` on: `amplitude` 0…1 at `hz`
    Vibration { start: f32, amplitude: f32, hz: f32 },
}

/// One tick's output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HapticOut {
    /// body motors, 0…1, combined with (never below) the original's levels by the caller
    pub strong: f32,
    pub weak: f32,
    /// a one-shot pulse of both body motors: (level, ms)
    pub pulse: Option<(f32, u32)>,
    /// trigger motors (Xbox), 0…1
    pub trigger_left: f32,
    pub trigger_right: f32,
    /// adaptive triggers (DualSense)
    pub adaptive_left: TriggerFx,
    pub adaptive_right: TriggerFx,
}

impl HapticOut {
    pub const NONE: Self = Self { strong: 0.0, weak: 0.0, pulse: None, trigger_left: 0.0, trigger_right: 0.0, adaptive_left: TriggerFx::Off, adaptive_right: TriggerFx::Off };
}

/// A surface's body texture (the guessed table; `surface` = the original's class).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Texture {
    /// smooth: nothing added
    Smooth,
    /// rough tarmac / cobbles: a steady low rumble
    Rough,
    /// gravel / dirt: random grainy noise on the strong motor
    Gravel,
    /// grass: a soft low wobble
    Grass,
    /// kerbs: short pulses at a speed-dependent rate
    Kerb,
}

/// The guessed surface table: from how the original's road rule (0x412C98) treats each class — classes it keeps
/// silent are smooth, the ones it makes slower and much stronger (+2000) loose ground, the faster-and-stronger one a
/// kerb. To be confirmed on the device (the overlay shows the class).
pub fn texture(surface: u32) -> Texture {
    match surface {
        7 | 11 => Texture::Gravel,
        5 => Texture::Kerb,
        4 => Texture::Rough,
        3 | 9 => Texture::Grass,
        _ => Texture::Smooth,
    }
}

/// The enhanced layer's state between ticks.
#[derive(Debug, Clone, Default)]
pub struct Haptics {
    gear: usize,
    tick: u64,
    rng: u32,
    limiter_on: u32,
}

/// Game ticks a second (the haptics' clock).
const TICK_HZ: f32 = 64.0;

impl Haptics {
    pub fn new() -> Self {
        Self { rng: 0x2545_F491, ..Default::default() }
    }

    /// A race starts: no gear change is felt for the gear the car starts in.
    pub fn reset(&mut self, gear: usize) {
        *self = Self { gear, ..Self::new() };
    }

    fn noise(&mut self) -> f32 {
        // xorshift: the grain only has to look random, never repeat a pattern you can feel
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// One game tick. `triggers` = Extras' trigger strength (0 off, 0.5 subtle, 1 strong).
    pub fn tick(&mut self, c: &HapticIn, triggers: f32) -> HapticOut {
        self.tick += 1;
        let mut o = HapticOut::NONE;
        let moving = c.speed_kmh > 3.0 && !c.airborne;
        // surface texture, growing with speed up to 120 km/h
        let sp = (c.speed_kmh / 120.0).clamp(0.0, 1.0);
        if moving {
            o.strong = match texture(c.surface) {
                Texture::Smooth => 0.0,
                Texture::Rough => 0.18 * sp,
                Texture::Gravel => (0.15 + 0.35 * self.noise()) * sp.max(0.3),
                Texture::Grass => 0.12 * (0.6 + 0.4 * (self.tick as f32 * 2.0 * std::f32::consts::PI * 3.0 / TICK_HZ).sin()) * sp.max(0.3),
                Texture::Kerb => {
                    // a kerb's ridges: one pulse every (0.4 s at walking pace … 0.07 s at speed)
                    let every = (TICK_HZ * (0.4 - 0.33 * sp)).max(4.0) as u64;
                    if self.tick % every == 0 { 0.6 } else { 0.0 }
                }
            };
        }
        // engine: only the top 15 % of the revs
        if c.redline > 0.0 {
            let top = ((c.rpm / c.redline - 0.85) / 0.15).clamp(0.0, 1.0);
            o.weak = 0.35 * top;
        }
        // rev limiter: sharp buzz pulses while the ignition is cut
        if c.limiter {
            self.limiter_on += 1;
            if self.limiter_on % 6 == 1 {
                o.pulse = Some((0.45, 40));
            }
        } else {
            self.limiter_on = 0;
        }
        // gear change: one thump, stronger down (the first tick only learns the gear the car starts in)
        if self.tick == 1 {
            self.gear = c.gear;
        }
        if c.gear != self.gear {
            let down = c.gear < self.gear && c.gear > 0;
            if self.gear != 0 || c.gear != 0 {
                o.pulse = Some((if down { 0.55 } else { 0.35 }, 60));
            }
            self.gear = c.gear;
        }
        if triggers <= 0.0 {
            return o;
        }
        // trigger motors (Xbox)
        let abs_chatter = c.abs && c.brake > 0.1;
        o.trigger_left = triggers
            * if abs_chatter {
                // on / off at ~12 Hz: ABS chatter
                if (self.tick / 3) % 2 == 0 { 0.7 } else { 0.15 }
            } else if c.locked && c.brake > 0.1 {
                0.45
            } else {
                0.0
            };
        let spin = (c.wheelspin * c.throttle).clamp(0.0, 1.0);
        o.trigger_right = triggers * (0.8 * spin).max(if c.limiter { 0.5 } else { 0.0 });
        // adaptive triggers (DualSense)
        o.adaptive_left = if abs_chatter {
            TriggerFx::Vibration { start: 0.2, amplitude: 0.6 * triggers, hz: 14.0 }
        } else if c.locked && c.brake > 0.1 {
            TriggerFx::Resistance { start: 0.15, strength: 0.8 * triggers }
        } else {
            TriggerFx::Resistance { start: 0.1, strength: 0.25 * triggers }
        };
        o.adaptive_right = if spin > 0.15 {
            TriggerFx::Vibration { start: 0.1, amplitude: (0.3 + 0.5 * spin) * triggers, hz: 30.0 }
        } else if c.limiter {
            TriggerFx::Vibration { start: 0.1, amplitude: 0.5 * triggers, hz: 40.0 }
        } else {
            TriggerFx::Resistance { start: 0.05, strength: 0.08 * triggers }
        };
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car() -> HapticIn {
        HapticIn { speed_kmh: 100.0, surface: 1, rpm: 4000.0, redline: 7000.0, gear: 3, throttle: 0.5, ..Default::default() }
    }

    #[test]
    fn smooth_road_mid_revs_adds_nothing() {
        let mut h = Haptics::new();
        h.reset(3);
        let o = h.tick(&car(), 0.0);
        assert_eq!((o.strong, o.weak, o.pulse), (0.0, 0.0, None));
    }

    #[test]
    fn gravel_rumbles_and_varies() {
        let mut h = Haptics::new();
        h.reset(3);
        let c = HapticIn { surface: 7, ..car() };
        let a: Vec<f32> = (0..20).map(|_| h.tick(&c, 0.0).strong).collect();
        assert!(a.iter().all(|&v| v > 0.1 && v <= 0.5), "{a:?}");
        assert!(a.iter().any(|&v| (v - a[0]).abs() > 0.05), "grainy, not constant: {a:?}");
        // standing still: nothing
        let still = HapticIn { speed_kmh: 0.0, ..c };
        assert_eq!(h.tick(&still, 0.0).strong, 0.0);
    }

    #[test]
    fn engine_only_near_the_redline_and_the_limiter_buzzes() {
        let mut h = Haptics::new();
        h.reset(3);
        assert_eq!(h.tick(&HapticIn { rpm: 5900.0, ..car() }, 0.0).weak, 0.0);
        assert!(h.tick(&HapticIn { rpm: 6900.0, ..car() }, 0.0).weak > 0.25);
        let lim = HapticIn { rpm: 7000.0, limiter: true, ..car() };
        let pulses = (0..12).filter(|_| h.tick(&lim, 0.0).pulse.is_some()).count();
        assert_eq!(pulses, 2, "one buzz every 6 ticks");
    }

    #[test]
    fn a_gear_change_thumps_once_and_harder_down() {
        let mut h = Haptics::new();
        h.reset(0);
        // the first tick learns the starting gear: no thump for it
        assert!(h.tick(&car(), 0.0).pulse.is_none());
        let up = h.tick(&HapticIn { gear: 4, ..car() }, 0.0).pulse.unwrap();
        assert!(h.tick(&HapticIn { gear: 4, ..car() }, 0.0).pulse.is_none());
        let down = h.tick(&HapticIn { gear: 3, ..car() }, 0.0).pulse.unwrap();
        assert!(down.0 > up.0 && up.1 == 60);
    }

    #[test]
    fn triggers_follow_abs_lock_and_wheelspin_and_switch_off() {
        let mut h = Haptics::new();
        h.reset(3);
        let abs = HapticIn { abs: true, brake: 1.0, throttle: 0.0, ..car() };
        let o = h.tick(&abs, 1.0);
        assert!(matches!(o.adaptive_left, TriggerFx::Vibration { .. }));
        let ls: Vec<f32> = (0..12).map(|_| h.tick(&abs, 1.0).trigger_left).collect();
        assert!(ls.iter().any(|&v| v > 0.6) && ls.iter().any(|&v| v < 0.2), "chatter: {ls:?}");
        let lock = HapticIn { locked: true, brake: 1.0, ..car() };
        assert!(matches!(h.tick(&lock, 1.0).adaptive_left, TriggerFx::Resistance { strength, .. } if strength > 0.5));
        let spin = HapticIn { wheelspin: 1.0, throttle: 1.0, ..car() };
        let o = h.tick(&spin, 0.5);
        assert!(o.trigger_right > 0.3 && matches!(o.adaptive_right, TriggerFx::Vibration { .. }));
        // "Trigger effects: Off" leaves the triggers alone
        let o = h.tick(&spin, 0.0);
        assert_eq!((o.trigger_left, o.trigger_right, o.adaptive_left, o.adaptive_right), (0.0, 0.0, TriggerFx::Off, TriggerFx::Off));
    }

    #[test]
    fn the_surface_table_matches_the_originals_rule() {
        // the classes the original keeps silent are smooth
        for s in [0, 1, 6, 8, 2, 10, 12] {
            assert_eq!(texture(s), Texture::Smooth, "{s}");
        }
        assert_eq!(texture(7), Texture::Gravel);
        assert_eq!(texture(5), Texture::Kerb);
    }
}
