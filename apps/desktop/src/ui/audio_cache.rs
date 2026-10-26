//! What the Library > Audio cards draw besides the title (VOC-1.AC10, board V3): the waveform
//! bars and the card facts (preset, speakers, transcript, report). Read from the bundle with the
//! headless helpers in `cm_library::audio` (read-only; `peaks.json` is the only file they write,
//! a cache), off the UI thread, and kept for the cards on screen.

use gpui::Context;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use cm_i18n::{tr, tr_ctx};
use cm_library::audio::{bundle_peaks, card_info, AudioCardInfo};

use crate::backend::Backend;

/// The waveform and facts of one audio capture.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AudioInfo {
    /// Bars of 0 to 255; empty when the bundle has no readable track.
    pub peaks: Vec<u8>,
    pub card: AudioCardInfo,
}

/// Reads one capture's [`AudioInfo`] from its bundle folder.
pub fn read_info(dir: &Path) -> AudioInfo {
    AudioInfo {
        peaks: bundle_peaks(dir).unwrap_or_default(),
        card: card_info(dir),
    }
}

/// `n` bars from `peaks`: the loudest of each group when there are more peaks than bars, the
/// nearest peak when there are fewer. Empty in, empty out.
pub fn bars_for(peaks: &[u8], n: usize) -> Vec<u8> {
    if peaks.is_empty() || n == 0 {
        return Vec::new();
    }
    (0..n)
        .map(|i| {
            let from = i * peaks.len() / n;
            let to = ((i + 1) * peaks.len() / n).clamp(from + 1, peaks.len());
            peaks[from..to].iter().copied().max().unwrap_or(0)
        })
        .collect()
}

/// The preset's label and icon (board V1 names): `room`, `online`, `lecture`, `product`, `note`.
/// Anything else (or none) is the generic "Audio" chip.
pub fn preset_chip(preset: Option<&str>) -> (&'static str, String) {
    match preset {
        Some("room") => ("users", tr_ctx!("audio preset", "In the room")),
        Some("online") => ("monitor-speaker", tr_ctx!("audio preset", "Online meeting")),
        Some("lecture") => ("graduation-cap", tr_ctx!("audio preset", "Lecture")),
        Some("product") => ("presentation", tr_ctx!("audio preset", "Product demo")),
        Some("note") => ("file", tr_ctx!("audio preset", "Voice note")),
        _ => ("audio-lines", tr_ctx!("audio preset", "Audio")),
    }
}

/// The status badges of a card, in the board's order: transcript, speakers, report. Each is
/// `(icon, tooltip)`.
pub fn badges(card: &AudioCardInfo) -> Vec<(&'static str, String)> {
    let mut v = Vec::new();
    if card.has_transcript {
        v.push(("captions", tr!("Transcript")));
    }
    if card.speakers.is_some() {
        v.push(("users", tr_ctx!("audio badge", "Speakers")));
    }
    if card.has_report {
        v.push(("file-text", tr_ctx!("audio badge", "Report")));
    }
    v
}

enum Entry {
    Pending,
    Ready(Arc<AudioInfo>),
}

/// Cache of [`AudioInfo`] by capture id, filled on first request.
pub struct AudioCache {
    backend: Arc<Backend>,
    entries: HashMap<String, Entry>,
}

impl AudioCache {
    pub fn new(backend: Arc<Backend>) -> Self {
        AudioCache {
            backend,
            entries: HashMap::new(),
        }
    }

    /// The info when it is loaded; starts loading otherwise (the cache notifies when done).
    pub fn get(&mut self, id: &str, cx: &mut Context<Self>) -> Option<Arc<AudioInfo>> {
        match self.entries.get(id) {
            Some(Entry::Ready(i)) => return Some(i.clone()),
            Some(Entry::Pending) => return None,
            None => {}
        }
        let root = self.backend.library_root.clone()?;
        self.entries.insert(id.to_owned(), Entry::Pending);
        let id = id.to_owned();
        cx.spawn(async move |this, cx| {
            let key = id.clone();
            let info = cx
                .background_executor()
                .spawn(async move {
                    crate::recording::files::find_bundle(&root, &key)
                        .map(|dir| read_info(&dir))
                        .unwrap_or_default()
                })
                .await;
            this.update(cx, |this, cx| {
                this.entries.insert(id, Entry::Ready(Arc::new(info)));
                cx.notify();
            })
            .ok();
        })
        .detach();
        None
    }

    /// Forget one capture (a transcript or a speaker run changed it).
    pub fn invalidate(&mut self, id: &str, cx: &mut Context<Self>) {
        self.entries.remove(id);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_pool_and_stretch() {
        assert!(bars_for(&[], 4).is_empty());
        assert!(bars_for(&[1, 2], 0).is_empty());
        // More peaks than bars: the loudest of each group.
        assert_eq!(bars_for(&[1, 9, 2, 3, 8, 4], 3), [9, 3, 8]);
        // Fewer peaks than bars: each bar takes the peak under it, none is skipped or zero.
        let b = bars_for(&[10, 200], 6);
        assert_eq!(b.len(), 6);
        assert!(b.iter().all(|v| *v == 10 || *v == 200));
        assert_eq!(b[0], 10);
        assert_eq!(b[5], 200);
    }

    #[test]
    fn preset_chips_follow_the_board() {
        assert_eq!(preset_chip(Some("online")).1, "Online meeting");
        assert_eq!(preset_chip(Some("room")).1, "In the room");
        assert_eq!(preset_chip(Some("lecture")).1, "Lecture");
        assert_eq!(preset_chip(Some("product")).1, "Product demo");
        assert_eq!(preset_chip(Some("note")).1, "Voice note");
        assert_eq!(preset_chip(None).1, "Audio");
        assert_eq!(preset_chip(Some("other")).0, "audio-lines");
        let registered = |name: &str| {
            let path = format!("icons/{name}.svg");
            crate::ui::paint::ASSETS.iter().any(|(p, _)| *p == path)
        };
        for p in [
            None,
            Some("room"),
            Some("online"),
            Some("lecture"),
            Some("product"),
            Some("note"),
        ] {
            assert!(registered(preset_chip(p).0));
        }
    }

    #[test]
    fn badges_show_only_what_exists() {
        let none = AudioCardInfo::default();
        assert!(badges(&none).is_empty());
        let all = AudioCardInfo {
            preset: None,
            speakers: Some(2),
            has_transcript: true,
            has_report: true,
        };
        let names: Vec<_> = badges(&all).into_iter().map(|b| b.1).collect();
        assert_eq!(names, ["Transcript", "Speakers", "Report"]);
        for (icon, _) in badges(&all) {
            assert!(crate::ui::paint::ASSETS
                .iter()
                .any(|(p, _)| *p == format!("icons/{icon}.svg")));
        }
    }

    /// A bundle folder with a WAV and a transcript gives bars and a badge; an empty folder gives
    /// neither.
    #[test]
    fn reads_a_bundle() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_info(dir.path()), AudioInfo::default());
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(dir.path().join("mic.wav"), spec).unwrap();
        for i in 0..8000 {
            w.write_sample((i % 100 * 200) as i16).unwrap();
        }
        w.finalize().unwrap();
        std::fs::write(dir.path().join("transcript.json"), "{}").unwrap();
        let info = read_info(dir.path());
        assert_eq!(info.peaks.len(), cm_library::audio::PEAK_BUCKETS);
        assert!(info.card.has_transcript);
    }
}
