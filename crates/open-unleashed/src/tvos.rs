//! **PLATFORM GLUE (tvOS)** — the start-up the Apple TV app shell drives (`platform/tvos/`, TVOS.md).
//!
//! On the desktop the game data is found before the window opens and a missing folder ends the start.
//! On tvOS the data is downloaded by the app shell's own Game Data screen into `Caches/game/` *after*
//! UIKit is up, and winit's UIKit backend owns the process (`UIApplicationMain`, never returns). So:
//!
//! 1. the shell's `main` calls [`ou_main`] with its callbacks;
//! 2. the winit event loop starts at once with a [`Boot`] handler (no game data needed);
//! 3. on the first `resumed` the shell's `launched` callback runs: it checks `Caches/game/` against the
//!    manifest and shows the Game Data screen, or calls [`ou_data_ready`] straight away;
//! 4. on [`ou_data_ready`] the handler runs the desktop's [`crate::build_app`] and from then on hands
//!    every event to the `App`, exactly as the desktop's `run_app(&mut app)` does.

use crate::*;
use std::ffi::{CString, c_char};
use std::sync::atomic::{AtomicBool, Ordering};

/// Set by the shell ([`ou_data_ready`]) when `Caches/game/` is complete and verified.
static DATA_READY: AtomicBool = AtomicBool::new(false);
/// Set by the shell's scene delegate ([`ou_scene_ready`]): windows may be made from now on.
static SCENE_READY: AtomicBool = AtomicBool::new(false);
/// Set by the shell ([`ou_request_quit`]) — e.g. the "Quit" of the Game Data screen.
static QUIT: AtomicBool = AtomicBool::new(false);

/// The app shell's callbacks (all called on the main thread).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OuShellCallbacks {
    /// UIKit is up: check the game data, show the Game Data screen or call [`ou_data_ready`].
    pub launched: Option<extern "C" fn()>,
    /// The engine could not start from the data the shell reported ready (a NUL-terminated message):
    /// show the Game Data screen again with this error.
    pub engine_error: Option<extern "C" fn(*const c_char)>,
}

/// The shell's entry: runs the engine's event loop (never returns on tvOS — UIKit ends the process).
///
/// # Safety
/// `callbacks` must be null or point to a valid [`OuShellCallbacks`] for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ou_main(callbacks: *const OuShellCallbacks) -> i32 {
    let cb = if callbacks.is_null() { OuShellCallbacks { launched: None, engine_error: None } } else { unsafe { *callbacks } };
    SHELL.with(|s| s.set(Some(cb)));
    crate::main();
    0
}

/// The shell: the game data in `Caches/game/` is complete — start the game.
#[unsafe(no_mangle)]
pub extern "C" fn ou_data_ready() {
    DATA_READY.store(true, Ordering::Release);
}

/// The shell: the window scene is connected (UIScene life cycle, required by the tvOS 27 SDK; winit 0.30
/// predates scenes, so nothing may make a window before this).
#[unsafe(no_mangle)]
pub extern "C" fn ou_scene_ready() {
    SCENE_READY.store(true, Ordering::Release);
}

/// The shell: end the app (from the Game Data screen).
#[unsafe(no_mangle)]
pub extern "C" fn ou_request_quit() {
    QUIT.store(true, Ordering::Release);
}

thread_local! {
    static SHELL: std::cell::Cell<Option<OuShellCallbacks>> = const { std::cell::Cell::new(None) };
}

fn shell() -> OuShellCallbacks {
    SHELL.with(|s| s.get()).unwrap_or(OuShellCallbacks { launched: None, engine_error: None })
}

/// The tvOS `run`: the event loop first, the `App` when the data is there.
pub(crate) fn run() -> Result<()> {
    let event_loop = new_event_loop(false)?;
    let mut boot = Boot { app: None, resumed: false, launched: false };
    event_loop.run_app(&mut boot)?;
    Ok(())
}

/// The event handler before and after the game data exists.
struct Boot {
    app: Option<App>,
    /// UIKit has sent `resumed` (the `App` needs it once it exists)
    resumed: bool,
    /// the shell's `launched` callback has run
    launched: bool,
}

impl Boot {
    /// Once UIKit has resumed and the scene is connected: the shell's `launched` (once).
    fn try_launch(&mut self, event_loop: &ActiveEventLoop) {
        if self.launched || !self.resumed || !SCENE_READY.load(Ordering::Acquire) {
            return;
        }
        self.launched = true;
        match shell().launched {
            Some(f) => f(),
            // no shell (a bare test run): start from whatever data is found
            None => ou_data_ready(),
        }
        self.try_start(event_loop);
    }

    fn try_start(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_some() || !DATA_READY.swap(false, Ordering::AcqRel) {
            return;
        }
        match build_app(&mut |_| Ok(())) {
            Ok(None) => event_loop.exit(),
            Ok(Some(mut app)) => {
                tracing::info!("tvOS: game data ready, the engine starts");
                event_loop.set_control_flow(ControlFlow::Poll);
                if self.resumed {
                    app.resumed(event_loop);
                }
                self.app = Some(app);
            }
            Err(e) => {
                tracing::error!("tvOS: the engine could not start: {e:#}");
                if let Some(f) = shell().engine_error {
                    let msg = CString::new(format!("{e:#}").replace('\0', " ")).unwrap_or_default();
                    f(msg.as_ptr());
                }
            }
        }
    }
}

impl ApplicationHandler for Boot {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.resumed = true;
        if let Some(app) = self.app.as_mut() {
            app.resumed(event_loop);
            return;
        }
        // poll the shell's flags ten times a second until the game runs
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100)));
        self.try_launch(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(app) = self.app.as_mut() {
            app.suspended(event_loop);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if QUIT.load(Ordering::Acquire) {
            event_loop.exit();
            return;
        }
        match self.app.as_mut() {
            Some(app) => {
                app.about_to_wait(event_loop);
                // winit's UIKit backend delivers redraws only between run-loop iterations: a redraw the
                // engine requests from inside its frame (as on the desktop) is lost there, so the next
                // frame is asked for here, every iteration (the frame pacing stays the engine's own).
                if let Some(run) = app.intro.as_ref() {
                    run.window.request_redraw();
                } else if let Some(w) = app.window.as_ref() {
                    w.request_redraw();
                }
            }
            None => {
                self.try_launch(event_loop);
                self.try_start(event_loop);
                if self.app.is_none() {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100)));
                }
            }
        }
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(app) = self.app.as_mut() {
            app.exiting(event_loop);
        }
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        if let Some(app) = self.app.as_mut() {
            app.device_event(event_loop, id, event);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(app) = self.app.as_mut() {
            app.window_event(event_loop, id, event);
        }
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(app) = self.app.as_mut() {
            app.memory_warning(event_loop);
        }
    }
}
