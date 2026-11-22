//! **PORT-DEVIATION (head-look, `airpods-plan.md` H)** — turning your head (AirPods head tracking, Mac only) looks
//! around inside the car. This is the pure signal path, platform-free and unit-tested; the Mac app feeds it the
//! headphones' attitude once a frame and applies the result to the bumper / driver view's look rotation.
//! Render-only: it never touches the simulation or a replay.
//!
//! attitude → yaw/pitch relative to a reference (captured at start / recentre) → dead zone → gain curve →
//! clamp → low-pass (also smooths the sensor's rate, lower than 60 fps). A **jump guard** recentres on a yaw jump
//! of more than 30° within one sample (a reported quirk when spatial audio switches on); **drift correction**
//! pulls the reference toward the current yaw at ~1°/s while the head is near centre; a held look key wins
//! (head-look fades out over 150 ms and back in after).

/// Tuning (Extras › Controller on the Mac).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadLookTuning {
    /// view degrees per head degree past the dead zone (default 20° head → 75° view)
    pub gain: f32,
    /// head degrees ignored around the centre
    pub dead_zone: f32,
    /// the most the view turns
    pub max_yaw: f32,
    /// pitch follows too (small gain, ±15°)
    pub pitch: bool,
    /// low-pass time constant, seconds
    pub smoothing: f32,
}

impl Default for HeadLookTuning {
    fn default() -> Self {
        Self { gain: 75.0 / 17.0, dead_zone: 3.0, max_yaw: 110.0, pitch: false, smoothing: 0.08 }
    }
}

/// Strength presets (Extras): the gain for "20° of head turn".
pub fn gain_for(view_at_20: f32, dead_zone: f32) -> f32 {
    view_at_20 / (20.0 - dead_zone).max(1.0)
}

#[derive(Debug, Clone, Default)]
pub struct HeadLook {
    reference: Option<(f32, f32)>,
    last_raw_yaw: Option<f32>,
    /// the filtered output, degrees
    yaw: f32,
    pitch: f32,
    /// 0…1: how much of the head-look is applied (fades for a held look key)
    weight: f32,
}

/// Wrap an angle difference into −180…180°.
fn wrap(d: f32) -> f32 {
    let mut d = d % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

fn dead(v: f32, dz: f32) -> f32 {
    if v.abs() <= dz { 0.0 } else { v - dz * v.signum() }
}

impl HeadLook {
    /// Forget the reference: the next sample becomes "straight ahead" (race start, unpause, the recentre button).
    pub fn recentre(&mut self) {
        self.reference = None;
        self.last_raw_yaw = None;
    }

    /// One frame. `raw` = the headphones' (yaw, pitch) in degrees, or `None` when no sample (disconnected /
    /// stopped: the output eases back to centre). `look_key` = a look key / stick is held (it wins).
    /// Returns the (yaw, pitch) to add to the view, degrees (yaw positive = turn left, like the look keys).
    pub fn update(&mut self, raw: Option<(f32, f32)>, dt: f32, look_key: bool, t: &HeadLookTuning) -> (f32, f32) {
        let dt = dt.clamp(0.0, 0.1);
        // a held look key fades head-look out in 150 ms, back in likewise
        let w_target = if look_key { 0.0 } else { 1.0 };
        let step = dt / 0.15;
        self.weight = if self.weight < w_target { (self.weight + step).min(w_target) } else { (self.weight - step).max(w_target) };

        let (target_yaw, target_pitch) = match raw {
            None => (0.0, 0.0),
            Some((yaw, pitch)) => {
                // jump guard
                if let Some(last) = self.last_raw_yaw
                    && wrap(yaw - last).abs() > 30.0
                {
                    self.reference = None;
                }
                self.last_raw_yaw = Some(yaw);
                let (ry, rp) = *self.reference.get_or_insert((yaw, pitch));
                let dy = wrap(yaw - ry);
                let dp = pitch - rp;
                // drift correction near centre, ~1°/s
                if dy.abs() < 10.0
                    && let Some(r) = self.reference.as_mut()
                {
                    let pull = (1.0 * dt).min(dy.abs());
                    r.0 = r.0 + pull * dy.signum();
                }
                let vy = (dead(dy, t.dead_zone) * t.gain).clamp(-t.max_yaw, t.max_yaw);
                let vp = if t.pitch { (dead(dp, t.dead_zone) * 0.5).clamp(-15.0, 15.0) } else { 0.0 };
                (vy, vp)
            }
        };
        // low-pass
        let a = if t.smoothing <= 0.0 { 1.0 } else { 1.0 - (-dt / t.smoothing).exp() };
        self.yaw += (target_yaw - self.yaw) * a;
        self.pitch += (target_pitch - self.pitch) * a;
        (self.yaw * self.weight, self.pitch * self.weight)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(h: &mut HeadLook, raw: Option<(f32, f32)>, frames: usize, key: bool) -> (f32, f32) {
        let t = HeadLookTuning::default();
        let mut out = (0.0, 0.0);
        for _ in 0..frames {
            out = h.update(raw, 1.0 / 60.0, key, &t);
        }
        out
    }

    #[test]
    fn first_sample_is_centre_and_dead_zone_holds() {
        let mut h = HeadLook::default();
        assert_eq!(run(&mut h, Some((40.0, 0.0)), 30, false).0.abs() < 0.01, true, "the start pose is straight ahead");
        let y = run(&mut h, Some((42.5, 0.0)), 60, false).0;
        assert!(y.abs() < 0.5, "inside the dead zone: {y}");
    }

    #[test]
    fn twenty_degrees_turns_about_seventy_five() {
        let mut h = HeadLook::default();
        run(&mut h, Some((0.0, 0.0)), 5, false);
        let y = run(&mut h, Some((20.0, 0.0)), 120, false).0;
        assert!((y - 75.0).abs() < 3.0, "{y}");
        let y = run(&mut h, Some((-60.0 + 360.0, 0.0)), 5, false).0; // wraps through 0/360; a 80° jump recentres
        assert!(y.abs() < 75.0);
    }

    #[test]
    fn clamp_and_jump_guard() {
        let mut h = HeadLook::default();
        run(&mut h, Some((0.0, 0.0)), 5, false);
        // turn slowly to 50° (no jump): clamped at 110
        for d in 0..=50 {
            run(&mut h, Some((d as f32, 0.0)), 2, false);
        }
        let y = run(&mut h, Some((50.0, 0.0)), 120, false).0;
        assert!(y <= 110.0 + 0.01 && y > 100.0, "{y}");
        // a 40° jump in one sample recentres instead of swinging
        let y = run(&mut h, Some((90.0, 0.0)), 120, false).0;
        assert!(y.abs() < 1.0, "{y}");
    }

    #[test]
    fn look_key_wins_and_no_sample_eases_back() {
        let mut h = HeadLook::default();
        run(&mut h, Some((0.0, 0.0)), 5, false);
        let y = run(&mut h, Some((20.0, 0.0)), 60, false).0;
        assert!(y > 60.0);
        let y = run(&mut h, Some((20.0, 0.0)), 12, true).0;
        assert!(y.abs() < 0.01, "a held look key fades head-look out within 150 ms: {y}");
        let y = run(&mut h, None, 120, false).0;
        assert!(y.abs() < 1.0, "disconnected: back to centre: {y}");
    }

    #[test]
    fn drift_correction_pulls_small_offsets_home() {
        let mut h = HeadLook::default();
        run(&mut h, Some((0.0, 0.0)), 5, false);
        // the head rests 6° off for 10 s: the reference follows, the view returns toward centre
        let y = run(&mut h, Some((6.0, 0.0)), 600, false).0;
        assert!(y.abs() < 1.0, "{y}");
    }
}
