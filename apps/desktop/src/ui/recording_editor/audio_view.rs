//! The editor of an audio recording (VOC-2, redesign V4): the "now speaking" card in place of the
//! video preview, the lanes (markers, speakers, one waveform per track, suggested cuts), the
//! overview strip, the header and the state the audio view keeps (language, translate chooser,
//! the Export audio sheet). The rules are in `crate::recedit::voice` (unit tested); this file
//! draws and sends daemon calls. Not seen by eye yet (`tests/manual/VOC-2.md`).
//!
//! Every change goes through the daemon: markers with `add_marker`, cuts and trims with the
//! editor's own request queue, translations with `translate_captions`, the export with
//! `export_audio`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use cm_ai::caption::CaptionLine;
use cm_core::MediaTime;
use cm_i18n::{tr, tr_ctx, trn};
use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, Bounds, ContentMask, Context, CursorStyle,
    FontWeight, Hsla, MouseButton, MouseDownEvent, PathBuilder, Pixels, Role, ScrollHandle,
    SharedString, Window,
};

use super::tabs::Inspector;
use super::{Drag, Load, RecordingEditor, Session};
use crate::recedit::audio_export::Form;
use crate::recedit::geom::{hatch_lines, tick_step_secs, SegKind, HATCH_SPACING, LABEL_W};
use crate::recedit::keys::Cmd;
use crate::recedit::model::Selection;
use crate::recedit::voice::{
    self, bar_height, clock, clock_tenths, initial, lane_blocks, lane_tops, speaker_colour,
    speaker_index, Row, Turn, VoiceData, MARKERS_H, SPEAKERS_H, WAVE_H,
};
use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::components::{
    badge, button_with_icon, crumb_link, icon, keycap, shrink_text, ButtonKind,
};
use crate::ui::paint::{rgba_of, TextExt};

use super::timeline::{ruler_label, BODY_GAP};

/// A click handler of a button.
type ClickFn = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static>;
/// The lines, turns and rows of the transcript shown.
type TurnsOf = (Rc<Vec<CaptionLine>>, Rc<Vec<Turn>>, Rc<Vec<Row>>);

/// Spacing of the waveform bars, px.
const BAR_SPACING: f32 = 4.0;
/// Height of the whole-recording strip, px.
const OVERVIEW_H: f32 = 48.0;

fn hsla(c: Color) -> Hsla {
    Hsla::from(rgba_of(c))
}

/// What the transcript list was built from; the key says when it is stale.
pub(super) struct TurnCache {
    /// Edits of the caption lines, reads of the bundle, the language shown.
    key: (u64, u64, Option<String>),
    pub lines: Rc<Vec<CaptionLine>>,
    pub turns: Rc<Vec<Turn>>,
    pub rows: Rc<Vec<Row>>,
}

/// The audio view's own state: what the user chose in the transcript, the follow, the sheet.
pub(super) struct VoiceUi {
    /// The language the transcript shows: `None` is the original.
    pub lang: Option<String>,
    /// The "Translate..." chooser is open.
    pub translate_open: bool,
    /// Languages being translated now (the job runs in the background).
    pub translating: Vec<String>,
    /// SRT with the speakers' names (VTT and TXT always have them).
    pub speaker_names: bool,
    pub scroll: ScrollHandle,
    /// The row the list last scrolled to by itself.
    pub followed: Cell<Option<usize>>,
    /// When the user last scrolled the list by hand (auto-follow pauses for a while).
    pub last_manual: Rc<Cell<Option<Instant>>>,
    /// Markers, speakers or translations may have changed: read them again.
    pub stale: bool,
    pub loading: bool,
    pub seen_library: u64,
    pub seen_voice: u64,
    /// The recording the first-open choices (the tab) were made for.
    pub inited_for: Option<String>,
    /// The Export audio sheet, when open.
    pub sheet: Option<super::export_sheet::Sheet>,
    /// The overview strip as painted last frame.
    pub overview: Rc<Cell<Bounds<Pixels>>>,
    pub cache: RefCell<Option<TurnCache>>,
}

impl VoiceUi {
    pub(super) fn new() -> VoiceUi {
        VoiceUi {
            lang: None,
            translate_open: false,
            translating: Vec::new(),
            speaker_names: true,
            scroll: ScrollHandle::new(),
            followed: Cell::new(None),
            last_manual: Rc::new(Cell::new(None)),
            stale: false,
            loading: false,
            seen_library: 0,
            seen_voice: 0,
            inited_for: None,
            sheet: None,
            overview: Rc::default(),
            cache: RefCell::new(None),
        }
    }
}

impl RecordingEditor {
    // ---- reading the bundle --------------------------------------------------------------

    /// Called every frame: the first-open choices for a recording, the Export audio sheet that
    /// another screen asked for, and a new read of the markers, speakers and translations when
    /// the library or a translation changed.
    pub(super) fn voice_sync(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else { return };
        let (library, voice_epoch, focus) = {
            let m = self.app.read(cx);
            (
                m.library_epoch,
                m.transcribe.voice_epoch,
                m.transcribe.export_audio_focus.clone(),
            )
        };
        self.voice_jobs_sync(cx);
        let Load::Ready(s) = &self.load else { return };
        let audio = s.voice.audio;
        // The first time a recording opens: an audio recording starts on its transcript, and a
        // tab of the other kind is never left open.
        if self.voice_ui.inited_for.as_deref() != Some(id.as_str()) {
            self.voice_ui.inited_for = Some(id.clone());
            self.voice_ui.seen_library = library;
            self.voice_ui.seen_voice = voice_epoch;
            if audio {
                self.tab = Inspector::Transcript;
            }
            // The report's title starts as the recording's own name.
            let title = s.title.clone();
            self.vp.title.update(cx, |f, cx| f.set_text(&title, cx));
        }
        // "Set up report" from the toast after a transcript: the Report tab on its setup.
        let report_focus = self.app.read(cx).transcribe.report_focus.clone();
        if report_focus.as_deref() == Some(id.as_str()) {
            self.tab = Inspector::Report;
            self.vp.setup = true;
            self.vp.template = None;
            self.vp.regenerating = None;
            let app = self.app.clone();
            cx.defer(move |cx| {
                app.update(cx, |m, _| m.transcribe.report_focus = None);
            });
        }
        if !Inspector::set(audio).contains(&self.tab) {
            self.tab = if audio && self.tab == Inspector::Captions {
                Inspector::Transcript
            } else {
                Inspector::Speed
            };
        }
        if focus.as_deref() == Some(id.as_str()) {
            let avail = s.voice.available();
            let copy = super::export_sheet::last_copy_dir();
            self.voice_ui.sheet = avail
                .any()
                .then(|| super::export_sheet::Sheet::new(Form::new(copy), avail));
            let app = self.app.clone();
            cx.defer(move |cx| {
                app.update(cx, |m, _| m.transcribe.export_audio_focus = None);
            });
        }
        let changed = library != self.voice_ui.seen_library
            || voice_epoch != self.voice_ui.seen_voice
            || self.voice_ui.stale;
        if changed && !self.voice_ui.loading {
            self.voice_ui.seen_library = library;
            self.voice_ui.seen_voice = voice_epoch;
            self.voice_ui.stale = false;
            self.reload_voice(cx);
        }
    }

    /// Reads the markers, speakers and translations again on a worker thread.
    fn reload_voice(&mut self, cx: &mut Context<Self>) {
        let Load::Ready(s) = &self.load else { return };
        let dir = s.bundle.clone();
        let key = self.id.clone();
        self.voice_ui.loading = true;
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let m = cm_library::read_manifest(&dir).ok()?;
                    Some(VoiceData::read(&dir, &m))
                })
                .await;
            this.update(cx, |this, cx| {
                this.voice_ui.loading = false;
                if this.id != key {
                    return;
                }
                if let (Some(mut data), Some(s)) = (read, this.session()) {
                    data.rev = s.voice.rev + 1;
                    s.voice = data;
                    // A translation that arrived is no longer "in progress".
                    let have: Vec<String> = s
                        .voice
                        .translations
                        .iter()
                        .map(|(c, _)| c.clone())
                        .collect();
                    this.voice_ui.translating.retain(|l| !have.contains(l));
                    if let Some(l) = &this.voice_ui.lang {
                        if !have.contains(l) {
                            this.voice_ui.lang = None;
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ---- actions -------------------------------------------------------------------------

    /// M: a marker at the playhead, sent as `add_marker` on the recorded clock (pauses removed).
    pub(super) fn add_marker_here(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else { return };
        let Some(s) = self.session() else { return };
        let at = voice::recorded_at(&s.model.basis.gaps, s.model.playhead);
        let req = crate::recedit::model::EditRequest {
            method: "add_marker",
            params: serde_json::json!({
                "capture_id": id,
                "at": format!("t:{}", at.ticks()),
            }),
        };
        self.send(vec![req], cx);
    }

    /// Moves the playhead (a click on a time, a line, a flag).
    pub(super) fn voice_seek(&mut self, t: MediaTime, cx: &mut Context<Self>) {
        if let Some(s) = self.session() {
            s.model.seek(t);
            s.pacer.stop();
            s.geom.reveal(t);
        }
        cx.notify();
    }

    /// "Translate..." chose a language: `translate_captions` as the user. The result arrives as
    /// `translation.ready`.
    pub(super) fn voice_translate(&mut self, code: &'static str, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        self.voice_ui.translate_open = false;
        if !self.voice_ui.translating.iter().any(|l| l == code) {
            self.voice_ui.translating.push(code.to_owned());
        }
        let app = self.app.clone();
        cx.spawn(async move |this, cx| {
            let res = client.translate_captions(&id, code).await;
            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    this.voice_ui.translating.retain(|l| l != code);
                    let msg = e.message.clone();
                    cx.defer(move |cx| {
                        crate::ui::app::push_toast(
                            &app,
                            crate::flows::error_toast_live(
                                || tr!("Couldn\u{2019}t translate the transcript"),
                                msg.clone(),
                            ),
                            cx,
                        )
                    });
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// "Upload" in the header: `upload` of the capture to the default destination.
    fn upload_click(&self) -> ClickFn {
        let (app, id) = (self.app.clone(), self.id.clone().unwrap_or_default());
        Box::new(move |_, _, cx| {
            let (app, id) = (app.clone(), id.clone());
            cx.defer(move |cx| crate::ui::upload_ui::upload(&app, Some(&id), None, cx));
        })
    }

    // ---- the page ------------------------------------------------------------------------

    /// Everything under the header of an audio recording.
    pub(super) fn audio_ready(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let left: AnyElement = match self
            .report_left_open(s, cx)
            .then(|| self.shown_report(s))
            .flatten()
        {
            Some(r) => self.report_left_column(t, s, r, cx),
            None => div()
                .flex_1()
                .min_w(px(0.))
                .min_h(px(0.))
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(self.now_speaking_card(t, s, cx))
                .child(self.audio_timeline_card(t, s, cx))
                .into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.audio_title_row(t, s, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap(px(16.))
                    .px(px(24.))
                    .py(px(16.))
                    .child(left)
                    .child(self.inspector(t, s, cx)),
            )
            .into_any_element()
    }

    /// The header (V4): "Audio" crumb, the title, a chip with the length, Reveal in Finder, Upload and
    /// Export audio... (the primary button). No Publish for audio.
    fn audio_title_row(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let app = self.app.clone();
        // TODO(design): the board's crumb goes to the Audio library page; until it exists the
        // crumb goes to the library.
        let crumb = crumb_link(
            t,
            "re-crumb",
            tr_ctx!("breadcrumb", "Audio /"),
            tr!("Back to the library"),
        )
        .on_click(move |_, _, cx| {
            let app = app.clone();
            cx.defer(move |cx| crate::ui::app::set_route(&app, crate::ui::app::Route::Library, cx));
        });
        let length = badge(t, clock(s.model.basis.end));
        let export = button_with_icon(
            t,
            "re-export-audio-top",
            tr!("Export audio\u{2026}"),
            ButtonKind::Primary,
            Some("file-headphone"),
        )
        .on_click(cx.listener(|this, _, _, cx| this.open_export_sheet(cx)));
        let upload = button_with_icon(
            t,
            "re-upload-top",
            tr_ctx!("button", "Upload"),
            ButtonKind::Secondary,
            Some("cloud-upload"),
        )
        .on_click(self.upload_click());
        crate::ui::components::page_toolbar_actions(
            t,
            "recording-editor-toolbar",
            tr!("Recording editor toolbar"),
            crumb,
            SharedString::from(s.title.clone()),
            vec![
                length.into_any_element(),
                self.audio_reveal_button(t, s, cx),
                upload.into_any_element(),
                export.into_any_element(),
            ],
        )
        .into_any_element()
    }

    /// The icon button that shows the recording's folder in Finder (the same control as the
    /// screenshot editor's). It replaces "Recording details": an audio recording has no details
    /// page.
    fn audio_reveal_button(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let label = if cfg!(target_os = "macos") {
            tr!("Reveal in Finder")
        } else {
            tr!("Show in folder")
        };
        let c = ButtonKind::Secondary.colors(t);
        let (hover, focus) = (rgba_of(c.hover), rgba_of(t.text));
        let dir = s.bundle.clone();
        div()
            .id("re-reveal")
            .role(Role::Button)
            .aria_label(label.clone())
            .focusable()
            .tab_stop(true)
            .size(px(scale::CONTROL_H))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(c.border))
            .bg(rgba_of(c.bg))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.border_color(focus))
            .tooltip(crate::ui::tooltip::tip(label, Vec::new()))
            .on_click(cx.listener(move |_, _, _, cx| cx.reveal_path(&dir)))
            .child(icon("folder-open", 16., c.fg))
            .into_any_element()
    }

    /// Opens the Export audio sheet with the choices of the last export.
    pub(super) fn open_export_sheet(&mut self, cx: &mut Context<Self>) {
        let Some(avail) = self.session().map(|s| s.voice.available()) else {
            return;
        };
        if !avail.any() {
            return;
        }
        let copy = super::export_sheet::last_copy_dir();
        self.voice_ui.sheet = Some(super::export_sheet::Sheet::new(Form::new(copy), avail));
        cx.notify();
    }

    /// The Output tab of an audio recording: the length after the cuts, what the export applies,
    /// Export audio... and Upload. No MP4 and no Publish.
    pub(super) fn audio_output_panel(
        &self,
        t: &Theme,
        s: &Session,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kept = s.model.output_duration();
        let length = div()
            .px(px(16.))
            .py(px(14.))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.ground))
            .child(
                div()
                    .text_size(px(type_scale::UI))
                    .text_color(rgba_of(t.text_2))
                    .child(tr_ctx!("output", "Length")),
            )
            .child(
                div()
                    .id("re-length")
                    .mono()
                    .tnum()
                    .text_size(px(24.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(clock_tenths(kept))),
            );
        let body = vec![
            length.into_any_element(),
            super::tabs::hint(
                t,
                "info",
                tr!(
                    "The export applies trims, cuts and speed, and can even out the loudness. \
                     M4A or WAV; the title and date are written into the file."
                ),
            )
            .into_any_element(),
            super::tabs::hint(
                t,
                "lock",
                tr!("Edits are saved with the recording and never change the original tracks."),
            )
            .into_any_element(),
        ];
        let export = button_with_icon(
            t,
            "re-export-audio",
            tr!("Export audio\u{2026}"),
            ButtonKind::Primary,
            Some("file-headphone"),
        )
        .flex_1()
        .on_click(cx.listener(|this, _, _, cx| this.open_export_sheet(cx)));
        let upload = button_with_icon(
            t,
            "re-upload",
            tr_ctx!("button", "Upload"),
            ButtonKind::Secondary,
            Some("cloud-upload"),
        )
        .flex_1()
        .on_click(self.upload_click());
        super::tabs::panel(
            t,
            Inspector::Output,
            tr!("What Export audio will produce"),
            body,
            vec![upload.into_any_element(), export.into_any_element()],
        )
    }

    // ---- the "now speaking" card ---------------------------------------------------------

    fn now_speaking_card(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let m = &s.model;
        let lines = self.ai.lines();
        let labels = self.ai_labels(s);
        let now = voice::now_speaking(lines, &self.ai.words, &labels, m.playhead);
        let speaker_ix = now
            .as_ref()
            .and_then(|n| n.speaker)
            .map(|id| speaker_index(&s.voice.speakers, id));
        let who: Option<(String, Color)> = now.as_ref().and_then(|n| {
            n.speaker.map(|id| {
                (
                    s.voice.name_of(id),
                    speaker_colour(speaker_index(&s.voice.speakers, id)),
                )
            })
        });
        let _ = speaker_ix;
        let head = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .h(px(32.))
            .children(who.map(|(name, colour)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(avatar(t, &name, colour, 32.))
                    .child(
                        div()
                            .text_size(px(type_scale::BODY))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(name)),
                    )
            }))
            .children(now.as_ref().map(|n| {
                div()
                    .mono()
                    .tnum()
                    .text_size(px(type_scale::UI))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(clock(n.since)))
            }));
        let text: AnyElement = match &now {
            Some(n) => {
                let mut styled = gpui::StyledText::new(SharedString::from(n.text.clone()));
                if let Some(r) = n.highlight.clone() {
                    styled = styled.with_highlights([(
                        r,
                        gpui::HighlightStyle {
                            background_color: Some(hsla(t.raised)),
                            ..Default::default()
                        },
                    )]);
                }
                div()
                    .id("re-now-text")
                    .role(Role::Status)
                    .aria_label(SharedString::from(n.text.clone()))
                    .text_size(px(20.))
                    .line_height(px(28.))
                    .text_color(rgba_of(t.text))
                    .child(styled)
                    .into_any_element()
            }
            None => div()
                .text_size(px(type_scale::BODY))
                .text_color(rgba_of(t.mist))
                .child(if self.ai.has_transcript() {
                    tr!("Play to follow the words here.")
                } else {
                    tr!("The words appear here once the recording has a transcript.")
                })
                .into_any_element(),
        };
        let play = div()
            .id("re-play")
            .role(Role::Button)
            .aria_label(if m.playing {
                tr_ctx!("button", "Pause")
            } else {
                tr_ctx!("button", "Play")
            })
            .focusable()
            .tab_stop(true)
            .size(px(52.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(26.))
            .border_1()
            .border_color(rgba_of(t.primary_bg))
            .bg(rgba_of(t.primary_bg))
            .cursor_pointer()
            .hover({
                let h = rgba_of(t.text_2);
                move |st| st.bg(h)
            })
            .focus_visible({
                let f = rgba_of(t.rec);
                move |st| st.border_color(f)
            })
            .child(icon(
                if m.playing { "rec-pause" } else { "rec-play" },
                20.0,
                t.primary_fg,
            ))
            .on_click(cx.listener(|this, _, _, cx| this.run(Cmd::PlayPause, cx)));
        let hint = |keys: &[&str], what: String| {
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .children(keys.iter().map(|k| keycap(t, k.to_string())))
                .child(
                    div()
                        .ml(px(2.))
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(what)),
                )
        };
        let transport = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.))
            .child(play)
            .child(
                div()
                    .mono()
                    .tnum()
                    .text_size(px(22.))
                    .flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("re-now-time")
                            .text_color(rgba_of(t.text))
                            .child(SharedString::from(clock_tenths(m.playhead))),
                    )
                    .child(div().text_color(rgba_of(t.mist)).child("/"))
                    .child(
                        div()
                            .text_color(rgba_of(t.mist))
                            .child(SharedString::from(clock_tenths(m.basis.end))),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_x(px(14.))
                    .gap_y(px(6.))
                    // i18n-ignore (key names)
                    .child(hint(&["Space"], tr_ctx!("editor shortcut", "Play")))
                    .child(hint(
                        &["J", "K", "L"],
                        tr_ctx!("editor shortcut", "Shuttle"),
                    ))
                    .child(hint(&["M"], tr_ctx!("editor shortcut", "Marker")))
                    .child(hint(&["\u{232B}"], tr_ctx!("editor shortcut", "Cut"))),
            );
        let notes: Vec<String> = [s.audio_note.clone(), m.note.map(|n| n.to_string())]
            .into_iter()
            .flatten()
            .collect();
        div()
            .id("re-now-speaking")
            .role(Role::Group)
            .aria_label(tr!("Now speaking"))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(24.))
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .child(head)
            .child(div().min_h(px(56.)).child(text))
            .child(transport)
            .children((!notes.is_empty()).then(|| {
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.warning))
                    .children(
                        notes
                            .into_iter()
                            .map(|n| div().child(SharedString::from(n))),
                    )
            }))
            .into_any_element()
    }

    // ---- the lanes -----------------------------------------------------------------------

    fn audio_timeline_card(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let g = s.geom;
        let (from, to) = crate::recedit::range::range_label_parts(&g);
        let extent = div()
            .ml(px(8.))
            .mr(px(8.))
            .mono()
            .text_size(px(type_scale::CAPTION))
            .text_color(rgba_of(t.mist))
            .child(SharedString::from(tr!(
                "{from} \u{2013} {to} of {total}",
                from = from,
                to = to,
                total = clock(s.model.basis.end)
            )));
        let cuts_note = self.suggested_cut_note(t, s);
        div()
            .id("re-audio-timeline")
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .child(div().flex_1().child(self.toolbar(t, s, true, cx)))
                    .child(extent),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .px(px(16.))
                    .pb(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(self.audio_lanes(t, s, cx))
                    .children(cuts_note)
                    .child(self.overview_strip(t, s, cx)),
            )
            .into_any_element()
    }

    /// "Suggested cut: 24 s of silence at 19:00" under the lanes (V4), when a suggestion list exists.
    fn suggested_cut_note(&self, t: &Theme, _s: &Session) -> Option<AnyElement> {
        let list = self.ai.cuts.as_ref()?;
        let first = list.items().first()?;
        let more = list.items().len() - 1;
        let text = if more > 0 {
            tr!(
                "Suggested cut: {detail} at {time} (and {more} more)",
                detail = first.detail,
                time = clock(first.start),
                more = more
            )
        } else {
            tr!(
                "Suggested cut: {detail} at {time}",
                detail = first.detail,
                time = clock(first.start)
            )
        };
        Some(
            div()
                .id("re-suggested-cut")
                .pl(px(LABEL_W + BODY_GAP))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(13.))
                .text_color(rgba_of(t.warning))
                .child(hatch_swatch(t))
                .child(shrink_text(SharedString::from(text)))
                .into_any_element(),
        )
    }

    /// The lanes: labels at the left, the ruler, markers, speakers and waveforms at the right.
    fn audio_lanes(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let g = s.geom;
        let waves = &s.waves;
        let tops = lane_tops(waves.len());
        let x_of = |tm: MediaTime| g.x_of(tm);
        let ph_x = x_of(s.model.playhead);
        let list = &s.model.list;
        let segs = s.model.segments();

        // ---- labels
        let label = |top: f32, h: f32, glyph: &'static str, name: String, sub: Option<String>| {
            div()
                .absolute()
                .left(px(0.))
                .top(px(top))
                .w(px(LABEL_W))
                .h(px(h))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_color(rgba_of(t.text_2))
                .child(icon(glyph, 14., t.mist))
                .child(
                    div()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(13.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(SharedString::from(name)),
                        )
                        .children(sub.map(|sub| {
                            div()
                                .text_size(px(11.))
                                .text_color(rgba_of(t.mist))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(SharedString::from(sub))
                        })),
                )
        };
        let mut labels: Vec<AnyElement> = vec![
            label(
                tops.markers,
                MARKERS_H,
                "bookmark",
                tr_ctx!("lane", "Markers"),
                None,
            )
            .into_any_element(),
            label(
                tops.speakers,
                SPEAKERS_H,
                "users",
                tr_ctx!("lane", "Speakers"),
                None,
            )
            .into_any_element(),
        ];
        for ((kind, _), top) in waves.iter().zip(&tops.waves) {
            let (glyph, sub) = match kind {
                super::RowKind::System => ("volume-2", tr_ctx!("lane", "call audio")),
                _ => ("mic", tr_ctx!("lane", "microphone")),
            };
            labels.push(label(*top, WAVE_H, glyph, kind.label(), Some(sub)).into_any_element());
        }
        let labels = div()
            .relative()
            .w(px(LABEL_W))
            .h(px(tops.height))
            .flex_none()
            .children(labels);

        // ---- ruler
        let step = tick_step_secs(&g, 72.0);
        let first = (g.scroll.as_secs_f64() / step).floor() * step;
        let mut ticks = Vec::new();
        let mut at = first;
        while ticks.len() < 400 {
            let tm = MediaTime::from_secs_f64(at.max(0.0)).unwrap_or(g.end);
            let x = x_of(tm);
            if x > g.width + 1.0 {
                break;
            }
            if x >= -1.0 {
                ticks.push((x, ruler_label(at, step)));
            }
            at += step;
        }
        let ruler = ticks.into_iter().map(|(x, text)| {
            div()
                .absolute()
                .left(px(x))
                .top(px(0.))
                .h(px(voice::RULER_H))
                .flex()
                .items_end()
                .child(div().w(px(1.)).h(px(8.)).bg(rgba_of(t.line)))
                .child(
                    div()
                        .pl(px(4.))
                        .pb(px(2.))
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.mist))
                        .mono()
                        .child(SharedString::from(text)),
                )
        });

        // ---- markers
        let flags = s.voice.markers.iter().enumerate().filter_map(|(i, m)| {
            let x = x_of(m.at);
            let w = voice::flag_width(&m.label);
            if x + w < 0.0 || x > g.width {
                return None;
            }
            Some(
                div()
                    .id(SharedString::from(format!("re-marker-{i}")))
                    .role(Role::Button)
                    .aria_label(SharedString::from(tr!(
                        "Marker {label} at {time}",
                        label = m.label,
                        time = clock(m.at)
                    )))
                    .absolute()
                    .left(px(x))
                    .top(px(tops.markers))
                    .h(px(MARKERS_H - 4.))
                    .max_w(px(w))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(8.))
                    .bg(rgba_of(t.text))
                    .text_color(rgba_of(t.primary_fg))
                    .text_size(px(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(icon("bookmark", 13., t.primary_fg))
                    .children((!m.label.is_empty()).then(|| SharedString::from(m.label.clone())))
                    .into_any_element(),
            )
        });

        // ---- speakers
        let join = MediaTime::from_millis(600);
        let blocks = lane_blocks(&s.voice.segments, join)
            .into_iter()
            .enumerate()
            .filter_map(|(bi, b)| {
                let (x0, x1) = (x_of(b.start), x_of(b.end));
                if x1 < 0.0 || x0 > g.width {
                    return None;
                }
                let (x0c, x1c) = (x0.max(-4.0), x1.min(g.width + 4.0));
                let name = s.voice.name_of(b.speaker);
                Some(
                    div()
                        .id(SharedString::from(format!("re-spk-{bi}")))
                        .absolute()
                        .left(px(x0c))
                        .top(px(tops.speakers))
                        .w(px((x1c - x0c - 1.0).max(1.0)))
                        .h(px(SPEAKERS_H))
                        .rounded(px(6.))
                        .bg(rgba_of(speaker_colour(speaker_index(
                            &s.voice.speakers,
                            b.speaker,
                        ))))
                        .tooltip(crate::ui::tooltip::tip(name, Vec::new()))
                        .into_any_element(),
                )
            });

        // ---- waveforms
        let (t0, t1) = (g.t_of(0.0), g.t_of(g.width));
        let n = ((g.width / BAR_SPACING).ceil() as usize).max(1);
        let wave_rows = waves.iter().zip(&tops.waves).map(|((_, w), top)| {
            let cols = w.columns(t0, t1, n);
            let (bright, dim) = (hsla(t.text_3), hsla(t.faint));
            let split_x = ph_x;
            div()
                .absolute()
                .left(px(0.))
                .top(px(*top))
                .w(px(g.width))
                .h(px(WAVE_H))
                .rounded(px(10.))
                .border_1()
                .border_color(rgba_of(t.border))
                .bg(rgba_of(t.field))
                .overflow_hidden()
                .child(
                    canvas(
                        |bounds: Bounds<Pixels>, _, _| bounds,
                        move |_, bounds: Bounds<Pixels>, window, _| {
                            let h = f32::from(bounds.size.height);
                            let mid = h / 2.0;
                            for (i, p) in cols.iter().enumerate() {
                                let x = BAR_SPACING / 2.0 + i as f32 * BAR_SPACING;
                                let half = bar_height(*p, h - 14.0) / 2.0;
                                let mut pb = PathBuilder::stroke(px(2.));
                                pb.move_to(point(
                                    bounds.origin.x + px(x),
                                    bounds.origin.y + px(mid - half),
                                ));
                                pb.line_to(point(
                                    bounds.origin.x + px(x),
                                    bounds.origin.y + px(mid + half),
                                ));
                                if let Ok(path) = pb.build() {
                                    window.paint_path(path, if x < split_x { bright } else { dim });
                                }
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element()
        });

        // ---- trimmed ends, cuts and suggested cuts (hatched), measuring
        let body_h = tops.height;
        let wave_top = tops.waves.first().copied().unwrap_or(tops.speakers);
        let wave_bottom = tops.waves.last().map_or(wave_top, |t| t + WAVE_H);
        let hatch_h = (wave_bottom - wave_top).max(0.0);
        let cut_rects: Vec<(f32, f32, bool)> = segs
            .iter()
            .filter_map(|sg| match sg.kind {
                SegKind::Cut { index } => {
                    let sel = matches!(s.model.selection, Selection::Cut(i) if i == index);
                    Some((
                        x_of(sg.span.from).max(-4.0),
                        x_of(sg.span.to).min(g.width + 4.0),
                        sel,
                    ))
                }
                _ => None,
            })
            .collect();
        let suggested: Vec<(f32, f32)> = self
            .ai
            .cuts
            .as_ref()
            .map(|l| {
                l.items()
                    .iter()
                    .map(|c| (x_of(c.start).max(-4.0), x_of(c.end).min(g.width + 4.0)))
                    .filter(|(a, b)| b > a)
                    .collect()
            })
            .unwrap_or_default();
        let gray = hsla(t.mist.with_alpha(70));
        let amber = hsla(t.warning);
        let amber_fill = rgba_of(t.warning.with_alpha(14));
        let suggested_boxes: Vec<AnyElement> = suggested
            .iter()
            .map(|(a, b)| {
                div()
                    .absolute()
                    .left(px(*a))
                    .top(px(wave_top))
                    .w(px((b - a).max(2.0)))
                    .h(px(hatch_h))
                    .bg(amber_fill)
                    .border_l_1()
                    .border_r_1()
                    .border_color(rgba_of(t.warning))
                    .into_any_element()
            })
            .collect();
        let cell = self.area.clone();
        let entity = cx.entity();
        let used = g.width;
        let (c_rects, s_rects) = (cut_rects.clone(), suggested.clone());
        let surface = canvas(
            move |bounds: Bounds<Pixels>, window, cx| {
                cell.set(bounds);
                let w = f32::from(bounds.size.width);
                if (w - used).abs() > 0.5 && w > 8.0 {
                    let e = entity.clone();
                    window.defer(cx, move |_w, cx| {
                        e.update(cx, |this, cx| {
                            if let Some(s) = this.session() {
                                s.geom.resize(w);
                            }
                            cx.notify();
                        })
                    });
                }
                bounds
            },
            move |_bounds, bounds: Bounds<Pixels>, window, _cx| {
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    let mut paint = |x0: f32, x1: f32, color: Hsla| {
                        for ((xa, ya), (xb, yb)) in
                            hatch_lines(x0 + 1.0, x1 - 1.0, hatch_h, HATCH_SPACING)
                        {
                            let mut pb = PathBuilder::stroke(px(1.));
                            pb.move_to(point(
                                bounds.origin.x + px(xa),
                                bounds.origin.y + px(wave_top + ya),
                            ));
                            pb.line_to(point(
                                bounds.origin.x + px(xb),
                                bounds.origin.y + px(wave_top + yb),
                            ));
                            if let Ok(path) = pb.build() {
                                window.paint_path(path, color);
                            }
                        }
                    };
                    for (x0, x1, _) in &c_rects {
                        paint(*x0, *x1, gray);
                    }
                    for (x0, x1) in &s_rects {
                        paint(*x0, *x1, amber);
                    }
                });
            },
        )
        .absolute()
        .size_full();
        let cut_boxes = cut_rects.iter().map(|(a, b, sel)| {
            div()
                .absolute()
                .left(px(*a))
                .top(px(wave_top))
                .w(px((b - a).max(1.0)))
                .h(px(hatch_h))
                .rounded(px(6.))
                .border_1()
                .border_color(rgba_of(if *sel { t.text } else { t.line }))
                .bg(rgba_of(t.surface.with_alpha(70)))
        });
        let (xin, xout) = (x_of(list.start), x_of(list.end));
        let dim = |x0: f32, x1: f32| {
            let (x0, x1) = (x0.clamp(0.0, g.width), x1.clamp(0.0, g.width));
            div()
                .absolute()
                .left(px(x0))
                .top(px(voice::RULER_H))
                .w(px((x1 - x0).max(0.0)))
                .h(px(body_h - voice::RULER_H))
                .bg(rgba_of(t.ground.with_alpha(62)))
        };
        let dims = [dim(0.0, xin), dim(xout, g.width)];
        let range_box = match s.model.selection {
            Selection::Range(r) => {
                let (a, b) = (x_of(r.from).max(0.0), x_of(r.to).min(g.width));
                Some(
                    div()
                        .absolute()
                        .left(px(a))
                        .top(px(voice::RULER_H))
                        .w(px((b - a).max(1.0)))
                        .h(px(body_h - voice::RULER_H))
                        .border_1()
                        .border_color(rgba_of(t.agent))
                        .bg(rgba_of(t.agent.with_alpha(18))),
                )
            }
            _ => None,
        };
        let splits = list.splits.iter().map(|sp| {
            div()
                .absolute()
                .left(px(x_of(*sp) - 1.0))
                .top(px(wave_top - 3.0))
                .w(px(2.))
                .h(px(hatch_h + 6.0))
                .bg(rgba_of(t.text_3))
        });
        let playhead = div()
            .absolute()
            .left(px(ph_x - 1.0))
            .top(px(0.))
            .w(px(2.))
            .h(px(body_h))
            .bg(rgba_of(t.text))
            .child(
                div()
                    .absolute()
                    .left(px(-4.))
                    .top(px(0.))
                    .w(px(10.))
                    .h(px(8.))
                    .rounded_b(px(3.))
                    .bg(rgba_of(t.text)),
            );
        let area = div()
            .id("re-timeline")
            .role(Role::Slider)
            .aria_label(tr!("Timeline"))
            .aria_description(tr!(
                "Left and right arrows move the playhead one frame, with Shift one second."
            ))
            .relative()
            .flex_1()
            .min_w(px(0.))
            .h(px(body_h))
            .overflow_hidden()
            .cursor(CursorStyle::Arrow)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_audio_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .children(ruler)
            .children(wave_rows)
            .children(suggested_boxes)
            .child(surface)
            .children(cut_boxes)
            .children(dims)
            .children(range_box)
            .children(blocks)
            .children(flags)
            .children(splits)
            .child(playhead);
        div()
            .flex_none()
            .flex()
            .gap(px(BODY_GAP))
            .child(labels)
            .child(area)
            .into_any_element()
    }

    /// A press on the lanes: a flag seeks to its marker; anywhere else it is the ruler's gesture
    /// (a click moves the playhead, a drag selects a range, Shift extends).
    fn on_audio_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        let (x, y) = self.local(ev.position);
        let Some(s) = self.session() else { return };
        let g = s.geom;
        s.model.playing = false;
        let tops = lane_tops(s.waves.len());
        if y >= tops.markers && y <= tops.markers + MARKERS_H {
            if let Some(i) = voice::marker_at(&s.voice.markers, |m| g.x_of(m), x) {
                let at = s.voice.markers[i].at;
                s.model.click_time(at);
                s.pacer.stop();
                cx.notify();
                return;
            }
        }
        let tm = g.t_of(x);
        if ev.modifiers.shift {
            s.model.shift_click(tm);
        } else {
            s.drag = Some(Drag::Ruler {
                anchor: tm,
                moved: false,
            });
        }
        cx.notify();
    }

    // ---- the overview --------------------------------------------------------------------

    fn overview_strip(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        self.overview_strip_with(t, s, Vec::new(), true, cx)
    }

    /// The strip with extra ticks (position as a fraction, colour); `padded` aligns it with the
    /// lanes (the label column), the Report view's left column does not.
    pub(super) fn overview_strip_with(
        &self,
        t: &Theme,
        s: &Session,
        extra: Vec<(f32, Color)>,
        padded: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let g = s.geom;
        let end = s.model.basis.end;
        let (v0, v1) = voice::viewport_fractions(&g);
        let n = 240usize;
        // The loudest of the tracks at each step: one picture of the whole recording.
        let cols: Vec<f32> = (0..n)
            .map(|i| {
                let a = MediaTime::from_ticks(end.ticks() * i as i64 / n as i64);
                let b = MediaTime::from_ticks(end.ticks() * (i as i64 + 1) / n as i64);
                s.waves
                    .iter()
                    .map(|(_, w)| w.columns(a, b, 1)[0])
                    .fold(0.0_f32, f32::max)
            })
            .collect();
        let color = hsla(t.mist.with_alpha(70));
        let cell = self.voice_ui.overview.clone();
        let bars = canvas(
            move |bounds: Bounds<Pixels>, _, _| {
                cell.set(bounds);
                bounds
            },
            move |_, bounds: Bounds<Pixels>, window, _| {
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                for (i, p) in cols.iter().enumerate() {
                    let x = (i as f32 + 0.5) / cols.len() as f32 * w;
                    let half = bar_height(*p, h - 12.0) / 2.0;
                    let mut pb = PathBuilder::stroke(px(1.5));
                    pb.move_to(point(
                        bounds.origin.x + px(x),
                        bounds.origin.y + px(h / 2.0 - half),
                    ));
                    pb.line_to(point(
                        bounds.origin.x + px(x),
                        bounds.origin.y + px(h / 2.0 + half),
                    ));
                    if let Ok(path) = pb.build() {
                        window.paint_path(path, color);
                    }
                }
            },
        )
        .absolute()
        .size_full();
        let tick = |frac: f32, w: f32, c: Color| {
            div()
                .absolute()
                .left(gpui::relative(frac))
                .top(px(-2.))
                .w(px(w))
                .h(px(OVERVIEW_H - 4.))
                .bg(rgba_of(c))
        };
        let strip = div()
            .id("re-overview")
            .role(Role::Slider)
            .aria_label(tr!("Whole recording"))
            .relative()
            .h(px(OVERVIEW_H - 4.))
            .rounded(px(10.))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.field))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    let b = this.voice_ui.overview.get();
                    let w = f32::from(b.size.width).max(1.0);
                    let frac = (f32::from(ev.position.x - b.origin.x) / w).clamp(0.0, 1.0);
                    if let Some(s) = this.session() {
                        let at = MediaTime::from_ticks(
                            (s.geom.end.ticks() as f64 * f64::from(frac)) as i64,
                        );
                        s.geom.scroll = voice::centered_scroll(&s.geom, at);
                    }
                    cx.notify();
                }),
            )
            .child(bars)
            .children(
                s.voice
                    .markers
                    .iter()
                    .map(|m| tick(voice::fraction_of(m.at, end), 1.5, t.text)),
            )
            .children(extra.into_iter().map(|(f, c)| tick(f, 2.0, c)))
            .child(
                div()
                    .absolute()
                    .left(gpui::relative(v0))
                    .w(gpui::relative((v1 - v0).max(0.01)))
                    .top(px(1.))
                    .h(px(OVERVIEW_H - 8.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgba_of(t.text))
                    .bg(rgba_of(t.text.with_alpha(8))),
            )
            .child(tick(voice::fraction_of(s.model.playhead, end), 2.0, t.text));
        div()
            .flex_none()
            .pl(px(if padded { LABEL_W + BODY_GAP } else { 0. }))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(shrink_text(if padded {
                        tr!("Whole recording")
                    } else {
                        tr!("Where the report\u{2019}s items come from")
                    }))
                    .child(
                        div()
                            .mono()
                            .child(SharedString::from(format!("00:00 - {}", clock(end)))),
                    ),
            )
            .child(strip)
            .into_any_element()
    }

    /// The speaker of each word (`assign_words`), for the card; empty without speakers.
    pub(super) fn ai_labels(&self, s: &Session) -> Vec<cm_ai::speakers::WordLabel> {
        let words = &self.ai.words;
        if s.voice.segments.is_empty() {
            return words
                .iter()
                .map(|_| cm_ai::speakers::WordLabel {
                    speaker: None,
                    overlap: false,
                })
                .collect();
        }
        cm_ai::speakers::assign_words(words, &s.voice.segments)
    }

    /// The transcript of the language shown, as turns and rows, kept until something it is built
    /// from changes.
    pub(super) fn voice_turns(&self, s: &Session) -> TurnsOf {
        let key = (self.ai.rev, s.voice.rev, self.voice_ui.lang.clone());
        if let Some(c) = self.voice_ui.cache.borrow().as_ref() {
            if c.key == key {
                return (c.lines.clone(), c.turns.clone(), c.rows.clone());
            }
        }
        let lines: Vec<CaptionLine> = match &self.voice_ui.lang {
            Some(l) => s.voice.lines_for(l).map(<[_]>::to_vec).unwrap_or_default(),
            None => self.ai.lines().to_vec(),
        };
        let turns = match &self.voice_ui.lang {
            Some(_) => voice::build_turns_by_segments(&lines, &s.voice.segments),
            None => {
                let labels = self.ai_labels(s);
                voice::build_turns(&lines, &self.ai.words, &labels)
            }
        };
        let rows = voice::rows(&turns, &s.voice.markers);
        let out = (Rc::new(lines), Rc::new(turns), Rc::new(rows));
        *self.voice_ui.cache.borrow_mut() = Some(TurnCache {
            key,
            lines: out.0.clone(),
            turns: out.1.clone(),
            rows: out.2.clone(),
        });
        out
    }
}

/// A round avatar with the speaker's initial in ink (the colours are light enough for it).
pub(super) fn avatar(t: &Theme, name: &str, colour: Color, size: f32) -> gpui::Div {
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(size / 2.))
        .bg(rgba_of(colour))
        .text_color(rgba_of(t.ink))
        .text_size(px(size * 0.42))
        .font_weight(FontWeight::SEMIBOLD)
        .child(SharedString::from(initial(name)))
}

/// The small hatched square before "Suggested cut".
fn hatch_swatch(t: &Theme) -> gpui::Div {
    let amber = hsla(t.warning);
    div()
        .size(px(14.))
        .flex_none()
        .rounded(px(3.))
        .border_1()
        .border_color(rgba_of(t.warning))
        .overflow_hidden()
        .child(
            canvas(
                |bounds: Bounds<Pixels>, _, _| bounds,
                move |_, bounds: Bounds<Pixels>, window, _| {
                    for ((xa, ya), (xb, yb)) in hatch_lines(0.0, 14.0, 14.0, 4.0) {
                        let mut pb = PathBuilder::stroke(px(1.));
                        pb.move_to(point(bounds.origin.x + px(xa), bounds.origin.y + px(ya)));
                        pb.line_to(point(bounds.origin.x + px(xb), bounds.origin.y + px(yb)));
                        if let Ok(path) = pb.build() {
                            window.paint_path(path, amber);
                        }
                    }
                },
            )
            .absolute()
            .size_full(),
        )
}

/// The "N speakers" fragment of the transcript's subtitle.
pub(super) fn speakers_label(n: usize) -> Option<String> {
    (n > 0).then(|| trn!("{n} speaker", "{n} speakers", n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_speakers_fragment_counts() {
        assert_eq!(speakers_label(0), None);
        assert_eq!(speakers_label(1).as_deref(), Some("1 speaker"));
        assert_eq!(speakers_label(4).as_deref(), Some("4 speakers"));
    }

    #[test]
    fn a_new_recording_starts_with_the_defaults() {
        let u = VoiceUi::new();
        assert!(u.lang.is_none() && u.sheet.is_none() && !u.translate_open);
        assert!(u.speaker_names && !u.stale);
    }
}
