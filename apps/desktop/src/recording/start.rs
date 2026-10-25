//! The start sheet's choices (REC-1, CAM-1, CAM-4, AUD-1) as `start_recording` parameters, and the
//! settings behind them.
//!
//! The engine fills everything the request leaves out from the `recording.*` settings
//! (`recording.size`, `fps`, `codec`, `quality`, `cursor`, `countdown_s`) and
//! `audio.noise_suppression`, so the desktop sends only what the user chose in the sheet: the
//! target, the camera with its bubble, the microphone and system audio. Defaults are therefore
//! defined in one place (the settings), whether the recording starts from the sheet, a hotkey or an
//! agent.
//!
//! # Settings keys
//!
//! | Key | Meaning |
//! | --- | --- |
//! | `recording.size` / `fps` / `codec` / `quality` / `cursor` | read by the engine (REC-2, REC-3, REC-7) |
//! | `recording.countdown_s` / `resume_countdown_s` | read by the engine (REC-4) |
//! | `audio.noise_suppression` | read by the engine (AUD-3) |
//! | `privacy.excluded_apps` | bundle ids added to the engine's default exclusions (REC-6) |
//! | `recording.last_target` | `display`, `window` or `region`: the sheet's remembered target kind |
//! | `recording.camera`, `recording.mic`, `recording.system_audio` | remembered on/off choices |
//! | `camera.device`, `audio.mic_device` | the devices chosen in Settings -> Webcam & audio (R13): the default of every start that names none, here and in the daemon; a missing one falls back to the system default (CAM-4.AC2) and stays saved. A device picked in the sheet is for that recording only and is not written |
//! | `camera.shape`, `camera.size`, `camera.mirror`, `camera.position` | the bubble (CAM-1); the position is only restored when `camera.remember_position` is on, else every recording starts bottom-left |
//! | `camera.remember_position` | bool, default false (CAM-1.AC8) |
//! | `recording.hud_positions` | `{ "<display>": [x, y] }` HUD center fractions (REC-5.AC4) |

use cm_i18n::{tr, tr_ctx};
use serde_json::{json, Map, Value};

use crate::recording::geometry::{BubblePosition, BubbleShape, BubbleSize};

pub const KEY_SIZE: &str = "recording.size";
pub const KEY_FPS: &str = "recording.fps";
pub const KEY_CODEC: &str = "recording.codec";
pub const KEY_QUALITY: &str = "recording.quality";
pub const KEY_CURSOR: &str = "recording.cursor";
pub const KEY_COUNTDOWN: &str = "recording.countdown_s";
pub const KEY_RESUME_COUNTDOWN: &str = "recording.resume_countdown_s";
pub const KEY_NOISE: &str = "audio.noise_suppression";
pub const KEY_EXCLUDED: &str = "privacy.excluded_apps";
pub const KEY_LAST_TARGET: &str = "recording.last_target";
pub const KEY_CAMERA_ON: &str = "recording.camera";
pub const KEY_MIC_ON: &str = "recording.mic";
pub const KEY_SYSTEM_AUDIO: &str = "recording.system_audio";
pub const KEY_CAMERA_DEVICE: &str = "camera.device";
pub const KEY_MIC_DEVICE: &str = "audio.mic_device";
pub const KEY_BUBBLE_SHAPE: &str = "camera.shape";
pub const KEY_BUBBLE_SIZE: &str = "camera.size";
pub const KEY_BUBBLE_MIRROR: &str = "camera.mirror";
pub const KEY_BUBBLE_POSITION: &str = "camera.position";
/// Bool, default false: restore the last dragged `camera.position` instead of bottom-left.
pub const KEY_BUBBLE_REMEMBER: &str = "camera.remember_position";
pub const KEY_HUD_POSITIONS: &str = "recording.hud_positions";

/// What the start sheet records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Display {
        display_id: u32,
    },
    Window {
        window_id: u32,
    },
    /// `rect` is `[x, y, w, h]` in physical px relative to the display (like screenshots).
    Region {
        display_id: u32,
        rect: [i64; 4],
    },
}

impl Target {
    pub fn kind(self) -> TargetKind {
        match self {
            Target::Display { .. } => TargetKind::Display,
            Target::Window { .. } => TargetKind::Window,
            Target::Region { .. } => TargetKind::Region,
        }
    }

    pub fn to_json(self) -> Value {
        match self {
            Target::Display { display_id } => json!({ "display_id": display_id }),
            Target::Window { window_id } => json!({ "window_id": window_id }),
            Target::Region { display_id, rect } => {
                json!({ "display_id": display_id, "rect": rect })
            }
        }
    }

    /// Short description for the sheet, e.g. `Display 1`, `Window 7`, `Region 1920 × 1080`.
    pub fn summary(self) -> String {
        match self {
            Target::Display { display_id } => tr!("Display {display_id}", display_id = display_id),
            Target::Window { window_id } => tr!("Window {window_id}", window_id = window_id),
            Target::Region { rect, .. } => {
                tr!("Region {w} \u{00D7} {h}", w = rect[2], h = rect[3])
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetKind {
    #[default]
    Display,
    Window,
    Region,
}

impl TargetKind {
    pub const ALL: [TargetKind; 3] = [TargetKind::Display, TargetKind::Window, TargetKind::Region];

    pub fn key(self) -> &'static str {
        match self {
            TargetKind::Display => "display",
            TargetKind::Window => "window",
            TargetKind::Region => "region",
        }
    }

    pub fn label(self) -> String {
        match self {
            TargetKind::Display => tr_ctx!("record target", "Display"),
            TargetKind::Window => tr_ctx!("record target", "Window"),
            TargetKind::Region => tr_ctx!("record target", "Region"),
        }
    }

    pub fn parse(s: &str) -> Option<TargetKind> {
        TargetKind::ALL.into_iter().find(|k| k.key() == s)
    }
}

/// The camera part of a start request.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraChoice {
    /// `None`: the system default camera.
    pub device: Option<String>,
    pub shape: BubbleShape,
    pub size: BubbleSize,
    pub mirror: bool,
    pub position: BubblePosition,
}

impl Default for CameraChoice {
    fn default() -> Self {
        CameraChoice {
            device: None,
            shape: BubbleShape::default(),
            size: BubbleSize::default(),
            mirror: true,
            position: BubblePosition::default(),
        }
    }
}

/// Everything the start sheet decides.
#[derive(Debug, Clone, PartialEq)]
pub struct StartOptions {
    pub target: Target,
    pub camera: Option<CameraChoice>,
    pub mic: bool,
    pub mic_device: Option<String>,
    pub system_audio: bool,
}

impl StartOptions {
    /// The `start_recording` parameters. Sizes, fps, codec, quality, cursor, countdown and noise
    /// suppression are left to the settings.
    pub fn to_params(&self) -> Value {
        let mut p = Map::new();
        p.insert("target".into(), self.target.to_json());
        p.insert(
            "camera".into(),
            match &self.camera {
                None => json!(false),
                Some(c) => {
                    let mut o = Map::new();
                    if let Some(d) = &c.device {
                        o.insert("device_id".into(), json!(d));
                    }
                    o.insert("shape".into(), json!(c.shape.key()));
                    o.insert("size".into(), json!(c.size.key()));
                    o.insert("mirror".into(), json!(c.mirror));
                    o.insert("position".into(), c.position.to_json());
                    Value::Object(o)
                }
            },
        );
        p.insert(
            "mic".into(),
            if !self.mic {
                json!(false)
            } else if let Some(d) = &self.mic_device {
                json!({ "device_id": d })
            } else {
                json!(true)
            },
        );
        p.insert("system_audio".into(), json!(self.system_audio));
        Value::Object(p)
    }

    /// The settings to remember after a start (so the next sheet opens the same way). The devices
    /// are not among them: `camera.device` and `audio.mic_device` are set in Settings -> Webcam &
    /// audio, and a device picked in the sheet counts for that recording only.
    pub fn remembered(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert(KEY_LAST_TARGET.into(), json!(self.target.kind().key()));
        m.insert(KEY_CAMERA_ON.into(), json!(self.camera.is_some()));
        m.insert(KEY_MIC_ON.into(), json!(self.mic));
        m.insert(KEY_SYSTEM_AUDIO.into(), json!(self.system_audio));
        if let Some(c) = &self.camera {
            // The position this recording starts at (CAM-1): written before the start so the
            // layout at stop sees any later drag as a change. Ignored by the next start unless
            // `camera.remember_position` is on.
            m.insert(KEY_BUBBLE_POSITION.into(), c.position.to_json());
        }
        m
    }
}

fn setting<T: serde::de::DeserializeOwned>(s: &Map<String, Value>, key: &str) -> Option<T> {
    s.get(key)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// One-off choices a launcher action (LNC-1 Actions) pre-fills in the start sheet. `None` keeps the
/// remembered value. The sheet still opens: pressing Start is the user's confirmation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SheetPreset {
    pub mic: Option<bool>,
    pub system_audio: Option<bool>,
    /// `start_recording.fps`; an explicit 60 is only accepted after the 60 fps self-test.
    pub fps: Option<u32>,
    /// `start_recording.countdown_s` for this recording only.
    pub countdown_s: Option<u32>,
}

impl SheetPreset {
    /// Applies the on/off choices to the sheet's model.
    pub fn apply(&self, mic: &mut bool, system_audio: &mut bool) {
        if let Some(m) = self.mic {
            *mic = m;
        }
        if let Some(s) = self.system_audio {
            *system_audio = s;
        }
    }

    /// The `start_recording` parameters this preset adds to the sheet's own.
    pub fn extra_params(&self) -> Map<String, Value> {
        let mut m = Map::new();
        if let Some(f) = self.fps {
            m.insert("fps".into(), json!(f));
        }
        if let Some(c) = self.countdown_s {
            m.insert("countdown_s".into(), json!(c));
        }
        m
    }
}

/// The remembered start sheet choices (defaults: display, camera off, microphone on, no system
/// audio).
#[derive(Debug, Clone, PartialEq)]
pub struct SheetDefaults {
    pub target_kind: TargetKind,
    pub camera: bool,
    pub camera_device: Option<String>,
    pub mic: bool,
    pub mic_device: Option<String>,
    pub system_audio: bool,
    pub bubble: CameraChoice,
}

impl SheetDefaults {
    pub fn from_settings(s: &Map<String, Value>) -> SheetDefaults {
        let camera_device = setting::<String>(s, KEY_CAMERA_DEVICE).filter(|d| !d.is_empty());
        SheetDefaults {
            target_kind: setting::<String>(s, KEY_LAST_TARGET)
                .and_then(|k| TargetKind::parse(&k))
                .unwrap_or_default(),
            camera: setting(s, KEY_CAMERA_ON).unwrap_or(false),
            camera_device: camera_device.clone(),
            mic: setting(s, KEY_MIC_ON).unwrap_or(true),
            mic_device: setting::<String>(s, KEY_MIC_DEVICE).filter(|d| !d.is_empty()),
            system_audio: setting(s, KEY_SYSTEM_AUDIO).unwrap_or(false),
            bubble: CameraChoice {
                device: camera_device,
                shape: setting::<String>(s, KEY_BUBBLE_SHAPE)
                    .and_then(|v| BubbleShape::parse(&v))
                    .unwrap_or_default(),
                size: setting::<String>(s, KEY_BUBBLE_SIZE)
                    .and_then(|v| BubbleSize::parse(&v))
                    .unwrap_or_default(),
                mirror: setting(s, KEY_BUBBLE_MIRROR).unwrap_or(true),
                // CAM-1.AC8: the dragged position comes back only when the user asked for it.
                position: if setting::<bool>(s, KEY_BUBBLE_REMEMBER).unwrap_or(false) {
                    s.get(KEY_BUBBLE_POSITION)
                        .and_then(BubblePosition::from_json)
                        .unwrap_or_default()
                } else {
                    BubblePosition::default()
                },
            },
        }
    }

    /// The options of a start with these defaults and `target`.
    pub fn options(&self, target: Target) -> StartOptions {
        StartOptions {
            target,
            camera: self.camera.then(|| self.bubble.clone()),
            mic: self.mic,
            mic_device: self.mic_device.clone(),
            system_audio: self.system_audio,
        }
    }
}

/// The options of a start that has no sheet to ask (the ⌥⇧R hotkey), given what the OS lets this
/// app use (R4-5). A microphone or camera the user has not allowed yet is dropped instead of
/// failing the whole start with `permission_missing`: the recording starts without it and the
/// second element names what was left out, so the caller can say so. Unknown states (the first
/// check has not run) are not treated as missing; the engine then decides.
pub fn quick_defaults(
    defaults: &SheetDefaults,
    mic: crate::permissions::State,
    camera: crate::permissions::State,
) -> (SheetDefaults, Vec<&'static str>) {
    use crate::permissions::State;
    let blocked = |s: State| {
        matches!(
            s,
            State::NotGranted | State::NotDetermined | State::Denied | State::Restricted
        )
    };
    let mut out = defaults.clone();
    // No sheet checked the devices: the daemon applies the saved ones when they are connected and
    // the system default otherwise, instead of failing on a device that was unplugged.
    out.camera_device = None;
    out.mic_device = None;
    out.bubble.device = None;
    let mut dropped = Vec::new();
    if out.mic && blocked(mic) {
        out.mic = false;
        dropped.push("microphone");
    }
    if out.camera && blocked(camera) {
        out.camera = false;
        dropped.push("camera");
    }
    (out, dropped)
}

/// Picks the camera to use (CAM-4.AC2): the remembered one if it is still listed, else the system
/// default; `fell_back` says the remembered camera is gone so the sheet can note it.
pub fn resolve_camera_device(
    remembered: Option<&str>,
    available: &[String],
) -> (Option<String>, bool) {
    match remembered {
        Some(id) if available.iter().any(|a| a == id) => (Some(id.to_owned()), false),
        Some(_) => (None, true),
        None => (None, false),
    }
}

/// The engine settings pages edit (Settings -> Capture & recording and Webcam & audio).
#[derive(Debug, Clone, PartialEq)]
pub struct RecordingSettings {
    pub size: String,
    pub fps: u32,
    pub codec: String,
    pub quality: String,
    pub cursor: bool,
    pub countdown_s: u32,
    pub resume_countdown_s: u32,
    pub noise_suppression: bool,
    pub excluded_apps: Vec<String>,
}

/// The allowed values (key, English label). The English label is the source text only: a screen
/// shows [`size_label`], [`codec_label`] and [`quality_label`] of the key, which translate.
pub const SIZES: [(&str, &str); 5] = [
    ("source", "Native"), // i18n-ignore: see size_label
    ("2160p", "2160p"),
    ("1440p", "1440p"),
    ("1080p", "1080p"),
    ("720p", "720p"),
];
pub const CODECS: [(&str, &str); 3] = [("auto", "Auto"), ("hevc", "HEVC"), ("h264", "H.264")]; // i18n-ignore: see codec_label
pub const QUALITIES: [(&str, &str); 3] = [
    ("high", "High"),         // i18n-ignore: see quality_label
    ("balanced", "Balanced"), // i18n-ignore: see quality_label
    ("small", "Small"),       // i18n-ignore: see quality_label
];

/// The label of a `recording.size` key in the current language (resolutions stay as they are).
pub fn size_label(key: &str) -> String {
    match key {
        "source" => tr_ctx!("recording size", "Native"),
        other => SIZES
            .iter()
            .find(|(k, _)| *k == other)
            .map_or_else(|| other.to_owned(), |(_, l)| (*l).to_owned()),
    }
}

/// The label of a `recording.codec` key (codec names are not translated, "Auto" is).
pub fn codec_label(key: &str) -> String {
    match key {
        "auto" => tr_ctx!("recording codec", "Auto"),
        other => CODECS
            .iter()
            .find(|(k, _)| *k == other)
            .map_or_else(|| other.to_owned(), |(_, l)| (*l).to_owned()),
    }
}

/// The label of a `recording.quality` key in the current language.
pub fn quality_label(key: &str) -> String {
    match key {
        "high" => tr_ctx!("recording quality", "High"),
        "balanced" => tr_ctx!("recording quality", "Balanced"),
        "small" => tr_ctx!("recording quality", "Small"),
        other => other.to_owned(),
    }
}

impl RecordingSettings {
    /// The effective values: what the engine will use for each key.
    pub fn from_settings(s: &Map<String, Value>) -> RecordingSettings {
        let pick = |key: &str, allowed: &[(&str, &str)], default: &str| -> String {
            setting::<String>(s, key)
                .filter(|v| allowed.iter().any(|(k, _)| k == v))
                .unwrap_or_else(|| default.to_owned())
        };
        let secs = |key: &str, default: u32| {
            setting::<u32>(s, key)
                .filter(|n| *n <= 10)
                .unwrap_or(default)
        };
        RecordingSettings {
            size: pick(KEY_SIZE, &SIZES, "source"),
            fps: setting::<u32>(s, KEY_FPS)
                .filter(|n| matches!(n, 30 | 60))
                .unwrap_or(30),
            codec: pick(KEY_CODEC, &CODECS, "auto"),
            quality: pick(KEY_QUALITY, &QUALITIES, "high"),
            cursor: setting(s, KEY_CURSOR).unwrap_or(true),
            countdown_s: secs(KEY_COUNTDOWN, 3),
            resume_countdown_s: secs(KEY_RESUME_COUNTDOWN, 1),
            noise_suppression: setting(s, KEY_NOISE).unwrap_or(true),
            excluded_apps: setting::<Vec<String>>(s, KEY_EXCLUDED).unwrap_or_default(),
        }
    }

    /// The key/value pairs that differ from the defaults (defaults are not written, like the rest
    /// of `settings.json`).
    pub fn to_settings(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert(KEY_SIZE.into(), json!(self.size));
        m.insert(KEY_FPS.into(), json!(self.fps));
        m.insert(KEY_CODEC.into(), json!(self.codec));
        m.insert(KEY_QUALITY.into(), json!(self.quality));
        m.insert(KEY_CURSOR.into(), json!(self.cursor));
        m.insert(KEY_COUNTDOWN.into(), json!(self.countdown_s));
        m.insert(KEY_RESUME_COUNTDOWN.into(), json!(self.resume_countdown_s));
        m.insert(KEY_NOISE.into(), json!(self.noise_suppression));
        m.insert(KEY_EXCLUDED.into(), json!(self.excluded_apps));
        m
    }
}

/// Validates a bundle id typed into the exclusion list: reverse-DNS, no spaces.
pub fn validate_bundle_id(input: &str) -> Result<String, BundleIdError> {
    let id = input.trim();
    if id.is_empty() {
        return Err(BundleIdError::Empty);
    }
    if id.len() > 128 || id.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(BundleIdError::Spaces);
    }
    if !id.contains('.') {
        return Err(BundleIdError::NotDotted);
    }
    Ok(id.to_owned())
}

/// Why a typed bundle id was refused. Kept as a reason and worded when drawn
/// ([`BundleIdError::text`]), so a line still on screen follows a language switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleIdError {
    Empty,
    Spaces,
    NotDotted,
}

impl BundleIdError {
    pub fn text(self) -> String {
        match self {
            BundleIdError::Empty => {
                tr!("Enter an app\u{2019}s bundle id, for example com.tinyspeck.slackmacgap.")
            }
            BundleIdError::Spaces => tr!("A bundle id has no spaces."),
            BundleIdError::NotDotted => tr!("A bundle id looks like com.company.app."),
        }
    }
}

/// The default exclusions, shown read-only in Settings (REC-6.AC3: they cannot be removed).
pub fn default_exclusions() -> Vec<(&'static str, &'static str)> {
    vec![
        // i18n-ignore: names of other companies' apps
        ("1Password", "com.1password.1password"),
        ("1Password 7", "com.agilebits.onepassword7"),
        ("Bitwarden", "com.bitwarden.desktop"), // i18n-ignore
        ("KeePassXC", "org.keepassxc.keepassxc"),
        ("Dashlane", "com.dashlane.dashlanephonefinal"), // i18n-ignore
        ("Passwords", "com.apple.Passwords"),            // i18n-ignore: the macOS app
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_daemon::recording::StartParams;

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap_or_default()
    }

    fn opts(target: Target) -> StartOptions {
        StartOptions {
            target,
            camera: None,
            mic: false,
            mic_device: None,
            system_audio: false,
        }
    }

    /// R4-5: the hotkey path drops a microphone or camera the OS has not allowed, instead of
    /// failing the start, and says what it dropped.
    #[test]
    fn r4_5_quick_start_drops_blocked_devices() {
        use crate::permissions::State;
        let on = SheetDefaults::from_settings(&map(json!({"recording.camera": true})));
        assert!(on.mic, "the microphone defaults to on");
        let (d, dropped) = quick_defaults(&on, State::NotDetermined, State::Denied);
        assert!(!d.mic && !d.camera);
        assert_eq!(dropped, vec!["microphone", "camera"]);
        let (d, dropped) = quick_defaults(&on, State::Granted, State::Granted);
        assert!(d.mic && d.camera && dropped.is_empty());
        // Not looked at yet: the engine decides.
        let (d, dropped) = quick_defaults(&on, State::Unknown, State::Unknown);
        assert!(d.mic && d.camera && dropped.is_empty());
        // Off stays off and is not reported.
        let off = SheetDefaults::from_settings(&map(json!({"recording.mic": false})));
        let (d, dropped) = quick_defaults(&off, State::Denied, State::Denied);
        assert!(!d.mic && dropped.is_empty());
    }

    /// LNC-1 Actions: a preset adds only what it names, and the result is still a request the
    /// engine's own parser accepts (with `fps` and `countdown_s` as the daemon reads them).
    #[test]
    fn lnc1_preset_adds_fps_and_countdown_to_a_valid_request() {
        assert!(SheetPreset::default().extra_params().is_empty());
        let preset = SheetPreset {
            mic: Some(false),
            system_audio: Some(false),
            fps: Some(60),
            countdown_s: Some(5),
        };
        let (mut mic, mut sys) = (true, true);
        preset.apply(&mut mic, &mut sys);
        assert!(!mic && !sys);
        let (mut mic, mut sys) = (true, false);
        SheetPreset::default().apply(&mut mic, &mut sys);
        assert!(mic && !sys, "an empty preset keeps the remembered choices");
        let mut params = opts(Target::Display { display_id: 1 }).to_params();
        params
            .as_object_mut()
            .unwrap()
            .extend(preset.extra_params());
        assert_eq!(params["fps"], 60);
        assert_eq!(params["countdown_s"], 5);
        let parsed = StartParams::parse(&map(params.clone())).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed.fps, Some(60));
        assert_eq!(parsed.countdown_s, Some(5));
    }

    /// REC-1: every target shape of the sheet is accepted by the engine's own parser.
    #[test]
    fn rec1_params_are_accepted_by_the_engine() {
        for t in [
            Target::Display { display_id: 1 },
            Target::Window { window_id: 7 },
            Target::Region {
                display_id: 2,
                rect: [10, 20, 1920, 1080],
            },
        ] {
            let p = opts(t).to_params();
            let parsed = StartParams::parse(&map(p.clone())).unwrap_or_else(|e| panic!("{p}: {e}"));
            assert!(parsed.camera.is_none() && parsed.mic.is_none() && !parsed.system_audio);
        }
        let full = StartOptions {
            target: Target::Display { display_id: 1 },
            camera: Some(CameraChoice {
                device: Some("cam-1".into()),
                shape: BubbleShape::Rounded,
                size: BubbleSize::L,
                mirror: false,
                position: BubblePosition::Fraction(0.25, 0.75),
            }),
            mic: true,
            mic_device: Some("mic-9".into()),
            system_audio: true,
        };
        let parsed = StartParams::parse(&map(full.to_params())).unwrap();
        let cam = parsed.camera.unwrap();
        assert_eq!(cam.device_id.as_deref(), Some("cam-1"));
        assert_eq!(
            (cam.shape.as_str(), cam.size.as_str(), cam.mirror),
            ("rounded", "l", false)
        );
        assert_eq!(parsed.mic.unwrap().device_id.as_deref(), Some("mic-9"));
        assert!(parsed.system_audio);
    }

    /// The settings are the single source of size, fps, codec and countdown.
    #[test]
    fn params_leave_the_engine_settings_alone() {
        let p = opts(Target::Display { display_id: 1 }).to_params();
        for k in [
            "size",
            "fps",
            "codec",
            "quality",
            "cursor",
            "countdown_s",
            "countdown",
        ] {
            assert!(p.get(k).is_none(), "{k}");
        }
        assert_eq!(p["camera"], json!(false));
        assert_eq!(p["mic"], json!(false));
    }

    #[test]
    fn mic_choices() {
        let mut o = opts(Target::Display { display_id: 1 });
        o.mic = true;
        assert_eq!(o.to_params()["mic"], json!(true));
        o.mic_device = Some("m".into());
        assert_eq!(o.to_params()["mic"], json!({"device_id": "m"}));
    }

    /// CAM-1: the bubble's remembered shape, size, mirror and position feed the next start.
    #[test]
    fn defaults_round_trip_through_settings() {
        let d = SheetDefaults::from_settings(&Map::new());
        assert_eq!(d.target_kind, TargetKind::Display);
        assert!(!d.camera && d.mic && !d.system_audio);
        assert_eq!(d.bubble, CameraChoice::default());
        let o = StartOptions {
            target: Target::Region {
                display_id: 1,
                rect: [0, 0, 100, 100],
            },
            camera: Some(CameraChoice {
                device: Some("cam-2".into()),
                shape: BubbleShape::Rounded,
                size: BubbleSize::S,
                mirror: false,
                position: BubblePosition::Fraction(0.5, 0.5),
            }),
            mic: false,
            mic_device: None,
            system_audio: true,
        };
        let mut stored = o.remembered();
        // The devices are never written by a start (Settings owns them).
        assert!(!stored.contains_key(KEY_CAMERA_DEVICE) && !stored.contains_key(KEY_MIC_DEVICE));
        stored.insert(KEY_CAMERA_DEVICE.into(), json!("cam-2"));
        stored.insert(KEY_BUBBLE_SHAPE.into(), json!("rounded"));
        stored.insert(KEY_BUBBLE_SIZE.into(), json!("s"));
        stored.insert(KEY_BUBBLE_MIRROR.into(), json!(false));
        stored.insert(KEY_BUBBLE_POSITION.into(), json!([0.5, 0.5]));
        // The stored position is ignored unless the user asked to remember it (CAM-1.AC8).
        let off = SheetDefaults::from_settings(&stored);
        assert_eq!(off.bubble.position, BubblePosition::default());
        stored.insert(KEY_BUBBLE_REMEMBER.into(), json!(true));
        let back = SheetDefaults::from_settings(&stored);
        assert_eq!(back.target_kind, TargetKind::Region);
        assert!(back.camera && !back.mic && back.system_audio);
        assert_eq!(back.camera_device.as_deref(), Some("cam-2"));
        assert_eq!(back.bubble, o.camera.clone().unwrap());
        // The target is chosen again; everything else is the same request.
        assert_eq!(back.options(o.target).to_params(), o.to_params());
    }

    /// R13: the configured devices are the sheet's first choice; a hotkey start leaves them to
    /// the daemon; a device picked for one recording is not saved.
    #[test]
    fn configured_devices_are_the_initial_choice() {
        let st = map(
            json!({"camera.device": "cam-b", "audio.mic_device": "mic-b",
                            "recording.camera": true}),
        );
        let d = SheetDefaults::from_settings(&st);
        assert_eq!(d.camera_device.as_deref(), Some("cam-b"));
        assert_eq!(d.mic_device.as_deref(), Some("mic-b"));
        assert_eq!(d.bubble.device.as_deref(), Some("cam-b"));
        let p = d.options(Target::Display { display_id: 1 }).to_params();
        assert_eq!(p["camera"]["device_id"], json!("cam-b"));
        assert_eq!(p["mic"], json!({"device_id": "mic-b"}));
        // Null (the system default) and a missing key read the same.
        for none in [map(json!({"camera.device": null})), Map::new()] {
            let n = SheetDefaults::from_settings(&none);
            assert_eq!((n.camera_device, n.mic_device), (None, None));
        }
        // The hotkey path names none: the daemon resolves them.
        let (q, _) = quick_defaults(
            &d,
            crate::permissions::State::Granted,
            crate::permissions::State::Granted,
        );
        let p = q.options(Target::Display { display_id: 1 }).to_params();
        assert_eq!(p["camera"].get("device_id"), None);
        assert_eq!(p["mic"], json!(true));
        // Picking another device for one recording writes nothing about devices.
        let mut o = d.options(Target::Display { display_id: 1 });
        o.mic_device = Some("mic-a".into());
        o.camera.as_mut().unwrap().device = Some("cam-a".into());
        assert!(!o.remembered().contains_key(KEY_MIC_DEVICE));
        assert!(!o.remembered().contains_key(KEY_CAMERA_DEVICE));
    }

    #[test]
    fn unusable_stored_values_fall_back() {
        let d = SheetDefaults::from_settings(&map(json!({
            "recording.last_target": "moon", "recording.camera": "yes",
            "camera.shape": "star", "camera.size": "xl", "camera.position": [3, 3],
            "camera.device": "", "camera.remember_position": true,
        })));
        assert_eq!(d.target_kind, TargetKind::Display);
        assert!(!d.camera);
        assert_eq!(d.camera_device, None);
        assert_eq!(d.bubble, CameraChoice::default());
    }

    /// CAM-4.AC2: an unplugged remembered camera falls back to the default and says so.
    #[test]
    fn cam4_ac2_remembered_camera_fallback() {
        let avail = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(
            resolve_camera_device(Some("b"), &avail),
            (Some("b".into()), false)
        );
        assert_eq!(resolve_camera_device(Some("gone"), &avail), (None, true));
        assert_eq!(resolve_camera_device(None, &avail), (None, false));
        assert_eq!(resolve_camera_device(Some("a"), &[]), (None, true));
    }

    /// REC-2, REC-3, REC-4: the settings keys the engine reads round-trip, with the defaults the
    /// engine uses.
    #[test]
    fn settings_keys_round_trip() {
        let d = RecordingSettings::from_settings(&Map::new());
        assert_eq!(
            (d.size.as_str(), d.fps, d.codec.as_str(), d.quality.as_str()),
            ("source", 30, "auto", "high")
        );
        assert!(d.cursor && d.noise_suppression);
        assert_eq!((d.countdown_s, d.resume_countdown_s), (3, 1));
        let mut changed = d.clone();
        changed.size = "1080p".into();
        changed.fps = 60;
        changed.codec = "hevc".into();
        changed.quality = "small".into();
        changed.cursor = false;
        changed.countdown_s = 0;
        changed.resume_countdown_s = 5;
        changed.noise_suppression = false;
        changed.excluded_apps = vec!["com.tinyspeck.slackmacgap".into()];
        let stored = changed.to_settings();
        assert_eq!(RecordingSettings::from_settings(&stored), changed);
        // The engine resolves the same values from the same keys.
        let parsed =
            StartParams::parse(&map(json!({"target": {"display_id": 1}, "mic": true}))).unwrap();
        let r = parsed.resolve(&stored);
        assert_eq!((r.fps, r.countdown_s), (60, 0));
        assert!(!r.cursor && !r.mic_denoise());
        assert_eq!(serde_json::to_value(r.size).unwrap(), json!("1080p"));
    }

    #[test]
    fn out_of_range_settings_use_defaults() {
        let s = RecordingSettings::from_settings(&map(json!({
            "recording.fps": 45, "recording.size": "8k", "recording.countdown_s": 11,
            "recording.codec": "vp9", "recording.quality": "ultra",
        })));
        assert_eq!((s.fps, s.size.as_str(), s.countdown_s), (30, "source", 3));
        assert_eq!((s.codec.as_str(), s.quality.as_str()), ("auto", "high"));
    }

    /// REC-6: a typed bundle id is checked; the defaults match the engine's own list.
    #[test]
    fn rec6_exclusion_list_input() {
        assert_eq!(
            validate_bundle_id("  com.tinyspeck.slackmacgap "),
            Ok("com.tinyspeck.slackmacgap".to_owned())
        );
        assert!(validate_bundle_id("").is_err());
        assert!(validate_bundle_id("Slack").is_err());
        assert!(validate_bundle_id("com.a b").is_err());
        let ours: Vec<&str> = default_exclusions().iter().map(|(_, id)| *id).collect();
        assert_eq!(ours, cm_daemon::recording::DEFAULT_EXCLUDED_BUNDLE_IDS);
    }

    #[test]
    fn target_summaries() {
        assert_eq!(Target::Display { display_id: 2 }.summary(), "Display 2");
        assert_eq!(
            Target::Region {
                display_id: 1,
                rect: [0, 0, 1920, 1080]
            }
            .summary(),
            "Region 1920 \u{00D7} 1080"
        );
        for k in TargetKind::ALL {
            assert_eq!(TargetKind::parse(k.key()), Some(k));
        }
    }

    /// I18N P2: a target summary and the bundle id errors translate with their placeholders.
    #[test]
    fn i18n_summaries_keep_their_numbers() {
        use cm_i18n::{with_language, Lang};
        assert_eq!(
            Target::Region {
                display_id: 1,
                rect: [0, 0, 1920, 1080]
            }
            .summary(),
            "Region 1920 \u{00D7} 1080"
        );
        with_language(Lang::EnXa, || {
            let s = Target::Region {
                display_id: 1,
                rect: [0, 0, 1920, 1080],
            }
            .summary();
            assert!(s.contains("1920") && s.contains("1080"), "{s}");
            assert_ne!(s, "Region 1920 \u{00D7} 1080");
        });
        assert_eq!(size_label("source"), "Native");
        assert_eq!(size_label("1080p"), "1080p");
        assert_eq!(codec_label("hevc"), "HEVC");
        assert_eq!(quality_label("balanced"), "Balanced");
        assert_eq!(
            validate_bundle_id("Slack").unwrap_err().text(),
            "A bundle id looks like com.company.app."
        );
    }
}
