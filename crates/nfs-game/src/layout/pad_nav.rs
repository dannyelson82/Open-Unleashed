//! **PORT-DEVIATION (tvOS, D-pad on the menus)**: the original's front end is mouse-driven, and its lists, drop-downs
//! and sliders take the pointer only (`controls.md` §4.171.8), so a pad's highlight never reached the garage's car
//! picker or the parts shop's lists (real Apple TV, 2026-11-27). On tvOS the D-pad moves a hidden pointer from one
//! target to the next instead, and A clicks: every widget then works exactly as it does under the mouse.
//!
//! The targets are found with the screen's own hit test, not recomputed geometry: a grid of points over the
//! 640 × 480 design is sorted by what each lands on — a widget a click does something to, a row of a list, a row
//! of an open drop-down — and each target is the point of its group nearest the group's middle.

use super::{LayoutScreen, WidgetClass};

/// What a point of the grid lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Target {
    Widget(usize),
    ListRow(usize, usize),
    ComboRow(usize, usize),
}

/// The grid's spacing, design pixels: fine enough for the smallest target in the shipped screens (a list row is
/// about 12 high, a check box 10 wide). The scan is a few milliseconds of hit tests, so its answer is kept
/// while [`pad_state_key`](LayoutScreen::pad_state_key) stays the same.
const STEP: f32 = 6.0;

impl LayoutScreen {
    /// Whether a click on widget `i` (flattened index) does something: a button, a drop-down, a box, a slider…
    fn pad_interactive(&self, i: usize) -> bool {
        if let Some((k, j)) = self.route(i) {
            return self.groups.get(&k).is_some_and(|g| g.pad_interactive(j));
        }
        let Some(e) = self.layout.elements.get(i) else { return false };
        let class = self.classes[i];
        if class == WidgetClass::ListBox {
            return false;   // its rows are the targets
        }
        let acts = |k: &str| e.get(k).is_some_and(|v| !v.is_empty());
        self.hoverable[i]
            || class.takes_click()
            || class.takes_focus_on_press()
            || acts("destination")
            || acts("callback")
            || matches!(
                class,
                WidgetClass::GotoButton
                    | WidgetClass::BottomBarButton
                    | WidgetClass::TabButton
                    | WidgetClass::GraphicTabButton
                    | WidgetClass::CarSettingButton
                    | WidgetClass::CarSettingSlider
                    | WidgetClass::ComboBoxSlider
                    | WidgetClass::ColorBox
                    | WidgetClass::CheckBox
                    | WidgetClass::ControllerSetup
                    | WidgetClass::CameraSelect
                    | WidgetClass::KnockoutTrackList
                    | WidgetClass::ChronicleSlider
                    | WidgetClass::ChroniclePictureIcon
            )
    }

    /// The D-pad's targets on this screen, in design pixels. While a drop-down is open only its rows count.
    pub fn pad_targets(&self) -> Vec<(f32, f32)> {
        let lists: Vec<usize> = self.list_boxes().into_iter().map(|w| w.index).collect();
        let open = self.any_open_combo().is_some();
        let mut groups: Vec<(Target, Vec<(f32, f32)>)> = Vec::new();
        let mut y = STEP / 2.0;
        while y < 480.0 {
            let mut x = STEP / 2.0;
            while x < 640.0 {
                let t = if open {
                    self.combo_row_at(x, y).map(|(c, r)| Target::ComboRow(c, r))
                } else {
                    lists
                        .iter()
                        .find_map(|&l| self.list_row_at(l, x, y).map(|r| Target::ListRow(l, r)))
                        .or_else(|| self.hit_at(x, y).filter(|&i| self.pad_interactive(i)).map(Target::Widget))
                };
                if let Some(t) = t {
                    match groups.iter_mut().find(|(g, _)| *g == t) {
                        Some((_, pts)) => pts.push((x, y)),
                        None => groups.push((t, vec![(x, y)])),
                    }
                }
                x += STEP;
            }
            y += STEP;
        }
        groups
            .into_iter()
            .filter_map(|(_, pts)| {
                let n = pts.len() as f32;
                let (mx, my) = pts.iter().fold((0.0, 0.0), |(a, b), (x, y)| (a + x / n, b + y / n));
                // the group's own point nearest its middle (an L-shaped area's middle can lie outside it)
                pts.into_iter().min_by(|a, b| {
                    let da = (a.0 - mx).powi(2) + (a.1 - my).powi(2);
                    let db = (b.0 - mx).powi(2) + (b.1 - my).powi(2);
                    da.total_cmp(&db)
                })
            })
            .collect()
    }

    /// What the targets depend on besides the screen itself: the open drop-down, the lists' rows and scroll
    /// positions, the hidden widgets and panels — here and in every embedded screen. Equal keys, same targets.
    pub fn pad_state_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.open_combo.hash(&mut h);
        let mut scroll: Vec<_> = self.scroll.iter().collect();
        scroll.sort_unstable();
        scroll.hash(&mut h);
        let mut lists: Vec<(usize, usize)> = self.lists.iter().map(|(k, (rows, _))| (*k, rows.len())).collect();
        lists.sort_unstable();
        lists.hash(&mut h);
        self.hidden_flags.hash(&mut h);
        let mut closed: Vec<_> = self.closed.iter().collect();
        closed.sort_unstable();
        closed.hash(&mut h);
        let mut keys: Vec<usize> = self.groups.keys().copied().collect();
        keys.sort_unstable();
        for k in keys {
            (k, self.groups[&k].pad_state_key()).hash(&mut h);
        }
        h.finish()
    }

    /// The list under a design point, for the right stick's scrolling; with none there, the screen's only list.
    pub fn pad_scroll_list(&self, at: Option<(f32, f32)>) -> Option<usize> {
        let lists: Vec<usize> = self.list_boxes().into_iter().map(|w| w.index).collect();
        if let Some((x, y)) = at
            && let Some(&l) = lists.iter().find(|&&l| self.hit_at(x, y) == Some(l) || self.list_row_at(l, x, y).is_some())
        {
            return Some(l);
        }
        (lists.len() == 1).then(|| lists[0])
    }
}

impl LayoutScreen {
    /// The right stick as a scroll wheel: `delta` rows on the open drop-down, else on the list under the pointer
    /// (or the screen's only list) — the same moves the lists' own arrow buttons make.
    pub fn pad_scroll(&mut self, at: Option<(f32, f32)>, delta: i32) {
        if let Some(i) = self.any_open_combo() {
            let now = self.pad_scroll_pos(i);
            self.set_combo_scroll_pos(i, now + delta);
        } else if let Some(l) = self.pad_scroll_list(at) {
            self.scroll_list(l, delta);
        }
    }

    fn pad_scroll_pos(&self, i: usize) -> i32 {
        if let Some((k, j)) = self.route(i) {
            return self.groups.get(&k).map_or(0, |g| g.pad_scroll_pos(j));
        }
        self.scroll.get(&i).copied().unwrap_or(0) as i32
    }
}

static POINTER_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// tvOS: the pointer is moved by the D-pad and not drawn (the widget under it shows its hover state instead).
pub fn set_pointer_hidden(hidden: bool) {
    POINTER_HIDDEN.store(hidden, std::sync::atomic::Ordering::Relaxed);
}

pub(super) fn pointer_hidden() -> bool {
    POINTER_HIDDEN.load(std::sync::atomic::Ordering::Relaxed)
}

/// The target the D-pad goes to from `from` in direction `dir` ((±1, 0) or (0, ±1)): the nearest one ahead, where
/// being off to the side counts double — so Down from a button goes to the one below it rather than to a nearer
/// one beside it.
pub fn pad_step(targets: &[(f32, f32)], from: (f32, f32), dir: (f32, f32)) -> Option<(f32, f32)> {
    targets
        .iter()
        .copied()
        .filter_map(|t| {
            let (dx, dy) = (t.0 - from.0, t.1 - from.1);
            let ahead = dx * dir.0 + dy * dir.1;
            let side = (dx * dir.1 - dy * dir.0).abs();
            (ahead > 2.0).then_some((t, ahead + 2.0 * side))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(t, _)| t)
}

#[cfg(test)]
mod tests {
    use super::pad_step;

    #[test]
    fn down_prefers_the_target_below_over_a_nearer_one_beside() {
        let targets = [(100.0, 100.0), (130.0, 105.0), (100.0, 140.0)];
        assert_eq!(pad_step(&targets, (100.0, 100.0), (0.0, 1.0)), Some((100.0, 140.0)));
        assert_eq!(pad_step(&targets, (100.0, 100.0), (1.0, 0.0)), Some((130.0, 105.0)));
        assert_eq!(pad_step(&targets, (100.0, 100.0), (0.0, -1.0)), None, "nothing above: stay");
    }
}
