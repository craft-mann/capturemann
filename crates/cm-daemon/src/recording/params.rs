//! `start_recording` input: parsing, validation and the settings defaults (REC-1..REC-4, CAM-1,
//! AUD-1, AUD-2, REC-6).
//!
//! The action layer keeps `start_recording`'s payload as pass-through JSON (`ExternalParams`, a
//! source-compatible shape); [`StartParams::parse`] gives it a type and rejects bad input before
//! any dialog opens. [`StartParams::resolve`] fills what the caller left out from the settings
//! (`recording.size`, `recording.fps`, `recording.codec`, `recording.quality`,
//! `recording.countdown_s`, `audio.noise_suppression`, `recording.cursor`) and from the built-in
//! defaults, which gives the concrete [`ResolvedStart`] the engine records and the dialog shows.
//!
//! # JSON
//!
//! ```json
//! { "target": {"display_id": 1} | {"window_id": 7} | {"display_id": 1, "rect": [x, y, w, h]},
//!   "size": "source|2160p|1440p|1080p|720p", "fps": 30|60, "codec": "auto|hevc|h264",
//!   "quality": "high|balanced|small",
//!   "camera": false | true | "<device id>" | { "on"?, "device_id"?, "resolution"?: "720p|1080p",
//!       "mirror"?, "shape"?: "circle|rounded", "size"?: "s|m|l", "position"?: "bottom-left"|..|[x, y] },
//!   "mic": false | true | "default" | "<device id>" | { "on"?, "device_id"?, "denoise"? },
//!   "system_audio": bool, "cursor": bool, "cursor_separate": bool, "max_duration_s": number, "countdown_s": 0..=10,
//!   "title": string, "exclude": { "bundle_ids"?: [string], "window_ids"?: [int] } }
//! ```
//!
//! `rect` is `[x, y, w, h]` in **physical px relative to the display**, exactly like
//! `capture_screenshot`. `countdown` is accepted as an alias of `countdown_s` (the spec's
//! spelling). `camera.shape/size/position` are the bubble facts of CAM-1: the engine does not draw
//! the bubble, it records them in the manifest settings and hands them to the HUD in
//! `recording.started`.

use cm_camera::CameraResolution;
use cm_core::CmError;
use cm_encode::{CodecChoice, OutputSize, Quality};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::action::{Fields, ScreenshotTarget};

/// Longest `max_duration_s` accepted (24 h; REC-2.AC5 has no length limit, this only bounds typos).
pub const MAX_DURATION_S: f64 = 24.0 * 3600.0;
/// Countdown range of REC-4 (0 to 10 s).
pub const MAX_COUNTDOWN_S: u32 = 10;
/// Default countdown before recording (REC-4: 3 s).
pub const DEFAULT_COUNTDOWN_S: u32 = 3;
/// Default countdown before a resume (REC-4: 1 s).
pub const DEFAULT_RESUME_COUNTDOWN_S: u32 = 1;

const BUBBLE_SHAPES: [&str; 2] = ["circle", "rounded"];
const BUBBLE_SIZES: [&str; 3] = ["s", "m", "l"];
const BUBBLE_POSITIONS: [&str; 8] = [
    "top-left",
    "top",
    "top-right",
    "left",
    "right",
    "bottom-left",
    "bottom",
    "bottom-right",
];

/// The camera sensor and the bubble facts (CAM-1).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CameraParams {
    /// `None`: the system default camera.
    pub device_id: Option<String>,
    pub resolution: CameraResolution,
    /// Metadata only (CAM-2.AC3): the stored track is never mirrored.
    pub mirror: bool,
    /// `circle` or `rounded`.
    pub shape: String,
    /// `s`, `m` or `l`.
    pub size: String,
    /// A named anchor (`bottom-left`, the default) or `[x, y]` fractions of the recorded area.
    pub position: Value,
}

impl CameraParams {
    fn default_camera() -> Self {
        CameraParams {
            device_id: None,
            resolution: CameraResolution::P1080,
            mirror: true,
            shape: "circle".to_owned(),
            size: "m".to_owned(),
            position: json!("bottom-left"),
        }
    }

    /// The bubble block of `recording.started` and the manifest settings.
    pub fn bubble(&self) -> Value {
        json!({ "shape": self.shape, "size": self.size, "position": self.position,
                "mirror": self.mirror })
    }
}

/// The microphone sensor (AUD-1, AUD-3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MicParams {
    /// `None`: the system default input.
    pub device_id: Option<String>,
    /// `None`: the `audio.noise_suppression` setting (default on).
    pub denoise: Option<bool>,
}

/// REC-6: what the caller adds to the always-excluded windows and the default list.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ExcludeParams {
    pub bundle_ids: Vec<String>,
    pub window_ids: Vec<u32>,
}

/// A validated `start_recording` request, before the settings fill the gaps.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StartParams {
    pub target: ScreenshotTarget,
    pub size: Option<OutputSize>,
    pub fps: Option<u32>,
    pub codec: Option<CodecChoice>,
    pub quality: Option<Quality>,
    pub camera: Option<CameraParams>,
    pub mic: Option<MicParams>,
    pub system_audio: bool,
    pub cursor: Option<bool>,
    /// EDT-4: leave the pointer out of the screen picture so the editor can restyle it.
    pub cursor_separate: Option<bool>,
    pub max_duration_s: Option<f64>,
    pub countdown_s: Option<u32>,
    pub title: Option<String>,
    pub exclude: ExcludeParams,
}

/// [`StartParams`] with every default applied: what actually gets recorded.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedStart {
    pub target: ScreenshotTarget,
    pub size: OutputSize,
    pub fps: u32,
    pub codec: CodecChoice,
    pub quality: Quality,
    pub camera: Option<CameraParams>,
    /// `denoise` is always `Some` here.
    pub mic: Option<MicParams>,
    pub system_audio: bool,
    pub cursor: bool,
    /// The pointer is not in the screen picture (needs the cursor telemetry): "Record cursor
    /// separately" (EDT-4).
    pub cursor_separate: bool,
    pub max_duration_s: Option<f64>,
    pub countdown_s: u32,
    pub title: Option<String>,
    pub exclude: ExcludeParams,
}

impl ResolvedStart {
    pub fn mic_denoise(&self) -> bool {
        self.mic.as_ref().and_then(|m| m.denoise).unwrap_or(true)
    }

    /// What is stored in `manifest.settings` (and the lock): the request as used.
    pub fn settings_json(&self, requested_by: &str) -> Value {
        json!({
            "target": self.target,
            "size": self.size,
            "fps": self.fps,
            "codec": self.codec,
            "quality": self.quality,
            "camera": self.camera,
            "mic": self.mic,
            "system_audio": self.system_audio,
            "cursor": self.cursor,
            "cursor_separate": self.cursor_separate,
            "max_duration_s": self.max_duration_s,
            "exclude": self.exclude,
            "requested_by": requested_by,
        })
    }
}

fn enum_field<T: serde::de::DeserializeOwned>(
    f: &Fields<'_>,
    key: &str,
    allowed: &str,
) -> Result<Option<T>, CmError> {
    match f.obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|_| CmError::invalid_field(key, format!("must be one of {allowed}"))),
    }
}

fn str_in(v: &Value, key: &str, allowed: &[&str]) -> Result<String, CmError> {
    match v.as_str() {
        Some(s) if allowed.contains(&s) => Ok(s.to_owned()),
        _ => Err(CmError::invalid_field(
            key,
            format!("must be one of {}", allowed.join(", ")),
        )),
    }
}

fn parse_position(v: &Value) -> Result<Value, CmError> {
    let bad = || {
        CmError::invalid_field(
            "camera.position",
            format!(
                "must be one of {} or [x, y] fractions between 0 and 1",
                BUBBLE_POSITIONS.join(", ")
            ),
        )
    };
    match v {
        Value::String(s) if BUBBLE_POSITIONS.contains(&s.as_str()) => Ok(v.clone()),
        Value::Array(a) if a.len() == 2 => {
            for n in a {
                if !n.as_f64().is_some_and(|x| (0.0..=1.0).contains(&x)) {
                    return Err(bad());
                }
            }
            Ok(v.clone())
        }
        _ => Err(bad()),
    }
}

fn on_flag(o: &Map<String, Value>, key: &str) -> Result<bool, CmError> {
    match o.get("on") {
        None | Some(Value::Null) => Ok(true),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(CmError::invalid_field(
            &format!("{key}.on"),
            "must be a boolean",
        )),
    }
}

fn parse_device_string(s: &str) -> Result<Option<Option<String>>, CmError> {
    // `Some(None)`: on, default device. `Some(Some(id))`: on with a device. `None`: off.
    let t = s.trim();
    if t.is_empty() {
        return Err(CmError::invalid_input("device id must not be empty"));
    }
    Ok(match t.to_ascii_lowercase().as_str() {
        "off" | "false" | "none" => None,
        "on" | "true" | "default" => Some(None),
        _ => Some(Some(t.to_owned())),
    })
}

fn parse_camera(v: &Value) -> Result<Option<CameraParams>, CmError> {
    match v {
        Value::Null | Value::Bool(false) => Ok(None),
        Value::Bool(true) => Ok(Some(CameraParams::default_camera())),
        Value::String(s) => Ok(parse_device_string(s)
            .map_err(|e| CmError::invalid_field("camera", e))?
            .map(|id| CameraParams {
                device_id: id,
                ..CameraParams::default_camera()
            })),
        Value::Object(o) => {
            for k in o.keys() {
                if !matches!(
                    k.as_str(),
                    "on" | "device_id" | "resolution" | "mirror" | "shape" | "size" | "position"
                ) {
                    return Err(CmError::invalid_field(
                        &format!("camera.{k}"),
                        "unknown parameter",
                    ));
                }
            }
            if !on_flag(o, "camera")? {
                return Ok(None);
            }
            let mut c = CameraParams::default_camera();
            match o.get("device_id") {
                None | Some(Value::Null) => {}
                Some(Value::String(s)) if !s.trim().is_empty() => {
                    c.device_id = Some(s.trim().to_owned())
                }
                Some(_) => {
                    return Err(CmError::invalid_field(
                        "camera.device_id",
                        "must be a non-empty string",
                    ))
                }
            }
            if let Some(r) = o.get("resolution").filter(|r| !r.is_null()) {
                c.resolution = serde_json::from_value(r.clone()).map_err(|_| {
                    CmError::invalid_field("camera.resolution", "must be 720p or 1080p")
                })?;
            }
            match o.get("mirror") {
                None | Some(Value::Null) => {}
                Some(Value::Bool(b)) => c.mirror = *b,
                Some(_) => {
                    return Err(CmError::invalid_field("camera.mirror", "must be a boolean"))
                }
            }
            if let Some(s) = o.get("shape").filter(|s| !s.is_null()) {
                c.shape = str_in(s, "camera.shape", &BUBBLE_SHAPES)?;
            }
            if let Some(s) = o.get("size").filter(|s| !s.is_null()) {
                c.size = str_in(s, "camera.size", &BUBBLE_SIZES)?;
            }
            if let Some(p) = o.get("position").filter(|p| !p.is_null()) {
                c.position = parse_position(p)?;
            }
            Ok(Some(c))
        }
        _ => Err(CmError::invalid_field(
            "camera",
            "must be a boolean, a device id or an object",
        )),
    }
}

fn parse_mic(v: &Value) -> Result<Option<MicParams>, CmError> {
    match v {
        Value::Null | Value::Bool(false) => Ok(None),
        Value::Bool(true) => Ok(Some(MicParams {
            device_id: None,
            denoise: None,
        })),
        Value::String(s) => Ok(parse_device_string(s)
            .map_err(|e| CmError::invalid_field("mic", e))?
            .map(|id| MicParams {
                device_id: id,
                denoise: None,
            })),
        Value::Object(o) => {
            for k in o.keys() {
                if !matches!(k.as_str(), "on" | "device_id" | "denoise") {
                    return Err(CmError::invalid_field(
                        &format!("mic.{k}"),
                        "unknown parameter",
                    ));
                }
            }
            if !on_flag(o, "mic")? {
                return Ok(None);
            }
            let device_id = match o.get("device_id") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_owned()),
                Some(_) => {
                    return Err(CmError::invalid_field(
                        "mic.device_id",
                        "must be a non-empty string",
                    ))
                }
            };
            let denoise = match o.get("denoise") {
                None | Some(Value::Null) => None,
                Some(Value::Bool(b)) => Some(*b),
                Some(_) => return Err(CmError::invalid_field("mic.denoise", "must be a boolean")),
            };
            Ok(Some(MicParams { device_id, denoise }))
        }
        _ => Err(CmError::invalid_field(
            "mic",
            "must be a boolean, \"default\", a device id or an object",
        )),
    }
}

fn parse_exclude(v: &Value) -> Result<ExcludeParams, CmError> {
    let o = match v {
        Value::Null => return Ok(ExcludeParams::default()),
        Value::Object(o) => o,
        _ => return Err(CmError::invalid_field("exclude", "must be an object")),
    };
    for k in o.keys() {
        if !matches!(k.as_str(), "bundle_ids" | "window_ids") {
            return Err(CmError::invalid_field(
                &format!("exclude.{k}"),
                "unknown parameter",
            ));
        }
    }
    let list = |key: &str| -> Result<Vec<Value>, CmError> {
        match o.get(key) {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(a)) => Ok(a.clone()),
            Some(_) => Err(CmError::invalid_field(
                &format!("exclude.{key}"),
                "must be an array",
            )),
        }
    };
    let bundle_ids = list("bundle_ids")?
        .into_iter()
        .map(|v| match v {
            Value::String(s) if !s.trim().is_empty() => Ok(s.trim().to_owned()),
            _ => Err(CmError::invalid_field(
                "exclude.bundle_ids",
                "must be an array of non-empty strings",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let window_ids = list("window_ids")?
        .into_iter()
        .map(|v| {
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| {
                    CmError::invalid_field(
                        "exclude.window_ids",
                        "must be an array of non-negative integers",
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ExcludeParams {
        bundle_ids,
        window_ids,
    })
}

/// The `recording.countdown_s` setting (REC-4), default [`DEFAULT_COUNTDOWN_S`].
pub fn countdown_setting(settings: &Map<String, Value>) -> u32 {
    settings
        .get("recording.countdown_s")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n <= MAX_COUNTDOWN_S)
        .unwrap_or(DEFAULT_COUNTDOWN_S)
}

/// The `recording.resume_countdown_s` setting (REC-4), default [`DEFAULT_RESUME_COUNTDOWN_S`].
pub fn resume_countdown_setting(settings: &Map<String, Value>) -> u32 {
    settings
        .get("recording.resume_countdown_s")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n <= MAX_COUNTDOWN_S)
        .unwrap_or(DEFAULT_RESUME_COUNTDOWN_S)
}

impl StartParams {
    /// Validates the pass-through fields of `start_recording` (`ExternalParams::rest`).
    pub fn parse(rest: &Map<String, Value>) -> Result<StartParams, CmError> {
        let value = Value::Object(rest.clone());
        let empty = Map::new();
        let f = Fields::new(&value, &empty)?;
        f.only(&[
            "target",
            "size",
            "fps",
            "codec",
            "quality",
            "camera",
            "mic",
            "system_audio",
            "cursor",
            "cursor_separate",
            "max_duration_s",
            "countdown_s",
            "countdown",
            "title",
            "exclude",
        ])?;
        let target = ScreenshotTarget::parse(
            f.obj
                .get("target")
                .ok_or_else(|| CmError::invalid_field("target", "is required"))?,
        )?;
        let fps = match f.obj.get("fps") {
            None | Some(Value::Null) => None,
            Some(v) => match v.as_u64() {
                Some(n @ (30 | 60)) => Some(n as u32),
                _ => return Err(CmError::invalid_field("fps", "must be 30 or 60")),
            },
        };
        let max_duration_s = match f.obj.get("max_duration_s") {
            None | Some(Value::Null) => None,
            Some(v) => match v.as_f64() {
                Some(n) if n > 0.0 && n <= MAX_DURATION_S => Some(n),
                _ => {
                    return Err(CmError::invalid_field(
                        "max_duration_s",
                        format!("must be a number of seconds above 0 and up to {MAX_DURATION_S}"),
                    ))
                }
            },
        };
        let countdown = |key: &str| -> Result<Option<u32>, CmError> {
            match f.obj.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(v) => v
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| *n <= MAX_COUNTDOWN_S)
                    .map(Some)
                    .ok_or_else(|| {
                        CmError::invalid_field(
                            key,
                            format!(
                                "must be a whole number of seconds from 0 to {MAX_COUNTDOWN_S}"
                            ),
                        )
                    }),
            }
        };
        let (a, b) = (countdown("countdown_s")?, countdown("countdown")?);
        if a.is_some() && b.is_some() {
            return Err(CmError::invalid_field(
                "countdown",
                "is an alias of `countdown_s`; pass only one",
            ));
        }
        let title = f.str_opt("title")?;
        if title.as_deref().is_some_and(|t| t.trim().is_empty()) {
            return Err(CmError::invalid_field("title", "must not be empty"));
        }
        let cursor = match f.obj.get("cursor") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(b)) => Some(*b),
            Some(_) => return Err(CmError::invalid_field("cursor", "must be a boolean")),
        };
        let cursor_separate = match f.obj.get("cursor_separate") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(b)) => Some(*b),
            Some(_) => {
                return Err(CmError::invalid_field(
                    "cursor_separate",
                    "must be a boolean",
                ))
            }
        };
        Ok(StartParams {
            target,
            size: enum_field(&f, "size", "source, 2160p, 1440p, 1080p, 720p")?,
            fps,
            codec: enum_field(&f, "codec", "auto, hevc, h264")?,
            quality: enum_field(&f, "quality", "high, balanced, small")?,
            camera: parse_camera(f.obj.get("camera").unwrap_or(&Value::Null))?,
            mic: parse_mic(f.obj.get("mic").unwrap_or(&Value::Null))?,
            system_audio: f.bool_or("system_audio", false)?,
            cursor,
            cursor_separate,
            max_duration_s,
            countdown_s: a.or(b),
            title,
            exclude: parse_exclude(f.obj.get("exclude").unwrap_or(&Value::Null))?,
        })
    }

    /// Fills the gaps from `settings` (dotted keys), then from the built-in defaults.
    /// Unusable stored values are ignored rather than blocking a recording.
    pub fn resolve(&self, settings: &Map<String, Value>) -> ResolvedStart {
        fn setting<T: serde::de::DeserializeOwned>(s: &Map<String, Value>, key: &str) -> Option<T> {
            s.get(key)
                .and_then(|v| serde_json::from_value(v.clone()).ok())
        }
        let fps = self
            .fps
            .or_else(|| setting::<u32>(settings, "recording.fps").filter(|n| matches!(n, 30 | 60)))
            .unwrap_or(30);
        let countdown_s = self
            .countdown_s
            .or_else(|| {
                setting::<u32>(settings, "recording.countdown_s").filter(|n| *n <= MAX_COUNTDOWN_S)
            })
            .unwrap_or(DEFAULT_COUNTDOWN_S);
        let denoise_default = setting::<bool>(settings, "audio.noise_suppression").unwrap_or(true);
        let cursor = self
            .cursor
            .or_else(|| setting(settings, "recording.cursor"))
            .unwrap_or(true);
        // Without the telemetry the pointer would be nowhere: it stays in the picture.
        let cursor_separate = cursor
            && self
                .cursor_separate
                .or_else(|| setting(settings, "recording.cursor_separate"))
                .unwrap_or(false);
        ResolvedStart {
            target: self.target,
            size: self
                .size
                .or_else(|| setting(settings, "recording.size"))
                .unwrap_or_default(),
            fps,
            codec: self
                .codec
                .or_else(|| setting(settings, "recording.codec"))
                .unwrap_or_default(),
            quality: self
                .quality
                .or_else(|| setting(settings, "recording.quality"))
                .unwrap_or_default(),
            camera: self.camera.clone(),
            mic: self.mic.clone().map(|m| MicParams {
                denoise: Some(m.denoise.unwrap_or(denoise_default)),
                ..m
            }),
            system_audio: self.system_audio,
            cursor,
            cursor_separate,
            max_duration_s: self.max_duration_s,
            countdown_s,
            title: self.title.clone(),
            exclude: self.exclude.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rest(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn minimal_request_resolves_to_the_spec_defaults() {
        let p = StartParams::parse(&rest(json!({"target": {"display_id": 1}}))).unwrap();
        let r = p.resolve(&Map::new());
        assert_eq!((r.fps, r.countdown_s), (30, 3));
        assert_eq!(r.size, OutputSize::Source);
        assert!(r.camera.is_none() && r.mic.is_none() && !r.system_audio && r.cursor);
    }

    #[test]
    fn settings_fill_the_gaps_and_the_request_wins() {
        let settings = rest(json!({"recording.fps": 60, "recording.size": "1080p",
                                   "recording.codec": "hevc", "audio.noise_suppression": false,
                                   "recording.countdown_s": 0}));
        let p = StartParams::parse(&rest(
            json!({"target": {"display_id": 1}, "mic": true, "size": "720p"}),
        ))
        .unwrap();
        let r = p.resolve(&settings);
        assert_eq!(
            (r.fps, r.size, r.codec, r.countdown_s),
            (60, OutputSize::P720, CodecChoice::Hevc, 0)
        );
        assert!(!r.mic_denoise());
        // A stored value that makes no sense never blocks a recording.
        let r = p.resolve(&rest(json!({"recording.fps": 45})));
        assert_eq!(r.fps, 30);
    }

    /// CAM-1: without a `position` the bubble starts bottom-left; an explicit one is kept.
    #[test]
    fn camera_position_defaults_to_bottom_left() {
        for cam in [json!(true), json!({"size": "s"}), json!({"position": null})] {
            let p = StartParams::parse(&rest(json!({"target": {"display_id": 1}, "camera": cam})))
                .unwrap();
            assert_eq!(p.camera.unwrap().position, json!("bottom-left"));
        }
        let p = StartParams::parse(&rest(
            json!({"target": {"display_id": 1}, "camera": {"position": "bottom-right"}}),
        ))
        .unwrap();
        assert_eq!(p.camera.unwrap().position, json!("bottom-right"));
    }

    #[test]
    fn camera_and_mic_accept_every_documented_shape() {
        let p = StartParams::parse(&rest(json!({
            "target": {"display_id": 1},
            "camera": {"device_id": "cam-1", "resolution": "720p", "shape": "rounded",
                       "size": "l", "position": [0.1, 0.9]},
            "mic": {"device_id": "m1", "denoise": false},
            "system_audio": true,
        })))
        .unwrap();
        let c = p.camera.unwrap();
        assert_eq!(c.device_id.as_deref(), Some("cam-1"));
        assert_eq!((c.shape.as_str(), c.size.as_str()), ("rounded", "l"));
        assert_eq!(p.mic.unwrap().denoise, Some(false));
        for (v, camera, mic) in [
            (json!(true), true, true),
            (json!("default"), true, true),
            (json!("off"), false, false),
            (json!(false), false, false),
            (json!({"on": false}), false, false),
        ] {
            let p = StartParams::parse(&rest(
                json!({"target": {"display_id": 1}, "camera": v, "mic": v}),
            ))
            .unwrap();
            assert_eq!((p.camera.is_some(), p.mic.is_some()), (camera, mic), "{v}");
        }
    }

    #[test]
    fn bad_input_names_the_field() {
        for (p, field) in [
            (json!({}), "target"),
            (json!({"target": {"display_id": 1}, "fps": 45}), "fps"),
            (json!({"target": {"display_id": 1}, "size": "8k"}), "size"),
            (
                json!({"target": {"display_id": 1}, "codec": "vp9"}),
                "codec",
            ),
            (
                json!({"target": {"display_id": 1}, "countdown_s": 11}),
                "countdown_s",
            ),
            (
                json!({"target": {"display_id": 1}, "countdown_s": 1, "countdown": 1}),
                "countdown",
            ),
            (
                json!({"target": {"display_id": 1}, "max_duration_s": 0}),
                "max_duration_s",
            ),
            (
                json!({"target": {"display_id": 1}, "camera": {"shape": "star"}}),
                "camera.shape",
            ),
            (
                json!({"target": {"display_id": 1}, "camera": {"position": "middle"}}),
                "camera.position",
            ),
            (
                json!({"target": {"display_id": 1}, "camera": {"zoom": 2}}),
                "camera.zoom",
            ),
            (json!({"target": {"display_id": 1}, "mic": 5}), "mic"),
            (
                json!({"target": {"display_id": 1}, "exclude": {"pids": [1]}}),
                "exclude.pids",
            ),
            (json!({"target": {"display_id": 1}, "bogus": true}), "bogus"),
            (json!({"target": {"display_id": 1}, "title": " "}), "title"),
        ] {
            let e = StartParams::parse(&rest(p.clone())).unwrap_err();
            assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput, "{p}");
            assert!(e.to_string().starts_with(field), "{p}: {e}");
        }
    }

    /// EDT-4: "Record cursor separately" is off by default, can come from the request or the
    /// `recording.cursor_separate` setting, and needs the cursor telemetry.
    #[test]
    fn edt4_cursor_separate_resolves_from_the_request_the_setting_and_the_telemetry() {
        let base = json!({"target": {"display_id": 1}});
        let parse = |extra: Value| {
            let mut v = base.clone();
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            StartParams::parse(&rest(v)).unwrap()
        };
        let none = Map::new();
        assert!(
            !parse(json!({})).resolve(&none).cursor_separate,
            "off by default"
        );
        assert!(
            parse(json!({"cursor_separate": true}))
                .resolve(&none)
                .cursor_separate
        );
        let set = rest(json!({"recording.cursor_separate": true}));
        assert!(
            parse(json!({})).resolve(&set).cursor_separate,
            "from the setting"
        );
        assert!(
            !parse(json!({"cursor_separate": false}))
                .resolve(&set)
                .cursor_separate,
            "the request wins over the setting"
        );
        // No telemetry, no separate cursor: the pointer stays in the picture.
        assert!(
            !parse(json!({"cursor_separate": true, "cursor": false}))
                .resolve(&none)
                .cursor_separate
        );
        let r = parse(json!({"cursor_separate": true})).resolve(&none);
        assert_eq!(r.settings_json("user")["cursor_separate"], true);
        let bad = StartParams::parse(&rest(
            json!({"target": {"display_id": 1}, "cursor_separate": "yes"}),
        ))
        .unwrap_err();
        assert!(bad.to_string().starts_with("cursor_separate"), "{bad}");
    }
}
