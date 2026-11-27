//! A repair of SDL 3.4.18's Apple trigger rumble, made at run time.
//!
//! SDL's GameController (MFi) driver starts its trigger method with `bool result = false;` and then
//! `result &= …` for each trigger motor (`SDL_mfijoystick.m`, `-[SDL3_RumbleContext rumbleLeftTrigger:andRightTrigger:]`),
//! so it reports failure every time, even when the motors started. Its body-motor method beside it starts from
//! `true`. Because of the false failure, `SDL_RumbleJoystickTriggers` never records the triggers as running, and
//! a stop (0, 0) then equals what it has recorded and never reaches the driver: on an Apple TV the triggers kept
//! rumbling after a race and under the pause menu (real Apple TV, 2026-11-26 and 2026-11-27).
//!
//! [`install`] gives that method the body sets SDL meant: each trigger motor's `setIntensity:`, and success when
//! both succeed. Nothing else in SDL changes. Remove this once the SDL in use starts the method from `true`.

use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{msg_send, sel};

/// The repaired `-[SDL3_RumbleContext rumbleLeftTrigger:andRightTrigger:]`.
unsafe extern "C-unwind" fn rumble_triggers(this: *mut AnyObject, _cmd: Sel, left: u16, right: u16) -> bool {
    // SAFETY: `this` is the live SDL3_RumbleContext the message was sent to; its two properties are
    // SDL3_RumbleMotor objects or nil, and `setIntensity:` takes a float and returns a C bool
    unsafe {
        let l: *mut AnyObject = msg_send![this, leftTriggerMotor];
        let r: *mut AnyObject = msg_send![this, rightTriggerMotor];
        if l.is_null() || r.is_null() {
            return false;   // SDL: SDL_Unsupported()
        }
        let lok: bool = msg_send![l, setIntensity: left as f32 / 65535.0];
        let rok: bool = msg_send![r, setIntensity: right as f32 / 65535.0];
        lok && rok
    }
}

/// Swap in the repaired method, once (after SDL's joystick layer has started: the class is in the linked SDL).
pub(crate) fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let Some(class) = AnyClass::get(c"SDL3_RumbleContext") else {
            tracing::warn!("SDL trigger rumble repair: no SDL3_RumbleContext class (SDL changed?)");
            return;
        };
        let Some(method) = class.instance_method(sel!(rumbleLeftTrigger:andRightTrigger:)) else {
            tracing::warn!("SDL trigger rumble repair: no rumbleLeftTrigger:andRightTrigger: (SDL changed?)");
            return;
        };
        // SAFETY: the replacement has the method's exact signature (self, _cmd, Uint16, Uint16) -> bool, and an
        // IMP is called through that signature only
        unsafe {
            let imp: Imp = std::mem::transmute::<unsafe extern "C-unwind" fn(*mut AnyObject, Sel, u16, u16) -> bool, Imp>(rumble_triggers);
            method.set_implementation(imp);
        }
        tracing::info!("SDL trigger rumble repair installed");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The linked SDL has the class and method, and after SDL starts the method runs the repair.
    #[test]
    fn the_repair_replaces_sdls_trigger_method() {
        let _pads = crate::gamepad::Gamepads::new();
        install();
        let class = AnyClass::get(c"SDL3_RumbleContext").expect("SDL's rumble class is linked");
        let method = class.instance_method(sel!(rumbleLeftTrigger:andRightTrigger:)).expect("SDL's trigger method");
        let ours: unsafe extern "C-unwind" fn(*mut AnyObject, Sel, u16, u16) -> bool = rumble_triggers;
        assert_eq!(method.implementation() as usize, ours as usize);
    }
}
