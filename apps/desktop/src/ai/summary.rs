//! What the recording details page knows about a recording's transcript, display-free: a cache of
//! the daemon's summary per recording (so the page shows the stored transcript after it is
//! closed and opened again), the wording of the summary line, and the draft of a "Re-generate"
//! (model and language for this run, and the confirmation when caption edits would be lost).

use std::collections::HashMap;

use cm_i18n::{tr, tr_ctx, trn};

use super::client::{CaptionOptionsInfo, TranscriptInfo};
use crate::daemon_text::ErrorNote;

/// What is known about one recording's transcript.
#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptState {
    /// The daemon is being asked.
    Loading,
    /// The recording has none.
    Missing,
    Present(TranscriptInfo),
    /// The daemon could not say (the message is shown instead of pretending there is none).
    Failed(ErrorNote),
}

#[derive(Debug, Clone)]
struct Entry {
    state: TranscriptState,
    /// The `info_epoch` this answer belongs to.
    epoch: u64,
}

/// The cache. `epoch` is a counter bumped whenever the answer may have changed (a transcript was
/// written, a revision such as edited captions was made); an older entry is read again.
#[derive(Debug, Clone, Default)]
pub struct TranscriptCache {
    entries: HashMap<String, Entry>,
}

impl TranscriptCache {
    pub fn state(&self, capture_id: &str) -> Option<&TranscriptState> {
        self.entries.get(capture_id).map(|e| &e.state)
    }

    pub fn info(&self, capture_id: &str) -> Option<&TranscriptInfo> {
        match self.state(capture_id)? {
            TranscriptState::Present(i) => Some(i),
            _ => None,
        }
    }

    /// Shows new caption switches on the cached summary at once (the page does not wait for the
    /// daemon). Returns the previous switches, to put back if the daemon refuses; `None` when the
    /// recording has no cached transcript.
    pub fn set_options(
        &mut self,
        capture_id: &str,
        options: CaptionOptionsInfo,
    ) -> Option<CaptionOptionsInfo> {
        match &mut self.entries.get_mut(capture_id)?.state {
            TranscriptState::Present(i) => Some(std::mem::replace(&mut i.options, options)),
            _ => None,
        }
    }

    /// Whether the page must ask the daemon now: never asked, or the answer is from an older
    /// epoch. A request in flight is not repeated. While it loads, the previous answer stays
    /// visible (no flicker back to "no transcript").
    pub fn needs_load(&self, capture_id: &str, epoch: u64) -> bool {
        match self.entries.get(capture_id) {
            None => true,
            Some(e) => e.state != TranscriptState::Loading && e.epoch != epoch,
        }
    }

    /// Marks a read as started; keeps a previous answer on screen until the new one lands.
    pub fn begin(&mut self, capture_id: &str, epoch: u64) {
        match self.entries.get_mut(capture_id) {
            Some(e) => {
                if !matches!(e.state, TranscriptState::Present(_)) {
                    e.state = TranscriptState::Loading;
                }
                e.epoch = epoch;
            }
            None => {
                self.entries.insert(
                    capture_id.to_owned(),
                    Entry {
                        state: TranscriptState::Loading,
                        epoch,
                    },
                );
            }
        }
    }

    pub fn set(&mut self, capture_id: &str, epoch: u64, state: TranscriptState) {
        self.entries
            .insert(capture_id.to_owned(), Entry { state, epoch });
    }
}

/// Languages offered for a re-generate (code, English label); the first is auto-detect. The label
/// is the English source text: show [`language_name`] to the user, which translates it.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("auto", "Auto"),     // i18n-ignore: shown through language_name()
    ("en", "English"),    // i18n-ignore: shown through language_name()
    ("vi", "Vietnamese"), // i18n-ignore: shown through language_name()
    ("es", "Spanish"),    // i18n-ignore: shown through language_name()
    ("fr", "French"),     // i18n-ignore: shown through language_name()
    ("de", "German"),     // i18n-ignore: shown through language_name()
    ("pt", "Portuguese"), // i18n-ignore: shown through language_name()
    ("ja", "Japanese"),   // i18n-ignore: shown through language_name()
    ("zh", "Chinese"),    // i18n-ignore: shown through language_name()
];

/// The translated name of a language code from [`LANGUAGES`] (`None` for any other code).
pub fn language_name(code: &str) -> Option<String> {
    let code = LANGUAGES
        .iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(code))?
        .0;
    Some(match code {
        "auto" => tr_ctx!("transcript language", "Auto"),
        "en" => tr_ctx!("transcript language", "English"),
        "vi" => tr_ctx!("transcript language", "Vietnamese"),
        "es" => tr_ctx!("transcript language", "Spanish"),
        "fr" => tr_ctx!("transcript language", "French"),
        "de" => tr_ctx!("transcript language", "German"),
        "pt" => tr_ctx!("transcript language", "Portuguese"),
        "ja" => tr_ctx!("transcript language", "Japanese"),
        "zh" => tr_ctx!("transcript language", "Chinese"),
        _ => return None,
    })
}

/// The name of a language code for the summary line (the code itself when it is not in the list).
pub fn language_label(code: &str) -> String {
    language_name(code).unwrap_or_else(|| code.to_owned())
}

/// "3 h ago" and friends, from the transcript's Unix time and now.
pub fn made_text(made_at: Option<u64>, now: u64) -> Option<String> {
    let made = made_at?;
    let ago = now.saturating_sub(made);
    Some(match ago {
        0..=44 => tr!("just now"),
        45..=3_599 => tr!("{n} min ago", n = (ago + 30) / 60),
        3_600..=86_399 => tr!("{n} h ago", n = ago / 3_600),
        86_400..=2_591_999 => {
            let d = ago / 86_400;
            if d == 1 {
                tr!("yesterday")
            } else {
                trn!("{n} day ago", "{n} days ago", d)
            }
        }
        _ => tr!("over a month ago"),
    })
}

/// "English · small · 27 words · 3 h ago".
pub fn summary_line(info: &TranscriptInfo, now: u64) -> String {
    let mut parts = vec![
        language_label(&info.language),
        info.model.clone(),
        trn!("{n} word", "{n} words", info.words),
    ];
    if let Some(when) = made_text(info.made_at, now) {
        parts.push(when);
    }
    parts.join(" \u{00B7} ")
}

/// The model and language chosen for one re-generate, before the user starts it.
#[derive(Debug, Clone, PartialEq)]
pub struct RegenDraft {
    /// An installed model id; `None` = the active model.
    pub model: Option<String>,
    /// A code from [`LANGUAGES`].
    pub language: String,
}

impl Default for RegenDraft {
    fn default() -> Self {
        RegenDraft {
            model: None,
            language: "auto".to_owned(),
        }
    }
}

impl RegenDraft {
    /// The `language` argument of `transcribe`: `None` for auto-detect.
    pub fn language_arg(&self) -> Option<&str> {
        (self.language != "auto").then_some(self.language.as_str())
    }
}

/// The sentence the Re-generate panel shows. When the user edited the captions it says plainly
/// what happens to them (AI-6a): an edit stays where the same words are heard again, and Undo
/// brings the previous captions back.
pub fn regen_warning(edited: bool) -> String {
    if edited {
        tr!("Re-generating replaces this transcript. Your caption edits are kept where the same words are heard again; the others are dropped. You can undo that from the editor.")
    } else {
        tr!("Re-generating replaces this transcript with a new one.")
    }
}

/// What a finished re-generate did to the user's caption edits (the `reapplied` and `dropped`
/// counts of `transcript.ready`), for the toast. `None` when there were no edits to place.
pub fn kept_edits_note(reapplied: usize, dropped: usize) -> Option<String> {
    match (reapplied, dropped) {
        (0, 0) => None,
        (kept, 0) => Some(trn!(
            "{n} caption edit was kept where the same words were heard again.",
            "{n} caption edits were kept where the same words were heard again.",
            kept
        )),
        (0, lost) => Some(trn!(
            "{n} caption edit could not be kept because its words changed.",
            "{n} caption edits could not be kept because their words changed.",
            lost
        )),
        (kept, lost) => Some(trn!(
            "{kept} of {n} caption edit was kept; the rest could not be matched.",
            "{kept} of {n} caption edits were kept; the rest could not be matched.",
            kept + lost,
            kept = kept
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> TranscriptInfo {
        TranscriptInfo {
            language: "en".into(),
            model: "small".into(),
            words: 27,
            made_at: Some(1_000),
            edited: false,
            options: Default::default(),
        }
    }

    /// The caption switches on the details page move at once and can be put back.
    #[test]
    fn caption_switches_update_the_cached_summary_and_can_be_restored() {
        let mut c = TranscriptCache::default();
        assert_eq!(
            c.set_options("rec_1", Default::default()),
            None,
            "nothing cached"
        );
        c.set("rec_1", 1, TranscriptState::Present(info()));
        let on = CaptionOptionsInfo {
            enabled: true,
            burn_in: true,
        };
        let before = c.set_options("rec_1", on).unwrap();
        assert_eq!(before, CaptionOptionsInfo::default());
        assert_eq!(c.info("rec_1").unwrap().options, on);
        c.set_options("rec_1", before);
        assert_eq!(
            c.info("rec_1").unwrap().options,
            CaptionOptionsInfo::default()
        );
        // A missing transcript has no switches to change.
        c.set("rec_2", 1, TranscriptState::Missing);
        assert_eq!(c.set_options("rec_2", on), None);
    }

    /// The reported bug: close the details and open them again, and the page must still show the
    /// stored transcript, not "no transcript yet". A cached answer is used until something may
    /// have changed it.
    #[test]
    fn a_stored_transcript_stays_visible_when_the_page_is_reopened() {
        let mut c = TranscriptCache::default();
        assert!(c.needs_load("rec_1", 0));
        c.begin("rec_1", 0);
        assert_eq!(c.state("rec_1"), Some(&TranscriptState::Loading));
        assert!(
            !c.needs_load("rec_1", 0),
            "a read in flight is not repeated"
        );
        c.set("rec_1", 0, TranscriptState::Present(info()));
        // Closed and opened again: same epoch, nothing to read, the transcript is there.
        assert!(!c.needs_load("rec_1", 0));
        assert_eq!(c.info("rec_1"), Some(&info()));
    }

    /// A newer epoch (transcript.ready, a revision) triggers a re-read, and the old answer stays
    /// on screen meanwhile instead of flashing "Transcribe".
    #[test]
    fn a_newer_epoch_rereads_and_keeps_the_old_answer_meanwhile() {
        let mut c = TranscriptCache::default();
        c.set("rec_1", 0, TranscriptState::Present(info()));
        assert!(c.needs_load("rec_1", 1));
        c.begin("rec_1", 1);
        assert_eq!(c.info("rec_1"), Some(&info()));
        assert!(!c.needs_load("rec_1", 1));
        let mut newer = info();
        newer.edited = true;
        c.set("rec_1", 1, TranscriptState::Present(newer.clone()));
        assert_eq!(c.info("rec_1"), Some(&newer));
        // A recording that had none starts as loading, then missing.
        c.begin("rec_2", 1);
        c.set("rec_2", 1, TranscriptState::Missing);
        assert_eq!(c.state("rec_2"), Some(&TranscriptState::Missing));
    }

    #[test]
    fn a_failed_read_says_so() {
        let mut c = TranscriptCache::default();
        c.set("rec_1", 0, TranscriptState::Failed("boom".into()));
        assert!(matches!(c.state("rec_1"), Some(TranscriptState::Failed(m)) if m.text() == "boom"));
        assert_eq!(c.info("rec_1"), None);
        // It is read again on the next epoch.
        assert!(c.needs_load("rec_1", 1));
    }

    #[test]
    fn the_summary_line_reads_naturally() {
        assert_eq!(
            summary_line(&info(), 1_000 + 3 * 3_600),
            "English \u{00B7} small \u{00B7} 27 words \u{00B7} 3 h ago"
        );
        let mut one = info();
        one.words = 1;
        one.made_at = None;
        one.language = "xx".into();
        assert_eq!(summary_line(&one, 5), "xx \u{00B7} small \u{00B7} 1 word");
        assert_eq!(made_text(Some(100), 110).as_deref(), Some("just now"));
        assert_eq!(
            made_text(Some(100), 100 + 600).as_deref(),
            Some("10 min ago")
        );
        assert_eq!(made_text(Some(0), 86_400).as_deref(), Some("yesterday"));
        assert_eq!(
            made_text(Some(0), 3 * 86_400).as_deref(),
            Some("3 days ago")
        );
        assert_eq!(made_text(None, 5), None);
    }

    #[test]
    fn a_draft_defaults_to_the_active_model_and_auto_language() {
        let d = RegenDraft::default();
        assert_eq!(d.model, None);
        assert_eq!(d.language_arg(), None);
        let d = RegenDraft {
            model: Some("medium".into()),
            language: "vi".into(),
        };
        assert_eq!(d.language_arg(), Some("vi"));
        assert_eq!(LANGUAGES[0].0, "auto");
    }

    /// Edited captions are named as lost, with the way back.
    #[test]
    fn the_warning_names_what_happens_to_edits() {
        assert!(regen_warning(true).contains("caption edits are kept"));
        assert!(regen_warning(true).contains("dropped"));
        assert!(regen_warning(true).contains("undo"));
        assert!(!regen_warning(false).contains("discard"));
        assert!(!regen_warning(false).contains("kept"));
    }

    /// The toast after a re-generate says how many edits stayed and how many did not (AI-6a).
    #[test]
    fn the_kept_edits_note_counts_with_plurals() {
        assert_eq!(kept_edits_note(0, 0), None);
        assert_eq!(
            kept_edits_note(1, 0).unwrap(),
            "1 caption edit was kept where the same words were heard again."
        );
        assert_eq!(
            kept_edits_note(4, 0).unwrap(),
            "4 caption edits were kept where the same words were heard again."
        );
        assert_eq!(
            kept_edits_note(0, 1).unwrap(),
            "1 caption edit could not be kept because its words changed."
        );
        assert_eq!(
            kept_edits_note(0, 3).unwrap(),
            "3 caption edits could not be kept because their words changed."
        );
        assert_eq!(
            kept_edits_note(2, 1).unwrap(),
            "2 of 3 caption edits were kept; the rest could not be matched."
        );
    }

    /// Counts go through `trn!`, never `if n == 1`; a language without the translation keeps the
    /// placeholders filled (I18N P2).
    #[test]
    fn counts_use_plural_messages_and_keep_their_placeholders_in_any_language() {
        let mut one = info();
        one.words = 1;
        assert!(
            summary_line(&one, 5).contains("1 word") && !summary_line(&one, 5).contains("words")
        );
        let mut none = info();
        none.words = 0;
        assert!(summary_line(&none, 5).contains("0 words"));
        assert_eq!(
            made_text(Some(0), 2 * 86_400).as_deref(),
            Some("2 days ago")
        );
        cm_i18n::with_language(cm_i18n::Lang::Vi, || {
            let line = summary_line(&info(), info().made_at.unwrap_or(0) + 3 * 3_600);
            assert!(line.contains("27"), "{line}");
            assert!(!line.contains("{n}"), "{line}");
        });
    }
}
