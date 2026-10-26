//! The Report tab (redesign V5 and V6, AI-9). A stub: the tab exists in both rail sets so its id
//! (`re-tab-report`) and this file stay stable; the panel is built later (Stage 5 of
//! `specs/plans/voice-implementation-plan.md`).

use cm_i18n::tr;
use gpui::{prelude::*, AnyElement, Context};

use super::tabs::{hint_box, panel, Inspector};
use super::{RecordingEditor, Session};
use crate::theme::Theme;

impl RecordingEditor {
    pub(super) fn report_panel(
        &self,
        t: &Theme,
        _s: &Session,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        // TODO(design): V5 "Report" setup (six templates in a 2x3 grid, context, output language,
        // "Write meeting minutes" with the model line and the Local badge) and V6 "Report" in wide
        // mode (summary, decisions, action items, every item with a time chip that seeks).
        panel(
            t,
            Inspector::Report,
            tr!("Meeting minutes and notes"),
            vec![hint_box(
                t,
                "file-text",
                tr!("Reports are coming here: minutes, decisions and action items."),
            )
            .into_any_element()],
            Vec::new(),
        )
    }
}
