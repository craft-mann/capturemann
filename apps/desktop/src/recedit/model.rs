//! The editor's state and what each command asks of the daemon, display-free.
//!
//! `EditorModel` holds the edit list as the daemon last returned it, the playhead (session time),
//! the selection and the snapping toggle. A command (`Cmd`, a drag, a button) never changes the
//! edit list itself: it returns [`EditRequest`]s, the view sends them to the daemon (`trim`,
//! `cut_segments`, `restore_segments`, `split`, `set_speed`, `set_background`, `undo`, `redo`),
//! and [`EditorModel::adopt`] takes the daemon's answer. So the GUI, the CLI and agents share one
//! edit list, one undo stack and the same validation.

use cm_core::{EditContext, EditList, MediaTime, PauseGap, Span, TICKS_PER_SECOND};
use cm_i18n::tr;
use serde_json::{json, Value};

use super::geom::{self, SegKind};
use super::inspector::BgSpec;
use super::keys::Cmd;
use super::snap::FrameGrid;
use super::viewedit::PreviewAspect;

/// One daemon call the view should make.
#[derive(Debug, Clone, PartialEq)]
pub struct EditRequest {
    pub method: &'static str,
    pub params: Value,
}

/// What is selected on the timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Selection {
    None,
    /// A dragged range on the ruler (or two shift-clicks).
    Range(Span),
    /// A kept segment, by index among the kept segments.
    Segment(usize),
    /// A cut, by index in `EditList::cuts`.
    Cut(usize),
}

/// What the manifest says about the recording, needed to read the daemon's answers.
#[derive(Debug, Clone)]
pub struct Basis {
    pub end: MediaTime,
    pub live_at: Option<MediaTime>,
    pub gaps: Vec<PauseGap>,
}

/// Why a command of the recording editor did nothing, or what a toggle did. The words are made
/// when the strip is drawn (`Display`), so a note that stays follows a language switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditNote {
    SelectSegmentForLayout,
    InBeforeOut,
    OutAfterIn,
    SplitInsideKept,
    AlreadySplit,
    SelectToCut,
    NothingToUndo,
    NothingToRedo,
    SelectionOutside,
    SelectCutToRestore,
    SelectSegmentForSpeed,
    LoopOn,
    LoopOff,
    ZoomInsideKept,
    SelectZoomFirst,
}

impl std::fmt::Display for EditNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            EditNote::SelectSegmentForLayout => tr!("Select a segment to change its camera layout"),
            EditNote::InBeforeOut => tr!("The in point has to be before the out point"),
            EditNote::OutAfterIn => tr!("The out point has to be after the in point"),
            EditNote::SplitInsideKept => tr!("Move the playhead inside the kept part to split"),
            EditNote::AlreadySplit => tr!("Already split here"),
            EditNote::SelectToCut => tr!("Select a range or a segment to cut"),
            EditNote::NothingToUndo => tr!("Nothing to undo"),
            EditNote::NothingToRedo => tr!("Nothing to redo"),
            EditNote::SelectionOutside => tr!("The selection is outside the kept part"),
            EditNote::SelectCutToRestore => tr!("Select a cut (the hatched part) to restore it"),
            EditNote::SelectSegmentForSpeed => tr!("Select a segment to change its speed"),
            EditNote::LoopOn => tr!("Loop on: playback starts again at the end"),
            EditNote::LoopOff => tr!("Loop off: playback stops at the end"),
            EditNote::ZoomInsideKept => tr!("Move the playhead inside the kept part to add a zoom"),
            EditNote::SelectZoomFirst => tr!("Select a zoom on the zoom lane first"),
        })
    }
}

pub struct EditorModel {
    pub capture_id: String,
    pub basis: Basis,
    pub list: EditList,
    pub grid: FrameGrid,
    pub playhead: MediaTime,
    pub selection: Selection,
    pub snapping: bool,
    pub playing: bool,
    /// Start again at the beginning when playback reaches the end.
    pub looping: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// First click of a shift-click range.
    pub range_anchor: Option<MediaTime>,
    /// A line for the status strip: why a command did nothing.
    pub note: Option<EditNote>,
    /// The output the preview shows (EDT-5: a vertical or square preview applies its reframe).
    pub preview_aspect: PreviewAspect,
    /// The selected zoom region, by its start (zooms never overlap, so it is unique; EDT-4).
    pub zoom_selected: Option<MediaTime>,
}

fn ticks(t: MediaTime) -> String {
    format!("t:{}", t.ticks())
}

impl EditorModel {
    pub fn new(
        capture_id: &str,
        basis: Basis,
        list: EditList,
        grid: FrameGrid,
        flags: (bool, bool),
    ) -> EditorModel {
        let playhead = list.start;
        EditorModel {
            capture_id: capture_id.to_owned(),
            basis,
            list,
            grid,
            playhead,
            selection: Selection::None,
            snapping: true,
            playing: false,
            looping: false,
            can_undo: flags.0,
            can_redo: flags.1,
            range_anchor: None,
            note: None,
            preview_aspect: PreviewAspect::default(),
            zoom_selected: None,
        }
    }

    pub fn end(&self) -> MediaTime {
        self.basis.end
    }

    pub fn segments(&self) -> Vec<geom::Seg> {
        geom::segments(&self.list, self.basis.end)
    }

    /// The span the selection covers.
    pub fn selected_span(&self) -> Option<Span> {
        match self.selection {
            Selection::None => None,
            Selection::Range(s) => Some(s),
            Selection::Cut(i) => self.list.cuts.get(i).copied(),
            Selection::Segment(i) => self.segments().into_iter().find_map(|s| match s.kind {
                SegKind::Kept { index, .. } if index == i => Some(s.span),
                _ => None,
            }),
        }
    }

    /// Speed and audio flag of the selected kept segment.
    pub fn selected_speed(&self) -> Option<(f64, bool)> {
        let Selection::Segment(i) = self.selection else {
            return None;
        };
        self.segments().into_iter().find_map(|s| match s.kind {
            SegKind::Kept {
                index,
                speed,
                keep_audio,
            } if index == i => Some((speed, keep_audio)),
            _ => None,
        })
    }

    pub fn background(&self) -> BgSpec {
        BgSpec::of(&self.list.background)
    }

    /// The bubble the recorder stored (`layout.camera`), behind every layout range (CAM-3).
    pub fn camera_base(&self) -> Option<cm_media::layout::CameraLayout> {
        self.list
            .rest
            .get("layout")
            .and_then(|l| l.get("camera"))
            .and_then(cm_media::layout::CameraLayout::from_json)
    }

    /// The span a camera layout applies to: the selected segment or range, else the kept segment
    /// under the playhead (CAM-3: a layout per segment).
    pub fn layout_target(&self) -> Option<Span> {
        if let Selection::Segment(_) | Selection::Range(_) = self.selection {
            return self.selected_span();
        }
        self.segments().into_iter().find_map(|s| match s.kind {
            SegKind::Kept { .. } if self.playhead >= s.span.from && self.playhead < s.span.to => {
                Some(s.span)
            }
            _ => None,
        })
    }

    /// The camera layout in force at the start of the target (the default bubble when no range
    /// is stored there).
    pub fn camera_spec(&self) -> Option<cm_core::CameraLayoutSpec> {
        let span = self.layout_target()?;
        Some(self.list.layout_at(span.from).current)
    }

    /// CAM-3: a camera layout over the target (one undo step in the daemon).
    pub fn set_camera_layout(&mut self, next: cm_core::CameraLayoutSpec) -> Vec<EditRequest> {
        self.note = None;
        let Some(span) = self.layout_target() else {
            return self.say(EditNote::SelectSegmentForLayout);
        };
        if self.camera_spec().as_ref() == Some(&next) {
            return Vec::new();
        }
        vec![self.req("set_camera_layout", super::camera::params(span, &next))]
    }

    fn req(&self, method: &'static str, mut params: Value) -> EditRequest {
        params["capture_id"] = json!(self.capture_id);
        EditRequest { method, params }
    }

    fn say(&mut self, note: EditNote) -> Vec<EditRequest> {
        self.note = Some(note);
        Vec::new()
    }

    // ---- playhead ------------------------------------------------------------------------

    pub fn seek(&mut self, t: MediaTime) {
        self.playhead = t.clamp(MediaTime::ZERO, self.basis.end);
    }

    /// Plays forward by `dt` seconds of output time, skipping cuts and the trimmed ends
    /// (EDT-1.AC2). Returns false (and stops) at the end.
    pub fn advance(&mut self, dt: f64) -> bool {
        match advance_playhead(&self.played(), self.playhead, dt) {
            Some(t) => {
                self.playhead = t;
                true
            }
            None => {
                self.playing = false;
                false
            }
        }
    }

    // ---- commands ------------------------------------------------------------------------

    /// Runs one key command; returns the daemon calls it needs.
    pub fn command(&mut self, cmd: Cmd) -> Vec<EditRequest> {
        self.note = None;
        match cmd {
            Cmd::SetIn => {
                let t = self.grid.round(self.playhead);
                if t >= self.list.end {
                    return self.say(EditNote::InBeforeOut);
                }
                self.trim_to(t, self.list.end)
            }
            Cmd::SetOut => {
                let t = self.grid.round(self.playhead);
                if t <= self.list.start {
                    return self.say(EditNote::OutAfterIn);
                }
                self.trim_to(self.list.start, t)
            }
            Cmd::Split => {
                let t = self.grid.round(self.playhead);
                if t <= self.list.start || t >= self.list.end {
                    return self.say(EditNote::SplitInsideKept);
                }
                if self.list.splits.contains(&t) {
                    return self.say(EditNote::AlreadySplit);
                }
                vec![self.req("split", json!({"at": ticks(t)}))]
            }
            Cmd::Delete => match self.selection {
                Selection::Cut(_) => self.restore_selection(),
                Selection::None => self.say(EditNote::SelectToCut),
                _ => self.cut_selection(),
            },
            Cmd::Restore => self.restore_selection(),
            Cmd::AddZoom => self.add_zoom(),
            Cmd::ToggleSnap => {
                self.snapping = !self.snapping;
                Vec::new()
            }
            Cmd::PlayPause => {
                self.playing = !self.playing;
                Vec::new()
            }
            Cmd::ToggleLoop => {
                self.looping = !self.looping;
                self.note = Some(if self.looping {
                    EditNote::LoopOn
                } else {
                    EditNote::LoopOff
                });
                Vec::new()
            }
            Cmd::Step { frames } => {
                let t = self.grid.step(self.playhead, frames);
                self.seek(t);
                Vec::new()
            }
            Cmd::StepSecs { secs } => {
                let t = self.grid.step_secs(self.playhead, secs);
                self.seek(t);
                Vec::new()
            }
            Cmd::Home => {
                self.seek(self.list.start);
                Vec::new()
            }
            Cmd::End => {
                self.seek(self.list.end);
                Vec::new()
            }
            Cmd::Undo => {
                if !self.can_undo {
                    return self.say(EditNote::NothingToUndo);
                }
                vec![self.req("undo", json!({}))]
            }
            Cmd::Redo => {
                if !self.can_redo {
                    return self.say(EditNote::NothingToRedo);
                }
                vec![self.req("redo", json!({}))]
            }
            Cmd::ClearSelection => {
                self.selection = Selection::None;
                self.range_anchor = None;
                Vec::new()
            }
        }
    }

    pub fn trim_to(&mut self, start: MediaTime, end: MediaTime) -> Vec<EditRequest> {
        if (start, end) == (self.list.start, self.list.end) {
            return Vec::new();
        }
        vec![self.req("trim", json!({"start": ticks(start), "end": ticks(end)}))]
    }

    fn cut_selection(&mut self) -> Vec<EditRequest> {
        let Some(span) = self.selected_span() else {
            return self.say(EditNote::SelectToCut);
        };
        let span = Span::new(span.from.max(self.list.start), span.to.min(self.list.end));
        if span.is_empty() {
            return self.say(EditNote::SelectionOutside);
        }
        self.selection = Selection::None;
        vec![self.req(
            "cut_segments",
            json!({"ranges": [[ticks(span.from), ticks(span.to)]]}),
        )]
    }

    fn restore_selection(&mut self) -> Vec<EditRequest> {
        let span = match self.selection {
            Selection::Cut(i) => self.list.cuts.get(i).copied(),
            Selection::Range(r) => self
                .list
                .cuts
                .iter()
                .any(|c| c.from < r.to && r.from < c.to)
                .then_some(r),
            _ => None,
        };
        let Some(span) = span else {
            return self.say(EditNote::SelectCutToRestore);
        };
        self.selection = Selection::None;
        vec![self.req(
            "restore_segments",
            json!({"ranges": [[ticks(span.from), ticks(span.to)]]}),
        )]
    }

    /// EDT-2: speed for the selected segment (or, with a range selected, for that range).
    pub fn set_speed(&mut self, factor: f64, keep_audio: bool) -> Vec<EditRequest> {
        self.note = None;
        let span = match self.selection {
            Selection::Segment(_) | Selection::Range(_) => self.selected_span(),
            _ => None,
        };
        let Some(span) = span else {
            return self.say(EditNote::SelectSegmentForSpeed);
        };
        vec![self.req(
            "set_speed",
            json!({
                "range": [ticks(span.from), ticks(span.to)],
                "factor": factor,
                "keep_audio": keep_audio,
            }),
        )]
    }

    /// EDT-3: apply a background (a preset or a tweak of the current one).
    pub fn set_background(&mut self, spec: &BgSpec) -> Vec<EditRequest> {
        self.note = None;
        if *spec == self.background() {
            return Vec::new();
        }
        let params = spec.params(&self.capture_id);
        vec![EditRequest {
            method: "set_background",
            params,
        }]
    }

    // ---- selection by pointer ------------------------------------------------------------

    /// A plain click on the ruler or a segment seeks and clears the range anchor.
    pub fn click_time(&mut self, t: MediaTime) {
        self.range_anchor = None;
        self.seek(t);
    }

    /// Shift+click: the first sets the anchor, the second selects the range between (EDT-1).
    pub fn shift_click(&mut self, t: MediaTime) {
        match self.range_anchor.take() {
            None => self.range_anchor = Some(t),
            Some(a) => {
                let (from, to) = (a.min(t), a.max(t));
                self.selection = if to > from {
                    Selection::Range(Span::new(from, to))
                } else {
                    Selection::None
                };
            }
        }
    }

    pub fn select_range(&mut self, a: MediaTime, b: MediaTime) {
        let (from, to) = (a.min(b), a.max(b));
        self.selection = if to > from {
            Selection::Range(Span::new(from, to))
        } else {
            Selection::None
        };
    }

    // ---- the daemon's answer -------------------------------------------------------------

    /// Takes the result of an edit, undo or redo: `{ edits, can_undo, can_redo, ... }`.
    pub fn adopt(&mut self, result: &Value) -> bool {
        let Some(edits) = result.get("edits") else {
            return false;
        };
        let list = {
            let cx = EditContext {
                end: self.basis.end,
                live_at: self.basis.live_at,
                gaps: &self.basis.gaps,
            };
            EditList::from_json(edits, &cx)
        };
        self.list = list;
        self.can_undo = result["can_undo"].as_bool().unwrap_or(self.can_undo);
        self.can_redo = result["can_redo"].as_bool().unwrap_or(self.can_redo);
        // Indices into splits/cuts/segments may have moved: keep the selection only if it still
        // names something.
        let valid = match self.selection {
            Selection::None | Selection::Range(_) => true,
            Selection::Cut(i) => i < self.list.cuts.len(),
            Selection::Segment(_) => self.selected_span().is_some(),
        };
        if !valid {
            self.selection = Selection::None;
        }
        if self.zoom_selected.is_some() && self.selected_zoom().is_none() {
            self.zoom_selected = None;
        }
        self.playhead = self.playhead.clamp(MediaTime::ZERO, self.basis.end);
        true
    }

    // ---- readouts ------------------------------------------------------------------------

    /// Length of the export (EDT-1.AC1: 2 to 58 s of a 60 s recording is 56 s).
    pub fn output_duration(&self) -> MediaTime {
        self.played().timeline().duration()
    }

    /// The list as it plays and exports: the pause gaps are cut too (REC-4: pauses are not in
    /// any media file). The timeline strip still shows the stored list, so a pause is visible
    /// there, but time on the transport and in the audio never includes it.
    pub fn played(&self) -> EditList {
        self.list.without_pauses(&self.basis.gaps)
    }

    /// Moves the playhead to output time `secs` (seconds on the exported timeline, which has the
    /// cuts, the pauses and the speed changes folded in). `false` at or past the end.
    pub fn set_output_time(&mut self, secs: f64) -> bool {
        let t = MediaTime::from_ticks((secs.max(0.0) * TICKS_PER_SECOND as f64).round() as i64);
        match self.played().timeline().at_output(t) {
            Some(s) => {
                self.playhead = s.source;
                true
            }
            None => false,
        }
    }

    /// The playhead on the output timeline (the nearest kept time if it sits in a cut).
    pub fn output_position(&self) -> MediaTime {
        let tl = self.played().timeline();
        let mut t = self.playhead.max(self.list.start);
        while t < self.list.end {
            if let Some(o) = tl.source_to_output(t) {
                return o;
            }
            // In a cut: look at the next piece start.
            match tl.pieces().iter().find(|p| p.source.from > t) {
                Some(p) => t = p.source.from,
                None => break,
            }
        }
        tl.duration()
    }
}

/// `mm:ss.ff` for `t` at `fps` frames per second.
pub fn timecode(t: MediaTime, fps: f64) -> String {
    let ticks = t.ticks().max(0);
    let total_frames = (ticks as f64 / TICKS_PER_SECOND as f64 * fps).floor() as i64;
    let fps_i = (fps.round() as i64).max(1);
    let (secs, frame) = (total_frames / fps_i, total_frames % fps_i);
    format!("{:02}:{:02}.{:02}", secs / 60, secs % 60, frame)
}

/// Plays `dt` seconds of output time forward from `playhead` (session time). `None` at the end.
/// A playhead in a cut or before the in-point jumps to the next kept piece first.
pub fn advance_playhead(list: &EditList, playhead: MediaTime, dt: f64) -> Option<MediaTime> {
    let tl = list.timeline();
    let from = tl.source_to_output(playhead).or_else(|| {
        tl.pieces()
            .iter()
            .find(|p| p.source.from >= playhead)
            .map(|p| p.out_start)
    })?;
    let target =
        MediaTime::from_ticks(from.ticks() + (dt * TICKS_PER_SECOND as f64).round() as i64);
    tl.at_output(target).map(|s| s.source)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: i64) -> MediaTime {
        MediaTime::from_secs(s)
    }

    fn model() -> EditorModel {
        let basis = Basis {
            end: secs(60),
            live_at: None,
            gaps: Vec::new(),
        };
        let list = EditList::full(&EditContext {
            end: basis.end,
            live_at: None,
            gaps: &[],
        });
        EditorModel::new(
            "rec_1",
            basis,
            list,
            FrameGrid::new(MediaTime::ZERO, Some(30.0)),
            (false, false),
        )
    }

    /// REC-4: a 5 s pause is in no media file, so the transport time, the duration and playback
    /// skip it (the stored list still shows it on the timeline).
    #[test]
    fn rec4_pauses_are_not_played_or_counted() {
        let mut m = model();
        m.basis.gaps = vec![cm_core::PauseGap {
            from: secs(10),
            to: secs(15),
        }];
        assert_eq!(m.output_duration(), secs(55));
        m.seek(secs(9));
        assert!(m.advance(2.0));
        assert_eq!(m.playhead, secs(16), "9 -> 10, pause, 15 -> 16");
        m.seek(secs(20));
        assert_eq!(m.output_position(), secs(15));
        // A paused playhead resumes where the pause ends.
        m.seek(secs(12));
        assert_eq!(m.output_position(), secs(10));
    }

    /// Playing by output time (the audio clock): cuts, pauses and speed are folded in; the end
    /// is `false`.
    #[test]
    fn set_output_time_maps_through_cuts_and_speed() {
        let mut m = with(|l| {
            l.cut(&[Span::new(secs(10), secs(15))])
                .unwrap()
                .set_speed(Span::new(secs(20), secs(30)), 2.0, false)
                .unwrap()
        });
        assert!(m.set_output_time(9.5));
        assert_eq!(m.playhead, MediaTime::from_millis(9_500));
        assert!(m.set_output_time(10.5), "after the cut");
        assert_eq!(m.playhead, MediaTime::from_millis(15_500));
        // Output 15 s is session 20 s; 1 s later is 2 s of source at 2x.
        assert!(m.set_output_time(15.0));
        assert_eq!(m.playhead, secs(20));
        assert!(m.set_output_time(16.0));
        assert_eq!(m.playhead, secs(22));
        assert!(!m.set_output_time(1_000.0));
    }

    /// The model keeps the note as a value, so a note still on the strip follows a language switch.
    #[test]
    fn a_kept_note_follows_the_language() {
        use cm_i18n::{with_language, Lang};
        let mut m = model();
        m.can_undo = false;
        assert!(m.command(Cmd::Undo).is_empty());
        let note = m.note.expect("a note");
        assert_eq!(note, EditNote::NothingToUndo);
        assert_eq!(
            with_language(Lang::En, || note.to_string()),
            "Nothing to undo"
        );
        assert_ne!(
            with_language(Lang::Vi, || note.to_string()),
            "Nothing to undo"
        );
    }

    #[test]
    fn loop_toggles_with_a_note() {
        let mut m = model();
        assert!(!m.looping);
        assert!(m.command(Cmd::ToggleLoop).is_empty());
        assert!(m.looping && m.note.unwrap().to_string().contains("Loop on"));
        m.command(Cmd::ToggleLoop);
        assert!(!m.looping);
    }

    /// A model whose list is `edit` applied (what the daemon would hold).
    fn with(edit: impl FnOnce(EditList) -> EditList) -> EditorModel {
        let mut m = model();
        m.list = edit(m.list.clone());
        m
    }

    fn only(r: Vec<EditRequest>) -> EditRequest {
        assert_eq!(r.len(), 1, "{r:?}");
        r.into_iter().next().unwrap()
    }

    /// EDT-1.AC1: I and O at 2 s and 58 s send one trim each, with exact ticks.
    #[test]
    fn edt1_ac1_in_and_out_send_trim() {
        let mut m = model();
        m.seek(secs(2));
        let r = only(m.command(Cmd::SetIn));
        assert_eq!(r.method, "trim");
        assert_eq!(r.params["start"], format!("t:{}", secs(2).ticks()));
        assert_eq!(r.params["end"], format!("t:{}", secs(60).ticks()));
        assert_eq!(r.params["capture_id"], "rec_1");
        m.seek(secs(58));
        let r = only(m.command(Cmd::SetOut));
        assert_eq!(r.params["end"], format!("t:{}", secs(58).ticks()));
    }

    #[test]
    fn edt1_in_after_out_is_refused_with_a_note() {
        let mut m = with(|l| l.trim(secs(10), secs(20), &ctx()).unwrap());
        m.seek(secs(25));
        assert!(m.command(Cmd::SetIn).is_empty());
        assert!(m.note.unwrap().to_string().contains("before the out point"));
    }

    fn ctx() -> EditContext<'static> {
        EditContext {
            end: secs(60),
            live_at: None,
            gaps: &[],
        }
    }

    /// EDT-1: S splits at the playhead, once.
    #[test]
    fn edt1_split_at_the_playhead() {
        let mut m = model();
        m.seek(secs(30));
        let r = only(m.command(Cmd::Split));
        assert_eq!(r.method, "split");
        assert_eq!(r.params["at"], format!("t:{}", secs(30).ticks()));
        m.list = m.list.split(secs(30)).unwrap();
        assert!(m.command(Cmd::Split).is_empty());
        m.seek(secs(0));
        assert!(m.command(Cmd::Split).is_empty());
    }

    /// EDT-1.AC2/AC5: Delete on a range sends one `cut_segments`; on a cut it restores (AC3).
    #[test]
    fn edt1_delete_cuts_a_range_and_restores_a_cut() {
        let mut m = model();
        m.select_range(secs(10), secs(15));
        let r = only(m.command(Cmd::Delete));
        assert_eq!(r.method, "cut_segments");
        assert_eq!(
            r.params["ranges"][0],
            json!([
                format!("t:{}", secs(10).ticks()),
                format!("t:{}", secs(15).ticks())
            ])
        );
        assert_eq!(m.selection, Selection::None);

        let mut m = with(|l| l.cut(&[Span::new(secs(10), secs(15))]).unwrap());
        m.selection = Selection::Cut(0);
        let r = only(m.command(Cmd::Delete));
        assert_eq!(r.method, "restore_segments");
    }

    #[test]
    fn edt1_delete_a_selected_segment_cuts_its_whole_span() {
        let mut m = with(|l| l.split(secs(30)).unwrap());
        m.selection = Selection::Segment(1);
        let r = only(m.command(Cmd::Delete));
        assert_eq!(r.method, "cut_segments");
        assert_eq!(r.params["ranges"][0][0], format!("t:{}", secs(30).ticks()));
    }

    #[test]
    fn delete_without_a_selection_explains_itself() {
        let mut m = model();
        assert!(m.command(Cmd::Delete).is_empty());
        assert!(m.note.is_some());
    }

    /// EDT-1: restoring a range only goes out when it touches a cut.
    #[test]
    fn edt1_restore_needs_a_cut() {
        let mut m = with(|l| l.cut(&[Span::new(secs(10), secs(15))]).unwrap());
        m.select_range(secs(20), secs(25));
        assert!(m.command(Cmd::Restore).is_empty());
        m.select_range(secs(12), secs(14));
        assert_eq!(only(m.command(Cmd::Restore)).method, "restore_segments");
    }

    /// EDT-2: speed applies to the selected segment's span.
    #[test]
    fn edt2_speed_for_the_selected_segment() {
        let mut m = with(|l| l.split(secs(30)).unwrap());
        assert!(m.set_speed(2.0, false).is_empty());
        m.selection = Selection::Segment(1);
        let r = only(m.set_speed(2.0, false));
        assert_eq!(r.method, "set_speed");
        assert_eq!(r.params["factor"], 2.0);
        assert_eq!(r.params["range"][0], format!("t:{}", secs(30).ticks()));
        assert_eq!(r.params["range"][1], format!("t:{}", secs(60).ticks()));
        assert_eq!(m.selected_speed(), Some((1.0, false)));
    }

    /// EDT-3: a background change sends all fields once, and nothing when it is unchanged.
    #[test]
    fn edt3_background_request() {
        use crate::recedit::inspector::Preset;
        let mut m = model();
        let soft = BgSpec::preset(Preset::SoftGray, &m.background());
        let r = only(m.set_background(&soft));
        assert_eq!(r.method, "set_background");
        assert_eq!(r.params["kind"], "color");
        m.list = m
            .list
            .set_background(&cm_core::BackgroundUpdate {
                kind: cm_core::BackgroundKind::Color,
                value: Some(json!("#e8e9ec")),
                padding_pct: Some(8.0),
                radius_px: Some(12),
                shadow: Some(true),
            })
            .unwrap();
        assert!(m.set_background(&soft).is_empty());
        assert_eq!(m.background().which(), Preset::SoftGray);
    }

    /// Undo and redo go to the daemon only when there is something to step.
    #[test]
    fn undo_and_redo_follow_the_flags() {
        let mut m = model();
        assert!(m.command(Cmd::Undo).is_empty());
        m.can_undo = true;
        assert_eq!(only(m.command(Cmd::Undo)).method, "undo");
        m.can_redo = true;
        assert_eq!(only(m.command(Cmd::Redo)).method, "redo");
    }

    /// The daemon's answer replaces the list and the flags, and drops a stale selection.
    #[test]
    fn adopt_takes_the_daemons_edit_list() {
        let mut m = model();
        m.selection = Selection::Cut(0);
        let l = m.list.trim(secs(2), secs(58), &ctx()).unwrap();
        assert!(m.adopt(&json!({"edits": l.to_json(), "can_undo": true, "can_redo": false})));
        assert_eq!((m.list.start, m.list.end), (secs(2), secs(58)));
        assert!(m.can_undo && !m.can_redo);
        assert_eq!(m.selection, Selection::None, "no cut 0 any more");
        assert!(!m.adopt(&json!({"nope": 1})));
    }

    /// EDT-1.AC1: the export length is the kept length (56 s for a 2-58 s trim).
    #[test]
    fn edt1_ac1_output_duration() {
        let m = with(|l| l.trim(secs(2), secs(58), &ctx()).unwrap());
        assert_eq!(m.output_duration(), secs(56));
    }

    /// EDT-1.AC2: playback jumps over a cut and the transport time does not count it.
    #[test]
    fn edt1_ac2_playback_skips_cuts() {
        let mut m = with(|l| l.cut(&[Span::new(secs(10), secs(15))]).unwrap());
        m.seek(secs(9));
        assert!(m.advance(2.0));
        assert_eq!(m.playhead, secs(16), "9 -> 10, cut, 15 -> 16");
        assert_eq!(m.output_duration(), secs(55));
        m.seek(secs(12));
        assert_eq!(
            m.output_position(),
            secs(10),
            "inside the cut: where it resumes"
        );
    }

    #[test]
    fn playback_stops_at_the_end_and_honours_speed() {
        let mut m = with(|l| {
            l.set_speed(Span::new(secs(0), secs(20)), 2.0, false)
                .unwrap()
        });
        m.seek(secs(0));
        assert!(m.advance(1.0));
        assert_eq!(m.playhead, secs(2), "1 s of output is 2 s of source at 2x");
        m.seek(secs(59));
        m.playing = true;
        assert!(!m.advance(5.0));
        assert!(!m.playing);
    }

    #[test]
    fn keyboard_steps_move_the_playhead_without_calls() {
        let mut m = model();
        m.seek(secs(1));
        assert!(m.command(Cmd::Step { frames: 1 }).is_empty());
        assert_eq!((m.playhead - secs(1)).ticks(), TICKS_PER_SECOND / 30);
        m.command(Cmd::StepSecs { secs: 1 });
        assert!(m.playhead > secs(2));
        m.command(Cmd::End);
        m.command(Cmd::Step { frames: 1 });
        assert_eq!(m.playhead, secs(60), "clamped to the recording");
        assert!(m.command(Cmd::ToggleSnap).is_empty());
        assert!(!m.snapping);
    }

    /// EDT-1: two shift-clicks select the range between them.
    #[test]
    fn edt1_shift_click_selects_a_range() {
        let mut m = model();
        m.shift_click(secs(20));
        assert_eq!(m.selection, Selection::None);
        m.shift_click(secs(10));
        assert_eq!(m.selection, Selection::Range(Span::new(secs(10), secs(20))));
    }

    #[test]
    fn timecode_is_minutes_seconds_frames() {
        assert_eq!(timecode(MediaTime::ZERO, 30.0), "00:00.00");
        assert_eq!(timecode(secs(75), 30.0), "01:15.00");
        let t = MediaTime::from_ticks(TICKS_PER_SECOND + TICKS_PER_SECOND / 2);
        assert_eq!(timecode(t, 30.0), "00:01.15");
    }
}
