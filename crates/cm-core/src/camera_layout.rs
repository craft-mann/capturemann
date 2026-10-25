//! CAM-3: the camera layout of a recording per time range, stored in `edits[0].layout.segments`.
//!
//! The recorder writes the *layout at stop* to `edits[0].layout.camera` (the bubble: shape, rect,
//! ring; see `cm-media`'s `layout` module). That stays the default for the whole recording. CAM-3
//! adds ranges that override it:
//!
//! ```json
//! "layout": {
//!   "camera": { "visible": true, "shape": "circle", "rect": [0.78, 0.72, 0.16, 0.28], ... },
//!   "segments": [
//!     { "from": 0, "to": 1800000, "mode": "camera", "transition": "cut" },
//!     { "from": 1800000, "to": 3600000, "mode": "bubble", "shape": "rounded",
//!       "rect": [0.05, 0.05, 0.2, 0.2], "ring": 0.0167, "transition": "smooth" }
//!   ]
//! }
//! ```
//!
//! Times are session ticks like every edit time. Segments are sorted and do not overlap; a range
//! with no segment uses the default bubble. `shape`, `rect`, `ring` and `mirror` are optional: a
//! missing one inherits the recorder's layout. A segment with `"transition": "smooth"` moves from
//! the layout before it (the previous adjacent segment, else the default) over
//! [`TRANSITION`] at its start; `"cut"` (the default) switches at once.

use serde_json::{json, Map, Value};

use crate::edit::{EditList, Span};
use crate::{CmError, MediaTime};

/// Length of a smooth layout change (CAM-3.AC3).
pub const TRANSITION: MediaTime = MediaTime::from_millis(300);
/// Smallest and largest bubble width as a fraction of the frame width.
pub const BUBBLE_WIDTH_RANGE: (f64, f64) = (0.05, 1.0);
/// Largest ring width as a fraction of the bubble width.
pub const RING_MAX: f64 = 0.5;
/// The `rec` ring of the recorder's bubble (4 pt of a 240 pt bubble), as a fraction of its width.
pub const DEFAULT_RING: f64 = 4.0 / 240.0;

/// How the screen and the camera share the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraMode {
    /// Screen with the camera bubble on top (the default).
    Bubble,
    /// Screen only: the camera is hidden.
    Screen,
    /// The camera fills the frame.
    Camera,
    /// Screen on two thirds of the width, camera on one third.
    SideBySide,
}

impl CameraMode {
    pub fn as_str(self) -> &'static str {
        match self {
            CameraMode::Bubble => "bubble",
            CameraMode::Screen => "screen",
            CameraMode::Camera => "camera",
            CameraMode::SideBySide => "side_by_side",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CmError> {
        match s {
            "bubble" => Ok(CameraMode::Bubble),
            "screen" => Ok(CameraMode::Screen),
            "camera" => Ok(CameraMode::Camera),
            "side_by_side" => Ok(CameraMode::SideBySide),
            _ => Err(CmError::invalid_field(
                "layout",
                "must be bubble, screen, camera or side_by_side",
            )),
        }
    }

    /// The camera picture is drawn in this mode.
    pub fn shows_camera(self) -> bool {
        !matches!(self, CameraMode::Screen)
    }
}

/// Bubble shape (CAM-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleKind {
    Circle,
    Rounded,
}

impl BubbleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BubbleKind::Circle => "circle",
            BubbleKind::Rounded => "rounded",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CmError> {
        match s {
            "circle" => Ok(BubbleKind::Circle),
            "rounded" => Ok(BubbleKind::Rounded),
            _ => Err(CmError::invalid_field("shape", "must be circle or rounded")),
        }
    }
}

/// Whether a layout change moves smoothly or switches at once (CAM-3.AC3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutTransition {
    #[default]
    Cut,
    Smooth,
}

impl LayoutTransition {
    pub fn as_str(self) -> &'static str {
        match self {
            LayoutTransition::Cut => "cut",
            LayoutTransition::Smooth => "smooth",
        }
    }

    pub fn parse(s: &str) -> Result<Self, CmError> {
        match s {
            "cut" => Ok(LayoutTransition::Cut),
            "smooth" => Ok(LayoutTransition::Smooth),
            _ => Err(CmError::invalid_field(
                "transition",
                "must be cut or smooth",
            )),
        }
    }
}

/// What a layout range sets. Bubble fields left `None` inherit the recorder's layout.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraLayoutSpec {
    pub mode: CameraMode,
    pub shape: Option<BubbleKind>,
    /// `[x, y, w, h]` as fractions of the output frame; `w` is authoritative (the shape fixes the
    /// aspect), the renderer clamps the bubble inside the frame (CAM-3.AC4).
    pub rect: Option<[f64; 4]>,
    /// Ring width as a fraction of the bubble width; `0` hides the ring.
    pub ring: Option<f64>,
    pub mirror: Option<bool>,
    pub transition: LayoutTransition,
}

impl CameraLayoutSpec {
    pub fn new(mode: CameraMode) -> Self {
        CameraLayoutSpec {
            mode,
            shape: None,
            rect: None,
            ring: None,
            mirror: None,
            transition: LayoutTransition::Cut,
        }
    }

    /// Checks the fields. Only a bubble uses `shape`, `rect` and `ring`; they are refused for the
    /// other modes so a stored segment never carries settings that do nothing.
    pub fn validate(&self) -> Result<(), CmError> {
        if self.mode != CameraMode::Bubble {
            for (name, set) in [
                ("shape", self.shape.is_some()),
                ("rect", self.rect.is_some()),
                ("ring", self.ring.is_some()),
            ] {
                if set {
                    return Err(CmError::invalid_field(
                        name,
                        format!(
                            "only applies to the bubble layout, not `{}`",
                            self.mode.as_str()
                        ),
                    ));
                }
            }
        }
        if let Some(r) = self.rect {
            if r.iter().any(|v| !v.is_finite()) {
                return Err(CmError::invalid_field("rect", "must be four numbers"));
            }
            let [x, y, w, h] = r;
            let ok = (0.0..=1.0).contains(&x)
                && (0.0..=1.0).contains(&y)
                && h >= 0.0
                && w >= BUBBLE_WIDTH_RANGE.0
                && w <= BUBBLE_WIDTH_RANGE.1;
            if !ok {
                return Err(CmError::invalid_field(
                    "rect",
                    format!(
                        "must be [x, y, w, h] as fractions of the frame, x and y from 0 to 1 and w from {} to {}",
                        BUBBLE_WIDTH_RANGE.0, BUBBLE_WIDTH_RANGE.1
                    ),
                ));
            }
        }
        if let Some(g) = self.ring {
            if !g.is_finite() || !(0.0..=RING_MAX).contains(&g) {
                return Err(CmError::invalid_field(
                    "ring",
                    format!("must be from 0 to {RING_MAX} (a fraction of the bubble width)"),
                ));
            }
        }
        Ok(())
    }
}

/// One stored range of the layout.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutSegment {
    pub span: Span,
    pub spec: CameraLayoutSpec,
}

impl LayoutSegment {
    fn to_json(&self) -> Value {
        let mut o = Map::new();
        o.insert("from".into(), json!(self.span.from.ticks()));
        o.insert("to".into(), json!(self.span.to.ticks()));
        o.insert("mode".into(), json!(self.spec.mode.as_str()));
        if let Some(s) = self.spec.shape {
            o.insert("shape".into(), json!(s.as_str()));
        }
        if let Some(r) = self.spec.rect {
            o.insert(
                "rect".into(),
                json!(r.map(|v| (v * 100_000.0).round() / 100_000.0)),
            );
        }
        if let Some(g) = self.spec.ring {
            o.insert("ring".into(), json!((g * 100_000.0).round() / 100_000.0));
        }
        if let Some(m) = self.spec.mirror {
            o.insert("mirror".into(), json!(m));
        }
        o.insert("transition".into(), json!(self.spec.transition.as_str()));
        Value::Object(o)
    }

    /// Reads one stored segment; a malformed one is dropped (the masters are intact, a bad edit
    /// list must not make a recording unreadable).
    fn from_json(v: &Value) -> Option<LayoutSegment> {
        let o = v.as_object()?;
        let span = Span::new(
            MediaTime::from_ticks(o.get("from")?.as_i64()?),
            MediaTime::from_ticks(o.get("to")?.as_i64()?),
        );
        if span.is_empty() {
            return None;
        }
        let rect = o.get("rect").and_then(Value::as_array).and_then(|a| {
            let mut r = [0.0; 4];
            if a.len() != 4 {
                return None;
            }
            for (slot, x) in r.iter_mut().zip(a) {
                *slot = x.as_f64().filter(|n| n.is_finite())?;
            }
            Some(r)
        });
        let spec = CameraLayoutSpec {
            mode: CameraMode::parse(o.get("mode")?.as_str()?).ok()?,
            shape: o
                .get("shape")
                .and_then(Value::as_str)
                .and_then(|s| BubbleKind::parse(s).ok()),
            rect,
            ring: o
                .get("ring")
                .and_then(Value::as_f64)
                .filter(|g| g.is_finite()),
            mirror: o.get("mirror").and_then(Value::as_bool),
            transition: o
                .get("transition")
                .and_then(Value::as_str)
                .and_then(|s| LayoutTransition::parse(s).ok())
                .unwrap_or_default(),
        };
        Some(LayoutSegment { span, spec })
    }
}

/// The layout in force at one instant, and the one it is moving away from during a smooth change.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutAt {
    pub current: CameraLayoutSpec,
    /// `Some((previous, f))` while a smooth change is under way: `f` runs from 0 (still the
    /// previous layout) to 1 (arrived). Linear in time; the renderer eases it.
    pub from: Option<(CameraLayoutSpec, f32)>,
}

impl EditList {
    /// The stored layout ranges, sorted, non-overlapping.
    pub fn camera_segments(&self) -> Vec<LayoutSegment> {
        let mut segs: Vec<LayoutSegment> = self
            .rest
            .get("layout")
            .and_then(|l| l.get("segments"))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(LayoutSegment::from_json).collect())
            .unwrap_or_default();
        segs.sort_by_key(|s| (s.span.from, s.span.to));
        // Defensive: stored overlaps (hand edited) are trimmed so lookups are unambiguous.
        let mut out: Vec<LayoutSegment> = Vec::with_capacity(segs.len());
        for mut s in segs {
            if let Some(last) = out.last() {
                s.span.from = s.span.from.max(last.span.to);
            }
            if !s.span.is_empty() {
                out.push(s);
            }
        }
        out
    }

    /// CAM-3: sets the layout over `range` (clamped to the kept part). Existing ranges that overlap
    /// are cut back around it; neighbours with the same settings are joined.
    pub fn set_camera_layout(
        &self,
        range: Span,
        spec: &CameraLayoutSpec,
    ) -> Result<EditList, CmError> {
        spec.validate()?;
        let span = Span::new(range.from.max(self.start), range.to.min(self.end));
        if span.is_empty() {
            return Err(CmError::invalid_field(
                "range",
                "must overlap the kept part of the recording",
            ));
        }
        let mut next: Vec<LayoutSegment> = Vec::new();
        for s in self.camera_segments() {
            if s.span.to <= span.from || s.span.from >= span.to {
                next.push(s);
                continue;
            }
            if s.span.from < span.from {
                next.push(LayoutSegment {
                    span: Span::new(s.span.from, span.from),
                    spec: s.spec.clone(),
                });
            }
            if s.span.to > span.to {
                next.push(LayoutSegment {
                    span: Span::new(span.to, s.span.to),
                    spec: s.spec.clone(),
                });
            }
        }
        next.push(LayoutSegment {
            span,
            spec: spec.clone(),
        });
        next.sort_by_key(|s| s.span.from);
        // Join adjacent ranges that read the same, so repeated edits do not pile up segments.
        let mut joined: Vec<LayoutSegment> = Vec::with_capacity(next.len());
        for s in next {
            match joined.last_mut() {
                Some(last) if last.span.to == s.span.from && last.spec == s.spec => {
                    last.span.to = s.span.to;
                }
                _ => joined.push(s),
            }
        }
        let mut l = self.clone();
        let mut layout = match l.rest.get("layout") {
            Some(Value::Object(o)) => o.clone(),
            _ => Map::new(),
        };
        if joined.is_empty() {
            layout.remove("segments");
        } else {
            layout.insert(
                "segments".into(),
                Value::Array(joined.iter().map(LayoutSegment::to_json).collect()),
            );
        }
        l.rest.insert("layout".into(), Value::Object(layout));
        Ok(l)
    }

    /// The layout in force at session time `t` (CAM-3). Outside every range it is the default
    /// bubble with the recorder's settings.
    pub fn layout_at(&self, t: MediaTime) -> LayoutAt {
        layout_at(&self.camera_segments(), t)
    }
}

/// [`EditList::layout_at`] on an already parsed list (an exporter parses once, then asks per frame).
pub fn layout_at(segments: &[LayoutSegment], t: MediaTime) -> LayoutAt {
    let default = CameraLayoutSpec::new(CameraMode::Bubble);
    let idx = segments
        .iter()
        .position(|s| t >= s.span.from && t < s.span.to);
    let Some(i) = idx else {
        return LayoutAt {
            current: default,
            from: None,
        };
    };
    let seg = &segments[i];
    let since = t - seg.span.from;
    let from = (seg.spec.transition == LayoutTransition::Smooth && since < TRANSITION).then(|| {
        let prev = i
            .checked_sub(1)
            .map(|p| &segments[p])
            .filter(|p| p.span.to == seg.span.from)
            .map_or(default, |p| p.spec.clone());
        let f = since.ticks() as f32 / TRANSITION.ticks() as f32;
        (prev, f.clamp(0.0, 1.0))
    });
    LayoutAt {
        current: seg.spec.clone(),
        from,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::EditContext;

    fn t(s: f64) -> MediaTime {
        MediaTime::from_secs_f64(s).unwrap()
    }

    fn span(a: f64, b: f64) -> Span {
        Span::new(t(a), t(b))
    }

    fn list() -> EditList {
        EditList::full(&EditContext {
            end: t(60.0),
            live_at: None,
            gaps: &[],
        })
    }

    fn mode(m: CameraMode) -> CameraLayoutSpec {
        CameraLayoutSpec::new(m)
    }

    /// CAM-3.AC2: a range set to camera full is found at its times and not outside.
    #[test]
    fn cam3_ac2_range_resolves_by_time() {
        let l = list()
            .set_camera_layout(span(0.0, 15.0), &mode(CameraMode::Camera))
            .unwrap();
        assert_eq!(l.layout_at(t(1.0)).current.mode, CameraMode::Camera);
        assert_eq!(l.layout_at(t(14.99)).current.mode, CameraMode::Camera);
        assert_eq!(l.layout_at(t(15.0)).current.mode, CameraMode::Bubble);
        assert_eq!(l.layout_at(t(40.0)).current.mode, CameraMode::Bubble);
    }

    /// A later range overwrites the overlap and keeps the rest of the older one.
    #[test]
    fn overlapping_ranges_are_cut_back() {
        let l = list()
            .set_camera_layout(span(0.0, 30.0), &mode(CameraMode::Camera))
            .unwrap()
            .set_camera_layout(span(10.0, 20.0), &mode(CameraMode::Screen))
            .unwrap();
        let s = l.camera_segments();
        assert_eq!(s.len(), 3);
        assert_eq!(
            (s[0].spec.mode, s[1].spec.mode, s[2].spec.mode),
            (CameraMode::Camera, CameraMode::Screen, CameraMode::Camera)
        );
        assert_eq!(s[1].span, span(10.0, 20.0));
        // Setting the same layout over the middle joins them again.
        let l = l
            .set_camera_layout(span(10.0, 20.0), &mode(CameraMode::Camera))
            .unwrap();
        assert_eq!(l.camera_segments().len(), 1);
    }

    /// The stored shape round-trips and keeps the recorder's `layout.camera`.
    #[test]
    fn stored_shape_round_trips_and_keeps_the_recorder_layout() {
        let mut l = list();
        l.rest.insert(
            "layout".into(),
            json!({"camera": {"visible": true, "shape": "circle"}}),
        );
        let mut spec = mode(CameraMode::Bubble);
        spec.shape = Some(BubbleKind::Rounded);
        spec.rect = Some([0.05, 0.05, 0.2, 0.2]);
        spec.ring = Some(0.0);
        spec.transition = LayoutTransition::Smooth;
        let l = l.set_camera_layout(span(5.0, 9.0), &spec).unwrap();
        let j = l.to_json();
        assert_eq!(j["layout"]["camera"]["shape"], "circle");
        assert_eq!(j["layout"]["segments"][0]["mode"], "bubble");
        assert_eq!(j["layout"]["segments"][0]["transition"], "smooth");
        let cx = EditContext {
            end: t(60.0),
            live_at: None,
            gaps: &[],
        };
        let back = EditList::from_json(&j, &cx);
        assert_eq!(back.camera_segments(), l.camera_segments());
    }

    /// CAM-3.AC3: a smooth segment moves from the layout before it over 300 ms, then stays.
    #[test]
    fn cam3_ac3_smooth_change_runs_for_300_ms() {
        let mut next = mode(CameraMode::Camera);
        next.transition = LayoutTransition::Smooth;
        let l = list()
            .set_camera_layout(span(0.0, 10.0), &mode(CameraMode::Screen))
            .unwrap()
            .set_camera_layout(span(10.0, 20.0), &next)
            .unwrap();
        assert!(l.layout_at(t(9.9)).from.is_none());
        let a = l.layout_at(t(10.0));
        let (prev, f) = a.from.unwrap();
        assert_eq!(prev.mode, CameraMode::Screen);
        assert!(f.abs() < 1e-6);
        let b = l.layout_at(t(10.15));
        assert!((b.from.unwrap().1 - 0.5).abs() < 0.01);
        assert!(l.layout_at(t(10.3)).from.is_none());
        // After a gap in the ranges the move starts from the default bubble.
        let l = list().set_camera_layout(span(10.0, 20.0), &next).unwrap();
        assert_eq!(
            l.layout_at(t(10.1)).from.unwrap().0.mode,
            CameraMode::Bubble
        );
    }

    #[test]
    fn invalid_specs_name_the_field() {
        let l = list();
        let mut s = mode(CameraMode::Camera);
        s.shape = Some(BubbleKind::Circle);
        assert!(l
            .set_camera_layout(span(0.0, 5.0), &s)
            .unwrap_err()
            .to_string()
            .starts_with("shape"));
        let mut s = mode(CameraMode::Bubble);
        s.rect = Some([0.5, 0.5, 0.01, 0.01]);
        assert!(l
            .set_camera_layout(span(0.0, 5.0), &s)
            .unwrap_err()
            .to_string()
            .starts_with("rect"));
        s.rect = None;
        s.ring = Some(0.9);
        assert!(l
            .set_camera_layout(span(0.0, 5.0), &s)
            .unwrap_err()
            .to_string()
            .starts_with("ring"));
        let e = l
            .set_camera_layout(span(70.0, 80.0), &mode(CameraMode::Screen))
            .unwrap_err();
        assert!(e.to_string().starts_with("range"), "{e}");
    }

    #[test]
    fn a_range_is_clamped_to_the_kept_part() {
        let l = list()
            .set_camera_layout(span(55.0, 90.0), &mode(CameraMode::Camera))
            .unwrap();
        assert_eq!(l.camera_segments()[0].span, span(55.0, 60.0));
    }
}
