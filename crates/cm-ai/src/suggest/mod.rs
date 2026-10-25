//! Suggestions (suggestions stream): deterministic auto-zoom (AI-3) and silence and filler-word
//! cuts (AI-4). Nothing is applied here; the UI or an agent accepts a suggestion with the
//! existing `set_zooms` and `cut_segments` actions ([`zooms_params`], [`cuts_params`] build their
//! parameters), so a suggestion never changes the edit list by itself (AI-3.AC3).
//!
//! Every time is **session time** in ticks, like the rest of the edit list. The inputs are plain
//! data (a parsed `cursor.jsonl`, PCM samples, a [`Transcript`](crate::transcribe::Transcript),
//! the [`EditList`] and the pause gaps), so the algorithms run headless and deterministically:
//! the same input gives the same suggestions.
//!
//! | Module | Content |
//! | --- | --- |
//! | [`zoom`] | AI-3: dwell and click candidates from the cursor telemetry |
//! | [`silence`] | AI-4: long pauses from the microphone level, with an adaptive noise floor |
//! | [`filler`] | AI-4: isolated filler words from the transcript, English and Vietnamese |

pub mod filler;
pub mod silence;
pub mod zoom;

use cm_core::edit::Span;
use cm_core::{EditList, MediaTime, PauseGap, ZoomRegion};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub use filler::{filler_cuts, FillerLists};
pub use silence::{
    decode_mono_f32, drop_active, frame_diff_fraction, silence_cuts, ScreenActivity,
    SilenceOptions, SCREEN_ACTIVITY_THRESHOLD,
};
pub use zoom::{suggest_zooms, ZoomOptions};

/// A proposed zoom region (session time). It maps one to one onto a `set_zooms` entry
/// ([`ZoomSuggestion::to_region`], [`zooms_params`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoomSuggestion {
    pub start: MediaTime,
    pub end: MediaTime,
    /// Zoom target centre as fractions of the screen (0.0 to 1.0), clamped so the zoomed view
    /// stays inside the frame.
    pub focus: (f32, f32),
    pub scale: f32,
    /// Why: `dwell` (the pointer rested) or `click`.
    pub reason: String,
}

impl ZoomSuggestion {
    /// The region `set_zooms` would store. Suggestions never follow the cursor: the focus is the
    /// dwell or click point.
    pub fn to_region(&self) -> ZoomRegion {
        ZoomRegion {
            at: self.start,
            length: self.end - self.start,
            focus: (f64::from(self.focus.0), f64::from(self.focus.1)),
            scale: f64::from(self.scale),
            follow_cursor: false,
        }
    }

    /// Time of the middle of the region.
    pub fn center(&self) -> MediaTime {
        MediaTime::from_ticks((self.start.ticks() + self.end.ticks()) / 2)
    }
}

/// A proposed cut (session time). It maps onto one `cut_segments` range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CutSuggestion {
    pub start: MediaTime,
    pub end: MediaTime,
    /// `silence` or `filler`.
    pub kind: String,
    /// For `silence` the length of the pause before cutting ("2.0 s pause"); for `filler` the
    /// word, for example "um".
    pub detail: String,
}

impl CutSuggestion {
    pub fn span(&self) -> Span {
        Span::new(self.start, self.end)
    }

    /// How much the cut removes.
    pub fn removes(&self) -> MediaTime {
        self.end - self.start
    }
}

/// `set_zooms` parameters (without `capture_id`) for accepting `zooms`: times as `"t:<ticks>"`,
/// merged into the existing zooms (`replace: false`; the daemon shortens overlapping ones).
pub fn zooms_params(zooms: &[ZoomSuggestion]) -> Value {
    json!({
        "zooms": zooms.iter().map(|z| {
            let r = z.to_region();
            json!({
                "at": format!("t:{}", r.at.ticks()),
                "length": format!("t:{}", r.length.ticks()),
                "focus": [r.focus.0, r.focus.1],
                "scale": r.scale,
                "follow_cursor": false,
            })
        }).collect::<Vec<_>>(),
        "replace": false,
    })
}

/// `cut_segments` parameters (without `capture_id`) for accepting `cuts`: one call, so one undo
/// step (AI-4.AC2).
pub fn cuts_params(cuts: &[CutSuggestion]) -> Value {
    json!({
        "ranges": cuts.iter()
            .map(|c| json!([format!("t:{}", c.start.ticks()), format!("t:{}", c.end.ticks())]))
            .collect::<Vec<_>>(),
    })
}

/// The line the timeline shows: "23 pauses, 41 fillers, remove 3 min 12 s".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CutSummary {
    pub pauses: usize,
    pub fillers: usize,
    pub removes: MediaTime,
}

impl CutSummary {
    pub fn of(cuts: &[CutSuggestion]) -> CutSummary {
        let count = |kind: &str| cuts.iter().filter(|c| c.kind == kind).count();
        CutSummary {
            pauses: count("silence"),
            fillers: count("filler"),
            removes: MediaTime::from_ticks(cuts.iter().map(|c| c.removes().ticks()).sum()),
        }
    }

    /// "23 pauses, 41 fillers, remove 3 min 12 s".
    pub fn label(&self) -> String {
        let total = self.removes.as_secs_f64().round() as i64;
        let (m, s) = (total / 60, total % 60);
        let time = if m > 0 {
            format!("{m} min {s} s")
        } else {
            format!("{s} s")
        };
        let plural =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        format!(
            "{}, {}, remove {time}",
            plural(self.pauses, "pause", "pauses"),
            plural(self.fillers, "filler", "fillers")
        )
    }
}

/// Everything `suggest_cuts` needs besides the audio and the transcript.
#[derive(Debug, Clone)]
pub struct CutOptions {
    pub silence: SilenceOptions,
    /// Include filler words (`fillers: true` of the agent interface).
    pub fillers: bool,
}

impl Default for CutOptions {
    fn default() -> Self {
        CutOptions {
            silence: SilenceOptions::default(),
            fillers: true,
        }
    }
}

/// Shortest cut worth suggesting.
pub const MIN_CUT: MediaTime = MediaTime::from_millis(100);

/// Merges silence and filler suggestions into one list sorted by start: a filler inside a pause
/// is dropped (the pause cut already removes it), and nothing overlaps a cut or a pause gap that
/// is already in the edit list or lies outside the kept range.
pub fn merge_cuts(
    silence: Vec<CutSuggestion>,
    fillers: Vec<CutSuggestion>,
    edits: &EditList,
    gaps: &[PauseGap],
) -> Vec<CutSuggestion> {
    let blocked = edits.without_pauses(gaps).cuts;
    let mut all: Vec<CutSuggestion> = silence.clone();
    all.extend(
        fillers
            .into_iter()
            .filter(|f| !silence.iter().any(|s| overlaps(s.span(), f.span()))),
    );
    let mut out = Vec::new();
    for c in all {
        for piece in subtract(c.span(), &blocked, edits.start, edits.end) {
            if piece.len() >= MIN_CUT {
                out.push(CutSuggestion {
                    start: piece.from,
                    end: piece.to,
                    kind: c.kind.clone(),
                    detail: c.detail.clone(),
                });
            }
        }
    }
    out.sort_by_key(|c| (c.start, c.end));
    out
}

pub(crate) fn overlaps(a: Span, b: Span) -> bool {
    a.from < b.to && b.from < a.to
}

/// `s` clipped to `[lo, hi)` minus the (sorted, merged) `blocked` spans.
pub(crate) fn subtract(s: Span, blocked: &[Span], lo: MediaTime, hi: MediaTime) -> Vec<Span> {
    let mut cur = Span::new(s.from.max(lo), s.to.min(hi));
    let mut out = Vec::new();
    for b in blocked {
        if cur.is_empty() {
            break;
        }
        if b.to <= cur.from || b.from >= cur.to {
            continue;
        }
        if b.from > cur.from {
            out.push(Span::new(cur.from, b.from));
        }
        cur = Span::new(b.to.max(cur.from), cur.to);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub(crate) fn ticks_of(secs: f64) -> i64 {
    (secs * cm_core::TICKS_PER_SECOND as f64).round() as i64
}

#[cfg(test)]
mod tests;
