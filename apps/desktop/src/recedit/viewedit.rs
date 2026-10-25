//! The editor's EDT-4 and EDT-5 choices, display-free: zoom regions, the cursor style and the
//! reframe window. Like every other edit they are daemon calls (`set_zooms`, `set_cursor_style`,
//! `set_reframe`); this module decides *which* call a button or key asks for, and the model adopts
//! the daemon's answer.

use cm_core::{
    Aspect, ClickEffect, CursorStyle, CursorStyleUpdate, EditList, MediaTime, Reframe, ReframeMode,
    Smoothing, ZoomRegion, CURSOR_SIZE_RANGE, ZOOM_DEFAULT_SCALE, ZOOM_MIN_LENGTH,
    ZOOM_SCALE_RANGE,
};
use serde_json::{json, Value};

use super::model::{EditRequest, EditorModel};
use super::render::preview_size;

/// A new zoom lasts this long when no range is selected.
pub const NEW_ZOOM_SECS: f64 = 3.0;
/// The steppers of the zoom card.
pub const SCALE_STEP: f64 = 0.2;
pub const LENGTH_STEP_SECS: f64 = 0.5;
pub const FOCUS_STEP: f64 = 0.05;
pub const CURSOR_SIZE_STEP: f64 = 0.25;
/// The longest a stepper makes a zoom, seconds.
pub const MAX_ZOOM_SECS: f64 = 60.0;

/// Which output the preview shows. 16:9 (the recording's own shape) is the default; a vertical or
/// square preview applies that aspect's reframe, so the picture is the one the export will have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewAspect {
    #[default]
    Wide,
    Vertical,
    Square,
}

impl PreviewAspect {
    pub const ALL: [PreviewAspect; 3] = [
        PreviewAspect::Wide,
        PreviewAspect::Vertical,
        PreviewAspect::Square,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PreviewAspect::Wide => "16:9",
            PreviewAspect::Vertical => "9:16",
            PreviewAspect::Square => "1:1",
        }
    }

    /// The reframe aspect, `None` for the recording's own shape.
    pub fn reframe_aspect(self) -> Option<Aspect> {
        match self {
            PreviewAspect::Wide => None,
            PreviewAspect::Vertical => Some(Aspect::Vertical),
            PreviewAspect::Square => Some(Aspect::Square),
        }
    }

    /// The preview picture's size for a screen of `source` px: even sides.
    pub fn out_size(self, source: (u32, u32)) -> (u32, u32) {
        match self {
            PreviewAspect::Wide => preview_size(source),
            PreviewAspect::Vertical => (540, 960),
            PreviewAspect::Square => (720, 720),
        }
    }
}

fn ticks(t: MediaTime) -> String {
    format!("t:{}", t.ticks())
}

/// One zoom region as `set_zooms` takes it.
pub fn zoom_value(z: &ZoomRegion) -> Value {
    json!({
        "at": ticks(z.at),
        "length": ticks(z.length),
        "focus": [z.focus.0, z.focus.1],
        "scale": z.scale,
        "follow_cursor": z.follow_cursor,
    })
}

/// What a stepper or switch of the zoom card changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZoomAdjust {
    /// Add to the scale (1.2 to 3).
    Scale(f64),
    /// Add seconds to the length (0.8 to 60, and within the recording).
    Length(f64),
    /// Move the focus by fractions of the frame (kept inside it).
    Focus(f64, f64),
    FollowCursor(bool),
}

/// The zoom with one adjustment applied and every field kept in its range.
pub fn adjusted(z: &ZoomRegion, a: ZoomAdjust, end: MediaTime) -> ZoomRegion {
    let mut n = *z;
    match a {
        ZoomAdjust::Scale(d) => {
            n.scale = (z.scale + d).clamp(ZOOM_SCALE_RANGE.0, ZOOM_SCALE_RANGE.1);
            // Steppers walk in tenths: round away float noise.
            n.scale = (n.scale * 100.0).round() / 100.0;
        }
        ZoomAdjust::Length(d) => {
            let secs = (z.length.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64 + d).clamp(
                ZOOM_MIN_LENGTH.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64,
                MAX_ZOOM_SECS,
            );
            let len =
                MediaTime::from_ticks((secs * cm_core::TICKS_PER_SECOND as f64).round() as i64);
            n.length = len.min(end - z.at).max(ZOOM_MIN_LENGTH);
        }
        ZoomAdjust::Focus(dx, dy) => {
            n.focus = (
                ((z.focus.0 + dx).clamp(0.0, 1.0) * 100.0).round() / 100.0,
                ((z.focus.1 + dy).clamp(0.0, 1.0) * 100.0).round() / 100.0,
            );
        }
        ZoomAdjust::FollowCursor(b) => n.follow_cursor = b,
    }
    n
}

/// A new zoom: over the selected range when there is one, else [`NEW_ZOOM_SECS`] from `at`,
/// both kept inside the recording.
pub fn new_zoom(
    at: MediaTime,
    range: Option<(MediaTime, MediaTime)>,
    end: MediaTime,
) -> ZoomRegion {
    let (from, to) = match range {
        Some((a, b)) if b - a >= ZOOM_MIN_LENGTH => (a, b),
        _ => (
            at,
            at + MediaTime::from_ticks(
                (NEW_ZOOM_SECS * cm_core::TICKS_PER_SECOND as f64).round() as i64
            ),
        ),
    };
    // Slide back so a zoom near the end still fits.
    let to = to.min(end);
    let from = from.min(to - ZOOM_MIN_LENGTH).max(MediaTime::ZERO);
    ZoomRegion {
        at: from,
        length: to - from,
        focus: (0.5, 0.5),
        scale: ZOOM_DEFAULT_SCALE,
        follow_cursor: false,
    }
}

impl EditorModel {
    fn view_req(&self, method: &'static str, mut params: Value) -> EditRequest {
        params["capture_id"] = json!(self.capture_id);
        EditRequest { method, params }
    }

    // ---- preview aspect ------------------------------------------------------------------

    pub fn set_preview_aspect(&mut self, a: PreviewAspect) {
        self.preview_aspect = a;
    }

    // ---- zoom regions (EDT-4) ------------------------------------------------------------

    /// The selected zoom, if it still exists.
    pub fn selected_zoom(&self) -> Option<ZoomRegion> {
        let at = self.zoom_selected?;
        self.list.view.zooms.iter().copied().find(|z| z.at == at)
    }

    pub fn select_zoom(&mut self, at: Option<MediaTime>) {
        self.zoom_selected = at;
        if let Some(a) = at {
            self.seek(a);
        }
    }

    /// Z: a zoom at the playhead (or over the selected range).
    pub fn add_zoom(&mut self) -> Vec<EditRequest> {
        self.note = None;
        let range = match self.selection {
            super::model::Selection::Range(r) => Some((r.from, r.to)),
            _ => None,
        };
        let at = self.playhead.clamp(self.list.start, self.list.end);
        let z = new_zoom(at, range, self.list.end);
        if z.length < ZOOM_MIN_LENGTH || z.at < self.list.start {
            self.note = Some(super::model::EditNote::ZoomInsideKept);
            return Vec::new();
        }
        self.zoom_selected = Some(z.at);
        vec![self.view_req(
            "set_zooms",
            json!({"zooms": [zoom_value(&z)], "replace": false}),
        )]
    }

    /// A stepper or the follow switch of the selected zoom.
    pub fn adjust_zoom(&mut self, a: ZoomAdjust) -> Vec<EditRequest> {
        self.note = None;
        let Some(z) = self.selected_zoom() else {
            self.note = Some(super::model::EditNote::SelectZoomFirst);
            return Vec::new();
        };
        let n = adjusted(&z, a, self.list.end);
        if n == z {
            return Vec::new();
        }
        // The new region covers the old one exactly, so the daemon swaps it.
        vec![self.view_req(
            "set_zooms",
            json!({"zooms": [zoom_value(&n)], "replace": false}),
        )]
    }

    /// Removes the selected zoom (the daemon call replaces the list without it).
    pub fn remove_zoom(&mut self) -> Vec<EditRequest> {
        self.note = None;
        let Some(sel) = self.selected_zoom() else {
            self.note = Some(super::model::EditNote::SelectZoomFirst);
            return Vec::new();
        };
        let rest: Vec<Value> = self
            .list
            .view
            .zooms
            .iter()
            .filter(|z| z.at != sel.at)
            .map(zoom_value)
            .collect();
        self.zoom_selected = None;
        vec![self.view_req("set_zooms", json!({"zooms": rest, "replace": true}))]
    }

    // ---- cursor (EDT-4) ------------------------------------------------------------------

    pub fn cursor_style(&self) -> CursorStyle {
        self.list.view.cursor
    }

    pub fn set_cursor(&mut self, u: CursorStyleUpdate) -> Vec<EditRequest> {
        self.note = None;
        let now = self.cursor_style();
        // Nothing to say when the update changes nothing.
        if now.updated(&u).map_or(true, |n| n == now) {
            return Vec::new();
        }
        let mut p = json!({});
        if let Some(v) = u.size {
            p["size"] = json!(v);
        }
        if let Some(v) = u.smoothing {
            p["smoothing"] = json!(v.as_str());
        }
        if let Some(v) = u.click_effect {
            p["click_effect"] = json!(v.as_str());
        }
        if let Some(v) = u.hide_idle {
            p["hide_idle"] = json!(v);
        }
        vec![self.view_req("set_cursor_style", p)]
    }

    /// Size stepper of the cursor card.
    pub fn nudge_cursor_size(&mut self, steps: i32) -> Vec<EditRequest> {
        let size = (self.cursor_style().size + f64::from(steps) * CURSOR_SIZE_STEP)
            .clamp(CURSOR_SIZE_RANGE.0, CURSOR_SIZE_RANGE.1);
        self.set_cursor(CursorStyleUpdate {
            size: Some(size),
            ..Default::default()
        })
    }

    pub fn set_smoothing(&mut self, s: Smoothing) -> Vec<EditRequest> {
        self.set_cursor(CursorStyleUpdate {
            smoothing: Some(s),
            ..Default::default()
        })
    }

    pub fn set_click_effect(&mut self, c: ClickEffect) -> Vec<EditRequest> {
        self.set_cursor(CursorStyleUpdate {
            click_effect: Some(c),
            ..Default::default()
        })
    }

    pub fn set_hide_idle(&mut self, b: bool) -> Vec<EditRequest> {
        self.set_cursor(CursorStyleUpdate {
            hide_idle: Some(b),
            ..Default::default()
        })
    }

    // ---- reframe (EDT-5) -----------------------------------------------------------------

    /// The stored reframe of an aspect (fit, no windows, when none is stored).
    pub fn reframe_of(&self, a: Aspect) -> Reframe {
        self.list.view.reframes.get(&a).cloned().unwrap_or_default()
    }

    pub fn set_reframe_mode(&mut self, a: Aspect, mode: ReframeMode) -> Vec<EditRequest> {
        self.note = None;
        if self.reframe_of(a).mode == mode {
            return Vec::new();
        }
        vec![self.view_req(
            "set_reframe",
            json!({"aspect": a.as_str(), "mode": mode.as_str()}),
        )]
    }

    /// Moves the window of the segment under the playhead by fractions of the frame. A first
    /// move also switches `fit` to `fixed`, since a window only shows in that mode.
    pub fn nudge_reframe(&mut self, a: Aspect, dx: f64, dy: f64) -> Vec<EditRequest> {
        self.note = None;
        let r = self.reframe_of(a);
        let at = self.playhead.clamp(self.list.start, self.list.end);
        let f = r.focus_at(at);
        let nf = (
            ((f.0 + dx).clamp(0.0, 1.0) * 100.0).round() / 100.0,
            ((f.1 + dy).clamp(0.0, 1.0) * 100.0).round() / 100.0,
        );
        if nf == f && r.mode != ReframeMode::Fit {
            return Vec::new();
        }
        let mut p = json!({"aspect": a.as_str(), "focus": [nf.0, nf.1], "at": ticks(at)});
        if r.mode == ReframeMode::Fit {
            p["mode"] = json!("fixed");
        }
        vec![self.view_req("set_reframe", p)]
    }

    /// Does a stored reframe of the previewed aspect crop the picture?
    pub fn preview_reframes(&self) -> bool {
        self.preview_aspect
            .reframe_aspect()
            .is_some_and(|a| self.reframe_of(a).mode != ReframeMode::Fit)
    }

    /// The list without a clamp: used by tests and the lane.
    pub fn zooms(&self) -> &[ZoomRegion] {
        &self.list.view.zooms
    }

    pub fn edit_list(&self) -> &EditList {
        &self.list
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recedit::model::{Basis, Selection};
    use crate::recedit::snap::FrameGrid;
    use cm_core::{EditContext, ReframeUpdate, TICKS_PER_SECOND};

    fn secs(s: f64) -> MediaTime {
        MediaTime::from_ticks((s * TICKS_PER_SECOND as f64) as i64)
    }

    fn model() -> EditorModel {
        let basis = Basis {
            end: secs(60.0),
            live_at: None,
            gaps: vec![],
        };
        let cx = EditContext {
            end: basis.end,
            live_at: None,
            gaps: &[],
        };
        EditorModel::new(
            "rec_1",
            basis,
            EditList::full(&cx),
            FrameGrid::new(MediaTime::ZERO, Some(30.0)),
            (false, false),
        )
    }

    fn only(mut v: Vec<EditRequest>) -> EditRequest {
        assert_eq!(v.len(), 1, "{v:?}");
        v.remove(0)
    }

    /// EDT-4.AC2: Z at 15 s adds a 3 s zoom at 1.8x with the focus in the middle.
    #[test]
    fn edt4_ac2_add_zoom_at_the_playhead() {
        let mut m = model();
        m.seek(secs(15.0));
        let r = only(m.add_zoom());
        assert_eq!(r.method, "set_zooms");
        let z = &r.params["zooms"][0];
        assert_eq!(z["at"], format!("t:{}", secs(15.0).ticks()));
        assert_eq!(z["length"], format!("t:{}", secs(3.0).ticks()));
        assert_eq!(z["scale"], 1.8);
        assert_eq!(z["focus"], json!([0.5, 0.5]));
        assert_eq!(r.params["replace"], false);
        assert_eq!(m.zoom_selected, Some(secs(15.0)));
    }

    #[test]
    fn a_selected_range_becomes_the_zoom() {
        let mut m = model();
        m.select_range(secs(10.0), secs(14.0));
        let r = only(m.add_zoom());
        assert_eq!(
            r.params["zooms"][0]["at"],
            format!("t:{}", secs(10.0).ticks())
        );
        assert_eq!(
            r.params["zooms"][0]["length"],
            format!("t:{}", secs(4.0).ticks())
        );
        assert!(matches!(m.selection, Selection::Range(_)));
    }

    #[test]
    fn a_zoom_near_the_end_slides_back_to_fit() {
        let z = new_zoom(secs(59.0), None, secs(60.0));
        assert_eq!(z.at + z.length, secs(60.0));
        assert!(z.length >= ZOOM_MIN_LENGTH);
        let z = new_zoom(secs(1.0), None, secs(60.0));
        assert_eq!((z.at, z.length), (secs(1.0), secs(3.0)));
    }

    #[test]
    fn steppers_stay_in_range() {
        let z = new_zoom(secs(10.0), None, secs(60.0));
        let s = adjusted(&z, ZoomAdjust::Scale(0.2), secs(60.0));
        assert!((s.scale - 2.0).abs() < 1e-9);
        let top = adjusted(&s, ZoomAdjust::Scale(5.0), secs(60.0));
        assert_eq!(top.scale, 3.0);
        let low = adjusted(&s, ZoomAdjust::Scale(-5.0), secs(60.0));
        assert_eq!(low.scale, 1.2);
        // Shorter never goes below 0.8 s, longer never past the end of the recording.
        let short = adjusted(&z, ZoomAdjust::Length(-10.0), secs(60.0));
        assert_eq!(short.length, ZOOM_MIN_LENGTH);
        let long = adjusted(
            &new_zoom(secs(58.0), None, secs(60.0)),
            ZoomAdjust::Length(5.0),
            secs(60.0),
        );
        assert_eq!(long.at + long.length, secs(60.0));
        let f = adjusted(&z, ZoomAdjust::Focus(-0.9, 0.9), secs(60.0));
        assert_eq!(f.focus, (0.0, 1.0));
    }

    #[test]
    fn adjusting_a_zoom_replaces_it_in_place_and_removing_replaces_the_list() {
        let mut m = model();
        m.list = m
            .list
            .set_zooms(
                &[
                    new_zoom(secs(10.0), None, secs(60.0)),
                    new_zoom(secs(30.0), None, secs(60.0)),
                ],
                false,
            )
            .unwrap();
        assert!(
            m.adjust_zoom(ZoomAdjust::Scale(0.2)).is_empty(),
            "nothing selected"
        );
        assert!(m.note.is_some());
        m.select_zoom(Some(secs(30.0)));
        assert_eq!(
            m.playhead,
            secs(30.0),
            "selecting a zoom moves the playhead to it"
        );
        let r = only(m.adjust_zoom(ZoomAdjust::Scale(0.2)));
        assert_eq!(r.params["zooms"].as_array().unwrap().len(), 1);
        assert_eq!(
            r.params["zooms"][0]["at"],
            format!("t:{}", secs(30.0).ticks())
        );
        assert_eq!(r.params["zooms"][0]["scale"], 2.0);
        let r = only(m.adjust_zoom(ZoomAdjust::FollowCursor(true)));
        assert_eq!(r.params["zooms"][0]["follow_cursor"], true);

        let r = only(m.remove_zoom());
        assert_eq!(r.params["replace"], true);
        let rest = r.params["zooms"].as_array().unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0]["at"], format!("t:{}", secs(10.0).ticks()));
        assert_eq!(m.zoom_selected, None);
    }

    #[test]
    fn the_cursor_card_sends_only_real_changes() {
        let mut m = model();
        let r = only(m.nudge_cursor_size(2));
        assert_eq!(r.method, "set_cursor_style");
        assert_eq!(r.params["size"], 1.5);
        assert!(m.nudge_cursor_size(-1).is_empty(), "already at 1x");
        let r = only(m.set_smoothing(Smoothing::Strong));
        assert_eq!(r.params["smoothing"], "strong");
        assert!(m.set_smoothing(Smoothing::Off).is_empty());
        let r = only(m.set_click_effect(ClickEffect::Ripple));
        assert_eq!(r.params["click_effect"], "ripple");
        let r = only(m.set_hide_idle(true));
        assert_eq!(r.params["hide_idle"], true);
        m.list.view.cursor.size = 3.0;
        assert!(m.nudge_cursor_size(4).is_empty(), "already at 3x");
    }

    /// EDT-5: the first nudge switches fit to fixed and addresses the segment under the playhead.
    #[test]
    fn edt5_nudging_the_window_sets_fixed_and_the_segment() {
        let mut m = model();
        m.list = m.list.split(secs(30.0)).unwrap();
        m.seek(secs(40.0));
        let r = only(m.nudge_reframe(Aspect::Vertical, -0.1, 0.0));
        assert_eq!(r.method, "set_reframe");
        assert_eq!(r.params["aspect"], "9:16");
        assert_eq!(r.params["mode"], "fixed");
        assert_eq!(r.params["focus"], json!([0.4, 0.5]));
        assert_eq!(r.params["at"], format!("t:{}", secs(40.0).ticks()));
        // With a fixed reframe stored, a move that changes nothing sends nothing.
        m.list = m
            .list
            .set_reframe(&ReframeUpdate {
                aspect: Aspect::Vertical,
                mode: Some(ReframeMode::Fixed),
                focus: Some((1.0, 0.5)),
                at: Some(secs(40.0)),
            })
            .unwrap();
        assert!(m.nudge_reframe(Aspect::Vertical, 0.1, 0.0).is_empty());
        let r = only(m.nudge_reframe(Aspect::Vertical, -0.05, 0.0));
        assert!(r.params.get("mode").is_none());
        assert_eq!(r.params["focus"], json!([0.95, 0.5]));
    }

    #[test]
    fn edt5_modes_and_the_preview_aspect() {
        let mut m = model();
        let r = only(m.set_reframe_mode(Aspect::Square, ReframeMode::Follow));
        assert_eq!(r.params["mode"], "follow");
        assert!(m
            .set_reframe_mode(Aspect::Square, ReframeMode::Fit)
            .is_empty());
        assert!(!m.preview_reframes());
        m.set_preview_aspect(PreviewAspect::Vertical);
        assert!(!m.preview_reframes(), "fit stores nothing that crops");
        m.list = m
            .list
            .set_reframe(&ReframeUpdate {
                aspect: Aspect::Vertical,
                mode: Some(ReframeMode::Follow),
                focus: None,
                at: None,
            })
            .unwrap();
        assert!(m.preview_reframes());
        m.set_preview_aspect(PreviewAspect::Square);
        assert!(
            !m.preview_reframes(),
            "the square aspect has its own setting"
        );
        assert_eq!(PreviewAspect::Vertical.out_size((1920, 1080)), (540, 960));
        assert_eq!(PreviewAspect::Wide.out_size((1920, 1080)), (1280, 720));
        assert_eq!(PreviewAspect::Square.reframe_aspect(), Some(Aspect::Square));
    }
}
