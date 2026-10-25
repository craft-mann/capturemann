//! The edit list of a recording (EDT-1 to EDT-3) and the pure timeline math a renderer needs.
//!
//! Lives in `cm-core` because it is plain data plus arithmetic with no I/O: the daemon edits it,
//! `cm-library` stores it in `manifest.edits[0]`, and the compositor and exporter (`cm-render`,
//! `cm-media`) ask it which source frame belongs to which output time.
//!
//! # Time
//!
//! Every time in the list is **session time** in `MediaTime` ticks (Technical design → One clock).
//! Media files are compacted at pauses and start at their track's first sample, so a reader maps
//! a session time into a file with [`track_time`]. `cursor.jsonl` stays in session time.
//!
//! # Stored shape (`manifest.edits[0]`)
//!
//! ```json
//! {
//!   "version": 2,
//!   "in": 36000, "out": 7236000,           // kept range, session ticks
//!   "speed": 1.0,                           // legacy base speed (always 1.0 from the recorder)
//!   "cuts": [[1200000, 1800000]],           // removed ranges, sorted, merged, inside in..out
//!   "splits": [2400000],                    // segment boundaries, sorted, inside in..out
//!   "speeds": [{"from": 2400000, "to": 3600000, "factor": 2.0, "keep_audio": false}],
//!   "background": {"kind": "color", "value": "#112233", "padding_pct": 8.0,
//!                  "radius_px": 12, "shadow": true},
//!   "audio": {"mic_gain_db": 0.0, "system_gain_db": -6.0, "mic_mute": false,
//!             "system_mute": false, "denoise": true},   // AUD-4; denoise omitted = as recorded
//!   "layout": { ... }                       // CAM-3, passed through untouched
//! }
//! ```
//!
//! An entry without `version` is the recorder's original `{in, out, speed, layout}`: its `out` is a
//! *file* time (the manifest `duration`), which [`EditList::from_json`] converts to session time,
//! and its `in` is replaced by `live_at` when the manifest has one. Unknown keys are kept.

use crate::edit_view::{CursorStyleUpdate, ReframeUpdate, ViewEdits, ZoomRegion};
use crate::{CmError, MediaTime, PauseGap, TICKS_PER_SECOND};
use serde_json::{json, Map, Value};

/// Version written into `manifest.edits[0]`.
pub const EDIT_VERSION: u32 = 2;
/// Shortest allowed export (EDT-1 edge case: a cut leaving less is refused).
pub const MIN_OUTPUT: MediaTime = MediaTime::from_millis(500);
/// Allowed speed factors (EDT-2).
pub const SPEED_RANGE: (f64, f64) = (0.25, 8.0);
/// Above this factor audio is muted unless `keep_audio` is set (EDT-2.AC3).
pub const MAX_AUDIO_SPEED: f64 = 2.0;
/// Background padding, percent of the frame (EDT-3).
pub const PADDING_PCT_RANGE: (f64, f64) = (0.0, 20.0);
/// Screen corner radius in px (EDT-3).
pub const RADIUS_PX_MAX: u32 = 24;

const NOTHING_LEFT: &str = "Nothing left to export";

/// A half-open span `[from, to)` of session time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: MediaTime,
    pub to: MediaTime,
}

impl Span {
    pub fn new(from: MediaTime, to: MediaTime) -> Self {
        Span { from, to }
    }

    pub fn len(&self) -> MediaTime {
        self.to - self.from
    }

    pub fn is_empty(&self) -> bool {
        self.to <= self.from
    }

    fn clamp(self, lo: MediaTime, hi: MediaTime) -> Span {
        Span::new(self.from.max(lo), self.to.min(hi))
    }

    fn to_json(self) -> Value {
        json!([self.from.ticks(), self.to.ticks()])
    }

    fn from_json(v: &Value) -> Option<Span> {
        let a = v.as_array().filter(|a| a.len() == 2)?;
        let s = Span::new(
            MediaTime::from_ticks(a[0].as_i64()?),
            MediaTime::from_ticks(a[1].as_i64()?),
        );
        (!s.is_empty()).then_some(s)
    }
}

/// A speed change over a span (always aligned to segment boundaries).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeedSpan {
    pub span: Span,
    pub factor: f64,
    pub keep_audio: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundKind {
    None,
    Color,
    Gradient,
    Image,
}

impl BackgroundKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BackgroundKind::None => "none",
            BackgroundKind::Color => "color",
            BackgroundKind::Gradient => "gradient",
            BackgroundKind::Image => "image",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CmError> {
        match s {
            "none" => Ok(BackgroundKind::None),
            "color" => Ok(BackgroundKind::Color),
            "gradient" => Ok(BackgroundKind::Gradient),
            "image" => Ok(BackgroundKind::Image),
            _ => Err(CmError::invalid_field(
                "kind",
                "must be none, color, gradient or image",
            )),
        }
    }
}

/// Capture-level background frame (EDT-3): the screen sits on it with padding, rounded corners and
/// an optional shadow. `value` is `"#rrggbb"` for a color, `["#rrggbb", "#rrggbb"]` for a gradient
/// and a file path for an image.
#[derive(Debug, Clone, PartialEq)]
pub struct Background {
    pub kind: BackgroundKind,
    pub value: Value,
    pub padding_pct: f64,
    pub radius_px: u32,
    pub shadow: bool,
}

impl Default for Background {
    fn default() -> Self {
        Background {
            kind: BackgroundKind::None,
            value: Value::Null,
            padding_pct: 0.0,
            radius_px: 0,
            shadow: false,
        }
    }
}

/// A partial change of the background: fields left `None` keep their value.
#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundUpdate {
    pub kind: BackgroundKind,
    pub value: Option<Value>,
    pub padding_pct: Option<f64>,
    pub radius_px: Option<u32>,
    pub shadow: Option<bool>,
}

fn hex_color(field: &str, v: &Value) -> Result<String, CmError> {
    let s = v
        .as_str()
        .ok_or_else(|| CmError::invalid_field(field, "must be a color like #1a2b3c"))?;
    let h = s.strip_prefix('#').unwrap_or("");
    let ok = h.chars().all(|c| c.is_ascii_hexdigit()) && (h.len() == 3 || h.len() == 6);
    if !ok {
        return Err(CmError::invalid_field(
            field,
            format!("`{s}` is not a color like #1a2b3c"),
        ));
    }
    let full: String = if h.len() == 3 {
        h.chars().flat_map(|c| [c, c]).collect()
    } else {
        h.to_owned()
    };
    Ok(format!("#{}", full.to_ascii_lowercase()))
}

impl Background {
    /// Applies an update and validates the result (EDT-3).
    pub fn updated(&self, u: &BackgroundUpdate) -> Result<Background, CmError> {
        let mut b = self.clone();
        b.kind = u.kind;
        if let Some(v) = &u.value {
            b.value = v.clone();
        }
        if let Some(p) = u.padding_pct {
            if !p.is_finite() || p < PADDING_PCT_RANGE.0 || p > PADDING_PCT_RANGE.1 {
                return Err(CmError::invalid_field(
                    "padding_pct",
                    format!(
                        "must be between {} and {}",
                        PADDING_PCT_RANGE.0, PADDING_PCT_RANGE.1
                    ),
                ));
            }
            b.padding_pct = p;
        }
        if let Some(r) = u.radius_px {
            if r > RADIUS_PX_MAX {
                return Err(CmError::invalid_field(
                    "radius_px",
                    format!("must be at most {RADIUS_PX_MAX}"),
                ));
            }
            b.radius_px = r;
        }
        if let Some(s) = u.shadow {
            b.shadow = s;
        }
        b.value = match b.kind {
            BackgroundKind::None => Value::Null,
            BackgroundKind::Color => Value::String(hex_color("value", &b.value)?),
            BackgroundKind::Gradient => {
                let stops = b.value.as_array().filter(|a| a.len() == 2).ok_or_else(|| {
                    CmError::invalid_field(
                        "value",
                        "a gradient needs two colors: [\"#000\", \"#fff\"]",
                    )
                })?;
                json!([
                    hex_color("value", &stops[0])?,
                    hex_color("value", &stops[1])?
                ])
            }
            BackgroundKind::Image => match b.value.as_str() {
                Some(p) if !p.trim().is_empty() && p.len() <= 4096 && !p.contains('\0') => {
                    b.value.clone()
                }
                _ => {
                    return Err(CmError::invalid_field(
                        "value",
                        "an image background needs a file path",
                    ))
                }
            },
        };
        Ok(b)
    }

    fn to_json(&self) -> Value {
        let mut o = Map::new();
        o.insert("kind".into(), json!(self.kind.as_str()));
        if self.kind != BackgroundKind::None {
            o.insert("value".into(), self.value.clone());
        }
        o.insert("padding_pct".into(), json!(self.padding_pct));
        o.insert("radius_px".into(), json!(self.radius_px));
        o.insert("shadow".into(), json!(self.shadow));
        Value::Object(o)
    }

    fn from_json(v: &Value) -> Background {
        let Some(o) = v.as_object() else {
            return Background::default();
        };
        let kind = o
            .get("kind")
            .and_then(Value::as_str)
            .and_then(|k| BackgroundKind::parse(k).ok())
            .unwrap_or(BackgroundKind::None);
        Background {
            kind,
            value: o.get("value").cloned().unwrap_or(Value::Null),
            padding_pct: o.get("padding_pct").and_then(Value::as_f64).unwrap_or(0.0),
            radius_px: o
                .get("radius_px")
                .and_then(Value::as_u64)
                .and_then(|r| u32::try_from(r).ok())
                .unwrap_or(0),
            shadow: o.get("shadow").and_then(Value::as_bool).unwrap_or(false),
        }
    }
}

/// Per-track level range of the editor in dB (AUD-4: -inf to +12; -inf is `mute`).
pub const AUDIO_GAIN_DB_RANGE: (f64, f64) = (-96.0, 12.0);

/// Capture-level audio mix (AUD-4): the mic and the system audio stay separate files and are
/// balanced here. Applied by the exporter (`cm_audio::mix`).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioMix {
    pub mic_gain_db: f64,
    pub system_gain_db: f64,
    pub mic_mute: bool,
    pub system_mute: bool,
    /// Use the denoised mic file (`true`), the raw one (`false`), or what was recorded (`None`).
    pub denoise: Option<bool>,
}

impl Default for AudioMix {
    fn default() -> Self {
        AudioMix {
            mic_gain_db: 0.0,
            system_gain_db: 0.0,
            mic_mute: false,
            system_mute: false,
            denoise: None,
        }
    }
}

/// A partial change of the mix: fields left `None` keep their value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioMixUpdate {
    pub mic_gain_db: Option<f64>,
    pub system_gain_db: Option<f64>,
    pub mic_mute: Option<bool>,
    pub system_mute: Option<bool>,
    pub denoise: Option<bool>,
}

impl AudioMixUpdate {
    pub fn is_empty(&self) -> bool {
        *self == AudioMixUpdate::default()
    }
}

fn check_gain(field: &str, db: f64) -> Result<f64, CmError> {
    if db.is_finite() && db >= AUDIO_GAIN_DB_RANGE.0 && db <= AUDIO_GAIN_DB_RANGE.1 {
        Ok(db)
    } else {
        Err(CmError::invalid_field(
            field,
            format!(
                "must be between {} and {} dB (use the mute option for silence)",
                AUDIO_GAIN_DB_RANGE.0, AUDIO_GAIN_DB_RANGE.1
            ),
        ))
    }
}

impl AudioMix {
    /// Applies an update and validates the result (AUD-4).
    pub fn updated(&self, u: &AudioMixUpdate) -> Result<AudioMix, CmError> {
        if u.is_empty() {
            return Err(CmError::invalid_input(
                "give at least one of mic_gain_db, system_gain_db, mic_mute, system_mute, denoise",
            ));
        }
        let mut m = self.clone();
        if let Some(g) = u.mic_gain_db {
            m.mic_gain_db = check_gain("mic_gain_db", g)?;
        }
        if let Some(g) = u.system_gain_db {
            m.system_gain_db = check_gain("system_gain_db", g)?;
        }
        if let Some(b) = u.mic_mute {
            m.mic_mute = b;
        }
        if let Some(b) = u.system_mute {
            m.system_mute = b;
        }
        if let Some(b) = u.denoise {
            m.denoise = Some(b);
        }
        Ok(m)
    }

    fn to_json(&self) -> Value {
        let mut o = Map::new();
        o.insert("mic_gain_db".into(), json!(self.mic_gain_db));
        o.insert("system_gain_db".into(), json!(self.system_gain_db));
        o.insert("mic_mute".into(), json!(self.mic_mute));
        o.insert("system_mute".into(), json!(self.system_mute));
        if let Some(d) = self.denoise {
            o.insert("denoise".into(), json!(d));
        }
        Value::Object(o)
    }

    fn from_json(v: &Value) -> AudioMix {
        let Some(o) = v.as_object() else {
            return AudioMix::default();
        };
        let gain = |k: &str| {
            o.get(k)
                .and_then(Value::as_f64)
                .and_then(|g| check_gain(k, g).ok())
                .unwrap_or(0.0)
        };
        let flag = |k: &str| o.get(k).and_then(Value::as_bool).unwrap_or(false);
        AudioMix {
            mic_gain_db: gain("mic_gain_db"),
            system_gain_db: gain("system_gain_db"),
            mic_mute: flag("mic_mute"),
            system_mute: flag("system_mute"),
            denoise: o.get("denoise").and_then(Value::as_bool),
        }
    }
}

/// What a reader of the manifest knows about the recording, needed to read legacy entries and to
/// bound edits.
#[derive(Debug, Clone, Copy)]
pub struct EditContext<'a> {
    /// Session time at which the last sample of any track ends.
    pub end: MediaTime,
    /// `live_at` of the manifest; the default in-point.
    pub live_at: Option<MediaTime>,
    pub gaps: &'a [PauseGap],
}

/// The edit list: trim, cuts, splits, speeds and background. Masters are never touched.
#[derive(Debug, Clone, PartialEq)]
pub struct EditList {
    pub start: MediaTime,
    pub end: MediaTime,
    /// Base speed from the legacy `speed` key.
    pub base_speed: f64,
    pub cuts: Vec<Span>,
    pub splits: Vec<MediaTime>,
    pub speeds: Vec<SpeedSpan>,
    pub background: Background,
    pub audio: AudioMix,
    /// The editable cursor, zoom regions and reframe windows (EDT-4, EDT-5).
    pub view: ViewEdits,
    /// Keys this module does not own (`layout`, ...), written back unchanged.
    pub rest: Map<String, Value>,
}

const OWNED: &[&str] = &[
    "version",
    "in",
    "out",
    "speed",
    "cuts",
    "splits",
    "speeds",
    "background",
    "audio",
    "cursor",
    "zooms",
    "reframe",
];

fn time_of(v: Option<&Value>) -> Option<MediaTime> {
    v.and_then(Value::as_i64).map(MediaTime::from_ticks)
}

fn merge_spans(mut spans: Vec<Span>) -> Vec<Span> {
    spans.retain(|s| !s.is_empty());
    spans.sort_by_key(|s| (s.from, s.to));
    let mut out: Vec<Span> = Vec::with_capacity(spans.len());
    for s in spans {
        match out.last_mut() {
            Some(last) if s.from <= last.to => last.to = last.to.max(s.to),
            _ => out.push(s),
        }
    }
    out
}

/// `spans` minus `hole`.
fn subtract(spans: &[Span], hole: Span) -> Vec<Span> {
    let mut out = Vec::with_capacity(spans.len() + 1);
    for s in spans {
        if hole.to <= s.from || hole.from >= s.to {
            out.push(*s);
            continue;
        }
        if hole.from > s.from {
            out.push(Span::new(s.from, hole.from));
        }
        if hole.to < s.to {
            out.push(Span::new(hole.to, s.to));
        }
    }
    out
}

impl EditList {
    /// One full-length segment at normal speed, in-point at `live_at` (or 0).
    pub fn full(cx: &EditContext<'_>) -> EditList {
        let start = cx
            .live_at
            .unwrap_or(MediaTime::ZERO)
            .clamp(MediaTime::ZERO, cx.end);
        EditList {
            start,
            end: cx.end,
            base_speed: 1.0,
            cuts: Vec::new(),
            splits: Vec::new(),
            speeds: Vec::new(),
            background: Background::default(),
            audio: AudioMix::default(),
            view: ViewEdits::default(),
            rest: Map::new(),
        }
    }

    /// Reads `manifest.edits[0]`, migrating the recorder's original entry (see the module docs).
    /// Malformed fields fall back to their defaults instead of failing: the masters are intact
    /// and a bad edit list must not make a recording unreadable.
    pub fn from_json(v: &Value, cx: &EditContext<'_>) -> EditList {
        let mut list = EditList::full(cx);
        let Some(o) = v.as_object() else {
            return list;
        };
        let versioned = o.get("version").and_then(Value::as_u64).unwrap_or(1) >= 2;
        if let Some(s) = o.get("speed").and_then(Value::as_f64) {
            if s.is_finite() && s >= SPEED_RANGE.0 && s <= SPEED_RANGE.1 {
                list.base_speed = s;
            }
        }
        if versioned {
            list.start = time_of(o.get("in")).unwrap_or(list.start);
            list.end = time_of(o.get("out")).unwrap_or(list.end);
        } else if let Some(file_out) = time_of(o.get("out")) {
            // Legacy `out` is the master's length on the file-time axis.
            list.end = file_to_session_end(cx.gaps, file_out);
        }
        list.cuts = merge_spans(
            o.get("cuts")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Span::from_json).collect())
                .unwrap_or_default(),
        );
        let mut splits: Vec<MediaTime> = o
            .get("splits")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.as_i64().map(MediaTime::from_ticks))
                    .collect()
            })
            .unwrap_or_default();
        splits.sort();
        splits.dedup();
        list.splits = splits;
        list.speeds = o
            .get("speeds")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|s| {
                        let span = Span::new(time_of(s.get("from"))?, time_of(s.get("to"))?);
                        let factor = s.get("factor")?.as_f64()?;
                        (!span.is_empty() && factor >= SPEED_RANGE.0 && factor <= SPEED_RANGE.1)
                            .then(|| SpeedSpan {
                                span,
                                factor,
                                keep_audio: s
                                    .get("keep_audio")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                            })
                    })
                    .collect()
            })
            .unwrap_or_default();
        list.speeds.sort_by_key(|s| s.span.from);
        list.background = o
            .get("background")
            .map(Background::from_json)
            .unwrap_or_default();
        list.audio = o.get("audio").map(AudioMix::from_json).unwrap_or_default();
        list.view = ViewEdits::read_json(o);
        for (k, val) in o {
            if !OWNED.contains(&k.as_str()) {
                list.rest.insert(k.clone(), val.clone());
            }
        }
        list.end = list.end.max(MediaTime::ZERO).min(cx.end);
        list.start = list.start.max(MediaTime::ZERO).min(list.end);
        list.normalize();
        list
    }

    /// The stored JSON (`manifest.edits[0]`).
    pub fn to_json(&self) -> Value {
        let mut o = self.rest.clone();
        o.insert("version".into(), json!(EDIT_VERSION));
        o.insert("in".into(), json!(self.start.ticks()));
        o.insert("out".into(), json!(self.end.ticks()));
        o.insert("speed".into(), json!(self.base_speed));
        o.insert(
            "cuts".into(),
            Value::Array(self.cuts.iter().map(|c| c.to_json()).collect()),
        );
        o.insert(
            "splits".into(),
            Value::Array(self.splits.iter().map(|t| json!(t.ticks())).collect()),
        );
        o.insert(
            "speeds".into(),
            Value::Array(
                self.speeds
                    .iter()
                    .map(|s| {
                        json!({
                            "from": s.span.from.ticks(),
                            "to": s.span.to.ticks(),
                            "factor": s.factor,
                            "keep_audio": s.keep_audio,
                        })
                    })
                    .collect(),
            ),
        );
        o.insert("background".into(), self.background.to_json());
        o.insert("audio".into(), self.audio.to_json());
        self.view.write_json(&mut o);
        Value::Object(o)
    }

    /// Keeps every part inside `start..end`, sorted and merged.
    fn normalize(&mut self) {
        let (lo, hi) = (self.start, self.end);
        self.cuts = merge_spans(self.cuts.iter().map(|c| c.clamp(lo, hi)).collect());
        self.splits.retain(|t| *t > lo && *t < hi);
        self.splits.sort();
        self.splits.dedup();
        self.speeds = self
            .speeds
            .iter()
            .filter_map(|s| {
                let span = s.span.clamp(lo, hi);
                (!span.is_empty()).then_some(SpeedSpan { span, ..*s })
            })
            .collect();
        self.speeds.sort_by_key(|s| s.span.from);
        self.view.clamp_to(lo, hi);
    }

    fn checked(mut self) -> Result<EditList, CmError> {
        self.normalize();
        if self.timeline().duration() < MIN_OUTPUT {
            return Err(CmError::invalid_input(NOTHING_LEFT));
        }
        Ok(self)
    }

    fn bounds_end(&self, cx: &EditContext<'_>) -> MediaTime {
        cx.end
    }

    // ---- edits ---------------------------------------------------------------------------

    /// EDT-1 trim: keep `start..end` (session time), clamped to the recording.
    pub fn trim(
        &self,
        start: MediaTime,
        end: MediaTime,
        cx: &EditContext<'_>,
    ) -> Result<EditList, CmError> {
        let (start, end) = (start.max(MediaTime::ZERO), end.min(self.bounds_end(cx)));
        if end <= start {
            return Err(CmError::invalid_field("end", "must be after start"));
        }
        let mut l = self.clone();
        l.start = start;
        l.end = end;
        l.checked()
    }

    /// The list as it is exported and played: every pause gap inside the kept part is added to
    /// the cuts (merged with the user's), because pauses are not part of any media file (REC-4)
    /// and are removed from every track. The stored list is not changed. Use this before
    /// [`EditList::timeline`] wherever an output duration or a source-to-output map is needed, so
    /// the preview, the export and the reported duration agree.
    pub fn without_pauses(&self, gaps: &[PauseGap]) -> EditList {
        let mut out = self.clone();
        let mut cuts: Vec<Span> = self.cuts.clone();
        cuts.extend(
            gaps.iter()
                .map(|g| Span::new(g.from.max(self.start), g.to.min(self.end)))
                .filter(|s| !s.is_empty()),
        );
        cuts.sort_by_key(|s| (s.from, s.to));
        let mut merged: Vec<Span> = Vec::with_capacity(cuts.len());
        for c in cuts {
            match merged.last_mut() {
                Some(last) if c.from <= last.to => last.to = last.to.max(c.to),
                _ => merged.push(c),
            }
        }
        out.cuts = merged;
        out
    }

    /// EDT-1 cut: removes the ranges (clamped to the kept part; overlaps merge).
    pub fn cut(&self, ranges: &[Span]) -> Result<EditList, CmError> {
        Self::check_ranges(ranges)?;
        let mut l = self.clone();
        let mut cuts = l.cuts.clone();
        let mut any = false;
        for r in ranges {
            let r = r.clamp(l.start, l.end);
            if !r.is_empty() {
                any = true;
                cuts.push(r);
            }
        }
        if !any {
            return Err(CmError::invalid_field(
                "ranges",
                "no range overlaps the part of the recording that is kept",
            ));
        }
        l.cuts = merge_spans(cuts);
        l.checked()
    }

    /// EDT-1 restore: brings cut ranges back (partial overlaps shrink or split a cut).
    pub fn restore(&self, ranges: &[Span]) -> Result<EditList, CmError> {
        Self::check_ranges(ranges)?;
        let mut l = self.clone();
        let before = l.cuts.clone();
        for r in ranges {
            l.cuts = subtract(&l.cuts, *r);
        }
        if l.cuts == before {
            return Err(CmError::invalid_field(
                "ranges",
                "no range overlaps a cut, so there is nothing to restore",
            ));
        }
        l.checked()
    }

    /// EDT-1 split: a segment boundary at `at`.
    pub fn split(&self, at: MediaTime) -> Result<EditList, CmError> {
        if at <= self.start || at >= self.end {
            return Err(CmError::invalid_field(
                "at",
                "must be inside the kept part of the recording",
            ));
        }
        if self.splits.contains(&at) {
            return Err(CmError::invalid_field(
                "at",
                "the recording is already split there",
            ));
        }
        let mut l = self.clone();
        l.splits.push(at);
        l.checked()
    }

    /// EDT-2 speed: `factor` over `range` (clamped to the kept part). The range edges become
    /// segment boundaries. A factor of 1 on a recording at base speed 1 clears the speed there.
    pub fn set_speed(
        &self,
        range: Span,
        factor: f64,
        keep_audio: bool,
    ) -> Result<EditList, CmError> {
        if !factor.is_finite() || factor < SPEED_RANGE.0 || factor > SPEED_RANGE.1 {
            return Err(CmError::invalid_field(
                "factor",
                format!("must be between {} and {}", SPEED_RANGE.0, SPEED_RANGE.1),
            ));
        }
        Self::check_ranges(&[range])?;
        let mut l = self.clone();
        let r = range.clamp(l.start, l.end);
        if r.is_empty() {
            return Err(CmError::invalid_field(
                "range",
                "does not overlap the part of the recording that is kept",
            ));
        }
        for edge in [r.from, r.to] {
            if edge > l.start && edge < l.end && !l.splits.contains(&edge) {
                l.splits.push(edge);
            }
        }
        let mut kept = Vec::with_capacity(l.speeds.len() + 1);
        for s in &l.speeds {
            for piece in subtract(&[s.span], r) {
                kept.push(SpeedSpan { span: piece, ..*s });
            }
        }
        if !((factor - 1.0).abs() < 1e-9 && (l.base_speed - 1.0).abs() < 1e-9) {
            kept.push(SpeedSpan {
                span: r,
                factor,
                keep_audio,
            });
        }
        l.speeds = kept;
        l.checked()
    }

    /// EDT-3 background.
    pub fn set_background(&self, u: &BackgroundUpdate) -> Result<EditList, CmError> {
        let mut l = self.clone();
        l.background = self.background.updated(u)?;
        Ok(l)
    }

    /// AUD-4 mix: per-track gain, mute and the denoised/raw mic choice.
    pub fn set_audio_mix(&self, u: &AudioMixUpdate) -> Result<EditList, CmError> {
        let mut l = self.clone();
        l.audio = self.audio.updated(u)?;
        Ok(l)
    }

    /// EDT-4 cursor style: size, smoothing, click effect and hide-when-idle.
    pub fn set_cursor_style(&self, u: &CursorStyleUpdate) -> Result<EditList, CmError> {
        let mut l = self.clone();
        l.view.cursor = self.view.cursor.updated(u)?;
        Ok(l)
    }

    /// EDT-4 zoom regions: adds `zooms` (or replaces all of them), clipped to the kept range and
    /// out of the cuts; a new region shortens the ones it overlaps.
    pub fn set_zooms(&self, zooms: &[ZoomRegion], replace: bool) -> Result<EditList, CmError> {
        let mut l = self.clone();
        l.view = self
            .view
            .set_zooms(zooms, replace, self.start, self.end, &self.cuts)?;
        Ok(l)
    }

    /// EDT-5 reframe of one aspect: its mode and/or the window focus of one segment.
    pub fn set_reframe(&self, u: &ReframeUpdate) -> Result<EditList, CmError> {
        let mut l = self.clone();
        l.view = self
            .view
            .set_reframe(u, self.start, self.end, &self.splits)?;
        Ok(l)
    }

    fn check_ranges(ranges: &[Span]) -> Result<(), CmError> {
        if ranges.is_empty() {
            return Err(CmError::invalid_field("ranges", "must not be empty"));
        }
        for (i, r) in ranges.iter().enumerate() {
            if r.is_empty() {
                return Err(CmError::invalid_field(
                    &format!("ranges[{i}]"),
                    "must end after it starts",
                ));
            }
        }
        Ok(())
    }

    // ---- timeline ------------------------------------------------------------------------

    /// The output timeline: kept pieces in order with their speeds.
    pub fn timeline(&self) -> Timeline {
        Timeline::build(self)
    }
}

/// One contiguous kept piece of the recording at one speed.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// Session-time span in the source.
    pub source: Span,
    /// Where the piece starts on the output timeline.
    pub out_start: MediaTime,
    /// How long it lasts on the output timeline.
    pub out_len: MediaTime,
    pub factor: f64,
    /// Audio plays (factor at most 2, or `keep_audio`).
    pub audio: bool,
    /// Segment index: the number of user splits at or before the piece.
    pub segment: usize,
}

/// What to draw at one output time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Session time to read from every track (map into files with [`track_time`]).
    pub source: MediaTime,
    pub speed: f64,
    pub segment: usize,
    pub audio: bool,
    /// Index into [`Timeline::pieces`].
    pub piece: usize,
}

/// Pure source/output time mapping of an [`EditList`]; cheap to build, so a renderer may rebuild
/// it after each edit.
#[derive(Debug, Clone, PartialEq)]
pub struct Timeline {
    pieces: Vec<Piece>,
    duration: MediaTime,
}

impl Timeline {
    fn build(l: &EditList) -> Timeline {
        let mut edges = vec![l.start, l.end];
        edges.extend(l.splits.iter().copied());
        for c in &l.cuts {
            edges.extend([c.from, c.to]);
        }
        for s in &l.speeds {
            edges.extend([s.span.from, s.span.to]);
        }
        edges.retain(|t| *t >= l.start && *t <= l.end);
        edges.sort();
        edges.dedup();
        let mut pieces = Vec::new();
        let mut at = MediaTime::ZERO;
        for w in edges.windows(2) {
            let source = Span::new(w[0], w[1]);
            if source.is_empty()
                || l.cuts
                    .iter()
                    .any(|c| c.from <= source.from && source.to <= c.to)
            {
                continue;
            }
            let (factor, keep) = l
                .speeds
                .iter()
                .find(|s| s.span.from <= source.from && source.to <= s.span.to)
                .map_or((l.base_speed, false), |s| (s.factor, s.keep_audio));
            let out_len =
                MediaTime::from_ticks((source.len().ticks() as f64 / factor).round() as i64);
            pieces.push(Piece {
                source,
                out_start: at,
                out_len,
                factor,
                audio: factor <= MAX_AUDIO_SPEED || keep,
                segment: l.splits.iter().filter(|s| **s <= source.from).count(),
            });
            at += out_len;
        }
        Timeline {
            pieces,
            duration: at,
        }
    }

    pub fn pieces(&self) -> &[Piece] {
        &self.pieces
    }

    /// Length of the export.
    pub fn duration(&self) -> MediaTime {
        self.duration
    }

    /// What to draw at output time `t`; `None` at or past the end.
    pub fn at_output(&self, t: MediaTime) -> Option<Sample> {
        if t < MediaTime::ZERO || t >= self.duration {
            return None;
        }
        let idx = self
            .pieces
            .partition_point(|p| p.out_start + p.out_len <= t);
        let p = self.pieces.get(idx)?;
        let into = (t - p.out_start).ticks() as f64 * p.factor;
        let source = (p.source.from + MediaTime::from_ticks(into.round() as i64))
            .min(p.source.to - MediaTime::from_ticks(1));
        Some(Sample {
            source,
            speed: p.factor,
            segment: p.segment,
            audio: p.audio,
            piece: idx,
        })
    }

    /// Output time of a source time; `None` when it is cut or outside the kept range.
    pub fn source_to_output(&self, s: MediaTime) -> Option<MediaTime> {
        let p = self
            .pieces
            .iter()
            .find(|p| p.source.from <= s && s < p.source.to)?;
        let into = (s - p.source.from).ticks() as f64 / p.factor;
        Some(p.out_start + MediaTime::from_ticks(into.round() as i64))
    }
}

// ---- tracks and files -----------------------------------------------------------------------

/// File time of a session time: the session time minus the pause time before it. A time inside a
/// gap maps to the gap's start.
pub fn session_to_file(gaps: &[PauseGap], t: MediaTime) -> MediaTime {
    let removed: i64 = gaps
        .iter()
        .map(|g| (t.min(g.to) - g.from).ticks().max(0))
        .sum();
    MediaTime::from_ticks(t.ticks() - removed)
}

/// Position inside a track's media file for a session time: `None` before the track's first
/// sample. File position 0 is the track's first sample. Not for `cursor.jsonl`, which stays in
/// session time.
pub fn track_time(gaps: &[PauseGap], track_start: MediaTime, t: MediaTime) -> Option<MediaTime> {
    let pos = session_to_file(gaps, t) - session_to_file(gaps, track_start);
    (pos >= MediaTime::ZERO).then_some(pos)
}

/// Session time of the end of a file whose length (file time) is `file`; an end exactly at a
/// gap's start stays before the gap.
fn file_to_session_end(gaps: &[PauseGap], file: MediaTime) -> MediaTime {
    let mut t = file.ticks();
    for g in gaps {
        if g.from.ticks() < t {
            t += (g.to - g.from).ticks();
        }
    }
    MediaTime::from_ticks(t)
}

/// Parses a time from the agent interface: seconds (a number) or `"t:<ticks>"`.
pub fn parse_time(field: &str, v: &Value) -> Result<MediaTime, CmError> {
    match v {
        Value::Number(n) => {
            let s = n
                .as_f64()
                .ok_or_else(|| CmError::invalid_field(field, "must be a number of seconds"))?;
            MediaTime::from_secs_f64(s)
                .map_err(|_| CmError::invalid_field(field, "is not a valid time"))
        }
        Value::String(s) => s
            .strip_prefix("t:")
            .and_then(|t| t.parse::<i64>().ok())
            .map(MediaTime::from_ticks)
            .ok_or_else(|| CmError::invalid_field(field, "must be seconds or \"t:<ticks>\"")),
        _ => Err(CmError::invalid_field(
            field,
            "must be seconds or \"t:<ticks>\"",
        )),
    }
}

/// Seconds, for results shown to people.
pub fn secs(t: MediaTime) -> f64 {
    t.ticks() as f64 / TICKS_PER_SECOND as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: f64) -> MediaTime {
        MediaTime::from_secs_f64(s).unwrap()
    }

    fn span(a: f64, b: f64) -> Span {
        Span::new(t(a), t(b))
    }

    fn cx(end: f64) -> EditContext<'static> {
        EditContext {
            end: t(end),
            live_at: None,
            gaps: &[],
        }
    }

    fn secs_of(l: &EditList) -> f64 {
        secs(l.timeline().duration())
    }

    /// EDT-1.AC1: trimming a 60 s recording to 2..58 s exports 56 s.
    #[test]
    fn edt1_ac1_trim_exports_56_seconds() {
        let c = cx(60.0);
        let l = EditList::full(&c).trim(t(2.0), t(58.0), &c).unwrap();
        assert!((secs_of(&l) - 56.0).abs() < 1.0 / 60.0);
    }

    /// EDT-1.AC2: a cut is skipped and later output time maps past it.
    #[test]
    fn edt1_ac2_cut_is_skipped_in_the_timeline() {
        let c = cx(60.0);
        let l = EditList::full(&c).cut(&[span(10.0, 15.0)]).unwrap();
        assert!((secs_of(&l) - 55.0).abs() < 1e-6);
        let before = l.timeline().at_output(t(9.9)).unwrap();
        assert!((secs(before.source) - 9.9).abs() < 1e-4);
        let after = l.timeline().at_output(t(10.0)).unwrap();
        assert!((secs(after.source) - 15.0).abs() < 1e-4);
        assert!(l.timeline().source_to_output(t(12.0)).is_none());
        assert_eq!(l.timeline().source_to_output(t(20.0)), Some(t(15.0)));
    }

    /// EDT-1.AC3: restoring a cut returns the original list.
    #[test]
    fn edt1_ac3_restore_returns_the_original() {
        let c = cx(60.0);
        let full = EditList::full(&c);
        let cut = full.cut(&[span(10.0, 15.0)]).unwrap();
        let back = cut.restore(&[span(10.0, 15.0)]).unwrap();
        assert_eq!(back, full);
        // A partial restore shrinks the cut.
        let part = cut.restore(&[span(12.0, 20.0)]).unwrap();
        assert_eq!(part.cuts, vec![span(10.0, 12.0)]);
        assert!(full.restore(&[span(1.0, 2.0)]).is_err());
    }

    /// EDT-1.AC5 / edge cases: overlapping cuts merge, outside ranges clamp, an empty kept part
    /// is refused.
    #[test]
    fn edt1_cuts_merge_clamp_and_leave_something() {
        let c = cx(10.0);
        let l = EditList::full(&c)
            .cut(&[span(1.0, 3.0), span(2.0, 5.0), span(8.0, 99.0)])
            .unwrap();
        assert_eq!(l.cuts, vec![span(1.0, 5.0), span(8.0, 10.0)]);
        let e = EditList::full(&c).cut(&[span(0.2, 10.0)]).unwrap_err();
        assert!(e.to_string().contains("Nothing left to export"), "{e}");
        assert!(EditList::full(&c).cut(&[span(20.0, 30.0)]).is_err());
        assert!(EditList::full(&c).cut(&[span(3.0, 3.0)]).is_err());
    }

    /// EDT-1: split adds a boundary once, and segments count splits.
    #[test]
    fn edt1_split_numbers_segments() {
        let c = cx(60.0);
        let l = EditList::full(&c)
            .split(t(20.0))
            .unwrap()
            .split(t(40.0))
            .unwrap();
        let segs: Vec<usize> = l.timeline().pieces().iter().map(|p| p.segment).collect();
        assert_eq!(segs, vec![0, 1, 2]);
        assert!(l.split(t(20.0)).is_err());
        assert!(l.split(t(0.0)).is_err());
        assert!(l.split(t(60.0)).is_err());
    }

    /// EDT-2.AC1: a 20 s segment at 2x lasts 10 s; the edges become splits.
    #[test]
    fn edt2_ac1_double_speed_halves_the_segment() {
        let c = cx(60.0);
        let l = EditList::full(&c)
            .set_speed(span(10.0, 30.0), 2.0, false)
            .unwrap();
        assert!((secs_of(&l) - 50.0).abs() < 1e-6);
        assert_eq!(l.splits, vec![t(10.0), t(30.0)]);
        // Output 10 s is where the sped-up part starts; 1 s later is 2 s of source.
        let s = l.timeline().at_output(t(11.0)).unwrap();
        assert!((secs(s.source) - 12.0).abs() < 1e-4);
        assert_eq!(s.speed, 2.0);
    }

    /// EDT-2.AC3: above 2x the audio is muted unless asked to stay.
    #[test]
    fn edt2_ac3_audio_mutes_above_2x() {
        let c = cx(60.0);
        let muted = EditList::full(&c)
            .set_speed(span(0.0, 20.0), 4.0, false)
            .unwrap();
        assert!(!muted.timeline().at_output(t(1.0)).unwrap().audio);
        let kept = EditList::full(&c)
            .set_speed(span(0.0, 20.0), 4.0, true)
            .unwrap();
        assert!(kept.timeline().at_output(t(1.0)).unwrap().audio);
        let slow = EditList::full(&c)
            .set_speed(span(0.0, 20.0), 1.5, false)
            .unwrap();
        assert!(slow.timeline().at_output(t(1.0)).unwrap().audio);
        assert!(!muted.timeline().at_output(t(30.0)).is_none());
    }

    #[test]
    fn edt2_speed_overlaps_replace_and_one_clears() {
        let c = cx(60.0);
        let l = EditList::full(&c)
            .set_speed(span(10.0, 30.0), 2.0, false)
            .unwrap()
            .set_speed(span(20.0, 40.0), 4.0, false)
            .unwrap();
        let f: Vec<(f64, f64, f64)> = l
            .speeds
            .iter()
            .map(|s| (secs(s.span.from), secs(s.span.to), s.factor))
            .collect();
        assert_eq!(f, vec![(10.0, 20.0, 2.0), (20.0, 40.0, 4.0)]);
        let cleared = l.set_speed(span(0.0, 60.0), 1.0, false).unwrap();
        assert!(cleared.speeds.is_empty());
        assert!(l.set_speed(span(0.0, 5.0), 9.0, false).is_err());
        assert!(l.set_speed(span(0.0, 5.0), 0.1, false).is_err());
        assert!(l.set_speed(span(0.0, 5.0), f64::NAN, false).is_err());
    }

    /// Cuts and speed combine: the cut part costs no output time at any speed.
    #[test]
    fn cut_and_speed_combine() {
        let c = cx(60.0);
        let l = EditList::full(&c)
            .set_speed(span(0.0, 20.0), 2.0, false)
            .unwrap()
            .cut(&[span(10.0, 20.0)])
            .unwrap();
        // 0..10 s at 2x = 5 s, plus 20..60 s = 40 s.
        assert!((secs_of(&l) - 45.0).abs() < 1e-6);
    }

    /// AUD-4: gains are range checked, partial updates keep the rest, and the mix round-trips
    /// through the stored JSON.
    #[test]
    fn aud4_audio_mix_validation_and_roundtrip() {
        let l = EditList::full(&EditContext {
            end: MediaTime::from_secs(60),
            live_at: None,
            gaps: &[],
        });
        assert_eq!(l.audio, AudioMix::default());
        let a = l
            .set_audio_mix(&AudioMixUpdate {
                system_gain_db: Some(-6.0),
                ..Default::default()
            })
            .unwrap();
        let b = a
            .set_audio_mix(&AudioMixUpdate {
                mic_mute: Some(true),
                denoise: Some(false),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(b.audio.system_gain_db, -6.0);
        assert!(b.audio.mic_mute && b.audio.denoise == Some(false));
        let json = b.to_json();
        assert_eq!(json["audio"]["system_gain_db"], -6.0);
        let cx = EditContext {
            end: MediaTime::from_secs(60),
            live_at: None,
            gaps: &[],
        };
        assert_eq!(EditList::from_json(&json, &cx).audio, b.audio);
        // An entry stored before AUD-4 has no `audio` key and reads as the defaults.
        let mut old = json.clone();
        old.as_object_mut().unwrap().remove("audio");
        assert_eq!(EditList::from_json(&old, &cx).audio, AudioMix::default());
        let bad = |u: AudioMixUpdate| l.set_audio_mix(&u).unwrap_err().to_string();
        assert!(bad(AudioMixUpdate {
            mic_gain_db: Some(12.5),
            ..Default::default()
        })
        .starts_with("mic_gain_db"));
        assert!(bad(AudioMixUpdate {
            system_gain_db: Some(f64::NEG_INFINITY),
            ..Default::default()
        })
        .starts_with("system_gain_db"));
        assert!(bad(AudioMixUpdate::default()).contains("at least one"));
        assert!(l
            .set_audio_mix(&AudioMixUpdate {
                mic_gain_db: Some(12.0),
                ..Default::default()
            })
            .is_ok());
    }

    /// EDT-3: settings are validated and normalized.
    #[test]
    fn edt3_background_validation() {
        let c = cx(10.0);
        let l = EditList::full(&c);
        let ok = l
            .set_background(&BackgroundUpdate {
                kind: BackgroundKind::Color,
                value: Some(json!("#ABC")),
                padding_pct: Some(8.0),
                radius_px: Some(12),
                shadow: Some(true),
            })
            .unwrap();
        assert_eq!(ok.background.value, json!("#aabbcc"));
        assert_eq!(ok.background.radius_px, 12);
        let bad = |u: BackgroundUpdate| l.set_background(&u).unwrap_err().to_string();
        let base = BackgroundUpdate {
            kind: BackgroundKind::Color,
            value: Some(json!("#000")),
            padding_pct: None,
            radius_px: None,
            shadow: None,
        };
        assert!(bad(BackgroundUpdate {
            padding_pct: Some(21.0),
            ..base.clone()
        })
        .contains("padding_pct"));
        assert!(bad(BackgroundUpdate {
            radius_px: Some(25),
            ..base.clone()
        })
        .contains("radius_px"));
        assert!(bad(BackgroundUpdate {
            value: Some(json!("red")),
            ..base.clone()
        })
        .contains("value"));
        assert!(bad(BackgroundUpdate {
            kind: BackgroundKind::Gradient,
            ..base.clone()
        })
        .contains("gradient"));
        assert!(bad(BackgroundUpdate {
            kind: BackgroundKind::Image,
            value: Some(json!("")),
            ..base.clone()
        })
        .contains("image"));
        let g = l
            .set_background(&BackgroundUpdate {
                kind: BackgroundKind::Gradient,
                value: Some(json!(["#000", "#fff"])),
                ..base.clone()
            })
            .unwrap();
        assert_eq!(g.background.value, json!(["#000000", "#ffffff"]));
        // Off clears the value but keeps padding settings for later.
        let off = ok
            .set_background(&BackgroundUpdate {
                kind: BackgroundKind::None,
                value: None,
                padding_pct: None,
                radius_px: None,
                shadow: None,
            })
            .unwrap();
        assert_eq!(off.background.kind, BackgroundKind::None);
        assert_eq!(off.background.padding_pct, 8.0);
    }

    /// The stored shape round-trips and keeps unknown keys such as `layout`.
    #[test]
    fn json_round_trip_keeps_layout() {
        let c = cx(60.0);
        let mut l = EditList::full(&c)
            .cut(&[span(5.0, 6.0)])
            .unwrap()
            .set_speed(span(10.0, 20.0), 2.0, true)
            .unwrap();
        l.rest
            .insert("layout".into(), json!({"camera": {"visible": true}}));
        let back = EditList::from_json(&l.to_json(), &c);
        assert_eq!(back, l);
        assert_eq!(l.to_json()["layout"]["camera"]["visible"], true);
        assert_eq!(l.to_json()["version"], 2);
    }

    /// The recorder's original entry migrates: `out` is file time and `in` becomes `live_at`.
    #[test]
    fn legacy_entry_migrates_through_the_gaps() {
        let gaps = [PauseGap {
            from: t(10.0),
            to: t(14.0),
        }];
        let cx = EditContext {
            end: t(34.0),
            live_at: Some(t(0.3)),
            gaps: &gaps,
        };
        // 30 s of file time plus a 4 s pause = 34 s of session time.
        let l = EditList::from_json(
            &json!({"in": 0, "out": t(30.0).ticks(), "speed": 1.0, "layout": {"x": 1}}),
            &cx,
        );
        assert_eq!(l.end, t(34.0));
        assert_eq!(l.start, t(0.3));
        assert_eq!(l.rest["layout"]["x"], 1);
        assert!(l.cuts.is_empty());
    }

    /// EDT-1 / REC-4: pauses are not in the media, so the exported and reported duration drops
    /// them; gaps merge with user cuts and the stored list is untouched.
    #[test]
    fn without_pauses_removes_gaps_from_the_output() {
        let gaps = [
            PauseGap {
                from: t(10.0),
                to: t(14.0),
            },
            PauseGap {
                from: t(30.0),
                to: t(31.0),
            },
        ];
        let cx = EditContext {
            end: t(60.0),
            live_at: None,
            gaps: &gaps,
        };
        let l = EditList::full(&cx)
            .cut(&[Span::new(t(12.0), t(20.0))])
            .unwrap();
        assert_eq!(secs(l.timeline().duration()), 52.0);
        let v = l.without_pauses(&gaps);
        // 60 s - (10..20 merged cut) - 1 s pause = 49 s.
        assert_eq!(secs(v.timeline().duration()), 49.0);
        assert_eq!(v.cuts.len(), 2);
        assert_eq!(l.cuts.len(), 1, "the stored list is not changed");
    }

    /// Mapping into track files: pauses are removed and file 0 is the track's first sample.
    #[test]
    fn track_time_removes_gaps_and_starts_at_the_first_sample() {
        let gaps = [PauseGap {
            from: t(10.0),
            to: t(14.0),
        }];
        assert_eq!(session_to_file(&gaps, t(5.0)), t(5.0));
        assert_eq!(session_to_file(&gaps, t(12.0)), t(10.0));
        assert_eq!(session_to_file(&gaps, t(20.0)), t(16.0));
        assert_eq!(track_time(&gaps, t(0.3), t(20.0)), Some(t(15.7)));
        assert_eq!(track_time(&gaps, t(0.3), t(0.1)), None);
    }

    #[test]
    fn parse_time_accepts_seconds_and_ticks() {
        assert_eq!(parse_time("at", &json!(1.5)).unwrap(), t(1.5));
        assert_eq!(parse_time("at", &json!("t:120000")).unwrap(), t(1.0));
        assert!(parse_time("at", &json!("1.5")).is_err());
        assert!(parse_time("at", &json!(null)).is_err());
    }
}
