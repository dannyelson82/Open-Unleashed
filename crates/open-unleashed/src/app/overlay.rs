//! **PORT-DEVIATION (port's own overlays, TVOS.md Stages 5.4 / 7.1)** — drawn after the HUD with the HUD's font,
//! like photo mode's hint:
//!
//! * the **frame-time overlay** (`NFS_FPS_OVERLAY=1`, or Extras › Display): frames per second, the average and
//!   worst frame time of the last two seconds, and the frame budget (16.7 ms at 60 Hz);
//! * tvOS: **"connect a controller"** while a race runs and no pad is connected (the Siri Remote drives the
//!   menus only).

use crate::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

/// The frame-time overlay is on.
pub(crate) static FRAME_OVERLAY: AtomicBool = AtomicBool::new(false);

thread_local! {
    static FRAMES: RefCell<VecDeque<f32>> = const { RefCell::new(VecDeque::new()) };
}

/// Read the start-up switch once.
pub(crate) fn init_overlay_switch() {
    if std::env::var_os("NFS_FPS_OVERLAY").is_some_and(|v| v != "0") {
        FRAME_OVERLAY.store(true, Ordering::Relaxed);
    }
}

/// (fps, average ms, worst ms) of the last ~2 s of frames.
fn frame_stats(dt: f32) -> (f32, f32, f32) {
    FRAMES.with(|f| {
        let mut f = f.borrow_mut();
        if dt > 0.0 && dt < 1.0 {
            f.push_back(dt);
        }
        while f.len() > 120 {
            f.pop_front();
        }
        let n = f.len().max(1) as f32;
        let avg = f.iter().sum::<f32>() / n;
        let worst = f.iter().copied().fold(0.0f32, f32::max);
        (if avg > 0.0 { 1.0 / avg } else { 0.0 }, avg * 1000.0, worst * 1000.0)
    })
}

impl App {
    /// The port's own overlays, appended after the HUD's sprites (and photo mode's hint).
    pub(crate) fn port_overlays(&mut self) {
        let Some(h) = self.hud.as_ref().filter(|h| h.font.is_some()) else { return };
        let aspect = self.renderer.as_ref().map_or(HUD_REF_W / HUD_REF_H, |r| r.aspect());
        let (left, top, right, bottom) = Hud::edges();
        let mut out = Vec::new();
        let mut text_from = Vec::new();

        if FRAME_OVERLAY.load(Ordering::Relaxed) {
            let (fps, avg, worst) = frame_stats(self.frame_dt);
            let line = format!("{fps:5.1} fps   avg {avg:5.2} ms   worst {worst:5.2} ms   budget 16.67 ms");
            let w = h.text_width(&line, FONT_SMALL);
            let (x, y) = (left + 8.0, top + 6.0);
            let col = if worst > 17.5 { [1.0, 0.55, 0.45, 1.0] } else { [0.75, 1.0, 0.75, 1.0] };
            out.push(h.quad(aspect, x - 4.0, y - 2.0, w + 10.0, FONT_SMALL + 6.0, [0.0, 0.0, 0.0, 0.55]));
            text_from.push(out.len());
            h.text(&mut out, aspect, &line, (x, y + 1.0), FONT_SMALL, col);
            // the HD texture pack's coverage of what this race uploaded through it
            if nfs_assets::hd::available() {
                let (hd, all) = nfs_assets::hd::counts();
                let line = format!("HD textures in use: {hd} / {all}{}", if nfs_game::extras::get().graphics.textures == 1 { "" } else { "  (Textures: Original)" });
                let w = h.text_width(&line, FONT_SMALL);
                let y = y + FONT_SMALL + 8.0;
                out.push(h.quad(aspect, x - 4.0, y - 2.0, w + 10.0, FONT_SMALL + 6.0, [0.0, 0.0, 0.0, 0.55]));
                text_from.push(out.len());
                h.text(&mut out, aspect, &line, (x, y + 1.0), FONT_SMALL, [0.75, 0.85, 1.0, 1.0]);
            }
        }

        if cfg!(target_os = "tvos") && self.vehicle.is_some() && self.extras_ui.is_none() && self.gamepads.racing_pad_count() == 0 && self.replay.stream.as_ref().is_none_or(|r| !matches!(r.mode, nfs_game::replay::Mode::Playing | nfs_game::replay::Mode::Exhausted)) {
            let lines = ["Connect a game controller to race", "(Xbox or PlayStation; the Siri Remote drives the menus only)"];
            let line_h = FONT_SMALL + 4.0;
            let w = lines.iter().map(|l| h.text_width(l, FONT_SMALL)).fold(0.0f32, f32::max);
            let hh = line_h * lines.len() as f32 + 10.0;
            let (x, y) = ((left + right - w) * 0.5, (top + bottom) * 0.5 - hh - 40.0);
            out.push(h.quad(aspect, x - 10.0, y, w + 20.0, hh, [0.0, 0.0, 0.0, 0.7]));
            text_from.push(out.len());
            for (n, l) in lines.iter().enumerate() {
                let lw = h.text_width(l, FONT_SMALL);
                h.text(&mut out, aspect, l, (x + (w - lw) * 0.5, y + 5.0 + line_h * n as f32), FONT_SMALL, [1.0, 1.0, 1.0, 1.0]);
            }
        }

        if out.is_empty() {
            return;
        }
        if let Some(&from) = text_from.first() {
            Hud::resolve_text(&mut out[from..], (0.0, 0.0));
        }
        PENDING_TEXT.with(|p| p.borrow_mut().clear());
        self.scene.sprites.extend(out);
    }
}
