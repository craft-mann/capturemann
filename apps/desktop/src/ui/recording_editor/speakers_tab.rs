//! The Speakers tab (redesign V5, AI-8). A stub: the tab exists in both rail sets so its id
//! (`re-tab-speakers`) and this file stay stable; the panel is built later (Stage 6 of
//! `specs/plans/voice-implementation-plan.md`).
//!
//! What the Transcript tab already shows when a recording has speakers (names, colours, turns,
//! the lane) is read from `recedit::voice` and the manifest; renaming, merging and "Find speakers"
//! go through `set_speakers` and `diarize`, which this file will call.

use cm_i18n::tr;
use gpui::{prelude::*, AnyElement, Context};

use super::tabs::{hint_box, panel, Inspector};
use super::{RecordingEditor, Session};
use crate::theme::Theme;

impl RecordingEditor {
    pub(super) fn speakers_panel(
        &self,
        t: &Theme,
        _s: &Session,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        // TODO(design): V5 "Speakers" tab. Labelled state: one row per speaker (colour, inline
        // rename, share of talk time with a bar, Play sample / Rename / Merge into...), the "My
        // microphone only hears me" switch and the privacy line. Not-run state: an empty card,
        // "How many people?", the no-model notice and Open AI page, with Find speakers disabled.
        panel(
            t,
            Inspector::Speakers,
            tr!("Who said what"),
            vec![hint_box(
                t,
                "users",
                tr!("Speaker labels are coming here: names, talk time and merging."),
            )
            .into_any_element()],
            Vec::new(),
        )
    }
}
