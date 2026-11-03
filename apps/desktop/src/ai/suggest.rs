//! Suggestion lists for AI-3 (auto-zoom) and AI-4 (silence and filler-word cuts), display-free.
//! A suggestion is only a proposal: Accept sends the existing `set_zooms` or `cut_segments`
//! action through the editor's own request queue, so it is one undo step like any other edit.

use cm_ai::suggest::{CutSuggestion, ZoomSuggestion};
use cm_core::MediaTime;
use cm_i18n::{fmt, tr, tr_ctx};
use serde_json::{json, Value};

use std::collections::HashMap;

use serde_json::Map;

use crate::recedit::model::EditRequest;

/// `ai.auto_suggest_cuts` (M2f): look for pauses and filler words when the editor opens. Default
/// on. It only starts the same `suggest_cuts` action the button sends; nothing is applied.
/// Agents can read it (like the other `ai.*` keys); only the person at the keyboard writes it.
pub const KEY_AUTO_SUGGEST_CUTS: &str = "ai.auto_suggest_cuts";

/// Whether the automatic run is on (a missing or odd value means the default, on).
pub fn auto_suggest_cuts_on(settings: &Map<String, Value>) -> bool {
    settings
        .get(KEY_AUTO_SUGGEST_CUTS)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// What to do when the editor has a recording open at some revision.
#[derive(Debug, Clone, PartialEq)]
pub enum AutoStart {
    /// Nothing: the setting is off, or this revision was already looked at and is being shown.
    No,
    /// This revision was asked before in this session: show the same list without asking again.
    Cached(Vec<CutSuggestion>),
    /// Ask the daemon.
    Run,
}

/// The "once per revision" rule (a transcript that arrives later makes fillers possible, so it counts
/// as a new question: pauses first, then pauses and fillers). A revision is the manifest's last revision number: any accepted
/// cut, trim or undo makes a new one, but a run starts only when a recording is opened, never
/// because the edit list changed under the user's hands.
#[derive(Debug, Default)]
pub struct AutoCuts {
    /// The (recording, revision) that has been asked or is being asked.
    asked: Option<(String, u64, bool)>,
    /// Answers kept for reopening a recording at the same revision (a few, the oldest goes).
    cache: HashMap<(String, u64, bool), Vec<CutSuggestion>>,
    order: Vec<(String, u64, bool)>,
}

const CACHE_KEEPS: usize = 8;

impl AutoCuts {
    /// Called when the recording `id` at `revision` is open and its transcript state is known.
    pub fn start(
        &mut self,
        enabled: bool,
        id: &str,
        revision: u64,
        has_transcript: bool,
    ) -> AutoStart {
        let key = (id.to_owned(), revision, has_transcript);
        if !enabled || self.asked.as_ref() == Some(&key) {
            return AutoStart::No;
        }
        self.asked = Some(key.clone());
        match self.cache.get(&key) {
            Some(list) => AutoStart::Cached(list.clone()),
            None => AutoStart::Run,
        }
    }

    /// The answer for `(id, revision)` arrived.
    pub fn finished(
        &mut self,
        id: &str,
        revision: u64,
        has_transcript: bool,
        list: &[CutSuggestion],
    ) {
        let key = (id.to_owned(), revision, has_transcript);
        if self.cache.insert(key.clone(), list.to_vec()).is_none() {
            self.order.push(key);
        }
        while self.order.len() > CACHE_KEEPS {
            let old = self.order.remove(0);
            self.cache.remove(&old);
        }
    }

    /// The run was cancelled or failed: the same revision may be asked again.
    pub fn forget_ask(&mut self) {
        self.asked = None;
    }

    /// The editor closed or moved to another recording.
    pub fn closed(&mut self) {
        self.asked = None;
    }
}

/// A list the user can prune before accepting: removing a row drops that proposal only.
#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionList<T> {
    items: Vec<T>,
}

impl<T> Default for SuggestionList<T> {
    fn default() -> Self {
        SuggestionList { items: Vec::new() }
    }
}

impl<T: Clone> SuggestionList<T> {
    pub fn new(items: Vec<T>) -> Self {
        SuggestionList { items }
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn remove(&mut self, i: usize) -> Option<T> {
        (i < self.items.len()).then(|| self.items.remove(i))
    }

    /// Takes row `i` out for Accept.
    pub fn take(&mut self, i: usize) -> Option<T> {
        self.remove(i)
    }

    /// Takes every row out for Accept all.
    pub fn take_all(&mut self) -> Vec<T> {
        std::mem::take(&mut self.items)
    }
}

fn ticks(t: MediaTime) -> String {
    format!("t:{}", t.ticks())
}

/// `m:ss` of a session time, for the row labels.
pub fn clock_label(t: MediaTime) -> String {
    let s = t.as_secs_f64().max(0.0);
    format!("{}:{:02}", (s / 60.0) as u64, (s % 60.0) as u64)
}

pub fn zoom_label(z: &ZoomSuggestion) -> String {
    tr!(
        "{start} \u{00B7} {scale}\u{00D7} \u{00B7} {reason}",
        start = clock_label(z.start),
        scale = fmt::decimal(f64::from(z.scale), 1),
        reason = reason_label(&z.reason)
    )
}

fn reason_label(r: &str) -> String {
    match r {
        "click" => tr_ctx!("zoom suggestion", "after a click"),
        "typing" => tr_ctx!("zoom suggestion", "while typing"),
        "activity" => tr_ctx!("zoom suggestion", "busy area"),
        other => other.to_owned(),
    }
}

pub fn cut_label(c: &CutSuggestion) -> String {
    let len = (c.end - c.start).as_secs_f64();
    let what = match c.kind.as_str() {
        "silence" => tr_ctx!("cut suggestion", "silence"),
        "filler" => tr!("\u{201C}{word}\u{201D}", word = c.detail),
        other => other.to_owned(),
    };
    tr!(
        "{start} \u{00B7} {secs} s \u{00B7} {what}",
        start = clock_label(c.start),
        secs = fmt::decimal(len, 1),
        what = what
    )
}

/// `set_zooms` for the accepted zooms, added to the ones already there.
pub fn zoom_request(capture_id: &str, zooms: &[ZoomSuggestion]) -> Option<EditRequest> {
    if zooms.is_empty() {
        return None;
    }
    let values: Vec<Value> = zooms
        .iter()
        .map(|z| {
            json!({
                "at": ticks(z.start),
                "length": ticks(z.end - z.start),
                "focus": [z.focus.0, z.focus.1],
                "scale": z.scale,
                "follow_cursor": false,
            })
        })
        .collect();
    Some(EditRequest {
        method: "set_zooms",
        params: json!({"capture_id": capture_id, "zooms": values, "replace": false}),
    })
}

/// `cut_segments` for the accepted cuts: sorted, overlapping or touching ranges merged, so the
/// daemon applies them as one undo step.
pub fn cut_request(capture_id: &str, cuts: &[CutSuggestion]) -> Option<EditRequest> {
    let mut ranges: Vec<(MediaTime, MediaTime)> = cuts
        .iter()
        .filter(|c| c.end > c.start)
        .map(|c| (c.start, c.end))
        .collect();
    if ranges.is_empty() {
        return None;
    }
    ranges.sort();
    let mut merged: Vec<(MediaTime, MediaTime)> = Vec::new();
    for (a, b) in ranges {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    let values: Vec<Value> = merged
        .iter()
        .map(|(a, b)| json!([ticks(*a), ticks(*b)]))
        .collect();
    Some(EditRequest {
        method: "cut_segments",
        params: json!({"capture_id": capture_id, "ranges": values}),
    })
}

/// What an empty answer was asked for. Kept as data and worded when drawn: each case is one
/// whole translated sentence (a sentence is never glued from a translated noun).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nothing {
    ZoomSpots,
    Silences,
    SilencesOrFillers,
    /// The recording has no transcript, so only pauses were looked for, and none were found.
    NoPausesNoTranscript,
    /// The recording has no transcript, so only pauses were looked for.
    PausesOnlyNoTranscript,
}

/// A short line for an empty answer.
pub fn nothing_found(what: Nothing) -> String {
    match what {
        Nothing::ZoomSpots => tr!("No zoom spots found in this recording."),
        Nothing::Silences => tr!("No silences found in this recording."),
        Nothing::SilencesOrFillers => {
            tr!("No silences or filler words found in this recording.")
        }
        Nothing::NoPausesNoTranscript => {
            tr!("No silences found. Filler words need a transcript, and this recording has none.")
        }
        Nothing::PausesOnlyNoTranscript => {
            tr!("Looked for silences only: filler words need a transcript, and this recording has none.")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: f64) -> MediaTime {
        MediaTime::from_secs_f64(s).unwrap()
    }

    fn zoom(a: f64, b: f64) -> ZoomSuggestion {
        ZoomSuggestion {
            start: t(a),
            end: t(b),
            focus: (0.25, 0.75),
            scale: 2.0,
            reason: "click".into(),
        }
    }

    fn cut(a: f64, b: f64, kind: &str, detail: &str) -> CutSuggestion {
        CutSuggestion {
            start: t(a),
            end: t(b),
            kind: kind.into(),
            detail: detail.into(),
        }
    }

    /// M2f: the trigger rule.
    #[test]
    fn the_automatic_run_starts_once_per_revision() {
        let mut a = AutoCuts::default();
        assert_eq!(a.start(true, "cap_1", 3, true), AutoStart::Run);
        // The editor redraws, the transcript arrives: still the same revision, no second run.
        assert_eq!(a.start(true, "cap_1", 3, true), AutoStart::No);
        // Another revision (an edit was accepted, then the recording is opened again) asks again.
        assert_eq!(a.start(true, "cap_1", 4, true), AutoStart::Run);
        assert_eq!(a.start(true, "cap_2", 4, true), AutoStart::Run);
        // The transcript was not there at first (pauses only) and arrives later: fillers can be
        // looked for now.
        assert_eq!(a.start(true, "cap_3", 1, false), AutoStart::Run);
        assert_eq!(a.start(true, "cap_3", 1, false), AutoStart::No);
        assert_eq!(a.start(true, "cap_3", 1, true), AutoStart::Run);
    }

    #[test]
    fn the_setting_turns_the_automatic_run_off_and_defaults_to_on() {
        let mut a = AutoCuts::default();
        assert_eq!(a.start(false, "cap_1", 1, true), AutoStart::No);
        let mut s = Map::new();
        assert!(auto_suggest_cuts_on(&s), "default on");
        s.insert(KEY_AUTO_SUGGEST_CUTS.into(), json!(false));
        assert!(!auto_suggest_cuts_on(&s));
        s.insert(KEY_AUTO_SUGGEST_CUTS.into(), json!("junk"));
        assert!(
            auto_suggest_cuts_on(&s),
            "a value that is not a switch means the default"
        );
        // Turning it back on later starts a run for the open recording.
        assert_eq!(a.start(true, "cap_1", 1, true), AutoStart::Run);
    }

    #[test]
    fn closing_or_cancelling_lets_the_next_open_ask_again_and_an_answer_is_reused() {
        let mut a = AutoCuts::default();
        assert_eq!(a.start(true, "cap_1", 2, true), AutoStart::Run);
        // The editor closed before the answer: the run was cancelled, nothing was kept.
        a.closed();
        assert_eq!(a.start(true, "cap_1", 2, true), AutoStart::Run);
        let list = vec![cut(1.0, 2.0, "silence", "1.0 s of silence")];
        a.finished("cap_1", 2, true, &list);
        a.closed();
        // Reopened at the same revision: the answer is shown, the daemon is not asked again.
        assert_eq!(a.start(true, "cap_1", 2, true), AutoStart::Cached(list));
    }

    #[test]
    fn the_answer_cache_is_bounded() {
        let mut a = AutoCuts::default();
        for i in 0..20 {
            a.finished(&format!("cap_{i}"), 1, true, &[]);
        }
        assert_eq!(
            a.start(true, "cap_0", 1, true),
            AutoStart::Run,
            "the oldest went"
        );
        assert_eq!(a.start(true, "cap_19", 1, true), AutoStart::Cached(vec![]));
    }

    #[test]
    fn removing_a_row_drops_only_that_proposal() {
        let mut l = SuggestionList::new(vec![zoom(1.0, 3.0), zoom(5.0, 7.0), zoom(9.0, 11.0)]);
        assert_eq!(l.remove(1).map(|z| z.start), Some(t(5.0)));
        assert_eq!(l.len(), 2);
        assert!(l.remove(7).is_none());
        let rest = l.take_all();
        assert_eq!(rest.len(), 2);
        assert!(l.is_empty());
    }

    /// AI-3: accepting goes through `set_zooms` (added, not replaced) with the daemon's shapes.
    #[test]
    fn accepting_zooms_sends_set_zooms_without_replacing() {
        let r = zoom_request("rec_1", &[zoom(1.0, 3.5)]).unwrap();
        assert_eq!(r.method, "set_zooms");
        assert_eq!(r.params["capture_id"], "rec_1");
        assert_eq!(r.params["replace"], false);
        let z = &r.params["zooms"][0];
        assert_eq!(z["at"], format!("t:{}", t(1.0).ticks()));
        assert_eq!(z["length"], format!("t:{}", t(2.5).ticks()));
        assert_eq!(z["focus"], json!([0.25, 0.75]));
        assert_eq!(z["scale"], 2.0);
        assert_eq!(z["follow_cursor"], false);
        assert!(zoom_request("rec_1", &[]).is_none());
    }

    /// AI-4: accepting all cuts is one `cut_segments`, merged and sorted (one undo step).
    #[test]
    fn accepting_cuts_sends_one_merged_cut_segments() {
        let r = cut_request(
            "rec_1",
            &[
                cut(10.0, 12.0, "silence", ""),
                cut(1.0, 2.0, "filler", "um"),
                cut(11.5, 13.0, "silence", ""),
                cut(5.0, 5.0, "silence", ""),
            ],
        )
        .unwrap();
        assert_eq!(r.method, "cut_segments");
        let ranges = r.params["ranges"].as_array().unwrap();
        assert_eq!(ranges.len(), 2, "overlapping cuts merge, empty ones drop");
        assert_eq!(
            ranges[0],
            json!([
                format!("t:{}", t(1.0).ticks()),
                format!("t:{}", t(2.0).ticks())
            ])
        );
        assert_eq!(
            ranges[1],
            json!([
                format!("t:{}", t(10.0).ticks()),
                format!("t:{}", t(13.0).ticks())
            ])
        );
        assert!(cut_request("rec_1", &[]).is_none());
    }

    #[test]
    fn labels_say_when_what_and_how_long() {
        assert_eq!(clock_label(t(75.0)), "1:15");
        assert_eq!(
            zoom_label(&zoom(65.0, 68.0)),
            "1:05 \u{00B7} 2.0\u{00D7} \u{00B7} after a click"
        );
        assert_eq!(
            cut_label(&cut(10.0, 11.4, "silence", "")),
            "0:10 \u{00B7} 1.4 s \u{00B7} silence"
        );
        assert_eq!(
            cut_label(&cut(3.0, 3.4, "filler", "um")),
            "0:03 \u{00B7} 0.4 s \u{00B7} \u{201C}um\u{201D}"
        );
        assert_eq!(
            nothing_found(Nothing::Silences),
            "No silences found in this recording."
        );
        assert_eq!(
            nothing_found(Nothing::ZoomSpots),
            "No zoom spots found in this recording."
        );
        assert_eq!(
            nothing_found(Nothing::SilencesOrFillers),
            "No silences or filler words found in this recording."
        );
    }
}
