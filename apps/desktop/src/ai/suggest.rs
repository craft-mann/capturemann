//! Suggestion lists for AI-3 (auto-zoom) and AI-4 (silence and filler-word cuts), display-free.
//! A suggestion is only a proposal: Accept sends the existing `set_zooms` or `cut_segments`
//! action through the editor's own request queue, so it is one undo step like any other edit.

use cm_ai::suggest::{CutSuggestion, ZoomSuggestion};
use cm_core::MediaTime;
use cm_i18n::{fmt, tr, tr_ctx};
use serde_json::{json, Value};

use crate::recedit::model::EditRequest;

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
}

/// A short line for an empty answer.
pub fn nothing_found(what: Nothing) -> String {
    match what {
        Nothing::ZoomSpots => tr!("No zoom spots found in this recording."),
        Nothing::Silences => tr!("No silences found in this recording."),
        Nothing::SilencesOrFillers => {
            tr!("No silences or filler words found in this recording.")
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
