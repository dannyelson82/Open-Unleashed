//! **PORT-DEVIATION (Extras settings)** — the port's own settings, which the original's option screens don't
//! have (`enhancements-plan.md` §0). Kept apart from the original's profile options: `extras.json` in the data
//! folder (beside `SaveData/`), versioned, every default = the original's behaviour ("authentic first").
//! The settings are read anywhere through [`get`]; the Extras menu changes them with [`update`], which saves.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub const VERSION: u32 = 1;

/// Render resolution on tvOS (the UIKit view's scale): native = the screen's own (4K on an Apple TV 4K).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderScale {
    #[default]
    Native,
    P1440,
    P1080,
}

impl RenderScale {
    /// The view scale (1080p logical points × scale = pixels); `None` = leave the screen's own.
    pub fn factor(self) -> Option<f64> {
        match self {
            RenderScale::Native => None,
            RenderScale::P1440 => Some(1.5),
            RenderScale::P1080 => Some(1.0),
        }
    }
}

/// HUD size (`enhancements-plan.md` §2.2): the original draws the HUD in constant design pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudSize {
    #[default]
    Original,
    Large,
    ExtraLarge,
}

impl HudSize {
    pub fn scale(self) -> f32 {
        match self {
            HudSize::Original => 1.0,
            HudSize::Large => 1.5,
            HudSize::ExtraLarge => 2.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    pub fps_overlay: bool,
    pub render_scale: RenderScale,
    pub hud_size: HudSize,
}

/// Head-look strength (`airpods-plan.md` H): the view turn for 20° of head turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadLookStrength {
    Low,
    #[default]
    Medium,
    High,
}

impl HeadLookStrength {
    pub fn view_at_20(self) -> f32 {
        match self {
            HeadLookStrength::Low => 50.0,
            HeadLookStrength::Medium => 75.0,
            HeadLookStrength::High => 100.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Controller {
    /// Mac only: AirPods head tracking turns the in-car view (off = the original)
    pub head_look: bool,
    pub head_look_strength: HeadLookStrength,
    pub head_look_pitch: bool,
}

/// Driving assists (`enhancements-plan.md` §4; `nfs_game::assists`). Indices into the Extras rows' values.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Assists {
    /// steering response: 0 linear (original), 1 soft, 2 softer
    pub steering_response: u8,
    /// speed-sensitive steering: 0 off, 1 low, 2 medium, 3 high
    pub speed_steering: u8,
    /// countersteer assist: 0 off, 1 light, 2 strong
    pub countersteer: u8,
    pub abs: bool,
}

impl Assists {
    pub fn settings(&self) -> crate::assists::AssistSettings {
        crate::assists::AssistSettings {
            curve: [1.0, 1.5, 2.0][self.steering_response.min(2) as usize],
            lock_at_200: [1.0, 0.75, 0.55, 0.4][self.speed_steering.min(3) as usize],
            countersteer: [0.0, 0.5, 1.0][self.countersteer.min(2) as usize],
            abs: self.abs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExtrasSettings {
    pub version: u32,
    pub display: Display,
    pub controller: Controller,
    pub assists: Assists,
}

impl Default for ExtrasSettings {
    fn default() -> Self {
        Self { version: VERSION, display: Display::default(), controller: Controller::default(), assists: Assists::default() }
    }
}

impl ExtrasSettings {
    /// Parse `extras.json`; anything unknown or missing takes its default (a newer or older file never
    /// stops the game).
    pub fn parse(text: &str) -> Self {
        let mut s: Self = serde_json::from_str(text).unwrap_or_default();
        s.version = VERSION;
        s
    }

    pub fn to_json(&self) -> String {
        let mut t = serde_json::to_string_pretty(self).expect("extras serialise");
        t.push('\n');
        t
    }
}

/// `extras.json` beside `SaveData/` (`NFS5_EXTRAS` overrides, for tests).
pub fn path() -> PathBuf {
    std::env::var_os("NFS5_EXTRAS").map(PathBuf::from).unwrap_or_else(|| nfs_assets::platform::data_dir().join("extras.json"))
}

fn cell() -> &'static Mutex<ExtrasSettings> {
    static S: OnceLock<Mutex<ExtrasSettings>> = OnceLock::new();
    S.get_or_init(|| {
        let s = std::fs::read_to_string(path()).map(|t| ExtrasSettings::parse(&t)).unwrap_or_default();
        Mutex::new(s)
    })
}

/// The current settings (loaded from `extras.json` on first use).
pub fn get() -> ExtrasSettings {
    cell().lock().map(|s| s.clone()).unwrap_or_default()
}

/// Change the settings and save them (written through a temporary file; a failed write is logged, the change
/// stays for this run).
pub fn update(f: impl FnOnce(&mut ExtrasSettings)) -> ExtrasSettings {
    let snapshot = {
        let Ok(mut s) = cell().lock() else { return ExtrasSettings::default() };
        f(&mut s);
        s.clone()
    };
    let p = path();
    let write = || -> std::io::Result<()> {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, snapshot.to_json())?;
        std::fs::rename(tmp, &p)
    };
    if let Err(e) = write() {
        tracing::warn!(error = %e, path = %p.display(), "extras.json not saved");
    }
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_original() {
        let s = ExtrasSettings::default();
        assert!(!s.display.fps_overlay);
        assert_eq!(s.display.render_scale, RenderScale::Native);
        assert_eq!(s.display.hud_size, HudSize::Original);
        assert!(!s.assists.settings().any(), "no assist by default");
    }

    #[test]
    fn round_trip_and_tolerant_parse() {
        let mut s = ExtrasSettings::default();
        s.display.fps_overlay = true;
        s.display.render_scale = RenderScale::P1440;
        assert_eq!(ExtrasSettings::parse(&s.to_json()), s);
        // missing fields, unknown fields, garbage: defaults, never an error
        assert_eq!(ExtrasSettings::parse(r#"{"display":{"fps_overlay":true},"future":1}"#).display.fps_overlay, true);
        assert_eq!(ExtrasSettings::parse("not json"), ExtrasSettings::default());
        assert_eq!(ExtrasSettings::parse(r#"{"display":{"render_scale":"p1080"}}"#).display.render_scale, RenderScale::P1080);
    }
}
