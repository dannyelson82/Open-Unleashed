//! **PORT-DEVIATION (driving assists, `enhancements-plan.md` §4)** — optional help for a gamepad, applied to the
//! **local human player's** 32 Hz controller packet only, after it is sampled and before the physics, the replay
//! recorder and the network see it — so a replay plays back exactly what was driven and the other players get the
//! same packet the local physics used. Every default is off (the original's handling). Pure and unit-tested.

/// The assist settings (Extras › Driving assists).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AssistSettings {
    /// Steering response: 1.0 = linear (the original); 1.5 / 2.0 = softer around the centre.
    pub curve: f32,
    /// Speed-sensitive steering: the share of full lock left at 200 km/h (1.0 = off).
    pub lock_at_200: f32,
    /// Countersteer assist gain (0 = off).
    pub countersteer: f32,
    pub abs: bool,
}

impl AssistSettings {
    pub fn off() -> Self {
        Self { curve: 1.0, lock_at_200: 1.0, countersteer: 0.0, abs: false }
    }

    pub fn any(&self) -> bool {
        self.curve > 1.0 || self.lock_at_200 < 1.0 || self.countersteer > 0.0 || self.abs
    }
}

/// What the assists read of the player's car this tick.
#[derive(Debug, Clone, Copy, Default)]
pub struct CarState {
    /// m/s, along the car's heading (positive = forwards)
    pub forward: f32,
    /// m/s, sideways in the car's frame (positive = to the car's right)
    pub lateral: f32,
    /// any wheel locked under braking
    pub any_locked: bool,
}

/// The assists' memory between ticks.
#[derive(Debug, Clone, Copy, Default)]
pub struct AssistState {
    /// ABS: ticks of release left, then ticks of hold before the next release may start
    abs_release: u8,
    abs_hold: u8,
    /// which assists acted this tick (HUD indicators): bit 0 ABS, bit 1 countersteer
    pub acted: u8,
}

/// The packet's three axes: throttle and brake 0…252 (even), steer −126…126 (even, + = right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Axes {
    pub throttle: u8,
    pub brake: u8,
    pub steer: i32,
}

const MAX_STEER_RAD: f32 = 0.6;

/// Apply the assists to one tick's packet.
pub fn apply(mut a: Axes, car: &CarState, s: &AssistSettings, st: &mut AssistState) -> Axes {
    st.acted = 0;
    if !s.any() {
        return a;
    }
    let speed = car.forward.hypot(car.lateral);
    // 1. steering response: curve, then speed sensitivity
    let mut x = a.steer as f32 / 126.0;
    if s.curve > 1.0 {
        x = x.signum() * x.abs().powf(s.curve);
    }
    if s.lock_at_200 < 1.0 {
        let k = (speed * 3.6 / 200.0).clamp(0.0, 1.0);
        x *= 1.0 - (1.0 - s.lock_at_200) * k;
    }
    // 2. countersteer: steer toward where the car is going when the body slides (β past 4°, above 18 km/h)
    if s.countersteer > 0.0 && car.forward > 5.0 {
        let beta = car.lateral.atan2(car.forward);
        if beta.abs() > 4f32.to_radians() {
            let assist = ((beta / MAX_STEER_RAD) * s.countersteer).clamp(-0.3, 0.3);
            let before = x;
            x = if x * assist >= 0.0 {
                // the driver already steers that way (or not at all): never less than the driver asks
                if assist.abs() > x.abs() { assist } else { x }
            } else {
                // the driver steers the other way: the assist takes some of it back
                x + assist
            };
            if x != before {
                st.acted |= 2;
            }
        }
    }
    a.steer = ((x.clamp(-1.0, 1.0) * 126.0 / 2.0).round() as i32).clamp(-63, 63) * 2;
    // 3. ABS: on a locked wheel under braking above 10 km/h, release to 40 % for 2 ticks, then hold 1
    if s.abs {
        if st.abs_release > 0 {
            st.abs_release -= 1;
            a.brake = ((a.brake as u32 * 2 / 5) as u8) & !1;
            st.acted |= 1;
            if st.abs_release == 0 {
                st.abs_hold = 1;
            }
        } else if st.abs_hold > 0 {
            st.abs_hold -= 1;
        } else if a.brake > 0 && car.any_locked && speed * 3.6 > 10.0 {
            st.abs_release = 1;
            a.brake = ((a.brake as u32 * 2 / 5) as u8) & !1;
            st.acted |= 1;
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axes(t: u8, b: u8, s: i32) -> Axes {
        Axes { throttle: t, brake: b, steer: s }
    }

    #[test]
    fn off_is_the_original() {
        let mut st = AssistState::default();
        let car = CarState { forward: 40.0, lateral: 8.0, any_locked: true };
        let a = axes(100, 200, -60);
        assert_eq!(apply(a, &car, &AssistSettings::off(), &mut st), a);
    }

    #[test]
    fn curve_softens_the_centre_keeps_full_lock() {
        let s = AssistSettings { curve: 2.0, ..AssistSettings::off() };
        let mut st = AssistState::default();
        let car = CarState::default();
        assert_eq!(apply(axes(0, 0, 126), &car, &s, &mut st).steer, 126);
        assert_eq!(apply(axes(0, 0, -126), &car, &s, &mut st).steer, -126);
        let half = apply(axes(0, 0, 64), &car, &s, &mut st).steer;
        assert!((30..=34).contains(&half), "{half}");
        assert_eq!(half % 2, 0, "packet values stay even");
    }

    #[test]
    fn speed_sensitive_lock() {
        let s = AssistSettings { lock_at_200: 0.5, ..AssistSettings::off() };
        let mut st = AssistState::default();
        let slow = apply(axes(0, 0, 126), &CarState { forward: 0.0, ..Default::default() }, &s, &mut st).steer;
        let fast = apply(axes(0, 0, 126), &CarState { forward: 200.0 / 3.6, ..Default::default() }, &s, &mut st).steer;
        assert_eq!(slow, 126);
        assert_eq!(fast, 64);
    }

    #[test]
    fn countersteer_sign_cap_and_no_fighting() {
        let s = AssistSettings { countersteer: 1.0, ..AssistSettings::off() };
        let mut st = AssistState::default();
        // sliding to the right (velocity points right of the heading): steer right, at most 30 % of lock
        let car = CarState { forward: 30.0, lateral: 12.0, any_locked: false };
        let out = apply(axes(0, 0, 0), &car, &s, &mut st).steer;
        assert!(out > 0 && out <= 38, "{out}");
        assert_eq!(st.acted & 2, 2);
        // the driver countersteers harder already: untouched
        assert_eq!(apply(axes(0, 0, 100), &car, &s, &mut st).steer, 100);
        // the driver steers into the slide's wrong side: reduced
        let wrong = apply(axes(0, 0, -100), &car, &s, &mut st).steer;
        assert!(wrong > -100 && wrong < 0, "{wrong}");
        // no slide: nothing
        let straight = CarState { forward: 30.0, lateral: 0.5, any_locked: false };
        assert_eq!(apply(axes(0, 0, 20), &straight, &s, &mut st).steer, 20);
    }

    #[test]
    fn abs_pulses_on_lock() {
        let s = AssistSettings { abs: true, ..AssistSettings::off() };
        let mut st = AssistState::default();
        let locked = CarState { forward: 30.0, lateral: 0.0, any_locked: true };
        let seq: Vec<u8> = (0..6).map(|_| apply(axes(0, 252, 0), &locked, &s, &mut st).brake).collect();
        // release, release, hold, release, release, hold
        assert_eq!(seq, vec![100, 100, 252, 100, 100, 252]);
        // below 10 km/h or not braking: nothing
        let mut st = AssistState::default();
        let slow = CarState { forward: 2.0, lateral: 0.0, any_locked: true };
        assert_eq!(apply(axes(0, 252, 0), &slow, &s, &mut st).brake, 252);
        let free = CarState { forward: 30.0, lateral: 0.0, any_locked: false };
        assert_eq!(apply(axes(0, 252, 0), &free, &s, &mut st).brake, 252);
    }
}
