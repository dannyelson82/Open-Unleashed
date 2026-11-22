//! **PORT-DEVIATION (Extras menu, `enhancements-plan.md` §0)** — the port's own settings menu, drawn like the
//! port's other overlays (the HUD's font and quads), opened from the pause menu's last item. Fully driven by
//! the d-pad / arrows (up/down: rows; left/right: values, or pages on the page row; A/Enter: the reset row;
//! B/Esc: close). Each row shows its value, a one-line explanation and when it applies. The settings live in
//! `nfs_game::extras` (`extras.json`).

use crate::*;
use nfs_game::extras::{self, ExtrasSettings, HeadLookStrength, HudSize, RenderScale};

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
    NextStart,
}

/// One setting row.
struct Row {
    label: &'static str,
    help: &'static str,
    applies: Applies,
    /// the platforms it means something on
    shown: bool,
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
                values: &["Off", "On"],
                get: |s| usize::from(s.display.fps_overlay),
                set: |s, v| s.display.fps_overlay = v == 1,
            },
            Row {
                label: "HUD size",
                help: "Scales the race HUD. The original draws it in fixed pixels, small on a big TV.",
                applies: Applies::Now,
                shown: true,
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
        title: "Controller",
        rows: vec![
            Row {
                label: "Head-look (AirPods)",
                help: "Turn your head to look around inside the car (bumper and driver views). Needs AirPods with head tracking.",
                applies: Applies::Now,
                shown: cfg!(target_os = "macos"),
                values: &["Off", "On"],
                get: |s| usize::from(s.controller.head_look),
                set: |s, v| s.controller.head_look = v == 1,
            },
            Row {
                label: "Head-look strength",
                help: "How far the view turns for 20 degrees of head turn (50 / 75 / 100 degrees).",
                applies: Applies::Now,
                shown: cfg!(target_os = "macos"),
                values: &["Low", "Medium", "High"],
                get: |s| s.controller.head_look_strength as usize,
                set: |s, v| s.controller.head_look_strength = [HeadLookStrength::Low, HeadLookStrength::Medium, HeadLookStrength::High][v.min(2)],
            },
            Row {
                label: "Head-look up and down",
                help: "The view also tilts a little with your head (at most 15 degrees).",
                applies: Applies::Now,
                shown: cfg!(target_os = "macos"),
                values: &["Off", "On"],
                get: |s| usize::from(s.controller.head_look_pitch),
                set: |s, v| s.controller.head_look_pitch = v == 1,
            },
        ],
        reset: |s| s.controller = Default::default(),
    }]
    .into_iter()
    .map(|mut p| {
        p.rows.retain(|r| r.shown);
        p
    })
    .filter(|p| !p.rows.is_empty())
    .collect()
}

/// Apply what takes effect at once (called after every change and at start-up).
pub(crate) fn apply_now(s: &ExtrasSettings) {
    overlay::FRAME_OVERLAY.store(s.display.fps_overlay || std::env::var_os("NFS_FPS_OVERLAY").is_some_and(|v| v != "0"), std::sync::atomic::Ordering::Relaxed);
    crate::hud::set_hud_scale(s.display.hud_size.scale());
}

impl App {
    pub(crate) fn extras_open(&mut self) {
        self.extras_ui = Some(ExtrasUi::default());
    }

    /// A key while the Extras menu is up.
    pub(crate) fn extras_key(&mut self, key: MenuKey) {
        let Some(mut ui) = self.extras_ui else { return };
        let pages = pages();
        let page = &pages[ui.page.min(pages.len() - 1)];
        let last = page.rows.len() + 1; // the reset row
        match key {
            MenuKey::Up => ui.row = ui.row.saturating_sub(1),
            MenuKey::Down => ui.row = (ui.row + 1).min(last),
            MenuKey::Left | MenuKey::Right => {
                let d: isize = if matches!(key, MenuKey::Left) { -1 } else { 1 };
                if ui.row == 0 {
                    ui.page = (ui.page as isize + d).rem_euclid(pages.len() as isize) as usize;
                } else if let Some(r) = page.rows.get(ui.row - 1) {
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
                    let s = extras::update(|s| (page.reset)(s));
                    apply_now(&s);
                    self.fe_sound(9);
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

    /// The menu over the paused race.
    pub(crate) fn extras_sprites(&mut self) {
        let Some(ui) = self.extras_ui else { return };
        let Some(h) = self.hud.as_ref().filter(|h| h.font.is_some()) else { return };
        let aspect = self.hud_aspect();
        let s = extras::get();
        let pages = pages();
        let page = &pages[ui.page.min(pages.len() - 1)];
        let (w, line) = (420.0f32, FONT_SMALL + 8.0);
        let rows = page.rows.len() + 2;
        let hgt = line * (rows as f32 + 4.0) + 16.0;
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
            let v = r.values[(r.get)(&s).min(r.values.len() - 1)];
            let vt = if sel { format!("<  {v}  >") } else { v.to_string() };
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
        let help = if ui.row >= 1 && ui.row <= page.rows.len() {
            let r = &page.rows[ui.row - 1];
            let when = match r.applies {
                Applies::Now => "applies now",
                Applies::NextStart => "applies at the next start",
            };
            format!("{} ({when})", r.help)
        } else if reset_sel {
            "Every setting on this page back to the original game's behaviour.".to_string()
        } else {
            "Left / right: page.  B / Esc: close.".to_string()
        };
        h.text(&mut out, aspect, &help, (x + 12.0, yy), FONT_ITALIC, dim);
        Hud::resolve_text(&mut out[from..], (0.0, 0.0));
        PENDING_TEXT.with(|p| p.borrow_mut().clear());
        self.scene.sprites.extend(out);
    }
}
