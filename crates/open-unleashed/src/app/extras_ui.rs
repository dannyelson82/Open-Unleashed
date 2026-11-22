//! **PORT-DEVIATION (Extras menu, `enhancements-plan.md` §0)** — the port's own settings menu, drawn like the
//! port's other overlays (the HUD's font and quads), opened from the pause menu's last item. Fully driven by
//! the d-pad / arrows (up/down: rows; left/right: values, or pages on the page row; A/Enter: the reset row;
//! B/Esc: close). Each row shows its value, a one-line explanation and when it applies. The settings live in
//! `nfs_game::extras` (`extras.json`; the Network page's in `network.json`, this device's own).
//!
//! **Network** has text rows (server address, password): Enter / A starts typing — on the Mac in place (the
//! keyboard, Cmd+V pastes, Enter keeps, Esc drops), on tvOS in the on-screen keyboard (`text_entry.rs`). On
//! tvOS a change is also handed to the app shell, whose copy the Game Data screen uses.

use crate::*;
use nfs_game::extras::{self, ExtrasSettings, HeadLookStrength, HudSize, RenderScale};
use std::cell::{Cell, RefCell};
use std::sync::Mutex;

/// An Extras text row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    Server,
    Token,
}

impl Field {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Field::Server => "NFS Haven server (name or IP address)",
            Field::Token => "Server password",
        }
    }

    pub(crate) fn secure(self) -> bool {
        self == Field::Token
    }

    fn value(self, s: &ExtrasSettings) -> &str {
        match self {
            Field::Server => &s.network.server,
            Field::Token => &s.network.token,
        }
    }
}

/// The longest text a row takes.
pub(crate) const MAX_TEXT: usize = 200;

thread_local! {
    /// the text row being typed into, and its text so far
    static EDIT: RefCell<Option<(Field, String)>> = const { RefCell::new(None) };
    /// Cmd (macOS) / Ctrl held — for paste
    static COMMAND: Cell<bool> = const { Cell::new(false) };
}

/// Test connection's last answer.
#[derive(Debug, Clone)]
enum Test {
    Idle,
    Running,
    Ok(String),
    Failed(String),
}

static TEST: Mutex<Test> = Mutex::new(Test::Idle);

fn test_state() -> Test {
    TEST.lock().map(|t| t.clone()).unwrap_or(Test::Idle)
}

fn set_test(t: Test) {
    if let Ok(mut g) = TEST.lock() {
        *g = t;
    }
}

/// The text row being typed into (field, text so far).
pub(crate) fn editing() -> Option<(Field, String)> {
    EDIT.with(|e| e.borrow().clone())
}

pub(crate) fn cancel_edit() {
    EDIT.with(|e| *e.borrow_mut() = None);
}

fn set_edit(v: Option<(Field, String)>) {
    EDIT.with(|e| *e.borrow_mut() = v);
}

/// **TEST GLUE** (`NFS_RACE_KEYS` `type=`): the text row being typed into gets this text (replacing it).
pub(crate) fn type_text(t: &str) {
    EDIT.with(|e| {
        if let Some((_, buf)) = e.borrow_mut().as_mut() {
            *buf = t.chars().filter(|c| !c.is_control()).collect();
        }
    });
}

/// The window's modifier keys changed (Cmd+V pastes into a text row on the Mac).
pub(crate) fn set_modifiers(m: winit::keyboard::ModifiersState) {
    COMMAND.with(|c| c.set(m.super_key() || m.control_key()));
}

/// The clipboard's text (the Mac's `pbpaste`), first line only.
fn clipboard_text() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/usr/bin/pbpaste").output().ok()?;
        let t = String::from_utf8_lossy(&out.stdout);
        return t.lines().next().map(|l| l.trim().to_string());
    }
    #[allow(unreachable_code)]
    None
}

/// The server settings changed: tvOS hands them to the app shell too (its Game Data screen and next start).
fn network_changed() {
    set_test(Test::Idle);
    #[cfg(target_os = "tvos")]
    {
        let n = extras::get().network;
        crate::tvos::server_changed(&n.server, &n.token);
    }
}

/// What a row is.
#[derive(Clone, Copy)]
enum Kind {
    /// a value picked with left / right
    Choice,
    /// text typed in (Enter / A)
    Text(Field),
    /// Test connection (Enter / A)
    Test,
    /// Test effects: plays every rumble and trigger effect in turn (Enter / A)
    HapticsTest,
    /// Speaker test: a tone on each speaker in turn (Enter / A)
    SpeakerTest,
}

/// The menu's place.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExtrasUi {
    pub(crate) page: usize,
    /// 0 = the page row; 1.. = the page's rows, then "reset this page"
    pub(crate) row: usize,
}

/// When a change takes effect.
#[derive(Clone, Copy)]
enum Applies {
    Now,
    NextRace,
    NextStart,
}

/// One setting row.
struct Row {
    label: &'static str,
    help: &'static str,
    applies: Applies,
    /// the platforms it means something on
    shown: bool,
    kind: Kind,
    values: &'static [&'static str],
    get: fn(&ExtrasSettings) -> usize,
    set: fn(&mut ExtrasSettings, usize),
}

struct Page {
    title: &'static str,
    rows: Vec<Row>,
    reset: fn(&mut ExtrasSettings),
}

fn pages() -> Vec<Page> {
    vec![Page {
        title: "Display & HUD",
        rows: vec![
            Row {
                label: "Frame-time overlay",
                help: "Frames per second and frame times in the top-left corner.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["Off", "On"],
                get: |s| usize::from(s.display.fps_overlay),
                set: |s, v| s.display.fps_overlay = v == 1,
            },
            Row {
                label: "HUD size",
                help: "Scales the race HUD. The original draws it in fixed pixels, small on a big TV.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["Original", "Large", "Extra large"],
                get: |s| match s.display.hud_size {
                    HudSize::Original => 0,
                    HudSize::Large => 1,
                    HudSize::ExtraLarge => 2,
                },
                set: |s, v| s.display.hud_size = [HudSize::Original, HudSize::Large, HudSize::ExtraLarge][v.min(2)],
            },
            Row {
                label: "Render resolution",
                help: "Pixels the game draws. The Apple TV scales them to the TV, lower = more headroom.",
                applies: Applies::NextStart,
                shown: cfg!(target_os = "tvos"),
                kind: Kind::Choice,
                values: &["Native (4K)", "1440p", "1080p"],
                get: |s| match s.display.render_scale {
                    RenderScale::Native => 0,
                    RenderScale::P1440 => 1,
                    RenderScale::P1080 => 2,
                },
                set: |s, v| s.display.render_scale = [RenderScale::Native, RenderScale::P1440, RenderScale::P1080][v.min(2)],
            },
        ],
        reset: |s| s.display = Default::default(),
    },
    Page {
        title: "Graphics",
        rows: vec![
            Row {
                label: "Anti-aliasing",
                help: "Smooths jagged 3D edges (the HUD and menus are not touched). FXAA: a light filter. MSAA 4x: true multisampling of the 3D scene.",
                applies: Applies::NextStart,
                shown: true,
                kind: Kind::Choice,
                values: &["Off (original)", "FXAA", "MSAA 4x"],
                get: |s| s.graphics.anti_aliasing.min(2) as usize,
                set: |s, v| s.graphics.anti_aliasing = v as u8,
            },
            Row {
                label: "Textures",
                help: "HD: the HD texture pack's sharper copies where it has one (tracks, sky, effects), the original elsewhere. Needs a pack.",
                applies: Applies::NextRace,
                shown: true,
                kind: Kind::Choice,
                values: &["Original", "HD"],
                get: |s| s.graphics.textures.min(1) as usize,
                set: |s, v| s.graphics.textures = v as u8,
            },
            Row {
                label: "Texture filtering",
                help: "Sharper road and ground textures at shallow angles. Original = trilinear.",
                applies: Applies::NextStart,
                shown: true,
                kind: Kind::Choice,
                values: &["Original", "Anisotropic 4x", "Anisotropic 8x", "Anisotropic 16x"],
                get: |s| s.graphics.filtering as usize,
                set: |s, v| s.graphics.filtering = v as u8,
            },
            Row {
                label: "Draw distance",
                help: "Beyond the original's longest, with the fog pushed out to match. Check tracks for pop-in.",
                applies: Applies::NextRace,
                shown: true,
                kind: Kind::Choice,
                values: &["Original", "125 percent", "150 percent", "200 percent"],
                get: |s| s.graphics.draw_distance as usize,
                set: |s, v| s.graphics.draw_distance = v as u8,
            },
            Row {
                label: "Field of view, chase views",
                help: "Added to the original's widescreen-correct field of view.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["-5 degrees", "0", "+5 degrees", "+10 degrees"],
                get: |s| s.graphics.fov_chase as usize,
                set: |s, v| s.graphics.fov_chase = v as u8,
            },
            Row {
                label: "Field of view, in-car views",
                help: "Bumper and driver views.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["-5 degrees", "0", "+5 degrees", "+10 degrees"],
                get: |s| s.graphics.fov_incar as usize,
                set: |s, v| s.graphics.fov_incar = v as u8,
            },
        ],
        reset: |s| s.graphics = Default::default(),
    },
    Page {
        title: "Controller",
        rows: vec![
            Row {
                label: "Rumble",
                help: "Enhanced: road surfaces, gear shifts and the rev limiter on top of the original's force feedback (whose levels stay in Options).",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["Original", "Enhanced"],
                get: |s| s.controller.rumble.min(1) as usize,
                set: |s, v| s.controller.rumble = v as u8,
            },
            Row {
                label: "Trigger effects",
                help: "Xbox: trigger rumble for ABS and wheelspin. DualSense: brake resistance, ABS and wheelspin felt in the triggers.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["Off", "Subtle", "Strong"],
                get: |s| s.controller.trigger_effects.min(2) as usize,
                set: |s, v| s.controller.trigger_effects = v as u8,
            },
            Row {
                label: "Test effects",
                help: "Plays each effect in turn on the connected controller: a crash, a gear shift, both trigger motors, both adaptive triggers.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::HapticsTest,
                values: &[],
                get: |_| 0,
                set: |_, _| {},
            },
            Row {
                label: "Head-look (AirPods)",
                help: "Turn your head to look around inside the car (bumper and driver views). Needs AirPods with head tracking.",
                applies: Applies::Now,
                shown: cfg!(target_os = "macos"),
                kind: Kind::Choice,
                values: &["Off", "On"],
                get: |s| usize::from(s.controller.head_look),
                set: |s, v| s.controller.head_look = v == 1,
            },
            Row {
                label: "Head-look strength",
                help: "How far the view turns for 20 degrees of head turn (50 / 75 / 100 degrees).",
                applies: Applies::Now,
                shown: cfg!(target_os = "macos"),
                kind: Kind::Choice,
                values: &["Low", "Medium", "High"],
                get: |s| s.controller.head_look_strength as usize,
                set: |s, v| s.controller.head_look_strength = [HeadLookStrength::Low, HeadLookStrength::Medium, HeadLookStrength::High][v.min(2)],
            },
            Row {
                label: "Head-look up and down",
                help: "The view also tilts a little with your head (at most 15 degrees).",
                applies: Applies::Now,
                shown: cfg!(target_os = "macos"),
                kind: Kind::Choice,
                values: &["Off", "On"],
                get: |s| usize::from(s.controller.head_look_pitch),
                set: |s, v| s.controller.head_look_pitch = v == 1,
            },
        ],
        reset: |s| s.controller = Default::default(),
    },
    Page {
        title: "Audio",
        rows: vec![Row {
            label: "Speakers",
            help: "5.1: cars behind you sound behind you and tunnels surround you, when the Apple TV outputs 5.1 (Settings > Video and Audio > Audio Format). Otherwise stereo.",
            applies: Applies::Now,
            shown: true,
            kind: Kind::Choice,
            values: &["Stereo (original)", "5.1 when available"],
            get: |s| s.audio.speakers.min(1) as usize,
            set: |s, v| s.audio.speakers = v as u8,
        },
        Row {
            label: "Speaker test",
            help: "A tone from each speaker in turn: left, centre, right, right surround, left surround (stereo: left, right).",
            applies: Applies::Now,
            shown: true,
            kind: Kind::SpeakerTest,
            values: &[],
            get: |_| 0,
            set: |_, _| {},
        },
        Row {
            label: "Pause when headphones disconnect",
            help: "A single-player race pauses when AirPods or other headphones go away mid-race.",
            applies: Applies::Now,
            shown: true,
            kind: Kind::Choice,
            values: &["Off", "On"],
            get: |s| usize::from(s.audio.pause_on_headphones_gone),
            set: |s, v| s.audio.pause_on_headphones_gone = v == 1,
        }],
        reset: |s| s.audio = Default::default(),
    },
    Page {
        title: "Driving assists",
        rows: vec![
            Row {
                label: "Preset",
                help: "Sets every assist on this page at once. Light: soft steering, low speed sensitivity, ABS. Full: softer steering, medium, steering assist light, ABS.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["Custom", "Off", "Light", "Full"],
                get: |s| {
                    let a = &s.assists;
                    match (a.steering_response, a.speed_steering, a.countersteer, a.abs) {
                        (0, 0, 0, false) => 1,
                        (1, 1, 0, true) => 2,
                        (2, 2, 1, true) => 3,
                        _ => 0,
                    }
                },
                set: |s, v| {
                    let a = &mut s.assists;
                    match v {
                        1 => (a.steering_response, a.speed_steering, a.countersteer, a.abs) = (0, 0, 0, false),
                        2 => (a.steering_response, a.speed_steering, a.countersteer, a.abs) = (1, 1, 0, true),
                        3 => (a.steering_response, a.speed_steering, a.countersteer, a.abs) = (2, 2, 1, true),
                        _ => {}
                    }
                },
            },
            Row {
                label: "Steering response",
                help: "Softer = finer control around the centre of the stick, full lock unchanged.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["Linear (original)", "Soft", "Softer"],
                get: |s| s.assists.steering_response as usize,
                set: |s, v| s.assists.steering_response = v as u8,
            },
            Row {
                label: "Speed-sensitive steering",
                help: "Less steering lock at high speed (at 200 km/h: 75 / 55 / 40 percent).",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["Off", "Low", "Medium", "High"],
                get: |s| s.assists.speed_steering as usize,
                set: |s, v| s.assists.speed_steering = v as u8,
            },
            Row {
                label: "Steering assist",
                help: "Countersteers a little when the rear slides. Never fights your own countersteer.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["Off", "Light", "Strong"],
                get: |s| s.assists.countersteer as usize,
                set: |s, v| s.assists.countersteer = v as u8,
            },
            Row {
                label: "ABS",
                help: "Pulses the brake when a wheel locks, so you can still steer while braking hard.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Choice,
                values: &["Off", "On"],
                get: |s| usize::from(s.assists.abs),
                set: |s, v| s.assists.abs = v == 1,
            },
        ],
        reset: |s| s.assists = Default::default(),
    },
    Page {
        title: "Network",
        rows: vec![
            Row {
                label: "Server",
                help: "Your NFS Haven server: tower.local, an IP address or a Tailscale name. Port 8642 unless given.",
                applies: Applies::NextStart,
                shown: true,
                kind: Kind::Text(Field::Server),
                values: &[],
                get: |_| 0,
                set: |_, _| {},
            },
            Row {
                label: "Password",
                help: "The password set in the nfs-haven-server container's settings.",
                applies: Applies::NextStart,
                shown: true,
                kind: Kind::Text(Field::Token),
                values: &[],
                get: |_| 0,
                set: |_, _| {},
            },
            Row {
                label: "Sync saves",
                help: "Keep your saves on the server, shared with the Apple TV. Off: this Mac's saves stay here.",
                applies: Applies::NextStart,
                // tvOS always syncs once a server is set (PROGRESS.md decision 30)
                shown: !cfg!(target_os = "tvos"),
                kind: Kind::Choice,
                values: &["Off", "On"],
                get: |s| usize::from(s.network.sync_saves),
                set: |s, v| s.network.sync_saves = v == 1,
            },
            Row {
                label: "Test connection",
                help: "Asks the server above whether it answers and accepts the password.",
                applies: Applies::Now,
                shown: true,
                kind: Kind::Test,
                values: &[],
                get: |_| 0,
                set: |_, _| {},
            },
        ],
        reset: |s| s.network = Default::default(),
    }]
    .into_iter()
    .map(|mut p| {
        p.rows.retain(|r| r.shown);
        p
    })
    .filter(|p| !p.rows.is_empty())
    .collect()
}

impl App {
    /// Extras › Controller › Test effects, once a frame while it plays (about 4 s): a crash pulse, a gear-shift
    /// thump, the left then the right trigger motor, the left then the right adaptive trigger; then all off.
    pub(crate) fn haptics_test_tick(&mut self) {
        let Some(t0) = self.haptics_test else { return };
        let t = t0.elapsed().as_secs_f32();
        let step = |a: f32, b: f32| t >= a && t < b;
        let pads = &mut self.gamepads;
        if step(0.0, 0.05) {
            pads.rumble_pulse(0.8, 400);
        } else if step(0.7, 0.75) {
            pads.rumble_pulse(0.4, 60);
        }
        if step(1.0, 1.6) {
            pads.trigger_rumble(0.8, 0.0);
        } else if step(1.6, 2.2) {
            pads.trigger_rumble(0.0, 0.8);
        } else {
            pads.trigger_rumble(0.0, 0.0);
        }
        #[cfg(any(target_os = "macos", target_os = "tvos"))]
        {
            use nfs_input::dualsense_apple::TriggerMode as M;
            let vib = M::Vibration { start: 0.1, amplitude: 0.7, hz: 14.0 };
            if step(2.4, 3.2) {
                pads.adaptive_triggers(vib, M::Off);
            } else if step(3.2, 4.0) {
                pads.adaptive_triggers(M::Off, vib);
            }
        }
        if t >= 4.0 {
            self.haptics_test = None;
            self.gamepads.triggers_off();
            tracing::info!("Extras: haptics test done");
        }
    }
}

/// Apply what takes effect at once (called after every change and at start-up).
pub(crate) fn apply_now(s: &ExtrasSettings) {
    // speakers: from the next audio callback
    nfs_audio::mixer::set_surround(s.audio.speakers == 1);
    // HD textures: from the next textures loaded (the next race / screen)
    nfs_assets::hd::set_enabled(s.graphics.textures == 1);
    overlay::FRAME_OVERLAY.store(s.display.fps_overlay || std::env::var_os("NFS_FPS_OVERLAY").is_some_and(|v| v != "0"), std::sync::atomic::Ordering::Relaxed);
    crate::hud::set_hud_scale(s.display.hud_size.scale());
}

impl App {
    pub(crate) fn extras_open(&mut self) {
        // over the front end: the HUD's fonts, loaded once (the race has its own HUD)
        if self.vehicle.is_none()
            && self.fe_hud.is_none()
            && let Some(r) = self.renderer.as_mut()
        {
            self.fe_hud = Hud::load(r, &self.paths, None, false, false);
        }
        self.extras_ui = Some(ExtrasUi::default());
        self.fe_sound(9);
    }

    /// The HUD whose fonts draw the menu: the race's, else the front end's.
    fn extras_hud(&self) -> Option<&Hud> {
        let has_font = |h: &&Hud| h.font.is_some();
        if self.vehicle.is_some() { self.hud.as_ref().filter(has_font) } else { self.fe_hud.as_ref().filter(has_font).or(self.hud.as_ref().filter(has_font)) }
    }

    /// Whether the front end can open Extras now (no dialog, no transition, no text field or binding row waiting).
    fn extras_fe_ready(&self) -> bool {
        self.vehicle.is_none() && self.fe.menu.is_some() && self.fe.dialog.is_none() && self.fe.binding_base.is_none() && !self.fe_input_swallowed() && self.fe.layout_preview.as_ref().is_none_or(|l| l.focused_input().is_none())
    }

    /// **The pad in the front end** (once a frame, before the front end reads it): View — and on tvOS the Siri
    /// Remote's Play/Pause (its X) — opens Extras; while it is up every pad key is its.
    pub(crate) fn extras_fe_pad(&mut self) {
        use nfs_input::MenuNav as N;
        if self.extras_ui.is_none() {
            let open = self.input.pad_menu.iter().any(|k| matches!(k, N::View) || (cfg!(target_os = "tvos") && matches!(k, N::Aux)));
            if open && self.extras_fe_ready() {
                self.input.pad_menu.clear();
                self.extras_open();
            }
            return;
        }
        for k in std::mem::take(&mut self.input.pad_menu) {
            let key = match k {
                N::Up => MenuKey::Up,
                N::Down => MenuKey::Down,
                N::Left => MenuKey::Left,
                N::Right => MenuKey::Right,
                N::Select => MenuKey::Enter,
                N::Back | N::View => MenuKey::Back,
                N::Aux if cfg!(target_os = "tvos") => MenuKey::Back,
                N::Alt | N::Aux => continue,
            };
            self.extras_key(key);
            if self.extras_ui.is_none() {
                break;
            }
        }
    }

    /// A key while the Extras menu is up.
    pub(crate) fn extras_key(&mut self, key: MenuKey) {
        let Some(mut ui) = self.extras_ui else { return };
        let pages = pages();
        let page = &pages[ui.page.min(pages.len() - 1)];
        let last = page.rows.len() + 1; // the reset row
        // typing into a text row: Enter / A keeps it, B / Esc drops it, nothing else moves (the keyboard's own
        // path types the characters)
        if let Some((field, text)) = editing() {
            match key {
                MenuKey::Back => cancel_edit(),
                MenuKey::Enter => self.extras_commit_text(field, &text),
                _ => {}
            }
            return;
        }
        match key {
            MenuKey::Up => ui.row = ui.row.saturating_sub(1),
            MenuKey::Down => ui.row = (ui.row + 1).min(last),
            MenuKey::Left | MenuKey::Right => {
                let d: isize = if matches!(key, MenuKey::Left) { -1 } else { 1 };
                if ui.row == 0 {
                    ui.page = (ui.page as isize + d).rem_euclid(pages.len() as isize) as usize;
                } else if let Some(r) = page.rows.get(ui.row - 1)
                    && matches!(r.kind, Kind::Choice)
                {
                    let n = r.values.len() as isize;
                    let s = extras::update(|s| {
                        let v = ((r.get)(s) as isize + d).rem_euclid(n) as usize;
                        (r.set)(s, v);
                    });
                    apply_now(&s);
                    self.fe_sound(9);
                }
            }
            MenuKey::Enter => {
                if ui.row == last {
                    let before = extras::get().network;
                    let s = extras::update(|s| (page.reset)(s));
                    apply_now(&s);
                    if s.network != before {
                        network_changed();
                    }
                    self.fe_sound(9);
                } else if let Some(Kind::Text(field)) = page.rows.get(ui.row - 1).map(|r| r.kind) {
                    // start typing: in place on the Mac, the on-screen keyboard on tvOS (`text_request`)
                    set_edit(Some((field, field.value(&extras::get()).to_string())));
                    self.fe_sound(9);
                } else if let Some(Kind::SpeakerTest) = page.rows.get(ui.row - 1).map(|r| r.kind) {
                    nfs_audio::mixer::speaker_test();
                } else if let Some(Kind::HapticsTest) = page.rows.get(ui.row - 1).map(|r| r.kind) {
                    self.haptics_test = Some(std::time::Instant::now());
                    self.fe_sound(9);
                } else if let Some(Kind::Test) = page.rows.get(ui.row - 1).map(|r| r.kind) {
                    if !matches!(test_state(), Test::Running) {
                        set_test(Test::Running);
                        let _ = std::thread::Builder::new().name("ou-test-server".into()).spawn(|| {
                            set_test(match crate::sync::test_connection() {
                                Ok(who) => Test::Ok(who),
                                Err(e) => Test::Failed(e),
                            });
                        });
                        self.fe_sound(9);
                    }
                } else if ui.row >= 1 {
                    // Enter steps the value forward, like Right
                    self.extras_ui = Some(ui);
                    self.extras_key(MenuKey::Right);
                    return;
                }
            }
            MenuKey::Back => {
                self.extras_ui = None;
                return;
            }
        }
        self.extras_ui = Some(ui);
    }

    /// A text row's new text (Enter on the Mac, OK in tvOS's keyboard).
    pub(crate) fn extras_commit_text(&mut self, field: Field, text: &str) {
        let t: String = text.trim().chars().filter(|c| !c.is_control()).take(MAX_TEXT).collect();
        cancel_edit();
        extras::update(|s| match field {
            Field::Server => s.network.server = t,
            Field::Token => s.network.token = t,
        });
        tracing::info!(?field, "Extras: network setting changed");
        network_changed();
        self.fe_sound(9);
    }

    /// **The keyboard while the Extras menu is up** (before photo mode, the race keys and the pause menu see it):
    /// a text row being typed into takes the characters; otherwise the arrows, Enter and Esc drive the menu.
    /// Returns whether the key was taken (every press is, so no race key acts under the menu).
    pub(crate) fn extras_keyboard(&mut self, event: &winit::event::KeyEvent) -> bool {
        use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
        if event.state != winit::event::ElementState::Pressed {
            return self.extras_ui.is_some() && !matches!(event.physical_key, PhysicalKey::Code(KeyCode::F12));
        }
        // F12 opens and closes it in the front end (the pause menu has its own item in a race)
        if matches!(event.physical_key, PhysicalKey::Code(KeyCode::F12)) && !event.repeat {
            if self.extras_ui.is_some() && editing().is_none() {
                self.extras_ui = None;
                return true;
            }
            if self.extras_ui.is_none() && self.extras_fe_ready() {
                self.extras_open();
                return true;
            }
        }
        if self.extras_ui.is_none() {
            return false;
        }
        if let Some((field, mut buf)) = editing() {
            match event.logical_key.as_ref() {
                Key::Named(NamedKey::Enter) => {
                    self.extras_commit_text(field, &buf);
                    return true;
                }
                Key::Named(NamedKey::Escape) => {
                    cancel_edit();
                    return true;
                }
                Key::Named(NamedKey::Backspace) => {
                    buf.pop();
                }
                Key::Named(NamedKey::Space) => buf.push(' '),
                Key::Character(t) if COMMAND.with(Cell::get) => {
                    if t.eq_ignore_ascii_case("v")
                        && let Some(clip) = clipboard_text()
                    {
                        buf.push_str(&clip);
                    }
                }
                Key::Character(t) => buf.extend(t.chars().filter(|c| !c.is_control())),
                _ => {}
            }
            buf = buf.chars().take(MAX_TEXT).collect();
            set_edit(Some((field, buf)));
            return true;
        }
        if event.repeat && !matches!(event.physical_key, PhysicalKey::Code(KeyCode::ArrowUp | KeyCode::ArrowDown)) {
            return true;
        }
        let key = match event.physical_key {
            PhysicalKey::Code(KeyCode::ArrowUp) => Some(MenuKey::Up),
            PhysicalKey::Code(KeyCode::ArrowDown) => Some(MenuKey::Down),
            PhysicalKey::Code(KeyCode::ArrowLeft) => Some(MenuKey::Left),
            PhysicalKey::Code(KeyCode::ArrowRight) => Some(MenuKey::Right),
            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => Some(MenuKey::Enter),
            PhysicalKey::Code(KeyCode::Escape | KeyCode::Backspace) => Some(MenuKey::Back),
            _ => None,
        };
        if let Some(k) = key {
            self.extras_key(k);
        }
        true
    }

    /// The menu over the paused race.
    pub(crate) fn extras_sprites(&mut self) {
        let Some(ui) = self.extras_ui else { return };
        let Some(h) = self.extras_hud() else { return };
        let aspect = self.hud_aspect();
        let s = extras::get();
        let pages = pages();
        let page = &pages[ui.page.min(pages.len() - 1)];
        let (w, line) = (420.0f32, FONT_SMALL + 8.0);
        let rows = page.rows.len() + 2;
        // room for a help text of up to three lines
        let hgt = line * (rows as f32 + 6.0) + 16.0;
        let (x, y) = ((HUD_REF_W - w) * 0.5, (HUD_REF_H - hgt) * 0.5);
        let mut out = vec![h.quad(aspect, x, y, w, hgt, [0.0, 0.0, 0.0, 0.82])];
        let from = out.len();
        let white = [1.0, 1.0, 1.0, 1.0];
        let dim = [0.7, 0.7, 0.7, 1.0];
        let hi = [1.0, 0.82, 0.2, 1.0];
        h.text(&mut out, aspect, "Extras", (x + 12.0, y + 8.0), FONT_MAIN, white);
        let mut yy = y + 8.0 + FONT_MAIN + 10.0;
        let sel_bar = |out: &mut Vec<Sprite>, yy: f32| out.push(h.quad(aspect, x + 6.0, yy - 3.0, w - 12.0, line, [0.85, 0.1, 0.1, 0.55]));
        // page row
        if ui.row == 0 {
            sel_bar(&mut out, yy);
        }
        let ptxt = if pages.len() > 1 { format!("<  {}  >", page.title) } else { page.title.to_string() };
        h.text(&mut out, aspect, &ptxt, (x + 12.0, yy), FONT_SMALL, if ui.row == 0 { hi } else { white });
        yy += line * 1.4;
        for (n, r) in page.rows.iter().enumerate() {
            let sel = ui.row == n + 1;
            if sel {
                sel_bar(&mut out, yy);
            }
            h.text(&mut out, aspect, r.label, (x + 12.0, yy), FONT_SMALL, if sel { hi } else { white });
            let vt = match r.kind {
                Kind::Choice => {
                    let v = r.values[(r.get)(&s).min(r.values.len() - 1)];
                    if sel { format!("<  {v}  >") } else { v.to_string() }
                }
                Kind::Text(field) => match editing().filter(|(f, _)| *f == field) {
                    // the token shows as stars while typed, and never in full
                    Some((_, t)) => format!("{}_", if field.secure() { "*".repeat(t.chars().count()) } else { t }),
                    None => {
                        let v = field.value(&s);
                        if v.is_empty() {
                            "not set".to_string()
                        } else if field.secure() {
                            "********".to_string()
                        } else {
                            v.to_string()
                        }
                    }
                },
                Kind::HapticsTest => if self.haptics_test.is_some() { "playing...".to_string() } else { String::new() },
                Kind::SpeakerTest => String::new(),
                Kind::Test => match test_state() {
                    Test::Idle => String::new(),
                    Test::Running => "testing...".to_string(),
                    Test::Ok(_) => "OK".to_string(),
                    Test::Failed(_) => "failed".to_string(),
                },
            };
            // a long address keeps its end in view (the start gives way)
            let room = w * 0.55;
            let mut vt = vt;
            while vt.chars().count() > 4 && h.text_width(&vt, FONT_SMALL) > room {
                vt = format!("...{}", vt.chars().skip(4).collect::<String>());
            }
            let vw = h.text_width(&vt, FONT_SMALL);
            h.text(&mut out, aspect, &vt, (x + w - 12.0 - vw, yy), FONT_SMALL, if sel { hi } else { white });
            yy += line;
        }
        let reset_sel = ui.row == page.rows.len() + 1;
        if reset_sel {
            sel_bar(&mut out, yy);
        }
        h.text(&mut out, aspect, "Reset this page to original", (x + 12.0, yy), FONT_SMALL, if reset_sel { hi } else { dim });
        yy += line * 1.5;
        // the selected row's explanation
        let help = if ui.row >= 1 && ui.row <= page.rows.len() && editing().is_some() {
            if cfg!(target_os = "macos") { "Type, Cmd+V pastes. Enter: keep.  Esc: cancel.".to_string() } else { "Enter the text in the keyboard.".to_string() }
        } else if ui.row >= 1 && ui.row <= page.rows.len() && matches!(page.rows[ui.row - 1].kind, Kind::Test) {
            match test_state() {
                Test::Ok(who) => format!("Connected: {who}."),
                Test::Failed(e) => format!("Not connected: {e}."),
                Test::Running => "Asking the server...".to_string(),
                Test::Idle => format!("{} Enter / A: test.", page.rows[ui.row - 1].help),
            }
        } else if ui.row >= 1 && ui.row <= page.rows.len() {
            let r = &page.rows[ui.row - 1];
            let when = match r.applies {
                Applies::Now => "applies now",
                Applies::NextRace => "applies from the next race",
                Applies::NextStart => "applies at the next start",
            };
            let act = if matches!(r.kind, Kind::Text(_)) { " Enter / A: type." } else { "" };
            format!("{}{act} ({when})", r.help)
        } else if reset_sel {
            "Every setting on this page back to the original game's behaviour.".to_string()
        } else {
            "Left / right: page.  B / Esc: close.".to_string()
        };
        // wrapped to the panel (at most three lines)
        let mut lines: Vec<String> = vec![String::new()];
        for word in help.split(' ') {
            let cur = lines.last().cloned().unwrap_or_default();
            let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if !cur.is_empty() && h.text_width(&next, FONT_ITALIC) > w - 24.0 {
                lines.push(word.to_string());
            } else if let Some(l) = lines.last_mut() {
                *l = next;
            }
        }
        for l in lines.iter().take(3) {
            h.text(&mut out, aspect, l, (x + 12.0, yy), FONT_ITALIC, dim);
            yy += FONT_ITALIC + 4.0;
        }
        Hud::resolve_text(&mut out[from..], (0.0, 0.0));
        PENDING_TEXT.with(|p| p.borrow_mut().clear());
        self.scene.sprites.extend(out);
    }
}
