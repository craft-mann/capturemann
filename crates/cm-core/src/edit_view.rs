//! What the viewer sees of the screen: the editable cursor and zoom regions (EDT-4) and the
//! reframe window of a vertical or square export (EDT-5). Plain data and validation; the pure
//! camera math that turns it into a crop per frame is `cm_cursor::view`, and the compositor only
//! draws the result.
//!
//! Stored in `manifest.edits[0]` next to the rest of the edit list ([`crate::edit`]); every time is
//! **session time** in ticks and every position is a fraction of the recorded screen frame
//! (`0.0..=1.0`, y down).
//!
//! ```json
//! {
//!   "cursor": {"size": 2.0, "smoothing": "light", "click_effect": "ripple", "hide_idle": true},
//!   "zooms": [{"at": 1800000, "length": 360000, "focus": [0.5, 0.4], "scale": 1.8,
//!              "follow_cursor": false}],
//!   "reframe": {"9:16": {"mode": "fixed", "windows": [{"from": 0, "focus": [0.3, 0.5]}]}}
//! }
//! ```
//!
//! A key is written only when it differs from the default, so an untouched recording keeps its
//! stored JSON byte for byte.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::edit::Span;
use crate::{CmError, MediaTime};

/// Zoom scale range (EDT-4: 1.2x to 3x).
pub const ZOOM_SCALE_RANGE: (f64, f64) = (1.2, 3.0);
/// Scale of a new zoom.
pub const ZOOM_DEFAULT_SCALE: f64 = 1.8;
/// Easing in and out of a zoom (EDT-4: 400 ms each).
pub const ZOOM_EASE: MediaTime = MediaTime::from_millis(400);
/// The shortest zoom region; a shorter one has no room for both eases.
pub const ZOOM_MIN_LENGTH: MediaTime = MediaTime::from_millis(800);
/// Zoom regions per recording.
pub const MAX_ZOOMS: usize = 256;
/// Cursor size range as a multiple of the recorded cursor (EDT-4: 1x to 3x).
pub const CURSOR_SIZE_RANGE: (f64, f64) = (1.0, 3.0);
/// "Hide when idle": the cursor fades out after this long without movement or clicks.
pub const IDLE_HIDE: MediaTime = MediaTime::from_millis(2000);
/// Reframe windows per aspect (one per segment).
pub const MAX_REFRAME_WINDOWS: usize = 64;

/// Spring smoothing of the telemetry path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Smoothing {
    #[default]
    Off,
    Light,
    Strong,
}

impl Smoothing {
    pub fn as_str(self) -> &'static str {
        match self {
            Smoothing::Off => "off",
            Smoothing::Light => "light",
            Smoothing::Strong => "strong",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CmError> {
        match s {
            "off" => Ok(Smoothing::Off),
            "light" => Ok(Smoothing::Light),
            "strong" => Ok(Smoothing::Strong),
            _ => Err(CmError::invalid_field(
                "smoothing",
                "must be off, light or strong",
            )),
        }
    }
}

/// What a click looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClickEffect {
    #[default]
    None,
    Ring,
    Ripple,
}

impl ClickEffect {
    pub fn as_str(self) -> &'static str {
        match self {
            ClickEffect::None => "none",
            ClickEffect::Ring => "ring",
            ClickEffect::Ripple => "ripple",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CmError> {
        match s {
            "none" => Ok(ClickEffect::None),
            "ring" => Ok(ClickEffect::Ring),
            "ripple" => Ok(ClickEffect::Ripple),
            _ => Err(CmError::invalid_field(
                "click_effect",
                "must be none, ring or ripple",
            )),
        }
    }
}

/// The look of the telemetry cursor drawn over a recording made with "Record cursor separately".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorStyle {
    pub size: f64,
    pub smoothing: Smoothing,
    pub click_effect: ClickEffect,
    pub hide_idle: bool,
}

impl Default for CursorStyle {
    fn default() -> Self {
        CursorStyle {
            size: 1.0,
            smoothing: Smoothing::Off,
            click_effect: ClickEffect::None,
            hide_idle: false,
        }
    }
}

/// A partial change of the cursor style: fields left `None` keep their value.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CursorStyleUpdate {
    pub size: Option<f64>,
    pub smoothing: Option<Smoothing>,
    pub click_effect: Option<ClickEffect>,
    pub hide_idle: Option<bool>,
}

impl CursorStyleUpdate {
    pub fn is_empty(&self) -> bool {
        *self == CursorStyleUpdate::default()
    }
}

impl CursorStyle {
    pub fn updated(&self, u: &CursorStyleUpdate) -> Result<CursorStyle, CmError> {
        if u.is_empty() {
            return Err(CmError::invalid_input(
                "give at least one of size, smoothing, click_effect, hide_idle",
            ));
        }
        let mut s = *self;
        if let Some(size) = u.size {
            if !size.is_finite() || size < CURSOR_SIZE_RANGE.0 || size > CURSOR_SIZE_RANGE.1 {
                return Err(CmError::invalid_field(
                    "size",
                    format!(
                        "must be between {} and {}",
                        CURSOR_SIZE_RANGE.0, CURSOR_SIZE_RANGE.1
                    ),
                ));
            }
            s.size = size;
        }
        if let Some(v) = u.smoothing {
            s.smoothing = v;
        }
        if let Some(v) = u.click_effect {
            s.click_effect = v;
        }
        if let Some(v) = u.hide_idle {
            s.hide_idle = v;
        }
        Ok(s)
    }

    fn to_json(self) -> Value {
        json!({
            "size": self.size,
            "smoothing": self.smoothing.as_str(),
            "click_effect": self.click_effect.as_str(),
            "hide_idle": self.hide_idle,
        })
    }

    fn from_json(v: &Value) -> CursorStyle {
        let d = CursorStyle::default();
        let Some(o) = v.as_object() else {
            return d;
        };
        CursorStyle {
            size: o
                .get("size")
                .and_then(Value::as_f64)
                .filter(|s| s.is_finite() && *s >= CURSOR_SIZE_RANGE.0 && *s <= CURSOR_SIZE_RANGE.1)
                .unwrap_or(d.size),
            smoothing: o
                .get("smoothing")
                .and_then(Value::as_str)
                .and_then(|s| Smoothing::parse(s).ok())
                .unwrap_or(d.smoothing),
            click_effect: o
                .get("click_effect")
                .and_then(Value::as_str)
                .and_then(|s| ClickEffect::parse(s).ok())
                .unwrap_or(d.click_effect),
            hide_idle: o
                .get("hide_idle")
                .and_then(Value::as_bool)
                .unwrap_or(d.hide_idle),
        }
    }
}

/// One zoom region on the zoom lane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoomRegion {
    /// Session time the region starts (the ease in starts here).
    pub at: MediaTime,
    pub length: MediaTime,
    /// The point kept in the middle of the view, as fractions of the frame.
    pub focus: (f64, f64),
    pub scale: f64,
    /// Keep the focus on the cursor path while zoomed (dead zone, EDT-4.AC3).
    pub follow_cursor: bool,
}

impl ZoomRegion {
    pub fn span(&self) -> Span {
        Span::new(self.at, self.at + self.length)
    }

    pub fn end(&self) -> MediaTime {
        self.at + self.length
    }

    /// Checks the fields of one region (a region comes from a user or an agent).
    pub fn validate(&self, field: &str) -> Result<(), CmError> {
        let f = |s: &str| format!("{field}.{s}");
        if self.at < MediaTime::ZERO {
            return Err(CmError::invalid_field(&f("at"), "must not be negative"));
        }
        if self.length < ZOOM_MIN_LENGTH {
            return Err(CmError::invalid_field(
                &f("length"),
                "a zoom lasts at least 0.8 s",
            ));
        }
        if !(self.scale.is_finite()
            && self.scale >= ZOOM_SCALE_RANGE.0
            && self.scale <= ZOOM_SCALE_RANGE.1)
        {
            return Err(CmError::invalid_field(
                &f("scale"),
                format!(
                    "must be between {} and {}",
                    ZOOM_SCALE_RANGE.0, ZOOM_SCALE_RANGE.1
                ),
            ));
        }
        check_focus(&f("focus"), self.focus)
    }

    fn to_json(self) -> Value {
        json!({
            "at": self.at.ticks(),
            "length": self.length.ticks(),
            "focus": [self.focus.0, self.focus.1],
            "scale": self.scale,
            "follow_cursor": self.follow_cursor,
        })
    }

    fn from_json(v: &Value) -> Option<ZoomRegion> {
        let o = v.as_object()?;
        let focus = o.get("focus").and_then(Value::as_array)?;
        let z = ZoomRegion {
            at: MediaTime::from_ticks(o.get("at")?.as_i64()?),
            length: MediaTime::from_ticks(o.get("length")?.as_i64()?),
            focus: (focus.first()?.as_f64()?, focus.get(1)?.as_f64()?),
            scale: o
                .get("scale")
                .and_then(Value::as_f64)
                .unwrap_or(ZOOM_DEFAULT_SCALE),
            follow_cursor: o
                .get("follow_cursor")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
        z.validate("zoom").ok().map(|()| z)
    }
}

fn check_focus(field: &str, focus: (f64, f64)) -> Result<(), CmError> {
    let ok = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
    if ok(focus.0) && ok(focus.1) {
        Ok(())
    } else {
        Err(CmError::invalid_field(
            field,
            "must be two fractions of the frame between 0 and 1",
        ))
    }
}

/// The output aspects EDT-5 reframes to. The stored settings are per aspect, so the 16:9 edit
/// stays untouched (EDT-5.AC3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Aspect {
    /// 9:16
    Vertical,
    /// 1:1
    Square,
}

impl Aspect {
    pub fn as_str(self) -> &'static str {
        match self {
            Aspect::Vertical => "9:16",
            Aspect::Square => "1:1",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CmError> {
        match s {
            "9:16" => Ok(Aspect::Vertical),
            "1:1" => Ok(Aspect::Square),
            _ => Err(CmError::invalid_field("aspect", "must be 9:16 or 1:1")),
        }
    }

    /// Width over height.
    pub fn ratio(self) -> f64 {
        match self {
            Aspect::Vertical => 9.0 / 16.0,
            Aspect::Square => 1.0,
        }
    }

    /// The reframe aspect of an output frame, if it is one (within 1%).
    pub fn of_frame(width: u32, height: u32) -> Option<Aspect> {
        if width == 0 || height == 0 {
            return None;
        }
        let r = f64::from(width) / f64::from(height);
        [Aspect::Vertical, Aspect::Square]
            .into_iter()
            .find(|a| (r / a.ratio() - 1.0).abs() < 0.01)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReframeMode {
    /// The whole screen letterboxed on the background (the default for every export).
    #[default]
    Fit,
    /// A crop window the user positioned (per segment).
    Fixed,
    /// The crop window follows the cursor with the dead-zone smoothing of EDT-4.
    Follow,
}

impl ReframeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ReframeMode::Fit => "fit",
            ReframeMode::Fixed => "fixed",
            ReframeMode::Follow => "follow",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CmError> {
        match s {
            "fit" => Ok(ReframeMode::Fit),
            "fixed" => Ok(ReframeMode::Fixed),
            // `follow_cursor` is the spec's name for it.
            "follow" | "follow_cursor" => Ok(ReframeMode::Follow),
            _ => Err(CmError::invalid_field(
                "mode",
                "must be fit, fixed or follow",
            )),
        }
    }
}

/// The center of the crop window from `from` (session time) on, until the next window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReframeWindow {
    pub from: MediaTime,
    pub focus: (f64, f64),
}

/// Reframe settings of one aspect.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reframe {
    pub mode: ReframeMode,
    /// Sorted by `from`; the first applies from the start. Empty means centered.
    pub windows: Vec<ReframeWindow>,
}

impl Reframe {
    /// The window center at session time `t` (centered without a window).
    pub fn focus_at(&self, t: MediaTime) -> (f64, f64) {
        self.windows
            .iter()
            .rev()
            .find(|w| w.from <= t)
            .or_else(|| self.windows.first())
            .map_or((0.5, 0.5), |w| w.focus)
    }

    fn to_json(&self) -> Value {
        json!({
            "mode": self.mode.as_str(),
            "windows": self.windows.iter().map(|w| json!({
                "from": w.from.ticks(),
                "focus": [w.focus.0, w.focus.1],
            })).collect::<Vec<_>>(),
        })
    }

    fn from_json(v: &Value) -> Reframe {
        let Some(o) = v.as_object() else {
            return Reframe::default();
        };
        let mut windows: Vec<ReframeWindow> = o
            .get("windows")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|w| {
                        let f = w.get("focus")?.as_array()?;
                        let focus = (f.first()?.as_f64()?, f.get(1)?.as_f64()?);
                        check_focus("focus", focus).ok()?;
                        Some(ReframeWindow {
                            from: MediaTime::from_ticks(w.get("from")?.as_i64()?),
                            focus,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        windows.sort_by_key(|w| w.from);
        windows.truncate(MAX_REFRAME_WINDOWS);
        Reframe {
            mode: o
                .get("mode")
                .and_then(Value::as_str)
                .and_then(|m| ReframeMode::parse(m).ok())
                .unwrap_or_default(),
            windows,
        }
    }
}

/// A change of one aspect's reframe settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReframeUpdate {
    pub aspect: Aspect,
    pub mode: Option<ReframeMode>,
    /// The new window center.
    pub focus: Option<(f64, f64)>,
    /// The session time of the segment the focus applies to; `None` means the first segment.
    pub at: Option<MediaTime>,
}

/// Everything EDT-4 and EDT-5 store.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewEdits {
    pub cursor: CursorStyle,
    pub zooms: Vec<ZoomRegion>,
    pub reframes: BTreeMap<Aspect, Reframe>,
}

impl ViewEdits {
    /// True when nothing here changes the picture or the cursor.
    pub fn is_default(&self) -> bool {
        self.cursor == CursorStyle::default()
            && self.zooms.is_empty()
            && self.reframes.values().all(|r| r.mode == ReframeMode::Fit)
    }

    /// The reframe that applies to an output frame of this size, if it crops.
    pub fn reframe_for_frame(&self, width: u32, height: u32) -> Option<(Aspect, &Reframe)> {
        let a = Aspect::of_frame(width, height)?;
        self.reframes
            .get(&a)
            .filter(|r| r.mode != ReframeMode::Fit)
            .map(|r| (a, r))
    }

    pub(crate) fn write_json(&self, o: &mut Map<String, Value>) {
        if self.cursor != CursorStyle::default() {
            o.insert("cursor".into(), self.cursor.to_json());
        }
        if !self.zooms.is_empty() {
            o.insert(
                "zooms".into(),
                Value::Array(self.zooms.iter().map(|z| z.to_json()).collect()),
            );
        }
        if !self.reframes.is_empty() {
            let m: Map<String, Value> = self
                .reframes
                .iter()
                .map(|(a, r)| (a.as_str().to_owned(), r.to_json()))
                .collect();
            o.insert("reframe".into(), Value::Object(m));
        }
    }

    pub(crate) fn read_json(o: &Map<String, Value>) -> ViewEdits {
        let mut zooms: Vec<ZoomRegion> = o
            .get("zooms")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(ZoomRegion::from_json).collect())
            .unwrap_or_default();
        zooms.sort_by_key(|z| z.at);
        zooms.truncate(MAX_ZOOMS);
        ViewEdits {
            cursor: o
                .get("cursor")
                .map(CursorStyle::from_json)
                .unwrap_or_default(),
            zooms,
            reframes: o
                .get("reframe")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| Some((Aspect::parse(k).ok()?, Reframe::from_json(v))))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Keeps every zoom inside the kept range `lo..hi`; drops those that no longer fit.
    pub(crate) fn clamp_to(&mut self, lo: MediaTime, hi: MediaTime) {
        self.zooms = self
            .zooms
            .iter()
            .filter_map(|z| {
                let s = z.span().clamp_to(lo, hi);
                (s.len() >= ZOOM_MIN_LENGTH).then_some(ZoomRegion {
                    at: s.from,
                    length: s.len(),
                    ..*z
                })
            })
            .collect();
        for r in self.reframes.values_mut() {
            r.windows.retain(|w| w.from < hi);
        }
    }

    /// EDT-4: adds `new` zooms (or replaces all with them). Each is clipped to the kept range
    /// and out of the cuts; an existing region is shortened where a new one overlaps it.
    pub(crate) fn set_zooms(
        &self,
        new: &[ZoomRegion],
        replace: bool,
        lo: MediaTime,
        hi: MediaTime,
        cuts: &[Span],
    ) -> Result<ViewEdits, CmError> {
        for (i, z) in new.iter().enumerate() {
            z.validate(&format!("zooms[{i}]"))?;
        }
        let mut incoming: Vec<ZoomRegion> = Vec::with_capacity(new.len());
        for (i, z) in new.iter().enumerate() {
            let mut s = z.span().clamp_to(lo, hi);
            // "Zoom over a cut range: clipped to the remaining content."
            for c in cuts {
                if s.from >= c.from && s.from < c.to {
                    s.from = c.to;
                }
                if s.to > c.from && s.to <= c.to {
                    s.to = c.from;
                }
            }
            if s.len() < ZOOM_MIN_LENGTH {
                return Err(CmError::invalid_field(
                    &format!("zooms[{i}]"),
                    "lies outside the kept part of the recording",
                ));
            }
            incoming.push(ZoomRegion {
                at: s.from,
                length: s.len(),
                ..*z
            });
        }
        // Later entries win over earlier ones of the same call.
        let mut kept: Vec<ZoomRegion> = if replace {
            Vec::new()
        } else {
            self.zooms.clone()
        };
        for z in incoming {
            let hole = z.span();
            kept = kept
                .into_iter()
                .flat_map(|e| subtract_zoom(e, hole))
                .collect();
            kept.push(z);
        }
        kept.sort_by_key(|z| z.at);
        if kept.len() > MAX_ZOOMS {
            return Err(CmError::invalid_input(format!(
                "at most {MAX_ZOOMS} zoom regions"
            )));
        }
        Ok(ViewEdits {
            zooms: kept,
            ..self.clone()
        })
    }

    /// EDT-5: changes one aspect's mode and/or the focus of one segment.
    pub(crate) fn set_reframe(
        &self,
        u: &ReframeUpdate,
        lo: MediaTime,
        hi: MediaTime,
        splits: &[MediaTime],
    ) -> Result<ViewEdits, CmError> {
        if u.mode.is_none() && u.focus.is_none() {
            return Err(CmError::invalid_input("give at least one of mode, focus"));
        }
        let mut out = self.clone();
        let r = out.reframes.entry(u.aspect).or_default();
        if let Some(m) = u.mode {
            r.mode = m;
        }
        if let Some(f) = u.focus {
            check_focus("focus", f)?;
            let at = u.at.unwrap_or(lo).clamp(lo, hi);
            // The segment containing `at` starts at the last boundary before it.
            let from = std::iter::once(lo)
                .chain(splits.iter().copied())
                .filter(|b| *b <= at)
                .max()
                .unwrap_or(lo);
            match r.windows.iter_mut().find(|w| w.from == from) {
                Some(w) => w.focus = f,
                None => {
                    r.windows.push(ReframeWindow { from, focus: f });
                    r.windows.sort_by_key(|w| w.from);
                }
            }
            if r.windows.len() > MAX_REFRAME_WINDOWS {
                return Err(CmError::invalid_input("too many reframe windows"));
            }
        }
        Ok(out)
    }
}

/// `z` minus `hole`: up to two pieces, each kept when long enough.
fn subtract_zoom(z: ZoomRegion, hole: Span) -> Vec<ZoomRegion> {
    let s = z.span();
    if hole.to <= s.from || hole.from >= s.to {
        return vec![z];
    }
    let mut out = Vec::new();
    for (from, to) in [(s.from, hole.from.min(s.to)), (hole.to.max(s.from), s.to)] {
        if to - from >= ZOOM_MIN_LENGTH {
            out.push(ZoomRegion {
                at: from,
                length: to - from,
                ..z
            });
        }
    }
    out
}

impl Span {
    fn clamp_to(self, lo: MediaTime, hi: MediaTime) -> Span {
        Span::new(self.from.max(lo), self.to.min(hi))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: f64) -> MediaTime {
        MediaTime::from_ticks((secs * crate::TICKS_PER_SECOND as f64) as i64)
    }

    fn zoom(at: f64, len: f64) -> ZoomRegion {
        ZoomRegion {
            at: t(at),
            length: t(len),
            focus: (0.5, 0.5),
            scale: 1.8,
            follow_cursor: false,
        }
    }

    #[test]
    fn edt4_cursor_size_is_range_checked() {
        let s = CursorStyle::default();
        let u = CursorStyleUpdate {
            size: Some(2.0),
            ..Default::default()
        };
        assert_eq!(s.updated(&u).unwrap().size, 2.0);
        for bad in [0.5, 3.5, f64::NAN] {
            let u = CursorStyleUpdate {
                size: Some(bad),
                ..Default::default()
            };
            assert!(s.updated(&u).is_err());
        }
        assert!(s.updated(&CursorStyleUpdate::default()).is_err());
    }

    #[test]
    fn edt4_zoom_fields_are_validated() {
        assert!(zoom(1.0, 3.0).validate("z").is_ok());
        assert!(zoom(1.0, 0.2).validate("z").is_err());
        let mut z = zoom(1.0, 3.0);
        z.scale = 5.0;
        assert!(z.validate("z").is_err());
        z.scale = 1.8;
        z.focus = (1.5, 0.5);
        assert!(z.validate("z").is_err());
    }

    #[test]
    fn edt4_a_new_zoom_shortens_its_neighbour() {
        let v = ViewEdits::default();
        let (lo, hi) = (t(0.0), t(60.0));
        let v = v
            .set_zooms(&[zoom(10.0, 10.0)], false, lo, hi, &[])
            .unwrap();
        let v = v
            .set_zooms(&[zoom(15.0, 10.0)], false, lo, hi, &[])
            .unwrap();
        assert_eq!(v.zooms.len(), 2);
        assert_eq!(v.zooms[0].at, t(10.0));
        assert_eq!(
            v.zooms[0].length,
            t(5.0),
            "the first one now ends where the new one starts"
        );
        assert_eq!(v.zooms[1].at, t(15.0));
        // A new zoom in the middle keeps both ends when each is long enough.
        let v = v.set_zooms(&[zoom(17.0, 2.0)], false, lo, hi, &[]).unwrap();
        let spans: Vec<(i64, i64)> = v
            .zooms
            .iter()
            .map(|z| (z.at.ticks() / 120_000, z.end().ticks() / 120_000))
            .collect();
        assert_eq!(spans, vec![(10, 15), (15, 17), (17, 19), (19, 25)]);
        // Replace swaps everything.
        let v = v.set_zooms(&[zoom(1.0, 2.0)], true, lo, hi, &[]).unwrap();
        assert_eq!(v.zooms.len(), 1);
    }

    #[test]
    fn edt4_a_zoom_over_a_cut_is_clipped_to_the_remaining_content() {
        let cuts = [Span::new(t(12.0), t(14.0))];
        let v = ViewEdits::default()
            .set_zooms(&[zoom(10.0, 4.0)], false, t(0.0), t(60.0), &cuts)
            .unwrap();
        assert_eq!(
            v.zooms[0].end(),
            t(12.0),
            "its end moved to the start of the cut"
        );
        let v = ViewEdits::default()
            .set_zooms(&[zoom(13.0, 4.0)], false, t(0.0), t(60.0), &cuts)
            .unwrap();
        assert_eq!(
            v.zooms[0].at,
            t(14.0),
            "its start moved to the end of the cut"
        );
        // Outside the kept range there is nothing left to zoom.
        assert!(ViewEdits::default()
            .set_zooms(&[zoom(70.0, 4.0)], false, t(0.0), t(60.0), &[])
            .is_err());
    }

    #[test]
    fn edt5_reframe_windows_are_per_segment_and_per_aspect() {
        let (lo, hi) = (t(0.0), t(60.0));
        let splits = [t(20.0), t(40.0)];
        let u = |aspect, at: Option<f64>, x| ReframeUpdate {
            aspect,
            mode: Some(ReframeMode::Fixed),
            focus: Some((x, 0.5)),
            at: at.map(t),
        };
        let v = ViewEdits::default()
            .set_reframe(&u(Aspect::Vertical, None, 0.2), lo, hi, &splits)
            .unwrap();
        let v = v
            .set_reframe(&u(Aspect::Vertical, Some(25.0), 0.8), lo, hi, &splits)
            .unwrap();
        let r = &v.reframes[&Aspect::Vertical];
        assert_eq!(r.windows.len(), 2);
        assert_eq!(r.focus_at(t(5.0)), (0.2, 0.5));
        assert_eq!(r.focus_at(t(30.0)), (0.8, 0.5));
        assert_eq!(r.focus_at(t(50.0)), (0.8, 0.5), "the last window holds");
        assert!(
            !v.reframes.contains_key(&Aspect::Square),
            "EDT-5.AC3: other aspects untouched"
        );
        assert!(v.reframe_for_frame(1080, 1920).is_some());
        assert!(v.reframe_for_frame(1080, 1080).is_none());
        assert!(v.reframe_for_frame(1920, 1080).is_none());
        // A focus outside the frame is refused.
        let mut bad = u(Aspect::Vertical, None, 1.5);
        bad.focus = Some((1.5, 0.5));
        assert!(v.set_reframe(&bad, lo, hi, &splits).is_err());
    }

    #[test]
    fn edt5_follow_cursor_is_a_synonym_of_follow() {
        assert_eq!(
            ReframeMode::parse("follow_cursor").unwrap(),
            ReframeMode::Follow
        );
        assert_eq!(ReframeMode::parse("follow").unwrap(), ReframeMode::Follow);
        assert_eq!(
            ReframeMode::Follow.as_str(),
            "follow",
            "stored under one name"
        );
        assert!(ReframeMode::parse("chase").is_err());
    }

    #[test]
    fn the_json_round_trips_and_a_default_writes_nothing() {
        let mut o = Map::new();
        ViewEdits::default().write_json(&mut o);
        assert!(o.is_empty());
        let mut v = ViewEdits {
            cursor: CursorStyle {
                size: 2.0,
                smoothing: Smoothing::Light,
                click_effect: ClickEffect::Ripple,
                hide_idle: true,
            },
            zooms: vec![zoom(2.0, 3.0)],
            ..Default::default()
        };
        v.reframes.insert(
            Aspect::Square,
            Reframe {
                mode: ReframeMode::Follow,
                windows: vec![ReframeWindow {
                    from: t(0.0),
                    focus: (0.4, 0.6),
                }],
            },
        );
        let mut o = Map::new();
        v.write_json(&mut o);
        assert_eq!(ViewEdits::read_json(&o), v);
    }

    #[test]
    fn malformed_stored_values_fall_back_to_defaults() {
        let o = json!({
            "cursor": {"size": 99, "smoothing": "wobbly"},
            "zooms": [{"at": 0, "length": 5, "focus": [0.5, 0.5]}, 7],
            "reframe": {"4:3": {}, "9:16": {"mode": "nope", "windows": [{"from": 0, "focus": [3, 3]}]}},
        });
        let v = ViewEdits::read_json(o.as_object().unwrap());
        assert_eq!(v.cursor, CursorStyle::default());
        assert!(v.zooms.is_empty());
        let r = &v.reframes[&Aspect::Vertical];
        assert_eq!(r.mode, ReframeMode::Fit);
        assert!(r.windows.is_empty());
    }

    #[test]
    fn clamping_to_the_kept_range_drops_zooms_that_no_longer_fit() {
        let mut v = ViewEdits {
            zooms: vec![zoom(2.0, 4.0), zoom(30.0, 4.0)],
            ..Default::default()
        };
        v.clamp_to(t(0.0), t(31.0));
        assert_eq!(
            v.zooms.len(),
            2,
            "the second keeps 1 s, enough for both eases"
        );
        assert_eq!(v.zooms[1].length, t(1.0));
        assert_eq!(v.zooms[0].at, t(2.0));
        let mut w = ViewEdits {
            zooms: vec![zoom(30.0, 4.0)],
            ..Default::default()
        };
        w.clamp_to(t(0.0), t(30.5));
        assert!(w.zooms.is_empty());
    }
}
