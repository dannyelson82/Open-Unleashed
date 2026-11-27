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
    /// Extras › Network changed the server address / token: keep the shell's copy (UserDefaults, which the
    /// Game Data screen uses and which survives a Caches purge) the same.
    pub server_changed: Option<extern "C" fn(*const c_char, *const c_char)>,
}

/// The shell's entry: runs the engine's event loop (never returns on tvOS — UIKit ends the process).
///
/// # Safety
/// `callbacks` must be null or point to a valid [`OuShellCallbacks`] for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ou_main(callbacks: *const OuShellCallbacks) -> i32 {
    let cb = if callbacks.is_null() { OuShellCallbacks { launched: None, engine_error: None, text_prompt: None, server_changed: None } } else { unsafe { *callbacks } };
    SHELL.with(|s| s.set(Some(cb)));
    // the pointer is not parked with `NFS_FE_CURSOR` here: the front end ignores every live move while that is set,
    // and the Siri Remote now drives the pointer (`remote_pump`). Until the remote moves it, the front end has no
    // pointer, so nothing shows or hovers.
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

/// Screen saver (enhancements-plan.md §2.7): a race, a replay or the intro keeps the TV awake; the menus don't.
static NO_SLEEP: AtomicBool = AtomicBool::new(false);

/// The shell (every few seconds): whether to keep the idle timer off now.
#[unsafe(no_mangle)]
pub extern "C" fn ou_wants_no_sleep() -> bool {
    NO_SLEEP.load(Ordering::Relaxed)
}

/// The shell: the audio route changed (AVAudioSession). `old_device_unavailable` = headphones went away (AirPods
/// out / disconnected): the race pauses (`airpods-plan.md` E). The output reopens on the new route at once.
#[unsafe(no_mangle)]
pub extern "C" fn ou_audio_route_changed(old_device_unavailable: bool) {
    if old_device_unavailable {
        crate::app::lifecycle_route_old_gone();
    }
    crate::app::lifecycle_route_force();
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
    let secure = req.secure;
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
    // while the keyboard is up, or its answer has just come in, the player is busy: no idle movie
    if TEXT.lock().map(|s| s.open.is_some() || s.answer.is_some()).unwrap_or(false) {
        app.fe_touch_idle();
    }
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
            None => {
                tracing::info!(?target, "tvOS: on-screen keyboard cancelled");
                app.text_cancelled(target);
            }
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
            // A opens the keyboard only with the pad's highlight on the field itself; Y opens it from anywhere.
            // A box that is always focused (the player name on the driver screen) is the text target wherever
            // the highlight is, so A on "done" kept opening the keyboard (real Apple TV, 2026-11-26).
            let on_field = match r.target {
                crate::app::TextTarget::Input(i) => app.fe_pad_highlight() == Some(i),
                _ => true,
            };
            let asked = if r.chat {
                app.input.pad_menu.contains(&N::Alt)
            } else {
                app.input.pad_menu.contains(&N::Alt) || (on_field && app.input.pad_menu.contains(&N::Select))
            };
            let auto = !r.chat && auto_done != Some(r.target);
            if (asked || auto) && open_prompt(&r) {
                if let Ok(mut s) = TEXT.lock() {
                    s.auto_done = Some(r.target);
                }
                app.input.pad_menu.retain(|k| !matches!(k, N::Select | N::Alt));   // the press opened the keyboard
            }
        }
    }
}

/// Extras › Network: hand the new server address and token to the shell.
pub(crate) fn server_changed(address: &str, token: &str) {
    let Some(f) = shell().server_changed else { return };
    let a = CString::new(address.replace('\0', "")).unwrap_or_default();
    let t = CString::new(token.replace('\0', "")).unwrap_or_default();
    f(a.as_ptr(), t.as_ptr());
}

fn shell() -> OuShellCallbacks {
    SHELL.with(|s| s.get()).unwrap_or(OuShellCallbacks { launched: None, engine_error: None, text_prompt: None, server_changed: None })
}

/// Stage 7.3, render resolution: the UIKit view's content scale decides how many pixels the Metal surface has
/// (1080p logical × scale) and the system scales the picture to the TV's mode. `NFS_RENDER_SCALE` (set by the
/// shell from the Display setting): `1` = 1080p, `1.5` = 1440p, `2` = native 4K; unset = the screen's own.
pub(crate) fn apply_render_scale(window: &Window) {
    use winit::platform::ios::WindowExtIOS;
    // the shell's launch setting wins, else Extras › Render resolution
    let Some(s) = std::env::var("NFS_RENDER_SCALE").ok().and_then(|v| v.parse::<f64>().ok()).or_else(|| nfs_game::extras::get().display.render_scale.factor()) else { return };
    let s = s.clamp(1.0, 2.0);
    window.set_scale_factor(s);
    let px = window.inner_size();
    tracing::info!(scale = s, width = px.width, height = px.height, "tvOS: render scale (surface pixels)");
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

/// The Siri Remote as a mouse (the shell's `RemoteInput.swift`): its touch surface moves the front end's
/// pointer like a trackpad, a click is the left button on a menu screen (else the pad's A), Menu is the pad's B,
/// Play/Pause its View. The remote is no longer an SDL joystick (`SDL_HINT_TV_REMOTE_AS_JOYSTICK` off), so a
/// swipe doesn't also move the pad's highlight. Asked for on the real Apple TV, 2026-11-26.
#[derive(Default)]
struct Remote {
    /// touch movement not yet applied, in remote units (the touch surface spans about -1…1)
    dx: f32,
    dy: f32,
    /// clicks in order (down / up)
    clicks: Vec<bool>,
    /// buttons: 1 = Menu, 2 = Play/Pause
    buttons: Vec<u32>,
    /// the pointer, in window pixels, once the remote has moved it
    at: Option<(f64, f64)>,
    /// the last pump, for the left stick's speed
    stick_at: Option<Instant>,
    /// the pointer is drawn (the remote's touch or the pad's stick moved it); the d-pad hides it and moves it itself
    shown: bool,
    /// the pad's A, as the left button, is down
    a_down: bool,
    /// the d-pad direction held, and when it steps again (first at once, then after a pause, then quicker)
    dpad_held: Option<((f32, f32), Instant)>,
    /// the right stick's scroll not yet a whole row
    scroll_acc: f32,
}

static REMOTE: std::sync::Mutex<Remote> = std::sync::Mutex::new(Remote { dx: 0.0, dy: 0.0, clicks: Vec::new(), buttons: Vec::new(), at: None, stick_at: None, shown: false, a_down: false, dpad_held: None, scroll_acc: 0.0 });

/// The shell: the finger moved on the touch surface by (dx, dy) (right and down positive).
#[unsafe(no_mangle)]
pub extern "C" fn ou_remote_touch(dx: f32, dy: f32) {
    if let Ok(mut r) = REMOTE.lock() {
        r.dx += dx;
        r.dy += dy;
    }
}

/// The shell: the touch surface was clicked (true) or released (false).
#[unsafe(no_mangle)]
pub extern "C" fn ou_remote_click(down: bool) {
    tracing::debug!(down, "Siri Remote: click from the shell");
    if let Ok(mut r) = REMOTE.lock() {
        r.clicks.push(down);
    }
}

/// The shell: a remote button was pressed: 1 = Menu, 2 = Play/Pause.
#[unsafe(no_mangle)]
pub extern "C" fn ou_remote_button(kind: u32) {
    tracing::info!(kind, "Siri Remote: button from the shell (0 = attached)");
    if kind == 0 {
        return;
    }
    if let Ok(mut r) = REMOTE.lock() {
        r.buttons.push(kind);
    }
}

/// The horizontal range, in window pixels, of the front end's 4:3 picture (the same letterbox `CursorMoved`'s
/// conversion uses: the design 640 × 480 fills the height and is centred).
fn pointer_bounds(w: f64, h: f64) -> (f64, f64) {
    let design = 640.0 / 480.0;
    if w / h > design {
        let margin = (w - h * design) / 2.0;
        (margin, w - margin - 1.0)
    } else {
        (0.0, w - 1.0)
    }
}

/// Once per loop iteration: the Siri Remote and the pad as the mouse on a menu screen (asked for on the real Apple
/// TV, 2026-11-27), through the same window events as a Mac's mouse:
///
/// * the remote's touch surface and the pad's left stick move the pointer, which shows;
/// * the pad's d-pad moves it, hidden, from one target to the next — a button, a list row, a drop-down and its
///   rows (`LayoutScreen::pad_targets`) — and the widget under it shows its hover state;
/// * the remote's click and the pad's A are the left button, either way;
/// * the pad's right stick scrolls the list under the pointer (or the open drop-down), as a wheel would.
///
/// The remote's Menu and Play/Pause become the pad's B and View. Off a menu screen (a race, the pause or Extras
/// menu) the remote's click is the pad's A and the pad is left alone.
fn remote_pump(app: &mut App, event_loop: &ActiveEventLoop) {
    // development check (`NFSH_REMOTE_SELFTEST=1` at launch): the pointer circles slowly through the same path as
    // the remote's touches, so the pointer can be checked on the TV with screenshots and no hands on the remote
    if std::env::var_os("NFSH_REMOTE_SELFTEST").is_some() {
        static T: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let t = T.fetch_add(1, Ordering::Relaxed) as f32 * 0.01;
        ou_remote_touch(-0.01 * t.sin(), 0.01 * t.cos());
    }
    // a menu screen takes the pointer; elsewhere (a race, the pause or Extras menu, the text menu) a click is A
    let pointer_screen = app.vehicle.is_none() && app.extras_ui.is_none() && app.fe.layout_preview.is_some();
    let pad = if pointer_screen { app.gamepads.pad_snapshot() } else { None };
    let (stick, right) = if pointer_screen { app.gamepads.photo_pad().map(|p| (p.left, p.right)).unwrap_or_default() } else { Default::default() };
    // the d-pad's direction (up / right / down / left), screen y down
    let dpad_dir = pad.as_ref().and_then(|p| {
        let d = p.dpad;
        if d[0] { Some((0.0, -1.0)) } else if d[2] { Some((0.0, 1.0)) } else if d[1] { Some((1.0, 0.0)) } else if d[3] { Some((-1.0, 0.0)) } else { None }
    });
    let pad_a = pad.as_ref().is_some_and(|p| p.buttons[0]);   // `BUTTON_ORDER[0]`: South, the pad's A
    let now = Instant::now();
    let Ok(mut r) = REMOTE.lock() else { return };
    let dt = r.stick_at.map_or(0.0, |t| now.duration_since(t).as_secs_f32().min(0.1));
    r.stick_at = Some(now);
    let touched = r.dx != 0.0 || r.dy != 0.0;
    // the stick in remote units: the touch surface's two units cross 40 % of the picture, so its full width is
    // five; full travel crosses it in about 1.2 s, and the curve (value × |value|) keeps small pushes precise
    let (dx, dy) = (r.dx + stick.0 * stick.0.abs() * dt * 5.0 / 1.2, r.dy - stick.1 * stick.1.abs() * dt * 5.0 / 1.2);
    let (clicks, buttons) = (std::mem::take(&mut r.clicks), std::mem::take(&mut r.buttons));
    r.dx = 0.0;
    r.dy = 0.0;
    for b in buttons {
        app.input.pad_menu.push(match b {
            1 => nfs_input::MenuNav::Back,
            _ => nfs_input::MenuNav::View,
        });
    }
    let Some(window) = app.window.clone() else { return };
    // the pad's stick, d-pad and A are the pointer's on a menu screen, the menus' own keys elsewhere
    app.gamepads.set_stick_menu(!pointer_screen);
    app.gamepads.set_dpad_menu(!pointer_screen);
    app.gamepads.set_a_select(!pointer_screen);
    if !pointer_screen {
        nfs_game::layout::pad_nav::set_pointer_hidden(false);
        r.dpad_held = None;
        for down in clicks {
            if down {
                app.input.pad_menu.push(nfs_input::MenuNav::Select);
            }
        }
        return;
    }
    let send = |app: &mut App, ev: WindowEvent| app.window_event(event_loop, window.id(), ev);
    let left = |down: bool| WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state: if down { winit::event::ElementState::Pressed } else { winit::event::ElementState::Released },
        button: winit::event::MouseButton::Left,
    };
    let size = window.inner_size();
    let (w, h) = (size.width.max(1) as f64, size.height.max(1) as f64);
    // the front end is a 4:3 picture in the middle of the 16:9 screen: the pointer stays inside the picture,
    // or it went into the black bars at the sides, where nothing draws it (real Apple TV, 2026-11-27)
    let (lo, hi) = pointer_bounds(w, h);
    let to_design = |p: (f64, f64)| (((p.0 - lo) / (hi - lo) * 640.0) as f32, (p.1 / h * 480.0) as f32);
    let to_window = |d: (f32, f32)| (lo + d.0 as f64 / 640.0 * (hi - lo), d.1 as f64 / 480.0 * h);
    let mut moved_to: Option<(f64, f64)> = None;

    // the d-pad: a step at once, then after 0.4 s every 0.15 s while held
    let step = match (dpad_dir, r.dpad_held) {
        (Some(d), Some((held, due))) if held == d => (now >= due).then(|| {
            r.dpad_held = Some((d, now + Duration::from_millis(150)));
            d
        }),
        (Some(d), _) => {
            r.dpad_held = Some((d, now + Duration::from_millis(400)));
            Some(d)
        }
        (None, _) => {
            r.dpad_held = None;
            None
        }
    };
    if let Some(dir) = step {
        let targets = pad_targets(app);
        let from = r.at.map(to_design);
        let next = match from {
            Some(f) if r.shown || app.fe.cursor.is_some() => nfs_game::layout::pad_nav::pad_step(&targets, f, dir),
            // no pointer yet: the first target in reading order
            _ => targets.iter().copied().min_by(|a, b| (a.1, a.0).partial_cmp(&(b.1, b.0)).unwrap_or(std::cmp::Ordering::Equal)),
        };
        if r.shown {
            r.shown = false;
            r.a_down = pad_a;   // an A held through the switch is no click
        }
        nfs_game::layout::pad_nav::set_pointer_hidden(true);
        if let Some(t) = next {
            moved_to = Some(to_window(t));
        }
    }
    // the remote's touch or the left stick: the pointer shows and moves from where it is
    if touched || stick != (0.0, 0.0) {
        if !r.shown {
            r.shown = true;
            r.a_down = pad_a;
        }
        nfs_game::layout::pad_nav::set_pointer_hidden(false);
        if dx != 0.0 || dy != 0.0 {
            // a full swipe across the surface (about two units) crosses about 40 % of the picture's width
            let speed = (hi - lo) * 0.2;
            let (x0, y0) = r.at.unwrap_or((w / 2.0, h / 2.0));
            moved_to = Some(((x0 + dx as f64 * speed).clamp(lo, hi), (y0 + dy as f64 * speed).clamp(0.0, h - 1.0)));
        }
    }
    if let Some(p) = moved_to {
        r.at = Some(p);
    }
    // every pump: a screen change may have taken the front end's pointer away
    if let Some(p) = r.at
        && (moved_to.is_some() || app.fe.cursor.is_none())
    {
        send(app, WindowEvent::CursorMoved { device_id: DeviceId::dummy(), position: winit::dpi::PhysicalPosition::new(p.0, p.1) });
    }
    for down in clicks {
        send(app, left(down));
    }
    if pad_a != r.a_down {
        r.a_down = pad_a;
        send(app, left(pad_a));
    }
    // the right stick as a wheel: up to twelve rows a second at full travel
    let v = -right.1;   // down positive, as the list's rows run
    if v == 0.0 {
        r.scroll_acc = 0.0;
    } else {
        r.scroll_acc += v * v.abs() * dt * 12.0;
        let rows = r.scroll_acc.trunc();
        if rows != 0.0 {
            r.scroll_acc -= rows;
            let at = r.at.map(to_design);
            if let Some(l) = app.fe.layout_preview.as_mut() {
                l.pad_scroll(at, rows as i32);
            }
        }
    }
}

/// The d-pad's targets on the screen in front, in design pixels: a dialog's buttons while one is up, else the
/// screen's own and the bottom bar's (back, options, exit). The screen's scan is kept until something it depends
/// on changes (`LayoutScreen::pad_state_key`): scanning on every press made the d-pad lag (real Apple TV,
/// 2026-11-27).
fn pad_targets(app: &App) -> Vec<(f32, f32)> {
    type Cached = Option<((Option<String>, bool, u64, u64), Vec<(f32, f32)>)>;
    static CACHE: std::sync::Mutex<Cached> = std::sync::Mutex::new(None);
    let key = (
        app.fe.layout_name.clone(),
        app.fe.dialog.as_ref().is_some_and(|(d, _)| d.up),
        app.fe.layout_preview.as_ref().map_or(0, |l| l.pad_state_key()),
        app.fe.layout_bottom.as_ref().map_or(0, |l| l.pad_state_key()),
    );
    if let Ok(c) = CACHE.lock()
        && let Some((k, t)) = c.as_ref()
        && *k == key
    {
        return t.clone();
    }
    let started = Instant::now();
    let t = pad_targets_scan(app);
    tracing::debug!(targets = t.len(), ms = started.elapsed().as_secs_f32() * 1000.0, "d-pad targets scanned");
    if let Ok(mut c) = CACHE.lock() {
        *c = Some((key, t.clone()));
    }
    t
}

fn pad_targets_scan(app: &App) -> Vec<(f32, f32)> {
    let base = app.fe.layout_preview.as_ref();
    if let (Some((d, _)), Some(l)) = (app.fe.dialog.as_ref().filter(|(d, _)| d.up), base.or(app.fe.layout_bottom.as_ref())) {
        let th = l.dialog_text_height(&d.text);
        return (0..d.buttons.len()).map(|i| {
            let (x, y, w, h) = d.button_rect(i, th);
            (x + w / 2.0, y + h / 2.0)
        }).collect();
    }
    let mut t = base.map(|l| l.pad_targets()).unwrap_or_default();
    if let Some(b) = app.fe.layout_bottom.as_ref() {
        t.extend(b.pad_targets());
    }
    t
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
                NO_SLEEP.store(app.vehicle.is_some() || app.intro.is_some(), Ordering::Relaxed);
                remote_pump(app, event_loop);
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
