//! **PORT-DEVIATION (Mac head-look, `airpods-plan.md` H)** — AirPods head tracking turns the bumper / driver view.
//! macOS 14+ only: CoreMotion's `CMHeadphoneMotionManager` (no such API on tvOS). Polled once a frame, no
//! callbacks or threads; the signal path is `nfs_game::headlook`. Render-only.
//!
//! Started only when Extras › Controller › Head-look is on **and** a race runs in view 0x00 / 0x01, not paused,
//! not in photo mode; stopped otherwise. Recentred at race start, after a pause and on L3 + R3. Needs
//! `NSMotionUsageDescription` in the app's Info.plist (`packaging/make_app.sh`): without it (the bare `cargo run`
//! binary) macOS would kill the process on first use, so it then stays off and says why, once.

use crate::*;
use nfs_game::headlook::{HeadLook, HeadLookTuning, gain_for};
use objc2::rc::Retained;
use objc2_core_motion::{CMAuthorizationStatus, CMHeadphoneMotionManager};
use std::cell::RefCell;

#[derive(Default)]
struct Motion {
    mgr: Option<Retained<CMHeadphoneMotionManager>>,
    running: bool,
    /// a reason it can't run on this Mac / in this build (logged once)
    unusable: Option<&'static str>,
    filter: HeadLook,
    was_paused: bool,
    had_race: bool,
    recentre_combo: bool,
}

thread_local! {
    static MOTION: RefCell<Motion> = RefCell::new(Motion::default());
}

/// The running app declares why it wants motion data (else TCC kills it on first use).
fn has_usage_string() -> bool {
    use objc2_foundation::{NSBundle, NSString};
    let b = NSBundle::mainBundle();
    let key = NSString::from_str("NSMotionUsageDescription");
    b.objectForInfoDictionaryKey(&key).is_some()
}

impl Motion {
    fn start(&mut self) {
        if self.running || self.unusable.is_some() {
            return;
        }
        if !has_usage_string() {
            self.unusable = Some("this build has no NSMotionUsageDescription — run the app bundle (packaging/make_app.sh)");
        } else {
            match unsafe { CMHeadphoneMotionManager::authorizationStatus() } {
                CMAuthorizationStatus::Denied | CMAuthorizationStatus::Restricted => {
                    self.unusable = Some("motion access denied — System Settings › Privacy & Security › Motion & Fitness");
                }
                _ => {}
            }
        }
        if let Some(why) = self.unusable {
            tracing::warn!("head-look unavailable: {why}");
            return;
        }
        let mgr = self.mgr.get_or_insert_with(|| unsafe { CMHeadphoneMotionManager::new() });
        if !unsafe { mgr.isDeviceMotionAvailable() } {
            return; // no headphones with head tracking connected now; try again next frame
        }
        unsafe { mgr.startDeviceMotionUpdates() };
        self.running = true;
        self.filter.recentre();
        tracing::info!("head-look: AirPods motion updates started");
    }

    fn stop(&mut self) {
        if self.running {
            if let Some(m) = self.mgr.as_ref() {
                unsafe { m.stopDeviceMotionUpdates() };
            }
            self.running = false;
            tracing::info!("head-look: stopped");
        }
    }

    /// The headphones' (yaw, pitch) in degrees, if a sample is there.
    fn sample(&self) -> Option<(f32, f32)> {
        let m = self.mgr.as_ref()?;
        let dm = unsafe { m.deviceMotion() }?;
        let a = unsafe { dm.attitude() };
        Some((unsafe { a.yaw() }.to_degrees() as f32, unsafe { a.pitch() }.to_degrees() as f32))
    }
}

impl App {
    /// The head-look rotation for this frame (identity when off / not applicable).
    pub(crate) fn head_look(&mut self, view: u16, look_key: bool) -> Quat {
        let s = nfs_game::extras::get().controller;
        let race = self.vehicle.is_some();
        let wanted = s.head_look && race && matches!(view, 0x00 | 0x01) && !self.paused && !self.photo.active;
        // L3 + R3 on a standard pad recentres
        let combo = self.gamepads.records().iter().any(|r| r.standard && r.buttons[7] && r.buttons[8]);
        MOTION.with(|m| {
            let mut m = m.borrow_mut();
            // race start and the end of a pause recentre
            if (race && !m.had_race) || (m.was_paused && !self.paused) || (combo && !m.recentre_combo) {
                m.filter.recentre();
            }
            m.had_race = race;
            m.was_paused = self.paused;
            m.recentre_combo = combo;
            if !wanted {
                m.stop();
                // ease back to centre if it was turned
                let t = HeadLookTuning::default();
                let (y, _) = m.filter.update(None, self.frame_dt, look_key, &t);
                return if s.head_look && y.abs() > 0.05 && matches!(view, 0x00 | 0x01) { Quat::from_rotation_y(y.to_radians()) } else { Quat::IDENTITY };
            }
            m.start();
            if !m.running {
                return Quat::IDENTITY;
            }
            let dz = 3.0;
            let t = HeadLookTuning { gain: gain_for(s.head_look_strength.view_at_20(), dz), dead_zone: dz, pitch: s.head_look_pitch, ..HeadLookTuning::default() };
            let raw = m.sample();
            let (y, p) = m.filter.update(raw, self.frame_dt, look_key, &t);
            Quat::from_rotation_y(y.to_radians()) * Quat::from_rotation_x(p.to_radians())
        })
    }
}
