//! The inspector's choices and their meaning (EDT-2 speed, EDT-3 background), display-free.

use cm_core::edit::{MAX_AUDIO_SPEED, PADDING_PCT_RANGE, RADIUS_PX_MAX};
use cm_core::{Background, BackgroundKind};
use cm_i18n::{tr, tr_ctx};
use serde_json::{json, Value};

/// The speeds offered as buttons (EDT-2: 0.5×, 1×, 1.5×, 2×, 4×; the API takes 0.25 to 8).
pub const SPEEDS: [f64; 5] = [0.5, 1.0, 1.5, 2.0, 4.0];

/// What the audio does at `factor` (EDT-2.AC3): pitch-preserving stretch up to 2×, muted above it
/// unless "Keep audio" is on.
pub fn audio_note(factor: f64, keep_audio: bool) -> String {
    if factor <= MAX_AUDIO_SPEED {
        tr!("Audio follows the speed, pitch unchanged")
    } else if keep_audio {
        tr!("Audio is kept")
    } else {
        tr!("Audio is muted above 2\u{00D7}")
    }
}

/// Whether the "Keep audio" control applies at `factor`.
pub fn keep_audio_applies(factor: f64) -> bool {
    factor > MAX_AUDIO_SPEED
}

/// The background presets of EDT-3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    None,
    SoftGray,
    /// "Lens Violet gradient" in the EDT-3 spec; the design kit retires the Lens Violet palette
    /// for the app's own chrome (docs/DESIGN.md), so the spec and the kit disagree on the name. The
    /// label follows the spec until the owner decides.
    LensViolet,
    Custom,
}

impl Preset {
    pub const ALL: [Preset; 4] = [
        Preset::None,
        Preset::SoftGray,
        Preset::LensViolet,
        Preset::Custom,
    ];

    pub fn label(self) -> String {
        match self {
            Preset::None => tr_ctx!("background preset", "None"),
            Preset::SoftGray => tr_ctx!("background preset", "Soft gray"),
            Preset::LensViolet => tr_ctx!("background preset", "Lens Violet gradient"),
            Preset::Custom => tr_ctx!("background preset", "Custom"),
        }
    }
}

/// Swatches offered by the Custom preset.
pub const SWATCHES: [&str; 6] = [
    "#ffffff", "#111114", "#e8e9ec", "#ffd9c7", "#cfe8ff", "#d9f2dc",
];

const SOFT_GRAY: &str = "#e8e9ec";
const VIOLET: [&str; 2] = ["#4b2bd6", "#9a6bff"];

/// A complete background, the arguments of `set_background`.
#[derive(Debug, Clone, PartialEq)]
pub struct BgSpec {
    pub kind: BackgroundKind,
    pub value: Value,
    pub padding_pct: f64,
    pub radius_px: u32,
    pub shadow: bool,
}

impl BgSpec {
    pub fn of(b: &Background) -> BgSpec {
        BgSpec {
            kind: b.kind,
            value: b.value.clone(),
            padding_pct: b.padding_pct,
            radius_px: b.radius_px,
            shadow: b.shadow,
        }
    }

    /// The preset's own values (padding 8 %, radius 12 px and a shadow, EDT-3.AC1's example).
    pub fn preset(p: Preset, current: &BgSpec) -> BgSpec {
        match p {
            Preset::None => BgSpec {
                kind: BackgroundKind::None,
                value: Value::Null,
                padding_pct: 0.0,
                radius_px: 0,
                shadow: false,
            },
            Preset::SoftGray => BgSpec {
                kind: BackgroundKind::Color,
                value: json!(SOFT_GRAY),
                padding_pct: 8.0,
                radius_px: 12,
                shadow: true,
            },
            Preset::LensViolet => BgSpec {
                kind: BackgroundKind::Gradient,
                value: json!(VIOLET),
                padding_pct: 8.0,
                radius_px: 12,
                shadow: true,
            },
            // Custom starts from what is there, as a plain color when it was none.
            Preset::Custom => {
                if current.kind == BackgroundKind::None {
                    BgSpec {
                        kind: BackgroundKind::Color,
                        value: json!(SWATCHES[0]),
                        padding_pct: current.padding_pct.max(8.0),
                        radius_px: current.radius_px.max(12),
                        shadow: current.shadow,
                    }
                } else {
                    current.clone()
                }
            }
        }
    }

    /// Which preset this background is (Custom when it matches none).
    pub fn which(&self) -> Preset {
        let same = |p: Preset| {
            let s = BgSpec::preset(p, self);
            s.kind == self.kind
                && s.value == self.value
                && (s.padding_pct - self.padding_pct).abs() < 1e-6
                && s.radius_px == self.radius_px
                && s.shadow == self.shadow
        };
        if self.kind == BackgroundKind::None {
            Preset::None
        } else if same(Preset::SoftGray) {
            Preset::SoftGray
        } else if same(Preset::LensViolet) {
            Preset::LensViolet
        } else {
            Preset::Custom
        }
    }

    pub fn with_padding(&self, pct: f64) -> BgSpec {
        BgSpec {
            padding_pct: pct.clamp(PADDING_PCT_RANGE.0, PADDING_PCT_RANGE.1),
            ..self.clone()
        }
    }

    pub fn with_radius(&self, px: u32) -> BgSpec {
        BgSpec {
            radius_px: px.min(RADIUS_PX_MAX),
            ..self.clone()
        }
    }

    pub fn with_shadow(&self, on: bool) -> BgSpec {
        BgSpec {
            shadow: on,
            ..self.clone()
        }
    }

    pub fn with_color(&self, hex: &str) -> BgSpec {
        BgSpec {
            kind: BackgroundKind::Color,
            value: json!(hex),
            ..self.clone()
        }
    }

    /// The `set_background` input for capture `id` (the daemon validates it again).
    pub fn params(&self, id: &str) -> Value {
        let mut o = json!({
            "capture_id": id,
            "kind": self.kind.as_str(),
            "padding_pct": self.padding_pct,
            "radius_px": self.radius_px,
            "shadow": self.shadow,
        });
        if !self.value.is_null() {
            o["value"] = self.value.clone();
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> BgSpec {
        BgSpec::of(&Background::default())
    }

    /// EDT-3.AC3: the "None" preset is the background off, screen edge to edge.
    #[test]
    fn edt3_ac3_none_preset_is_edge_to_edge() {
        let s = BgSpec::preset(Preset::SoftGray, &none());
        let off = BgSpec::preset(Preset::None, &s);
        assert_eq!(off.kind, BackgroundKind::None);
        assert_eq!(
            (off.padding_pct, off.radius_px, off.shadow),
            (0.0, 0, false)
        );
        assert!(off.params("rec_1").get("value").is_none());
    }

    /// EDT-3.AC1: the filled presets use the 8 % padding and 12 px radius of the example.
    #[test]
    fn edt3_ac1_presets_use_8_percent_and_12_px() {
        for p in [Preset::SoftGray, Preset::LensViolet] {
            let s = BgSpec::preset(p, &none());
            assert_eq!((s.padding_pct, s.radius_px), (8.0, 12));
            assert!(s.shadow);
        }
    }

    #[test]
    fn presets_round_trip_through_which() {
        for p in [Preset::None, Preset::SoftGray, Preset::LensViolet] {
            assert_eq!(BgSpec::preset(p, &none()).which(), p);
        }
        let custom = BgSpec::preset(Preset::SoftGray, &none()).with_padding(5.0);
        assert_eq!(custom.which(), Preset::Custom);
        assert_eq!(
            BgSpec::preset(Preset::Custom, &none()).which(),
            Preset::Custom
        );
    }

    /// EDT-3 validation: padding 0-20 %, radius 0-24 px.
    #[test]
    fn edt3_limits_are_clamped() {
        let s = BgSpec::preset(Preset::SoftGray, &none());
        assert_eq!(s.with_padding(99.0).padding_pct, 20.0);
        assert_eq!(s.with_padding(-3.0).padding_pct, 0.0);
        assert_eq!(s.with_radius(500).radius_px, 24);
    }

    #[test]
    fn params_carry_every_field_and_the_value() {
        let p = BgSpec::preset(Preset::LensViolet, &none()).params("rec_9");
        assert_eq!(p["capture_id"], "rec_9");
        assert_eq!(p["kind"], "gradient");
        assert_eq!(p["value"], json!(["#4b2bd6", "#9a6bff"]));
        assert_eq!(p["padding_pct"], 8.0);
        assert_eq!(p["radius_px"], 12);
        assert_eq!(p["shadow"], true);
    }

    /// EDT-2.AC3: above 2× the audio is muted unless kept.
    #[test]
    fn edt2_ac3_audio_note_follows_the_speed() {
        assert!(keep_audio_applies(4.0));
        assert!(!keep_audio_applies(2.0));
        assert_eq!(audio_note(4.0, false), "Audio is muted above 2\u{00D7}");
        assert_eq!(audio_note(4.0, true), "Audio is kept");
        assert!(audio_note(1.5, false).contains("pitch"));
    }
}
