//! Burned-in captions (AI-1): which line shows at which output time.
//!
//! `cm-ai` depends on this crate, so the export cannot call it. The caller (the daemon for an
//! export, the editor for its preview) hands over a [`CaptionSource`]: given the output
//! [`Timeline`] it returns the caption lines already mapped onto the output time (cut ranges
//! removed, speed applied). The export builds the timeline itself (pauses removed like cuts), so
//! the cues and the picture always share one clock.

use std::sync::Arc;

use cm_core::edit::Timeline;
use cm_core::{CmError, MediaTime};
use cm_render::compose::CaptionLayer;

/// One caption line on the output timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptionCue {
    pub start: MediaTime,
    pub end: MediaTime,
    /// Plain text; rows are separated by `\n`.
    pub text: String,
}

/// Produces the cues for an output timeline.
pub type CaptionSource = Arc<dyn Fn(&Timeline) -> Result<Vec<CaptionCue>, CmError> + Send + Sync>;

/// The cue showing at output time `t` (`start <= t < end`); the later one when two overlap.
/// `cues` need not be sorted.
pub fn cue_at(cues: &[CaptionCue], t: MediaTime) -> Option<&CaptionCue> {
    cues.iter()
        .filter(|c| c.start <= t && t < c.end)
        .max_by_key(|c| c.start)
}

/// The compositor's caption layer at output time `t` for a frame of `output` pixels: the one
/// function the export and the editor's preview both call, so a caption appears on the same
/// frames, in the same place, in both.
pub fn caption_layer_at(
    cues: &[CaptionCue],
    t: MediaTime,
    output: (u32, u32),
) -> Option<CaptionLayer<'_>> {
    cue_at(cues, t).map(|c| CaptionLayer::for_output(&c.text, output))
}

/// The captions of one recording on its output timeline, for a preview that is driven by session
/// time (the playhead): [`CaptionTrack::layer_at`] maps the session time to output time with the
/// same timeline the export uses.
#[derive(Debug, Clone)]
pub struct CaptionTrack {
    cues: Vec<CaptionCue>,
    timeline: Timeline,
}

impl CaptionTrack {
    /// The cues the `source` makes for `timeline` (pauses cut like the export does: build the
    /// timeline from `EditList::without_pauses`).
    pub fn new(source: &CaptionSource, timeline: Timeline) -> Result<Self, CmError> {
        Ok(CaptionTrack {
            cues: source(&timeline)?,
            timeline,
        })
    }

    pub fn cues(&self) -> &[CaptionCue] {
        &self.cues
    }

    /// The caption to draw at session time `session`, if the playhead is on a kept part of the
    /// recording and a cue covers its output time.
    pub fn layer_at(&self, session: MediaTime, output: (u32, u32)) -> Option<CaptionLayer<'_>> {
        let t = self.timeline.source_to_output(session)?;
        caption_layer_at(&self.cues, t, output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(a: i64, b: i64, text: &str) -> CaptionCue {
        CaptionCue {
            start: MediaTime::from_ticks(a),
            end: MediaTime::from_ticks(b),
            text: text.to_owned(),
        }
    }

    #[test]
    fn the_cue_at_a_time_is_the_one_that_covers_it() {
        let cues = vec![cue(0, 100, "a"), cue(100, 200, "b"), cue(300, 400, "c")];
        let at = |t: i64| cue_at(&cues, MediaTime::from_ticks(t)).map(|c| c.text.as_str());
        assert_eq!(at(0), Some("a"));
        assert_eq!(at(99), Some("a"));
        assert_eq!(
            at(100),
            Some("b"),
            "the end is exclusive, the start inclusive"
        );
        assert_eq!(at(250), None, "a gap between lines shows nothing");
        assert_eq!(at(399), Some("c"));
        assert_eq!(at(400), None);
        assert_eq!(at(-1), None);
    }

    /// The preview is driven by session time: a track maps it through the export's timeline, so the
    /// caption shows on the same instants. A 2 s cut at 2 to 4 s moves session 5 s to output 3 s,
    /// and the playhead inside the cut shows nothing.
    #[test]
    fn a_track_maps_session_time_through_the_timeline_like_the_export() {
        use cm_core::edit::{EditContext, EditList, Span};
        let s = |secs: i64| MediaTime::from_ticks(secs * 120_000);
        let mut l = EditList::full(&EditContext {
            end: s(10),
            live_at: None,
            gaps: &[],
        });
        l.cuts = vec![Span::new(s(2), s(4))];
        let source: CaptionSource =
            Arc::new(move |_| Ok(vec![cue(3 * 120_000, 4 * 120_000, "Hi")]));
        let track = CaptionTrack::new(&source, l.timeline()).unwrap();
        let at = |t: MediaTime| track.layer_at(t, (1920, 1080)).map(|c| c.text.to_owned());
        assert_eq!(
            at(MediaTime::from_ticks(5 * 120_000 + 60_000)),
            Some("Hi".into())
        );
        assert_eq!(at(s(3)), None, "inside the cut");
        assert_eq!(at(s(8)), None, "after the cue");
        assert_eq!(at(s(1)), None, "before the cue");
        // The layer is what the export draws at the same output time.
        let direct = caption_layer_at(
            track.cues(),
            MediaTime::from_ticks(3 * 120_000 + 1),
            (1920, 1080),
        );
        assert_eq!(
            direct,
            track.layer_at(MediaTime::from_ticks(5 * 120_000 + 1), (1920, 1080))
        );
    }

    #[test]
    fn the_later_cue_wins_when_two_overlap() {
        let cues = vec![cue(0, 200, "early"), cue(100, 300, "late")];
        let t = MediaTime::from_ticks(150);
        assert_eq!(cue_at(&cues, t).unwrap().text, "late");
    }
}
