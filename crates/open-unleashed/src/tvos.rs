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
    /// A field wants text: show the on-screen keyboard (title, current text, maximum length, secure) and
    /// answer with [`ou_text_entered`] (NULL = cancelled).
    pub text_prompt: Option<extern "C" fn(*const c_char, *const c_char, u32, bool)>,
}

/// The shell's entry: runs the engine's event loop (never returns on tvOS — UIKit ends the process).
///
/// # Safety
/// `callbacks` must be null or point to a valid [`OuShellCallbacks`] for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ou_main(callbacks: *const OuShellCallbacks) -> i32 {
    let cb = if callbacks.is_null() { OuShellCallbacks { launched: None, engine_error: None, text_prompt: None } } else { unsafe { *callbacks } };
    SHELL.with(|s| s.set(Some(cb)));
    // no mouse on tvOS: the front end's pointer is parked outside the 640×480 design, so it neither shows nor
    // hovers (the same device a scripted capture uses, `NFS_FE_CURSOR`; TVOS.md)
    if std::env::var_os("NFS_FE_CURSOR").is_none() {
        // SAFETY: before any thread is started
        unsafe { std::env::set_var("NFS_FE_CURSOR", "-1000,-1000") };
    }
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

/// App life cycle from the shell's scene delegate (scene apps don't get the app-level callbacks winit listens to).
static BACKGROUND: AtomicBool = AtomicBool::new(false);
static FOREGROUND: AtomicBool = AtomicBool::new(false);

/// The shell: the app is leaving the foreground (Home button, another app, the screen saver).
#[unsafe(no_mangle)]
pub extern "C" fn ou_app_background() {
    BACKGROUND.store(true, Ordering::Release);
}

/// The shell: the app is in front again.
#[unsafe(no_mangle)]
pub extern "C" fn ou_app_foreground() {
    FOREGROUND.store(true, Ordering::Release);
}

/// The shell: end the app (from the Game Data screen).
#[unsafe(no_mangle)]
pub extern "C" fn ou_request_quit() {
    QUIT.store(true, Ordering::Release);
}

thread_local! {
    static SHELL: std::cell::Cell<Option<OuShellCallbacks>> = const { std::cell::Cell::new(None) };
}

/// The on-screen keyboard's state (main thread only, but the C entry point needs a static).
#[derive(Default)]
struct TextState {
    /// the prompt is up for this field
    open: Option<TextTarget>,
    /// the field the last automatic prompt was for (no second automatic prompt until it changes)
    auto_done: Option<TextTarget>,
    /// the shell's answer, waiting for the next frame: `Some(text)` entered, `None` cancelled
    answer: Option<Option<String>>,
}

static TEXT: std::sync::Mutex<TextState> = std::sync::Mutex::new(TextState { open: None, auto_done: None, answer: None });

/// The shell: the on-screen keyboard closed with `text` (UTF-8), or NULL when cancelled.
///
/// # Safety
/// `text` must be null or a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ou_text_entered(text: *const c_char) {
    let t = (!text.is_null()).then(|| unsafe { std::ffi::CStr::from_ptr(text) }.to_string_lossy().into_owned());
    if let Ok(mut s) = TEXT.lock() {
        s.answer = Some(t);
    }
}

fn open_prompt(req: &TextRequest) -> bool {
    let Some(f) = shell().text_prompt else { return false };
    let title = CString::new(req.title.replace('\0', " ")).unwrap_or_default();
    let cur = CString::new(req.current.replace('\0', " ")).unwrap_or_default();
    let secure = req.title == "Password";
    if let Ok(mut s) = TEXT.lock() {
        s.open = Some(req.target);
    }
    tracing::info!(target = ?req.target, title = %req.title, "tvOS: on-screen keyboard");
    f(title.as_ptr(), cur.as_ptr(), req.max_len as u32, secure);
    true
}

/// Once a frame, after the pads were read: A (Y for a chat box) on a field that takes text opens the
/// on-screen keyboard; a field appearing opens it once by itself; while it is up the pad does nothing in the
/// game; its answer goes into the field (`App::inject_text`).
pub(crate) fn text_pad(app: &mut App) {
    // the answer first
    let answer = TEXT.lock().ok().and_then(|mut s| {
        let a = s.answer.take()?;
        Some((s.open.take(), a))
    });
    if let Some((Some(target), a)) = answer {
        match a {
            Some(text) => {
                app.inject_text(target, &text);
            }
            None => tracing::info!(?target, "tvOS: on-screen keyboard cancelled"),
        }
        app.input.pad_menu.clear();
        return;
    }
    if TEXT.lock().map(|s| s.open.is_some()).unwrap_or(false) {
        app.input.pad_menu.clear();
        return;
    }
    let req = app.text_request();
    let auto_done = TEXT.lock().map(|s| s.auto_done).unwrap_or(None);
    match req {
        None => {
            if let Ok(mut s) = TEXT.lock() {
                s.auto_done = None;
            }
        }
        Some(r) => {
            use nfs_input::MenuNav as N;
            let asked = if r.chat { app.input.pad_menu.contains(&N::Alt) } else { app.input.pad_menu.iter().any(|k| matches!(k, N::Select | N::Alt)) };
            let auto = !r.chat && auto_done != Some(r.target);
            if (asked || auto) && open_prompt(&r) {
                if let Ok(mut s) = TEXT.lock() {
                    s.auto_done = Some(r.target);
                }
                app.input.pad_menu.retain(|k| !matches!(k, N::Select | N::Alt));
            }
        }
    }
}

fn shell() -> OuShellCallbacks {
    SHELL.with(|s| s.get()).unwrap_or(OuShellCallbacks { launched: None, engine_error: None, text_prompt: None })
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
                // Stage 6.3: background → the race pauses (never resumes by itself) and the audio stream stops
                if BACKGROUND.swap(false, Ordering::AcqRel) {
                    app.request_pause("app in background");
                    if let Some(a) = app.snd.audio.as_ref() {
                        a.set_stream_paused(true);
                    }
                }
                if FOREGROUND.swap(false, Ordering::AcqRel) {
                    if let Some(a) = app.snd.audio.as_ref() {
                        a.set_stream_paused(false);
                    }
                }
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
