//! Recording edits (EDT-1 to EDT-5, AUD-4, CAM-3): `trim`, `cut_segments`, `restore_segments`,
//! `split`, `set_speed`, `set_background`, `set_audio_mix`, `set_camera_layout`, `set_zooms`,
//! `set_cursor_style` and `set_reframe`.
//!
//! All eleven are Free tier. Each changes only `manifest.edits[0]` (the masters are never opened for
//! writing), is one revision tagged with the caller (`user` or `agent:<name>`) and one undo step
//! (`undo` / `redo` on a recording walk them). They run under the capture's lock, so they do not
//! overlap an export of the same recording, and are refused with `busy` while it is being
//! recorded or exported. Times are seconds (a number) or `"t:<ticks>"` in **session time**.

use cm_core::{
    edit::{parse_time, secs},
    Actor, Aspect, AudioMixUpdate, BackgroundKind, BackgroundUpdate, BubbleKind, CameraLayoutSpec,
    CameraMode, ClickEffect, CmError, CursorStyleUpdate, EditContext, EditList, LayoutTransition,
    MediaTime, ReframeMode, ReframeUpdate, Smoothing, Span, ZoomRegion, ZOOM_DEFAULT_SCALE,
};
use cm_library::manifest::CaptureKind;
use cm_library::rec_edit::{apply_edit, step_history, EditOutcome};
use serde::{Serialize, Serializer};
use serde_json::{json, Value};

use crate::action::{Action, Fields};
use crate::daemon::{blocking, lock_library, Inner};
use crate::error::DaemonError;
use crate::events;

/// One edit of a recording, validated.
#[derive(Debug, Clone, PartialEq)]
pub enum EditOp {
    Trim {
        start: MediaTime,
        end: MediaTime,
    },
    Cut {
        ranges: Vec<Span>,
    },
    Restore {
        ranges: Vec<Span>,
    },
    Split {
        at: MediaTime,
    },
    Speed {
        range: Span,
        factor: f64,
        keep_audio: bool,
    },
    Background(BackgroundUpdate),
    Audio(AudioMixUpdate),
    CameraLayout {
        range: Span,
        spec: CameraLayoutSpec,
    },
    /// EDT-4: add zoom regions (or replace them all).
    Zooms {
        zooms: Vec<ZoomRegion>,
        replace: bool,
    },
    /// EDT-4: size, smoothing, click effect and hide-when-idle of the telemetry cursor.
    Cursor(CursorStyleUpdate),
    /// EDT-5: one aspect's reframe mode and/or the window focus of one segment.
    Reframe(ReframeUpdate),
}

/// The edit method names, as actions, MCP tools and CLI subcommands (`edit <name>`).
pub const EDIT_METHODS: &[&str] = &[
    "trim",
    "cut_segments",
    "restore_segments",
    "split",
    "set_speed",
    "set_background",
    "set_audio_mix",
    "set_camera_layout",
    "set_zooms",
    "set_cursor_style",
    "set_reframe",
];

impl EditOp {
    pub fn method(&self) -> &'static str {
        match self {
            EditOp::Trim { .. } => "trim",
            EditOp::Cut { .. } => "cut_segments",
            EditOp::Restore { .. } => "restore_segments",
            EditOp::Split { .. } => "split",
            EditOp::Speed { .. } => "set_speed",
            EditOp::Background(_) => "set_background",
            EditOp::Audio(_) => "set_audio_mix",
            EditOp::CameraLayout { .. } => "set_camera_layout",
            EditOp::Zooms { .. } => "set_zooms",
            EditOp::Cursor(_) => "set_cursor_style",
            EditOp::Reframe(_) => "set_reframe",
        }
    }

    /// The input as the activity log shows it (ticks).
    pub fn to_json(&self) -> Value {
        let ranges = |r: &[Span]| -> Value {
            r.iter()
                .map(|s| json!([s.from.ticks(), s.to.ticks()]))
                .collect()
        };
        match self {
            EditOp::Trim { start, end } => json!({"start": start.ticks(), "end": end.ticks()}),
            EditOp::Cut { ranges: r } | EditOp::Restore { ranges: r } => {
                json!({ "ranges": ranges(r) })
            }
            EditOp::Split { at } => json!({"at": at.ticks()}),
            EditOp::Speed {
                range,
                factor,
                keep_audio,
            } => json!({
                "range": [range.from.ticks(), range.to.ticks()],
                "factor": factor,
                "keep_audio": keep_audio,
            }),
            EditOp::Background(u) => json!({
                "kind": u.kind.as_str(),
                "value": u.value,
                "padding_pct": u.padding_pct,
                "radius_px": u.radius_px,
                "shadow": u.shadow,
            }),
            EditOp::Audio(u) => {
                let mut o = serde_json::Map::new();
                for (k, v) in [
                    ("mic_gain_db", u.mic_gain_db.map(|v| json!(v))),
                    ("system_gain_db", u.system_gain_db.map(|v| json!(v))),
                    ("mic_mute", u.mic_mute.map(|v| json!(v))),
                    ("system_mute", u.system_mute.map(|v| json!(v))),
                    ("denoise", u.denoise.map(|v| json!(v))),
                ] {
                    if let Some(v) = v {
                        o.insert(k.into(), v);
                    }
                }
                Value::Object(o)
            }
            EditOp::CameraLayout { range, spec } => json!({
                "range": [range.from.ticks(), range.to.ticks()],
                "layout": spec.mode.as_str(),
                "shape": spec.shape.map(BubbleKind::as_str),
                "rect": spec.rect,
                "ring": spec.ring,
                "mirror": spec.mirror,
                "transition": spec.transition.as_str(),
            }),
            EditOp::Zooms { zooms, replace } => json!({
                "zooms": zooms.iter().map(|z| json!({
                    "at": z.at.ticks(),
                    "length": z.length.ticks(),
                    "focus": [z.focus.0, z.focus.1],
                    "scale": z.scale,
                    "follow_cursor": z.follow_cursor,
                })).collect::<Vec<_>>(),
                "replace": replace,
            }),
            EditOp::Cursor(u) => {
                let mut o = serde_json::Map::new();
                if let Some(v) = u.size {
                    o.insert("size".into(), json!(v));
                }
                if let Some(v) = u.smoothing {
                    o.insert("smoothing".into(), json!(v.as_str()));
                }
                if let Some(v) = u.click_effect {
                    o.insert("click_effect".into(), json!(v.as_str()));
                }
                if let Some(v) = u.hide_idle {
                    o.insert("hide_idle".into(), json!(v));
                }
                Value::Object(o)
            }
            EditOp::Reframe(u) => json!({
                "aspect": u.aspect.as_str(),
                "mode": u.mode.map(ReframeMode::as_str),
                "focus": u.focus.map(|f| json!([f.0, f.1])),
                "at": u.at.map(MediaTime::ticks),
            }),
        }
    }

    fn apply(&self, l: &EditList, cx: &EditContext<'_>) -> Result<EditList, CmError> {
        match self {
            EditOp::Trim { start, end } => l.trim(*start, *end, cx),
            EditOp::Cut { ranges } => l.cut(ranges),
            EditOp::Restore { ranges } => l.restore(ranges),
            EditOp::Split { at } => l.split(*at),
            EditOp::Speed {
                range,
                factor,
                keep_audio,
            } => l.set_speed(*range, *factor, *keep_audio),
            EditOp::Background(u) => l.set_background(u),
            EditOp::Audio(u) => l.set_audio_mix(u),
            EditOp::CameraLayout { range, spec } => l.set_camera_layout(*range, spec),
            EditOp::Zooms { zooms, replace } => l.set_zooms(zooms, *replace),
            EditOp::Cursor(u) => l.set_cursor_style(u),
            EditOp::Reframe(u) => l.set_reframe(u),
        }
    }
}

/// `trim`, `cut_segments`, ...: the capture, the edit and the idempotency key.
#[derive(Debug, Clone, PartialEq)]
pub struct EditParams {
    pub capture_id: String,
    pub op: EditOp,
    pub request_id: Option<String>,
}

impl Serialize for EditParams {
    /// The flat input object (`capture_id`, the edit's fields in ticks, `request_id`).
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut o = match self.op.to_json() {
            Value::Object(o) => o,
            _ => Default::default(),
        };
        o.insert("capture_id".into(), json!(self.capture_id));
        o.insert("request_id".into(), json!(self.request_id));
        Value::Object(o).serialize(s)
    }
}

fn time(f: &Fields<'_>, key: &str) -> Result<MediaTime, CmError> {
    parse_time(
        key,
        f.obj
            .get(key)
            .ok_or_else(|| CmError::invalid_field(key, "is required"))?,
    )
}

fn pair(field: &str, v: &Value) -> Result<Span, CmError> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or_else(|| CmError::invalid_field(field, "must be [start, end]"))?;
    let span = Span::new(parse_time(field, &a[0])?, parse_time(field, &a[1])?);
    if span.is_empty() {
        return Err(CmError::invalid_field(field, "must end after it starts"));
    }
    Ok(span)
}

fn ranges(f: &Fields<'_>) -> Result<Vec<Span>, CmError> {
    let list = f
        .obj
        .get("ranges")
        .ok_or_else(|| CmError::invalid_field("ranges", "is required"))?
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| {
            CmError::invalid_field("ranges", "must be a non-empty array of [start, end]")
        })?;
    list.iter()
        .enumerate()
        .map(|(i, r)| pair(&format!("ranges[{i}]"), r))
        .collect()
}

fn point_of(field: &str, v: &Value) -> Result<(f64, f64), CmError> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 2)
        .and_then(|a| Some((a[0].as_f64()?, a[1].as_f64()?)))
        .ok_or_else(|| {
            CmError::invalid_field(field, "must be [x, y], fractions of the frame from 0 to 1")
        })?;
    Ok(a)
}

/// One zoom region of `set_zooms`.
fn zoom_of(field: &str, v: &Value) -> Result<ZoomRegion, CmError> {
    let o = v
        .as_object()
        .ok_or_else(|| CmError::invalid_field(field, "must be an object"))?;
    if let Some(k) = o
        .keys()
        .find(|k| !["at", "length", "focus", "scale", "follow_cursor"].contains(&k.as_str()))
    {
        return Err(CmError::invalid_field(
            &format!("{field}.{k}"),
            "is not a zoom field",
        ));
    }
    let need = |k: &str| {
        o.get(k)
            .ok_or_else(|| CmError::invalid_field(&format!("{field}.{k}"), "is required"))
    };
    let z = ZoomRegion {
        at: parse_time(&format!("{field}.at"), need("at")?)?,
        length: parse_time(&format!("{field}.length"), need("length")?)?,
        focus: point_of(&format!("{field}.focus"), need("focus")?)?,
        scale: match o.get("scale") {
            None | Some(Value::Null) => ZOOM_DEFAULT_SCALE,
            Some(s) => s.as_f64().ok_or_else(|| {
                CmError::invalid_field(&format!("{field}.scale"), "must be a number")
            })?,
        },
        follow_cursor: match o.get("follow_cursor") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => {
                return Err(CmError::invalid_field(
                    &format!("{field}.follow_cursor"),
                    "must be a boolean",
                ))
            }
        },
    };
    z.validate(field)?;
    Ok(z)
}

/// Validates the input of one edit method.
pub(crate) fn parse(method: &str, f: &Fields<'_>) -> Result<Action, CmError> {
    let op = match method {
        "trim" => {
            f.only(&["capture_id", "start", "end", "request_id"])?;
            let (start, end) = (time(f, "start")?, time(f, "end")?);
            if end <= start {
                return Err(CmError::invalid_field("end", "must be after start"));
            }
            EditOp::Trim { start, end }
        }
        "cut_segments" | "restore_segments" => {
            f.only(&["capture_id", "ranges", "request_id"])?;
            let ranges = ranges(f)?;
            if method == "cut_segments" {
                EditOp::Cut { ranges }
            } else {
                EditOp::Restore { ranges }
            }
        }
        "split" => {
            f.only(&["capture_id", "at", "request_id"])?;
            EditOp::Split { at: time(f, "at")? }
        }
        "set_speed" => {
            f.only(&["capture_id", "range", "factor", "keep_audio", "request_id"])?;
            let range = pair(
                "range",
                f.obj
                    .get("range")
                    .ok_or_else(|| CmError::invalid_field("range", "is required"))?,
            )?;
            let factor = f
                .obj
                .get("factor")
                .and_then(Value::as_f64)
                .ok_or_else(|| CmError::invalid_field("factor", "must be a number"))?;
            EditOp::Speed {
                range,
                factor,
                keep_audio: f.bool_or("keep_audio", false)?,
            }
        }
        "set_background" => {
            f.only(&[
                "capture_id",
                "kind",
                "value",
                "padding_pct",
                "radius_px",
                "shadow",
                "request_id",
            ])?;
            let kind = BackgroundKind::parse(&f.str_req("kind")?)?;
            let padding_pct = match f.obj.get("padding_pct") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_f64()
                        .ok_or_else(|| CmError::invalid_field("padding_pct", "must be a number"))?,
                ),
            };
            let radius_px = match f.obj.get("radius_px") {
                None | Some(Value::Null) => None,
                Some(v) => Some(v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(
                    || CmError::invalid_field("radius_px", "must be a non-negative integer"),
                )?),
            };
            let shadow = match f.obj.get("shadow") {
                None | Some(Value::Null) => None,
                Some(Value::Bool(b)) => Some(*b),
                Some(_) => return Err(CmError::invalid_field("shadow", "must be a boolean")),
            };
            let value = f.obj.get("value").filter(|v| !v.is_null()).cloned();
            if matches!(
                kind,
                BackgroundKind::Color | BackgroundKind::Gradient | BackgroundKind::Image
            ) && value.is_none()
            {
                return Err(CmError::invalid_field(
                    "value",
                    format!("is required for `{}`", kind.as_str()),
                ));
            }
            EditOp::Background(BackgroundUpdate {
                kind,
                value,
                padding_pct,
                radius_px,
                shadow,
            })
        }
        "set_audio_mix" => {
            f.only(&[
                "capture_id",
                "mic_gain_db",
                "system_gain_db",
                "mic_mute",
                "system_mute",
                "denoise",
                "request_id",
            ])?;
            let gain = |key: &str| -> Result<Option<f64>, CmError> {
                match f.obj.get(key) {
                    None | Some(Value::Null) => Ok(None),
                    Some(v) => v
                        .as_f64()
                        .map(Some)
                        .ok_or_else(|| CmError::invalid_field(key, "must be a number (dB)")),
                }
            };
            let flag = |key: &str| -> Result<Option<bool>, CmError> {
                match f.obj.get(key) {
                    None | Some(Value::Null) => Ok(None),
                    Some(Value::Bool(b)) => Ok(Some(*b)),
                    Some(_) => Err(CmError::invalid_field(key, "must be a boolean")),
                }
            };
            let u = AudioMixUpdate {
                mic_gain_db: gain("mic_gain_db")?,
                system_gain_db: gain("system_gain_db")?,
                mic_mute: flag("mic_mute")?,
                system_mute: flag("system_mute")?,
                denoise: flag("denoise")?,
            };
            if u.is_empty() {
                return Err(CmError::invalid_input(
                    "give at least one of mic_gain_db, system_gain_db, mic_mute, system_mute, denoise",
                ));
            }
            EditOp::Audio(u)
        }
        "set_camera_layout" => {
            f.only(&[
                "capture_id",
                "range",
                "layout",
                "shape",
                "rect",
                "ring",
                "mirror",
                "transition",
                "request_id",
            ])?;
            let range = pair(
                "range",
                f.obj
                    .get("range")
                    .ok_or_else(|| CmError::invalid_field("range", "is required"))?,
            )?;
            let mode = CameraMode::parse(&f.str_req("layout")?)?;
            let shape = match f.obj.get("shape") {
                None | Some(Value::Null) => None,
                Some(v) => Some(BubbleKind::parse(v.as_str().ok_or_else(|| {
                    CmError::invalid_field("shape", "must be circle or rounded")
                })?)?),
            };
            let rect = match f.obj.get("rect") {
                None | Some(Value::Null) => None,
                Some(v) => {
                    let a = v.as_array().filter(|a| a.len() == 4).ok_or_else(|| {
                        CmError::invalid_field("rect", "must be [x, y, w, h] as fractions")
                    })?;
                    let mut r = [0.0; 4];
                    for (slot, x) in r.iter_mut().zip(a) {
                        *slot = x.as_f64().ok_or_else(|| {
                            CmError::invalid_field("rect", "must be four numbers")
                        })?;
                    }
                    Some(r)
                }
            };
            let ring = match f.obj.get("ring") {
                None | Some(Value::Null) => None,
                Some(Value::Bool(false)) => Some(0.0),
                Some(Value::Bool(true)) => Some(cm_core::camera_layout::DEFAULT_RING),
                Some(v) => Some(v.as_f64().ok_or_else(|| {
                    CmError::invalid_field("ring", "must be a number, true or false")
                })?),
            };
            let mirror = match f.obj.get("mirror") {
                None | Some(Value::Null) => None,
                Some(Value::Bool(b)) => Some(*b),
                Some(_) => return Err(CmError::invalid_field("mirror", "must be a boolean")),
            };
            let transition = match f.obj.get("transition") {
                None | Some(Value::Null) => LayoutTransition::Cut,
                Some(v) => LayoutTransition::parse(v.as_str().ok_or_else(|| {
                    CmError::invalid_field("transition", "must be cut or smooth")
                })?)?,
            };
            let spec = CameraLayoutSpec {
                mode,
                shape,
                rect,
                ring,
                mirror,
                transition,
            };
            spec.validate()?;
            EditOp::CameraLayout { range, spec }
        }
        "set_zooms" => {
            f.only(&["capture_id", "zooms", "replace", "request_id"])?;
            let list = f
                .obj
                .get("zooms")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    CmError::invalid_field("zooms", "must be an array of zoom regions")
                })?;
            let replace = f.bool_or("replace", false)?;
            if list.is_empty() && !replace {
                return Err(CmError::invalid_field(
                    "zooms",
                    "must not be empty (give `replace: true` to clear every zoom)",
                ));
            }
            let zooms = list
                .iter()
                .enumerate()
                .map(|(i, z)| zoom_of(&format!("zooms[{i}]"), z))
                .collect::<Result<Vec<_>, _>>()?;
            EditOp::Zooms { zooms, replace }
        }
        "set_cursor_style" => {
            f.only(&[
                "capture_id",
                "size",
                "smoothing",
                "click_effect",
                "hide_idle",
                "request_id",
            ])?;
            let present = |key: &str| f.obj.get(key).filter(|v| !v.is_null());
            let size = match present("size") {
                None => None,
                Some(v) => Some(
                    v.as_f64()
                        .ok_or_else(|| CmError::invalid_field("size", "must be a number"))?,
                ),
            };
            let word = |key: &str| -> Result<Option<String>, CmError> {
                match present(key) {
                    None => Ok(None),
                    Some(v) => v
                        .as_str()
                        .map(|s| Some(s.to_owned()))
                        .ok_or_else(|| CmError::invalid_field(key, "must be a string")),
                }
            };
            let hide_idle = match present("hide_idle") {
                None => None,
                Some(Value::Bool(b)) => Some(*b),
                Some(_) => return Err(CmError::invalid_field("hide_idle", "must be a boolean")),
            };
            let u = CursorStyleUpdate {
                size,
                smoothing: word("smoothing")?
                    .map(|s| Smoothing::parse(&s))
                    .transpose()?,
                click_effect: word("click_effect")?
                    .map(|s| ClickEffect::parse(&s))
                    .transpose()?,
                hide_idle,
            };
            if u.is_empty() {
                return Err(CmError::invalid_input(
                    "give at least one of size, smoothing, click_effect, hide_idle",
                ));
            }
            EditOp::Cursor(u)
        }
        "set_reframe" => {
            f.only(&["capture_id", "aspect", "mode", "focus", "at", "request_id"])?;
            let aspect = Aspect::parse(&f.str_req("aspect")?)?;
            let mode = match f.obj.get("mode").filter(|v| !v.is_null()) {
                None => None,
                Some(v) => Some(ReframeMode::parse(v.as_str().ok_or_else(|| {
                    CmError::invalid_field("mode", "must be fit, fixed or follow")
                })?)?),
            };
            let focus = match f.obj.get("focus").filter(|v| !v.is_null()) {
                None => None,
                Some(v) => Some(point_of("focus", v)?),
            };
            let at = match f.obj.get("at").filter(|v| !v.is_null()) {
                None => None,
                Some(v) => Some(parse_time("at", v)?),
            };
            if mode.is_none() && focus.is_none() {
                return Err(CmError::invalid_input("give at least one of mode, focus"));
            }
            EditOp::Reframe(ReframeUpdate {
                aspect,
                mode,
                focus,
                at,
            })
        }
        other => {
            return Err(CmError::invalid_input(format!("`{other}` is not an edit")));
        }
    };
    Ok(Action::Edit(EditParams {
        capture_id: f.str_req("capture_id")?,
        op,
        request_id: f.request_id()?,
    }))
}

/// One sentence for the activity log.
pub(crate) fn summary(p: &EditParams) -> String {
    let id = &p.capture_id;
    match &p.op {
        EditOp::Trim { start, end } => {
            format!("Trim {id} to {:.2}-{:.2} s", secs(*start), secs(*end))
        }
        EditOp::Cut { ranges } => format!("Cut {} range(s) from {id}", ranges.len()),
        EditOp::Restore { ranges } => format!("Restore {} range(s) of {id}", ranges.len()),
        EditOp::Split { at } => format!("Split {id} at {:.2} s", secs(*at)),
        EditOp::Speed { factor, .. } => format!("Set {factor}x speed on part of {id}"),
        EditOp::Background(u) => format!("Set the {} background of {id}", u.kind.as_str()),
        EditOp::Audio(_) => format!("Change the audio mix of {id}"),
        EditOp::CameraLayout { spec, .. } => {
            format!(
                "Set the {} camera layout on part of {id}",
                spec.mode.as_str()
            )
        }
        EditOp::Zooms { zooms, replace } => {
            if *replace && zooms.is_empty() {
                format!("Clear the zooms of {id}")
            } else {
                format!("Add {} zoom region(s) to {id}", zooms.len())
            }
        }
        EditOp::Cursor(_) => format!("Change the cursor style of {id}"),
        EditOp::Reframe(u) => format!("Reframe {id} for {}", u.aspect.as_str()),
    }
}

/// The manifest lists a camera track that is on disk.
fn has_camera_track(m: &cm_library::manifest::RecManifest) -> bool {
    m.tracks
        .get("camera")
        .is_some_and(|t| t.get("missing") != Some(&json!(true)))
}

/// The result of an edit, undo or redo of a recording.
fn result_json(capture_id: &str, o: &EditOutcome) -> Value {
    json!({
        "capture_id": capture_id,
        "revision": o.revision.rev,
        "action": o.revision.action,
        "by": o.revision.by,
        "changed": true,
        "duration": o.duration.ticks(),
        "duration_s": secs(o.duration),
        "edits": o.edits.to_json(),
        "can_undo": o.can_undo,
        "can_redo": o.can_redo,
    })
}

/// An image background must exist and decode (PNG or JPEG), checked the way the export finds it:
/// an absolute path, or one relative to the recording's bundle folder.
fn check_background_image(bundle: &std::path::Path, value: Option<&Value>) -> Result<(), CmError> {
    let name = value
        .and_then(Value::as_str)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| CmError::invalid_field("value", "an image background needs a file path"))?;
    let given = std::path::PathBuf::from(name);
    let path = if given.is_absolute() {
        given
    } else {
        bundle.join(given)
    };
    if !path.is_file() {
        return Err(CmError::invalid_field(
            "value",
            format!("the background image `{}` does not exist", path.display()),
        ));
    }
    image::ImageReader::open(&path)
        .and_then(|r| r.with_guessed_format())
        .map_err(|e| e.to_string())
        .and_then(|r| r.decode().map(|_| ()).map_err(|e| e.to_string()))
        .map_err(|e| {
            CmError::invalid_field(
                "value",
                format!(
                    "the background image `{}` cannot be read (PNG or JPEG): {e}",
                    path.display()
                ),
            )
        })
}

impl Inner {
    /// One recording edit, under the capture lock.
    pub(crate) async fn edit_capture(
        &self,
        actor: &Actor,
        p: EditParams,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&p.capture_id).await;
        let (library, this, now, by) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
        );
        let (id, op) = (p.capture_id.clone(), p.op.clone());
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if rec.kind != CaptureKind::Recording {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot: {} edits recordings (use `annotate` for screenshots)",
                    op.method()
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "edit")?;
            if let EditOp::CameraLayout { spec, .. } = &op {
                // CAM-3 edge case: a recording without a camera track has no camera layouts.
                if spec.mode.shows_camera() && !has_camera_track(&cm_library::read_manifest(&dir)?)
                {
                    return Err(CmError::invalid_input(format!(
                        "`{id}` has no camera track: only the `screen` layout applies"
                    )));
                }
            }
            if let EditOp::Background(u) = &op {
                // Report a missing or unreadable image now, not as a failed export later.
                if u.kind == BackgroundKind::Image {
                    check_background_image(&dir, u.value.as_ref())?;
                }
            }
            let out = apply_edit(&dir, op.method(), by, now, |l, cx| op.apply(l, cx))?;
            lib.index_dir(&dir)?;
            Ok(out)
        })
        .await?;
        self.events.publish(
            events::CAPTURE_REVISED,
            json!({
                "capture_id": p.capture_id,
                "revision": out.revision.rev,
                "action": out.revision.action,
                "by": out.revision.by,
            }),
        );
        Ok(result_json(&p.capture_id, &out))
    }

    /// `undo` / `redo` of a recording's edit history.
    pub(crate) async fn undo_redo_recording(
        &self,
        actor: &Actor,
        capture_id: &str,
        undo: bool,
    ) -> Result<Value, DaemonError> {
        let (library, this, now, by) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
        );
        let id = capture_id.to_owned();
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "edit")?;
            let out = step_history(&dir, undo, by, now)?;
            if out.is_some() {
                lib.index_dir(&dir)?;
            }
            let m = cm_library::read_manifest(&dir)?;
            Ok((out, cm_library::rec_edit::history_flags(&m)))
        })
        .await?;
        let (out, (can_undo, can_redo)) = out;
        let Some(o) = out else {
            // Nothing to undo/redo is not an error, like for screenshots.
            return Ok(json!({
                "capture_id": capture_id,
                "revision": null,
                "changed": false,
                "can_undo": can_undo,
                "can_redo": can_redo,
            }));
        };
        self.events.publish(
            events::CAPTURE_REVISED,
            json!({
                "capture_id": capture_id,
                "revision": o.revision.rev,
                "action": o.revision.action,
                "by": o.revision.by,
            }),
        );
        Ok(result_json(capture_id, &o))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse_ok(method: &str, params: Value) -> EditParams {
        match Action::parse(method, &params).unwrap() {
            Action::Edit(p) => p,
            other => panic!("not an edit: {other:?}"),
        }
    }

    /// Times accept seconds and ticks; ranges and flags validate by field name.
    #[test]
    fn parsing_accepts_seconds_and_ticks_and_names_bad_fields() {
        let p = parse_ok(
            "cut_segments",
            json!({"capture_id": "rec_1", "ranges": [[10, 15], ["t:120000", 3.5]], "request_id": "r"}),
        );
        assert_eq!(p.request_id.as_deref(), Some("r"));
        let EditOp::Cut { ranges } = &p.op else {
            panic!()
        };
        assert_eq!(
            ranges[0],
            Span::new(MediaTime::from_secs(10), MediaTime::from_secs(15))
        );
        assert_eq!(ranges[1].from, MediaTime::from_secs(1));
        for (m, params, field) in [
            (
                "trim",
                json!({"capture_id": "r", "start": 5, "end": 2}),
                "end",
            ),
            ("trim", json!({"capture_id": "r", "start": 5}), "end"),
            (
                "cut_segments",
                json!({"capture_id": "r", "ranges": []}),
                "ranges",
            ),
            (
                "cut_segments",
                json!({"capture_id": "r", "ranges": [[3, 3]]}),
                "ranges[0]",
            ),
            ("split", json!({"capture_id": "r", "at": "later"}), "at"),
            (
                "set_speed",
                json!({"capture_id": "r", "range": [1, 2]}),
                "factor",
            ),
            (
                "set_speed",
                json!({"capture_id": "r", "range": [1], "factor": 2}),
                "range",
            ),
            (
                "set_background",
                json!({"capture_id": "r", "kind": "color"}),
                "value",
            ),
            (
                "set_background",
                json!({"capture_id": "r", "kind": "paint"}),
                "kind",
            ),
            (
                "set_audio_mix",
                json!({"capture_id": "r", "mic_gain_db": "loud"}),
                "mic_gain_db",
            ),
            (
                "set_audio_mix",
                json!({"capture_id": "r", "mic_mute": 1}),
                "mic_mute",
            ),
            (
                "set_audio_mix",
                json!({"capture_id": "r"}),
                "give at least one",
            ),
            (
                "set_camera_layout",
                json!({"capture_id": "r", "range": [0, 5], "layout": "wide"}),
                "layout",
            ),
            (
                "set_camera_layout",
                json!({"capture_id": "r", "range": [0, 5], "layout": "camera", "rect": [0, 0, 1, 1]}),
                "rect",
            ),
            (
                "set_camera_layout",
                json!({"capture_id": "r", "range": [0, 5], "layout": "bubble", "ring": 3}),
                "ring",
            ),
            (
                "set_camera_layout",
                json!({"capture_id": "r", "layout": "bubble"}),
                "range",
            ),
            (
                "trim",
                json!({"capture_id": "r", "start": 1, "end": 2, "bogus": 1}),
                "bogus",
            ),
        ] {
            let e = Action::parse(m, &params).unwrap_err();
            assert!(e.to_string().starts_with(field), "{m} {params}: {e}");
        }
    }

    #[test]
    fn every_edit_method_is_a_free_mutating_action() {
        for m in EDIT_METHODS {
            let s = crate::action::spec_for(m).unwrap();
            assert_eq!(s.tier, crate::action::Tier::Free, "{m}");
            assert!(s.mutating, "{m}");
        }
    }
}
