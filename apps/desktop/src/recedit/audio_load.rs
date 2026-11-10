//! How the editor's sound loads after the window is open (VOC-2.AC6 to AC10).
//!
//! Opening a long recording used to wait for the whole sound to be decoded (a 4 hour file took
//! 21 s) before the window showed anything. Now everything else loads first and the window opens
//! at once; the sound decodes in the background. This module is the pure part: the state of that
//! decode, what the transport says about it, whether Play has to wait, and the two times
//! (`ui_ready`, `audio_ready`) the diagnostics report. No window, no thread, no FFmpeg.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cm_i18n::tr;

/// Where the sound of the open recording is.
#[derive(Debug, Clone, PartialEq)]
pub enum SoundState {
    /// The recording has no sound tracks (or the load is over and nothing is left to wait for).
    None,
    /// Decoding in the background; `frac` is 0.0 to 0.99 (never 1.0 before it is ready).
    Preparing { frac: f64 },
    /// The sound is in memory and the engine is open.
    Ready,
    /// The decode or the output device failed; the text says why (shown in the status strip).
    Failed(String),
    /// The user left the recording while it was decoding.
    Cancelled,
}

/// The state machine of the sound load.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundLoad {
    state: SoundState,
}

impl SoundLoad {
    /// A recording without sound tracks.
    pub fn none() -> SoundLoad {
        SoundLoad {
            state: SoundState::None,
        }
    }

    /// The decode is about to start.
    pub fn preparing() -> SoundLoad {
        SoundLoad {
            state: SoundState::Preparing { frac: 0.0 },
        }
    }

    pub fn state(&self) -> &SoundState {
        &self.state
    }

    pub fn is_preparing(&self) -> bool {
        matches!(self.state, SoundState::Preparing { .. })
    }

    /// Play waits while the sound prepares (it starts by itself when the sound is ready): the
    /// playhead does not move and no clock runs, so the picture and the sound start together.
    pub fn holds_playback(&self) -> bool {
        self.is_preparing()
    }

    /// The decode reported `frac`; the shown value only grows and stays under 100 % until
    /// [`SoundLoad::ready`]. Ignored once the load is over.
    pub fn progress(&mut self, frac: f64) {
        if let SoundState::Preparing { frac: f } = &mut self.state {
            *f = f.max(frac.clamp(0.0, 0.99));
        }
    }

    pub fn ready(&mut self) {
        if self.is_preparing() {
            self.state = SoundState::Ready;
        }
    }

    pub fn failed(&mut self, why: String) {
        if self.is_preparing() {
            self.state = SoundState::Failed(why);
        }
    }

    /// The user left the recording.
    pub fn cancel(&mut self) {
        if self.is_preparing() {
            self.state = SoundState::Cancelled;
        }
    }

    /// The transport's line while the sound prepares: `Preparing the sound\u{2026} 40 %`.
    pub fn label(&self) -> Option<String> {
        match self.state {
            SoundState::Preparing { frac } => Some(tr!(
                "Preparing the sound\u{2026} {pct} %",
                pct = (frac * 100.0).floor() as u32
            )),
            _ => None,
        }
    }

    /// A failure to show in the status strip.
    pub fn failure(&self) -> Option<&str> {
        match &self.state {
            SoundState::Failed(why) => Some(why),
            _ => None,
        }
    }
}

/// Set when the session that owns it is dropped (the user opened another recording or left the
/// editor): the background decode sees it, kills FFmpeg and drops what it read.
#[derive(Debug, Default)]
pub struct CancelOnDrop(Arc<AtomicBool>);

impl CancelOnDrop {
    pub fn flag(&self) -> Arc<AtomicBool> {
        self.0.clone()
    }

    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// The two times of an open, in seconds since the click: when the window showed and when the
/// sound was ready. Both go to the diagnostics as phases of the run `editor_load`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OpenTimes {
    pub ui_ready: Option<f64>,
    pub audio_ready: Option<f64>,
}

impl OpenTimes {
    pub fn ui(&mut self, secs: f64) {
        self.ui_ready.get_or_insert(secs);
    }

    /// The sound is ready; it cannot be before the window (the sound starts after it).
    pub fn audio(&mut self, secs: f64) {
        let floor = self.ui_ready.unwrap_or(0.0);
        self.audio_ready.get_or_insert(secs.max(floor));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VOC-2.AC6: the window opens first, the sound becomes ready later.
    #[test]
    fn the_window_is_ready_before_the_sound() {
        let mut t = OpenTimes::default();
        t.ui(0.4);
        let mut s = SoundLoad::preparing();
        assert!(s.is_preparing() && t.audio_ready.is_none());
        s.progress(0.5);
        s.ready();
        t.audio(22.0);
        assert_eq!(*s.state(), SoundState::Ready);
        assert_eq!((t.ui_ready, t.audio_ready), (Some(0.4), Some(22.0)));
        // A sound that reports before the window cannot make the window later.
        let mut early = OpenTimes::default();
        early.audio(0.1);
        early.ui(0.3);
        assert_eq!(early.audio_ready, Some(0.1));
    }

    /// VOC-2.AC7: the transport says how far the sound is; the value only grows, stays under
    /// 100 % until ready, and disappears when ready.
    #[test]
    fn the_label_follows_the_progress() {
        let mut s = SoundLoad::preparing();
        assert_eq!(s.label().unwrap(), "Preparing the sound\u{2026} 0 %");
        s.progress(0.4);
        assert_eq!(s.label().unwrap(), "Preparing the sound\u{2026} 40 %");
        s.progress(0.2);
        assert!(s.label().unwrap().contains("40 %"), "never goes back");
        s.progress(7.0);
        assert!(
            s.label().unwrap().contains("99 %"),
            "never 100 % before ready"
        );
        s.ready();
        assert!(s.label().is_none());
    }

    /// VOC-2.AC7: Play pressed early waits; once the sound is ready (or failed) it does not.
    #[test]
    fn play_waits_for_the_sound_and_then_starts() {
        let mut s = SoundLoad::preparing();
        assert!(s.holds_playback());
        s.ready();
        assert!(!s.holds_playback());
        let mut f = SoundLoad::preparing();
        f.failed("No sound in the preview: x".into());
        assert!(!f.holds_playback(), "a failed sound must not freeze Play");
        assert_eq!(f.failure(), Some("No sound in the preview: x"));
        assert!(!SoundLoad::none().holds_playback());
    }

    /// VOC-2.AC9: leaving cancels; a late result changes nothing.
    #[test]
    fn leaving_cancels_and_a_late_result_is_ignored() {
        let mut s = SoundLoad::preparing();
        s.cancel();
        assert_eq!(*s.state(), SoundState::Cancelled);
        s.ready();
        s.progress(0.9);
        s.failed("late".into());
        assert_eq!(*s.state(), SoundState::Cancelled);
        assert!(!s.holds_playback());
        // Ready stays ready when a cancel arrives afterwards.
        let mut r = SoundLoad::preparing();
        r.ready();
        r.cancel();
        assert_eq!(*r.state(), SoundState::Ready);
    }

    /// VOC-2.AC9: dropping the owner sets the flag the decode watches.
    #[test]
    fn dropping_the_owner_sets_the_cancel_flag() {
        let owner = CancelOnDrop::default();
        let flag = owner.flag();
        assert!(!flag.load(Ordering::Relaxed) && !owner.is_set());
        drop(owner);
        assert!(flag.load(Ordering::Relaxed));
    }
}
