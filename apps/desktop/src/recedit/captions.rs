//! The caption track the editor's preview draws (AI-1): switched on with `set_caption_options`
//! (`enabled`) and only when the recording has a transcript. It is rebuilt when what it is made
//! from changes, never per frame: the transcript was read again, caption lines were saved, a
//! switch flipped, or the edit list changed (cuts and speed move the captions on the output
//! timeline). Display-free: the builder is injected, so the rules are tested without files.

use std::path::Path;
use std::sync::Arc;

use cm_core::EditList;
use cm_media::CaptionTrack;

/// What the preview's caption track was built for, and the track.
#[derive(Default)]
pub struct PreviewCaptions {
    track: Option<Arc<CaptionTrack>>,
    /// Changes whenever the track (or its absence) changed, so a cached picture is not reused.
    id: u64,
    built_for: Option<(bool, u64, EditList)>,
}

impl PreviewCaptions {
    /// The track to hand to the renderer; `None` draws no captions.
    pub fn track(&self) -> Option<Arc<CaptionTrack>> {
        self.track.clone()
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Brings the track up to date. `on` is "captions are switched on and a transcript exists",
    /// `rev` counts the changes of the lines and the switches, `list` is the edit list in force.
    /// `build` makes the track (only called when it is needed). Returns whether the track
    /// changed, so the caller knows a new picture is wanted.
    pub fn sync(
        &mut self,
        on: bool,
        rev: u64,
        list: &EditList,
        build: impl FnOnce() -> Option<CaptionTrack>,
    ) -> bool {
        if let Some((o, r, l)) = &self.built_for {
            // Off stays off whatever the edit list does: nothing to rebuild.
            let same_list = !on || l == list;
            if *o == on && *r == rev && same_list {
                return false;
            }
        }
        self.built_for = Some((on, rev, list.clone()));
        let had = self.track.is_some();
        self.track = if on { build().map(Arc::new) } else { None };
        if had || self.track.is_some() {
            self.id += 1;
            true
        } else {
            false
        }
    }
}

/// The track for a recording: the user's edited lines, else lines built from its transcript,
/// laid on the timeline of the edit list as it plays and exports (pauses cut like the export).
/// `None` when there is nothing to show or the bundle cannot be read.
pub fn build_track(bundle: &Path, capture_id: &str, played: &EditList) -> Option<CaptionTrack> {
    let manifest = cm_library::read_manifest(bundle).ok()?;
    let manifest = serde_json::to_value(&manifest).ok()?;
    let source =
        cm_daemon::caption_options::burn_in_source(capture_id, bundle, &manifest, false).ok()??;
    CaptionTrack::new(&source, played.timeline()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::{EditContext, MediaTime, Span};
    use cm_media::CaptionCue;

    fn list(secs: f64) -> EditList {
        EditList::full(&EditContext {
            end: MediaTime::from_secs_f64(secs).unwrap(),
            live_at: None,
            gaps: &[],
        })
    }

    fn track(l: &EditList, text: &str) -> CaptionTrack {
        let text = text.to_owned();
        let source: cm_media::CaptionSource = Arc::new(move |_tl| {
            Ok(vec![CaptionCue {
                start: MediaTime::ZERO,
                end: MediaTime::from_secs_f64(2.0).unwrap(),
                text: text.clone(),
            }])
        });
        CaptionTrack::new(&source, l.timeline()).unwrap()
    }

    /// AI-1: captions are drawn only when switched on AND a transcript exists (`on`), and the
    /// track is not built at all otherwise.
    #[test]
    fn nothing_is_built_while_captions_are_off() {
        let l = list(10.0);
        let mut c = PreviewCaptions::default();
        let changed = c.sync(false, 1, &l, || panic!("must not build while off"));
        assert!(!changed && c.track().is_none() && c.id() == 0);
    }

    #[test]
    fn switching_on_builds_once_and_off_drops_the_track() {
        let l = list(10.0);
        let mut c = PreviewCaptions::default();
        assert!(c.sync(true, 1, &l, || Some(track(&l, "Hello"))));
        let id = c.id();
        assert!(c.track().is_some());
        // The same inputs: no rebuild, the same id (the cached picture stays valid).
        assert!(!c.sync(true, 1, &l, || panic!("no rebuild")));
        assert_eq!(c.id(), id);
        // Off: the track goes and the picture must be redrawn.
        assert!(c.sync(false, 2, &l, || panic!("no build")));
        assert!(c.track().is_none() && c.id() > id);
    }

    #[test]
    fn a_new_revision_or_a_changed_edit_list_rebuilds() {
        let l = list(10.0);
        let mut c = PreviewCaptions::default();
        c.sync(true, 1, &l, || Some(track(&l, "a")));
        // Lines were saved or a transcript arrived: the revision moved.
        let before = c.id();
        assert!(c.sync(true, 2, &l, || Some(track(&l, "b"))));
        assert!(c.id() > before);
        // A cut moves the captions on the output timeline.
        let mut cut = l.clone();
        cut.cuts = vec![Span::new(
            MediaTime::from_secs_f64(1.0).unwrap(),
            MediaTime::from_secs_f64(2.0).unwrap(),
        )];
        let before = c.id();
        assert!(c.sync(true, 2, &cut, || Some(track(&cut, "b"))));
        assert!(c.id() > before);
    }

    /// While off, edit-list changes do not matter and cost nothing.
    #[test]
    fn edits_while_off_do_not_rebuild() {
        let l = list(10.0);
        let mut c = PreviewCaptions::default();
        c.sync(false, 1, &l, || None);
        let mut other = l.clone();
        other.end = MediaTime::from_secs_f64(5.0).unwrap();
        assert!(!c.sync(false, 1, &other, || panic!("no build")));
    }

    /// An unreadable or empty source leaves the preview without captions instead of failing.
    #[test]
    fn a_failed_build_shows_no_captions() {
        let l = list(10.0);
        let mut c = PreviewCaptions::default();
        assert!(!c.sync(true, 1, &l, || None));
        assert!(c.track().is_none());
    }

    /// The caption shows on the playhead's frame only inside a cue and on a kept part.
    #[test]
    fn the_track_shows_a_caption_at_the_playhead() {
        let l = list(10.0);
        let t = track(&l, "Hello there");
        let at = |s: f64| {
            t.layer_at(MediaTime::from_secs_f64(s).unwrap(), (1280, 720))
                .is_some()
        };
        assert!(at(1.0));
        assert!(!at(5.0), "past the cue, nothing is shown");
    }
}
