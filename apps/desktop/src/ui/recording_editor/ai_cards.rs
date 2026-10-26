//! The Captions and Suggestions cards of the inspector (AI-1, AI-3, AI-4). Drawing and events
//! only: the caption rules are in `crate::ai::captions`, the suggestion lists and what Accept
//! sends are in `crate::ai::suggest` (both unit tested), and everything goes to the daemon through
//! the actions of the contract in `crates/cm-ai/src/lib.rs`.
//!
//! Accepting a suggestion sends the existing `set_zooms` or `cut_segments` through the editor's own
//! request queue, so it is one undo step like any other edit. Nothing here downloads a model.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use cm_ai::doubtful::{doubtful_spans, Doubtful};
use cm_ai::suggest::{CutSuggestion, ZoomSuggestion};
use cm_ai::transcribe::Word;
use cm_core::MediaTime;
use cm_i18n::{tr, tr_ctx, trn};
use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, Role, SharedString, Subscription,
    TextLayout, Window,
};

use super::tabs::{hint, hint_box, icon_only, panel, Inspector};
use super::{RecordingEditor, Session};
use crate::ai::caption_find::FindState;
use crate::ai::caption_view::LinePlay;
use crate::ai::captions::{lines_from_words, AddAtError, CaptionEditor};
use crate::ai::client::{CaptionFormat, CaptionOptionsInfo};
use crate::ai::suggest::{
    cut_label, cut_request, nothing_found, zoom_label, zoom_request, Nothing, SuggestionList,
};
use crate::daemon_text::ErrorNote;
use crate::flows;
use crate::theme::{scale, type_scale, Theme};
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::ai;
use crate::ui::app;
use crate::ui::components::icon;
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::small_button;
use crate::ui::text_field::{CaptionKey, CaretAt, TextField, TextFieldEvent};

/// A line of the caption and suggestion cards that says what happened. It is kept as data and
/// worded when drawn, so a note still on screen follows a language switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AiNote {
    /// The transcript's own note, as the daemon wrote it.
    Transcript(String),
    /// The transcript could not be read.
    ReadFailed(ErrorNote),
    /// A caption could not be added at the playhead.
    AddAt(AddAtError),
    /// "Replace all" changed this many places.
    Replaced(usize),
    /// The suggestion came back empty.
    Nothing(Nothing),
    /// A suggestion call failed.
    Failed(ErrorNote),
}

impl AiNote {
    pub(super) fn text(&self) -> String {
        match self {
            AiNote::Transcript(t) => t.clone(),
            AiNote::ReadFailed(e) => {
                tr!("Couldn\u{2019}t read the transcript: {why}", why = e.text())
            }
            AiNote::AddAt(e) => e.message(),
            AiNote::Replaced(n) => trn!(
                "Replaced {n} place in the captions.",
                "Replaced {n} places in the captions.",
                *n
            ),
            AiNote::Nothing(n) => nothing_found(*n),
            AiNote::Failed(e) => e.text(),
        }
    }
}

/// What the caption view does on the next frame: the key and click handlers have no window, so
/// anything that moves the keyboard focus waits for the render.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pending {
    /// Start editing a line (saving the one being left), with the caret there.
    Edit { line: usize, at: CaretAt },
    /// Give the keyboard back to the editor (the shortcuts work again).
    Defocus,
    /// A doubtful word (AI-6a): edit its line with the caret at `byte` and move the playhead to
    /// the word.
    Word {
        line: usize,
        byte: usize,
        start: MediaTime,
    },
}

/// What the caption list remembers between frames without owning it: the drawing code only
/// has `&self`, so these are cells. Shared with closures through an `Rc`.
#[derive(Default)]
pub(super) struct CapView {
    /// When the user last scrolled the list by hand (auto-follow pauses for a while).
    pub(super) last_manual: Cell<Option<Instant>>,
    /// The line the list last scrolled to by itself.
    pub(super) last_followed: Cell<Option<usize>>,
    /// A line to scroll into view whatever else is going on (a lane click, a find step).
    pub(super) scroll_to: Cell<Option<usize>>,
    /// The painted text layout of each idle row, to turn a click into a caret position.
    pub(super) layouts: RefCell<HashMap<usize, TextLayout>>,
    /// A caption or find field had the keyboard when the frame began.
    pub(super) typing: Cell<bool>,
    /// The window's width when the frame began, px.
    pub(super) window_w: Cell<f32>,
}

/// The editor's AI state: the caption lines being edited and the suggestion lists.
pub(super) struct AiEditor {
    /// The recording the lines below belong to.
    pub(super) loaded_for: Option<String>,
    /// `TranscribeVm::epoch` when they were read: a newer transcript reloads them.
    pub(super) epoch_seen: u64,
    pub(super) loading: bool,
    pub(super) has_transcript: bool,
    /// The language the transcript was made in (`en`, `vi`, ...).
    pub(super) transcript_language: Option<String>,
    pub(super) note: Option<AiNote>,
    pub(super) captions: CaptionEditor,
    /// The in-place field of the line being edited.
    pub(super) field: Entity<TextField>,
    /// The line the text field edits.
    pub(super) field_for: Option<usize>,
    pub(super) saving: bool,
    /// A save was asked for while one was running: run it again when that one ends.
    pub(super) resave: bool,
    pub(super) zooms: Option<SuggestionList<ZoomSuggestion>>,
    pub(super) cuts: Option<SuggestionList<CutSuggestion>>,
    pub(super) asking_zooms: bool,
    pub(super) asking_cuts: bool,
    pub(super) suggest_note: Option<AiNote>,
    /// The recording's caption switches (show in the preview, burn into exports).
    pub(super) options: CaptionOptionsInfo,
    /// The transcript's words with their confidence (from the one `get_transcript` read that
    /// `ai_sync` already does): the doubtful words of the lines are found in them (AI-6a).
    pub(super) words: Rc<Vec<Word>>,
    /// The doubtful words of the current lines, with the fingerprint of what they were computed
    /// from, so typing recomputes them and a plain repaint does not.
    doubtful_cache: RefCell<Option<(u64, Rc<Vec<Doubtful>>)>>,
    /// A switch change is on its way to the daemon.
    pub(super) options_busy: bool,
    /// Bumped whenever what the preview's captions are built from changed: a transcript was read,
    /// lines were saved, a switch flipped. The session rebuilds its caption track when it moves.
    pub(super) rev: u64,
    /// Read the transcript and switches again (an undo may have changed them).
    pub(super) reload: bool,
    /// The find and replace bar.
    pub(super) find_open: bool,
    pub(super) find: FindState,
    pub(super) find_field: Entity<TextField>,
    pub(super) replace_field: Entity<TextField>,
    /// The line being played on its own ("play this line").
    pub(super) line_play: Option<LinePlay>,
    pub(super) pending: Option<Pending>,
    pub(super) view: Rc<CapView>,
    _subs: Vec<Subscription>,
}

impl AiEditor {
    /// A caption, find or replace field has the keyboard focus.
    pub(super) fn field_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        [&self.field, &self.find_field, &self.replace_field]
            .iter()
            .any(|f| f.read(cx).focus.is_focused(window))
    }

    pub(super) fn new(cx: &mut Context<RecordingEditor>) -> AiEditor {
        let field = cx.new(|cx| {
            TextField::new(tr!("Caption text"), tr!("Type the caption"), cx)
                .without_icon()
                .multiline(1, 4)
                .in_place()
        });
        let find_field = cx.new(|cx| {
            TextField::new(tr!("Find in the captions"), tr!("Find"), cx)
                .height(32.)
                .without_icon()
        });
        let replace_field = cx.new(|cx| {
            TextField::new(tr!("Replace with"), tr!("Replace with"), cx)
                .height(32.)
                .without_icon()
        });
        let subs = vec![
            cx.subscribe(&field, |this, _f, ev: &TextFieldEvent, cx| {
                this.ai_field_event(ev, cx)
            }),
            cx.subscribe(&field, |this, _f, ev: &CaptionKey, cx| {
                this.ai_caption_key(*ev, cx)
            }),
            cx.subscribe(&find_field, |this, _f, ev: &TextFieldEvent, cx| {
                this.ai_find_field_event(ev, true, cx)
            }),
            cx.subscribe(&replace_field, |this, _f, ev: &TextFieldEvent, cx| {
                this.ai_find_field_event(ev, false, cx)
            }),
        ];
        AiEditor {
            loaded_for: None,
            epoch_seen: 0,
            loading: false,
            has_transcript: false,
            transcript_language: None,
            note: None,
            captions: CaptionEditor::default(),
            field,
            field_for: None,
            saving: false,
            resave: false,
            zooms: None,
            cuts: None,
            asking_zooms: false,
            asking_cuts: false,
            suggest_note: None,
            options: CaptionOptionsInfo::default(),
            words: Rc::default(),
            doubtful_cache: RefCell::new(None),
            options_busy: false,
            rev: 0,
            reload: false,
            find_open: false,
            find: FindState::default(),
            find_field,
            replace_field,
            line_play: None,
            pending: None,
            view: Rc::default(),
            _subs: subs,
        }
    }

    /// Forgets everything about the recording that was open.
    fn reset(&mut self) {
        self.loaded_for = None;
        self.loading = false;
        self.has_transcript = false;
        self.transcript_language = None;
        self.note = None;
        self.captions = CaptionEditor::default();
        self.field_for = None;
        self.saving = false;
        self.resave = false;
        self.zooms = None;
        self.cuts = None;
        self.asking_zooms = false;
        self.asking_cuts = false;
        self.suggest_note = None;
        self.options = CaptionOptionsInfo::default();
        self.words = Rc::default();
        *self.doubtful_cache.borrow_mut() = None;
        self.options_busy = false;
        self.reload = false;
        self.find_open = false;
        self.find = FindState::default();
        self.line_play = None;
        self.pending = None;
        self.view.last_followed.set(None);
        self.view.scroll_to.set(None);
        self.view.layouts.borrow_mut().clear();
        // The preview must drop the captions of the recording that was open.
        self.rev += 1;
    }

    /// The recording has a transcript (so captions exist to burn in).
    pub(super) fn has_transcript(&self) -> bool {
        self.has_transcript
    }

    /// The recording's own "burn into exports" setting (the default of the export tick).
    pub(super) fn burn_in_default(&self) -> bool {
        self.options.burn_in
    }

    /// Whether the preview draws captions: switched on and a transcript exists.
    pub(super) fn shows_captions(&self) -> bool {
        self.options.enabled && self.has_transcript
    }

    /// The doubtful words of the lines as they are now (edited words no longer match the
    /// transcript and drop out), in reading order.
    pub(super) fn doubtful(&self) -> Rc<Vec<Doubtful>> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.words.len().hash(&mut h);
        for l in self.captions.lines() {
            (l.start.ticks(), l.end.ticks(), &l.text).hash(&mut h);
        }
        let key = h.finish();
        if let Some((k, v)) = self.doubtful_cache.borrow().as_ref() {
            if *k == key {
                return v.clone();
            }
        }
        let v = Rc::new(doubtful_spans(self.captions.lines(), &self.words));
        *self.doubtful_cache.borrow_mut() = Some((key, v.clone()));
        v
    }

    /// The caption lines, for the timeline lane.
    pub(super) fn lines(&self) -> &[cm_ai::caption::CaptionLine] {
        self.captions.lines()
    }

    /// The selected line (the lane marks it).
    pub(super) fn selected_line(&self) -> Option<usize> {
        self.captions.selected
    }
}

/// `m:ss.f` of a session time.
pub(super) fn stamp(t: MediaTime) -> String {
    let s = t.as_secs_f64().max(0.0);
    format!("{}:{:04.1}", (s / 60.0) as u64, s % 60.0)
}

impl RecordingEditor {
    // ---- loading -------------------------------------------------------------------------

    /// Called every frame: reads the transcript and the caption lines when the recording changed
    /// or a new transcript arrived.
    pub(super) fn ai_sync(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else {
            if self.ai.loaded_for.is_some() {
                self.ai.reset();
            }
            return;
        };
        if !matches!(self.load, super::Load::Ready(_)) {
            return;
        }
        let epoch = self.app.read(cx).transcribe.epoch;
        let same = self.ai.loaded_for.as_deref() == Some(id.as_str());
        if self.ai.loading || (same && self.ai.epoch_seen == epoch && !self.ai.reload) {
            return;
        }
        if !same {
            self.ai.reset();
        } else if self.ai.captions.is_dirty() {
            // Unsaved edits win over a transcript that arrived meanwhile.
            self.ai.epoch_seen = epoch;
            return;
        }
        self.ai.loading = true;
        self.ai.reload = false;
        self.ai.loaded_for = Some(id.clone());
        self.ai.epoch_seen = epoch;
        let Some(client) = ai::client_of(&self.app, cx) else {
            self.ai.loading = false;
            return;
        };
        cx.spawn(async move |this, cx| {
            let transcript = client.get_transcript_with_options(&id).await;
            let stored = match &transcript {
                Ok(Some(_)) => client.get_captions(&id).await.unwrap_or(None),
                _ => None,
            };
            this.update(cx, |this, cx| {
                if this.id.as_deref() != Some(id.as_str()) {
                    return;
                }
                this.ai.loading = false;
                // The preview's caption track is rebuilt from what was just read.
                this.ai.rev += 1;
                match transcript {
                    Ok(Some((tr, options))) => {
                        let lines = stored.unwrap_or_else(|| lines_from_words(&tr.words));
                        this.ai.words = Rc::new(tr.words.clone());
                        this.ai.has_transcript = true;
                        this.ai.transcript_language = Some(tr.language.clone());
                        this.ai.options = options;
                        this.ai.captions = CaptionEditor::new(lines);
                        this.ai.note = tr.note.map(AiNote::Transcript);
                    }
                    Ok(None) => {
                        this.ai.has_transcript = false;
                        this.ai.words = Rc::default();
                        this.ai.options = CaptionOptionsInfo::default();
                        this.ai.captions = CaptionEditor::default();
                    }
                    // Not "no transcript": say what went wrong instead of showing nothing.
                    Err(e) => {
                        this.ai.has_transcript = false;
                        this.ai.note = Some(AiNote::ReadFailed(ErrorNote::from_ai(&e)));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ---- caption editing -----------------------------------------------------------------

    pub(super) fn ai_save(&mut self, cx: &mut Context<Self>) {
        if !self.ai.captions.is_dirty() {
            return;
        }
        if self.ai.saving {
            // Whatever changed meanwhile goes out as soon as this one is done.
            self.ai.resave = true;
            return;
        }
        let (Some(id), Some(client)) = (self.id.clone(), ai::client_of(&self.app, cx)) else {
            return;
        };
        let lines = self.ai.captions.lines().to_vec();
        self.ai.saving = true;
        cx.spawn(async move |this, cx| {
            let res = client.set_captions(&id, &lines).await;
            this.update(cx, |this, cx| {
                this.ai.saving = false;
                match res {
                    Ok(()) => {
                        // Only what was sent is saved: edits made meanwhile stay dirty.
                        if this.ai.captions.lines() == lines.as_slice() {
                            this.ai.captions.mark_saved();
                        }
                        if !matches!(this.ai.note, Some(AiNote::Replaced(_))) {
                            this.ai.note = None;
                        }
                        // The preview shows the saved lines.
                        this.ai.rev += 1;
                        if std::mem::take(&mut this.ai.resave) {
                            this.ai_save(cx);
                        }
                    }
                    Err(e) => {
                        let app = this.app.clone();
                        cx.defer(move |cx| {
                            app::push_toast(
                                &app,
                                flows::error_toast_live(
                                    || tr!("Couldn\u{2019}t save the captions"),
                                    e.message.clone(),
                                ),
                                cx,
                            )
                        });
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Flips a caption switch (`enabled` = show in the preview, `burn_in` = burn into exports).
    /// The switch moves at once and goes back with a message when the daemon refuses.
    pub(super) fn ai_set_option(
        &mut self,
        enabled: Option<bool>,
        burn_in: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let (Some(id), Some(client)) = (self.id.clone(), ai::client_of(&self.app, cx)) else {
            return;
        };
        if self.ai.options_busy {
            return;
        }
        let before = self.ai.options;
        if let Some(e) = enabled {
            self.ai.options.enabled = e;
        }
        if let Some(b) = burn_in {
            self.ai.options.burn_in = b;
        }
        self.ai.options_busy = true;
        self.ai.rev += 1;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = client.set_caption_options(&id, enabled, burn_in).await;
            this.update(cx, |this, cx| {
                this.ai.options_busy = false;
                match res {
                    Ok(now) => {
                        this.ai.options = now;
                        // The recording's own export setting changed: it is the default again.
                        if burn_in.is_some() {
                            this.app
                                .update(cx, |m, _| m.transcribe.clear_export_burn(&id));
                        }
                    }
                    Err(e) => {
                        this.ai.options = before;
                        let app = this.app.clone();
                        cx.defer(move |cx| {
                            app::push_toast(
                                &app,
                                flows::error_toast_live(
                                    || tr!("Couldn\u{2019}t change the captions"),
                                    e.message.clone(),
                                ),
                                cx,
                            )
                        });
                    }
                }
                this.ai.rev += 1;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn ai_export(&mut self, format: CaptionFormat, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), ai::client_of(&self.app, cx)) else {
            return;
        };
        let lines = self
            .ai
            .captions
            .is_dirty()
            .then(|| self.ai.captions.lines().to_vec());
        let app = self.app.clone();
        let timestamps = self.app.read(cx).transcribe.txt_timestamps;
        cx.spawn(async move |this, cx| {
            // The file is written from what the daemon has stored, so unsaved edits go first.
            let saved = match &lines {
                Some(l) => client.set_captions(&id, l).await,
                None => Ok(()),
            };
            let res = match saved {
                Ok(()) => client.export_captions(&id, format, timestamps).await,
                Err(e) => Err(e),
            };
            this.update(cx, |this, cx| {
                if res.is_ok() && lines.is_some() {
                    this.ai.captions.mark_saved();
                    this.ai.rev += 1;
                }
                let spec = match res {
                    Ok(path) => ToastSpec::new(
                        ToastKind::Success,
                        tr!("{format} saved", format = format.label()),
                    )
                    .body(path.display().to_string()),
                    Err(e) => flows::error_toast_live(
                        || tr!("Couldn\u{2019}t save the captions file"),
                        e.message.clone(),
                    ),
                };
                cx.defer(move |cx| app::push_toast(&app, spec, cx));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ---- suggestions ---------------------------------------------------------------------

    fn ai_suggest_zooms(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), ai::client_of(&self.app, cx)) else {
            return;
        };
        if self.ai.asking_zooms {
            return;
        }
        self.ai.asking_zooms = true;
        self.ai.suggest_note = None;
        cx.spawn(async move |this, cx| {
            let res = client.suggest_zooms(&id).await;
            this.update(cx, |this, cx| {
                this.ai.asking_zooms = false;
                match res {
                    Ok(list) if list.is_empty() => {
                        this.ai.zooms = None;
                        this.ai.suggest_note = Some(AiNote::Nothing(Nothing::ZoomSpots));
                    }
                    Ok(list) => this.ai.zooms = Some(SuggestionList::new(list)),
                    Err(e) => this.ai.suggest_note = Some(AiNote::Failed(ErrorNote::from_ai(&e))),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn ai_suggest_cuts(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), ai::client_of(&self.app, cx)) else {
            return;
        };
        if self.ai.asking_cuts {
            return;
        }
        self.ai.asking_cuts = true;
        self.ai.suggest_note = None;
        cx.spawn(async move |this, cx| {
            let res = client.suggest_cuts(&id).await;
            this.update(cx, |this, cx| {
                this.ai.asking_cuts = false;
                match res {
                    Ok(list) if list.is_empty() => {
                        this.ai.cuts = None;
                        this.ai.suggest_note = Some(AiNote::Nothing(Nothing::SilencesOrFillers));
                    }
                    Ok(list) => this.ai.cuts = Some(SuggestionList::new(list)),
                    Err(e) => this.ai.suggest_note = Some(AiNote::Failed(ErrorNote::from_ai(&e))),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Accept one zoom (`Some(i)`) or all of them (`None`).
    fn ai_accept_zooms(&mut self, which: Option<usize>, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else { return };
        let Some(list) = self.ai.zooms.as_mut() else {
            return;
        };
        let taken = match which {
            Some(i) => list.take(i).into_iter().collect::<Vec<_>>(),
            None => list.take_all(),
        };
        if list.is_empty() {
            self.ai.zooms = None;
        }
        if let Some(req) = zoom_request(&id, &taken) {
            self.send(vec![req], cx);
        }
        cx.notify();
    }

    fn ai_accept_cuts(&mut self, which: Option<usize>, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else { return };
        let Some(list) = self.ai.cuts.as_mut() else {
            return;
        };
        let taken = match which {
            Some(i) => list.take(i).into_iter().collect::<Vec<_>>(),
            None => list.take_all(),
        };
        if list.is_empty() {
            self.ai.cuts = None;
        }
        if let Some(req) = cut_request(&id, &taken) {
            self.send(vec![req], cx);
        }
        cx.notify();
    }

    // ---- the panels ----------------------------------------------------------------------

    pub(super) fn suggest_panel(
        &self,
        t: &Theme,
        s: &Session,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ai = &self.ai;
        // Zooms are for pictures: an audio recording is offered cuts only (VOC-2).
        let audio = s.voice.audio;
        // Design gap: the board puts a subtitle under each tile ("Proposed on the Zoom track",
        // "Proposed as ranges to cut"); that is new copy, so the tiles carry the title only.
        let tile = |id: &'static str, glyph: &'static str, label: String, busy: bool| {
            let shown = if busy { tr!("Looking\u{2026}") } else { label };
            let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
            div()
                .id(id)
                .role(Role::Button)
                .aria_label(shown.clone())
                .focusable()
                .tab_stop(true)
                .flex_1()
                .min_w(px(0.))
                .p(px(14.))
                .flex()
                .flex_col()
                .gap(px(10.))
                .rounded(px(scale::RADIUS_LG))
                .border_1()
                .border_color(rgba_of(t.border))
                .bg(rgba_of(t.ground))
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .focus_visible(move |s| s.border_color(focus))
                .child(
                    div()
                        .size(px(32.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(scale::RADIUS_SM))
                        .bg(rgba_of(t.agent.with_alpha(28)))
                        .child(icon(glyph, 16., t.agent_text)),
                )
                .child(
                    div()
                        .text_size(px(type_scale::UI))
                        .font_weight(FontWeight::MEDIUM)
                        .child(shown),
                )
        };
        let mut body: Vec<AnyElement> =
            vec![
            div()
                .flex()
                .gap(px(10.))
                .children((!audio).then(|| {
                    tile(
                        "re-suggest-zooms",
                        "zoom-in",
                        tr!("Suggest zooms"),
                        ai.asking_zooms,
                    )
                        .on_click(cx.listener(|this, _, _, cx| this.ai_suggest_zooms(cx)))
                }))
                .child(
                    tile(
                        "re-suggest-cuts",
                        "scissors",
                        tr!("Suggest cuts"),
                        ai.asking_cuts,
                    )
                        .on_click(cx.listener(|this, _, _, cx| this.ai_suggest_cuts(cx))),
                )
                .into_any_element(),
            hint_box(
                t,
                "undo-2",
                tr!("Nothing changes until you accept one, and an accepted change is one undo step."),
            )
            .into_any_element(),
        ];
        if let Some(n) = &ai.suggest_note {
            body.push(hint(t, "info", n.text()).into_any_element());
        }
        if let Some(z) = ai.zooms.as_ref().filter(|_| !audio) {
            body.push(self.suggestion_block(
                t,
                trn!("Zooms ({n})", "Zooms ({n})", z.items().len()),
                "zoom",
                tr!("Remove this zoom suggestion"),
                z.items().iter().map(zoom_label).collect(),
                cx,
                |this, w, cx| this.ai_accept_zooms(w, cx),
                |this, i, cx| {
                    if let Some(l) = this.ai.zooms.as_mut() {
                        l.remove(i);
                        if l.is_empty() {
                            this.ai.zooms = None;
                        }
                    }
                    cx.notify();
                },
            ));
        }
        if let Some(l) = &ai.cuts {
            body.push(self.suggestion_block(
                t,
                trn!("Cuts ({n})", "Cuts ({n})", l.items().len()),
                "cut",
                tr!("Remove this cut suggestion"),
                l.items().iter().map(cut_label).collect(),
                cx,
                |this, w, cx| this.ai_accept_cuts(w, cx),
                |this, i, cx| {
                    if let Some(l) = this.ai.cuts.as_mut() {
                        l.remove(i);
                        if l.is_empty() {
                            this.ai.cuts = None;
                        }
                    }
                    cx.notify();
                },
            ));
        }
        panel(
            t,
            Inspector::Suggestions,
            tr!("Proposals only"),
            body,
            Vec::new(),
        )
    }

    /// One list of suggestions: a header with Accept all, and a row each with Accept and Remove.
    #[allow(clippy::too_many_arguments)]
    fn suggestion_block(
        &self,
        t: &Theme,
        title: String,
        kind: &'static str,
        remove_label: String,
        labels: Vec<String>,
        cx: &mut Context<Self>,
        accept: fn(&mut Self, Option<usize>, &mut Context<Self>),
        remove: fn(&mut Self, usize, &mut Context<Self>),
    ) -> AnyElement {
        let rows = labels.into_iter().enumerate().map(|(i, label)| {
            div()
                .py(px(4.))
                .border_t_1()
                .border_color(rgba_of(t.row_divider()))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .mono()
                        .text_size(px(12.))
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(label)),
                )
                .child(
                    small_button(
                        t,
                        SharedString::from(format!("re-accept-{kind}-{i}")),
                        &tr_ctx!("button", "Accept"),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| accept(this, Some(i), cx))),
                )
                .child(
                    icon_only(
                        t,
                        SharedString::from(format!("re-remove-{kind}-{i}")),
                        "x",
                        SharedString::from(remove_label.clone()),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| remove(this, i, cx))),
                )
        });
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .min_w(px(0.))
                            .text_size(px(type_scale::UI))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(title)),
                    )
                    .child(
                        small_button(
                            t,
                            SharedString::from(format!("re-accept-all-{kind}")),
                            &tr!("Accept all"),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| accept(this, None, cx))),
                    ),
            )
            .children(rows)
            .into_any_element()
    }
}
