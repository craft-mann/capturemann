//! A recording in the library (LIB-1): a read-only details card with the poster, the facts from the
//! manifest, the exports (the single MP4 with the camera bubble and audio that follows every
//! recording; Play and Reveal act on it by default, the masters are listed under Tracks and can be
//! played too) and an Export button for recordings without one. Edit opens the recording editor
//! (`ui::recording_editor`); this card is deliberately not an editor.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    div, img, prelude::*, px, Context, Entity, FontWeight, ObjectFit, Render, RenderImage, Role,
    SharedString, Window,
};
use image::Frame;

use cm_i18n::{tr, tr_ctx};

use crate::library::{format_size, LocalClock};
use crate::poster::{decode_poster, MAX_POSTER_WIDTH};
use crate::recording::exports::Phase;
use crate::recording::files::{self, Details};
use crate::recording::model::clock_text;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, AppModel, Route};
use crate::ui::components::{
    button_with_icon, card, crumb_link, icon, icon_button, icon_size, page_toolbar_actions,
    section_label, shrink_text, switch, ButtonKind,
};
use crate::ui::export_ui;
use crate::ui::kit::{tone_colors, Tone};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::small_button_with_icon;
use crate::ui::studio::ThemeGlobal;
use crate::ui::tooltip::tip;

enum Load {
    Idle,
    Loading,
    Ready {
        details: Box<Details>,
        poster: Option<Arc<RenderImage>>,
        /// Why there is no poster, for the placeholder caption.
        poster_note: Option<files::PosterError>,
    },
    Failed(files::FileError),
}

pub struct RecordingDetails {
    app: Entity<AppModel>,
    id: Option<String>,
    load: Load,
    action_error: Option<files::FileError>,
    /// `library_epoch` the card was loaded at: an export finishing bumps it and reloads the card.
    epoch: u64,
}

impl RecordingDetails {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        // The model changes when the library does (an export finished: `capture.revised`) and
        // `export_ui` pokes it for progress: reload on the former, repaint on the latter.
        cx.observe(&app, |this, app, cx| {
            let epoch = app.read(cx).library_epoch;
            if epoch != this.epoch {
                this.epoch = epoch;
                this.reload(cx);
            } else {
                cx.notify();
            }
        })
        .detach();
        let epoch = app.read(cx).library_epoch;
        RecordingDetails {
            app,
            id: None,
            load: Load::Idle,
            action_error: None,
            epoch,
        }
    }

    /// Shows recording `id` (a no-op while it is already shown).
    pub fn set_capture(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.id.as_deref() == Some(id) {
            return;
        }
        self.id = Some(id.to_owned());
        self.load = Load::Loading;
        self.action_error = None;
        self.start_load(cx);
    }

    /// Loads the card again, keeping what is shown until the new data arrives.
    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.id.is_some() {
            self.start_load(cx);
        }
    }

    fn start_load(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else {
            return;
        };
        let root = self.app.read(cx).backend.library_root.clone();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move { load_blocking(root, &id).map(|l| (id, l)) })
                .await;
            this.update(cx, |this, cx| {
                match loaded {
                    Ok((id, (details, poster, note)))
                        if this.id.as_deref() == Some(id.as_str()) =>
                    {
                        let image =
                            poster.map(|rgba| Arc::new(RenderImage::new([Frame::new(rgba)])));
                        this.load = Load::Ready {
                            details: Box::new(details),
                            poster: image,
                            poster_note: note,
                        };
                    }
                    Ok(_) => {}
                    Err(e) => {
                        crate::ui::app::note_open_failed(&e.to_string(), cx);
                        this.load = Load::Failed(e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn run(
        &mut self,
        action: fn(&std::path::Path) -> Result<(), files::FileError>,
        cx: &mut Context<Self>,
    ) {
        if let Load::Ready { details, .. } = &self.load {
            if let Err(e) = action(&details.dir) {
                self.action_error = Some(e);
                cx.notify();
            }
        }
    }
}

type Loaded = (
    Details,
    Option<image::RgbaImage>,
    Option<files::PosterError>,
);

fn load_blocking(root: Option<PathBuf>, id: &str) -> Result<Loaded, files::FileError> {
    let root = root.ok_or(files::FileError::NoLibrary)?;
    let dir = files::find_bundle(&root, id).ok_or(files::FileError::NoFolder)?;
    let details = files::read_details(&dir)?;
    let (poster, note) = poster_blocking(&root, id, &dir);
    Ok((details, poster, note))
}

/// The cached poster, made on first use (FFmpeg or Quick Look); `None` with the reason when
/// neither works.
pub fn poster_blocking(
    root: &std::path::Path,
    id: &str,
    dir: &std::path::Path,
) -> (Option<image::RgbaImage>, Option<files::PosterError>) {
    let (path, note) = poster_file(root, id, dir);
    decode_poster_file(&path, note)
}

/// Makes the poster file if it does not exist (this may run FFmpeg or Quick Look, for up to
/// [`files::POSTER_TIMEOUT`]; hold no lock around it) and says why when it could not.
pub fn poster_file(
    root: &std::path::Path,
    id: &str,
    dir: &std::path::Path,
) -> (std::path::PathBuf, Option<files::PosterError>) {
    let path = files::poster_path(root, id);
    let mut note = None;
    if !path.is_file() {
        if let Err(e) = files::generate_poster(dir, &path) {
            note = Some(e);
        }
    }
    (path, note)
}

/// Reads and decodes the poster file.
pub fn decode_poster_file(
    path: &std::path::Path,
    note: Option<files::PosterError>,
) -> (Option<image::RgbaImage>, Option<files::PosterError>) {
    match std::fs::read(path)
        .map_err(|e| files::PosterError::Os(e.to_string()))
        .and_then(|b| decode_poster(&b, MAX_POSTER_WIDTH))
    {
        Ok(img) => (Some(img), None),
        Err(e) => (None, note.or(Some(e))),
    }
}

/// One fact of the details page: a stat tile (board R2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub glyph: &'static str,
    pub label: String,
    pub value: String,
}

/// The stat tiles, in order: Recorded, Length and Video only when known, Pauses only when there
/// were any (the board draws the first four; Pauses is kept, OD-20).
pub fn facts(
    recorded: String,
    length: Option<String>,
    size: String,
    video: Option<String>,
    pauses: usize,
) -> Vec<Fact> {
    let mut v = vec![Fact {
        glyph: "calendar",
        label: tr_ctx!("stat tile", "Recorded"),
        value: recorded,
    }];
    if let Some(value) = length {
        v.push(Fact {
            glyph: "clock",
            label: tr_ctx!("stat tile", "Length"),
            value,
        });
    }
    v.push(Fact {
        glyph: "hard-drive",
        label: tr_ctx!("stat tile", "Size on disk"),
        value: size,
    });
    if let Some(value) = video {
        v.push(Fact {
            glyph: "monitor",
            label: tr_ctx!("stat tile", "Video"),
            value,
        });
    }
    if pauses > 0 {
        v.push(Fact {
            glyph: "circle-pause",
            label: tr_ctx!("stat tile", "Pauses"),
            value: pauses.to_string(),
        });
    }
    v
}

/// The icon of a track pill by the track's name; unknown names get a generic video file.
pub fn track_glyph(name: &str) -> &'static str {
    match name {
        "screen" => "monitor",
        "cursor" => "mouse-pointer-2",
        "mic" => "mic",
        "camera" => "video",
        "system" | "system_audio" | "system-audio" => "volume-2",
        _ => "file-video",
    }
}

/// The meta line of an upload row: what was copied, its size and when (the destination is the
/// row's title).
pub fn upload_meta(u: &crate::published::UploadRecord, when: &str) -> String {
    let mut parts = vec![u.what_label().to_owned()];
    if u.bytes > 0 {
        parts.push(format_size(u.bytes));
    }
    if !when.is_empty() {
        parts.push(when.to_owned());
    }
    parts.join(" \u{00B7} ")
}

/// The meta line of a finished export: the file, its size, length and encoder.
pub fn export_meta(e: &files::ExportLine) -> String {
    if !e.exists {
        return tr!("{name} (the file was deleted)", name = file_name(&e.file));
    }
    let mut parts = vec![file_name(&e.file), format_size(e.bytes)];
    if let Some(ms) = e.duration_ms {
        parts.push(clock_text(ms));
    }
    if let Some(enc) = &e.encoder {
        parts.push(enc.clone());
    }
    parts.join(" \u{00B7} ")
}

/// The index of the export row that "Reveal export" acts on: the first one that is still on disk
/// (the default export, the one Play uses).
pub fn reveal_export_row(exports: &[files::ExportLine]) -> Option<usize> {
    exports.iter().position(|e| e.exists)
}

/// A 32 px icon-only button with a tooltip and an aria label (row actions on the page).
fn small_icon_button(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    aria: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let aria: SharedString = aria.into();
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(aria.clone())
        .focusable()
        .tab_stop(true)
        .size(px(32.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(rgba_of(t.line))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
        .tooltip(tip(aria, Vec::new()))
        .child(icon(glyph, icon_size::SMALL, t.text_2))
}

/// A stat tile: the icon and the label over a mono value (board R2).
fn fact_tile(t: &Theme, f: &Fact) -> gpui::AnyElement {
    div()
        .flex_basis(px(150.))
        .flex_grow(1.)
        .flex_shrink(1.)
        .min_w(px(0.))
        .px(px(16.))
        .py(px(14.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.surface))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(icon(f.glyph, 13., t.mist))
                .child(shrink_text(f.label.clone())),
        )
        .child(
            div()
                .text_size(px(15.))
                .text_color(rgba_of(t.text))
                .mono()
                .child(SharedString::from(f.value.clone())),
        )
        .into_any_element()
}

/// A track pill: the icon, the name and the size in mono; the file name is its tooltip and part
/// of its accessible name. A track that could not be recovered is a warning pill.
fn track_pill(t: &Theme, ix: usize, tr: &files::TrackLine) -> gpui::AnyElement {
    let tone = tone_colors(t, Tone::Brass);
    let tip_text = SharedString::from(tr.file.clone());
    let aria = if tr.missing {
        tr!(
            "{name}, {file}, could not be recovered",
            name = tr.name,
            file = tr.file
        )
    } else {
        tr!(
            "{name}, {file}, {size}",
            name = tr.name,
            file = tr.file,
            size = format_size(tr.bytes)
        )
    };
    let glyph_color = if tr.missing {
        tone.icon
    } else if tr.name == "camera" {
        t.rec_soft
    } else {
        t.text_2
    };
    div()
        .id(("rec-track", ix))
        .aria_label(SharedString::from(aria))
        .h(px(34.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .rounded(px(scale::RADIUS_PILL))
        .border_1()
        .border_color(rgba_of(if tr.missing { tone.border } else { t.border }))
        .bg(rgba_of(if tr.missing { tone.bg } else { t.surface_2 }))
        .text_size(px(13.))
        .text_color(rgba_of(if tr.missing { tone.fg } else { t.text }))
        .tooltip(tip(tip_text, Vec::new()))
        .child(icon(
            if tr.missing {
                "triangle-alert"
            } else {
                track_glyph(&tr.name)
            },
            icon_size::SMALL,
            glyph_color,
        ))
        .child(SharedString::from(tr.name.clone()))
        .child(
            div()
                .text_size(px(11.5))
                .text_color(rgba_of(if tr.missing { tone.fg } else { t.mist }))
                .mono()
                .child(SharedString::from(if tr.missing {
                    tr!("could not be recovered")
                } else {
                    format_size(tr.bytes)
                })),
        )
        .into_any_element()
}

/// One row of the Exports & uploads card: a 36 px icon tile, the title (with an optional status
/// icon after it), a mono meta line, and the row's actions at the right.
fn upload_row(
    t: &Theme,
    glyph: &'static str,
    title: SharedString,
    status_glyph: Option<(&'static str, crate::theme::Color)>,
    meta: SharedString,
    meta_color: crate::theme::Color,
    actions: Vec<gpui::AnyElement>,
) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .child(
            div()
                .size(px(36.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_MD))
                .bg(rgba_of(t.icon_tile()))
                .child(icon(glyph, 17., t.text)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(13.5))
                        .font_weight(FontWeight::MEDIUM)
                        .child(div().truncate().child(title))
                        .children(status_glyph.map(|(g, c)| icon(g, 13., c))),
                )
                .child(
                    div()
                        .w_full()
                        .truncate()
                        .text_size(px(11.5))
                        .text_color(rgba_of(meta_color))
                        .mono()
                        .child(meta),
                ),
        )
        .children(actions)
        .into_any_element()
}

impl Render for RecordingDetails {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let clock = self.app.read(cx).clock;
        let app = self.app.clone();
        let crumb = crumb_link(
            &t,
            "rec-back",
            tr_ctx!("breadcrumb", "Recordings /"),
            tr!("Back to recordings"),
        )
        .on_click(move |_, _, cx| {
            let app = app.clone();
            cx.defer(move |cx| app::set_route(&app, Route::Recordings, cx));
        });
        let mut actions: Vec<gpui::AnyElement> = Vec::new();
        let body = match &self.load {
            Load::Idle | Load::Loading => div()
                .p(px(24.))
                .text_color(rgba_of(t.text_3))
                .child(tr!("Loading\u{2026}"))
                .into_any_element(),
            Load::Failed(e) => div()
                .p(px(24.))
                .text_color(rgba_of(t.text_2))
                .child(SharedString::from(e.to_string()))
                .into_any_element(),
            Load::Ready {
                details,
                poster,
                poster_note,
            } => {
                let d = details;
                // The stored transcript is read when the page opens (and again when it may have
                // changed), so closing and reopening the details still shows it.
                crate::ui::ai_view::prime_transcript_info(&self.app, &d.id, cx);
                let has_export = d.exports.iter().any(|e| e.exists);
                let exporting =
                    export_ui::status_of(&d.id).is_some_and(|s| s.phase == Phase::Running);
                // The "Burn in captions" switch beside Export: only when the recording has a
                // transcript; it starts at the recording's own setting.
                let burn_choice: Option<bool> = {
                    let m = self.app.read(cx);
                    m.transcribe
                        .transcripts
                        .info(&d.id)
                        .map(|i| m.transcribe.export_burn(&d.id, i.options.burn_in))
                };

                // ---- the header's verbs: Reveal, Publish..., Edit (the neutral primary) -----
                let mac = cfg!(target_os = "macos");
                let reveal_label = if mac {
                    tr!("Reveal in Finder")
                } else {
                    tr!("Show in folder")
                };
                actions.push(
                    icon_button(
                        &t,
                        "rec-reveal",
                        "folder-open",
                        reveal_label,
                        false,
                        Some(Vec::new()),
                    )
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface_2))
                    .on_click(cx.listener(|this, _, _, cx| this.run(files::reveal, cx)))
                    .into_any_element(),
                );
                actions.push({
                    // PUB-2: the publish sheet (YouTube). It offers to export first.
                    let (app, id) = (self.app.clone(), d.id.clone());
                    button_with_icon(
                        &t,
                        "rec-publish",
                        tr!("Publish\u{2026}"),
                        ButtonKind::Secondary,
                        Some("send"),
                    )
                    .on_click(move |_, _, cx| {
                        let (app, id) = (app.clone(), id.clone());
                        cx.defer(move |cx| crate::ui::publish_sheet::open(&app, &id, cx));
                    })
                    .into_any_element()
                });
                actions.push({
                    let (app, id) = (self.app.clone(), d.id.clone());
                    button_with_icon(
                        &t,
                        "rec-edit",
                        tr_ctx!("button", "Edit"),
                        ButtonKind::Primary,
                        Some("square-pen"),
                    )
                    .on_click(move |_, _, cx| {
                        let (app, id) = (app.clone(), id.clone());
                        cx.defer(move |cx| app::set_route(&app, Route::RecordingEditor(id), cx));
                    })
                    .into_any_element()
                });

                // ---- the poster with its play button (the "Play export" handler) ------------
                let play_label = if has_export {
                    tr!("Play export")
                } else {
                    tr!("Play in system player")
                };
                let play_button = div()
                    .id("rec-poster-play")
                    .role(Role::Button)
                    .aria_label(play_label.clone())
                    .focusable()
                    .tab_stop(true)
                    .size(px(64.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(32.))
                    .border_1()
                    .border_color(rgba_of(Theme::DARK.text.with_alpha(18)))
                    .bg(rgba_of(Theme::DARK.ink.with_alpha(70)))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba_of(Theme::DARK.ink.with_alpha(88))))
                    .focus_visible(|s| s.border_color(rgba_of(Theme::DARK.text)))
                    .tooltip(tip(play_label.clone(), Vec::new()))
                    .child(icon("play", 24., Theme::DARK.text))
                    .on_click(
                        cx.listener(|this, _, _, cx| this.run(files::play_in_system_player, cx)),
                    );
                let frame = div()
                    .relative()
                    .w_full()
                    .aspect_ratio(16. / 10.)
                    .flex_none()
                    .overflow_hidden()
                    .rounded(px(scale::RADIUS_DIALOG))
                    .bg(rgba_of(t.ink))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .child(match poster {
                        Some(p) => img(p.clone())
                            .size_full()
                            .object_fit(ObjectFit::Contain)
                            .into_any_element(),
                        None => div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_end()
                            .gap(px(6.))
                            .pb(px(28.))
                            .child(
                                div()
                                    .max_w(px(360.))
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(type_scale::CAPTION))
                                    .text_color(rgba_of(t.mist))
                                    .child(SharedString::from(tr!(
                                        "No preview image yet: CaptureMann has no video decoder of its own until the editor ships. Play the recording to see it."
                                    ))),
                            )
                            .children(poster_note.clone().map(|n| {
                                div()
                                    .max_w(px(420.))
                                    .text_align(gpui::TextAlign::Center)
                                    .text_size(px(11.))
                                    .text_color(rgba_of(t.mist))
                                    .child(SharedString::from(n.to_string()))
                            }))
                            .into_any_element(),
                    })
                    .child(
                        div()
                            .absolute()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(play_button),
                    )
                    .children(d.duration_s.map(|s| {
                        div()
                            .absolute()
                            .right(px(14.))
                            .bottom(px(14.))
                            .h(px(24.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .rounded(px(7.))
                            .bg(rgba_of(Theme::DARK.ink.with_alpha(80)))
                            .text_size(px(12.))
                            .text_color(rgba_of(Theme::DARK.text))
                            .mono()
                            .child(SharedString::from(clock_text((s * 1000.0) as i64)))
                    }));
                let play_row = div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        button_with_icon(
                            &t,
                            "rec-play",
                            play_label,
                            ButtonKind::Secondary,
                            Some("play"),
                        )
                        .on_click(
                            cx.listener(|this, _, _, cx| {
                                this.run(files::play_in_system_player, cx)
                            }),
                        ),
                    )
                    .children(has_export.then(|| {
                        button_with_icon(
                            &t,
                            "rec-play-master",
                            tr!("Play original"),
                            ButtonKind::Ghost,
                            Some("film"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.run(files::play_master, cx)))
                    }))
                    .children(self.action_error.clone().map(|e| {
                        div()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.warning))
                            .child(SharedString::from(e.to_string()))
                    }));

                // ---- the facts and the tracks -----------------------------------------------
                let tiles = facts(
                    clock_label(&clock, d.created),
                    d.duration_s.map(|s| clock_text((s * 1000.0) as i64)),
                    format_size(d.size_bytes),
                    d.video.clone(),
                    d.pauses,
                );
                let tracks = div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(section_label(&t, &tr_ctx!("section", "Tracks")))
                    .child(
                        div().flex().flex_wrap().gap(px(8.)).children(
                            d.tracks
                                .iter()
                                .enumerate()
                                .map(|(i, tr)| track_pill(&t, i, tr)),
                        ),
                    );
                let left = {
                    let c = div()
                        .flex_basis(px(520.))
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .child(frame)
                        .child(play_row)
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(10.))
                                .children(tiles.iter().map(|f| fact_tile(&t, f))),
                        )
                        .child(tracks);
                    c.flex_grow(999.)
                };

                // ---- Exports & uploads --------------------------------------------------------
                let mut rows: Vec<gpui::AnyElement> = Vec::new();
                let status = export_ui::status_of(&d.id);
                let reveal_ix = reveal_export_row(&d.exports);
                for (i, e) in d.exports.iter().enumerate() {
                    let acts: Vec<gpui::AnyElement> = (Some(i) == reveal_ix)
                        .then(|| {
                            small_icon_button(
                                &t,
                                "rec-reveal-export",
                                "folder-open",
                                tr!("Reveal export"),
                            )
                            .on_click(
                                cx.listener(|this, _, _, cx| this.run(files::reveal_export, cx)),
                            )
                            .into_any_element()
                        })
                        .into_iter()
                        .collect();
                    rows.push(upload_row(
                        &t,
                        "file-output",
                        SharedString::from(e.preset.clone()),
                        None,
                        SharedString::from(export_meta(e)),
                        if e.exists { t.mist } else { t.warning },
                        acts,
                    ));
                }
                match status {
                    Some(s) if s.phase == Phase::Running => rows.push(upload_row(
                        &t,
                        "loader-circle",
                        tr_ctx!("export row", "Exporting").into(),
                        None,
                        format!("{}%", (s.progress * 100.0).floor() as u32).into(),
                        t.text_2,
                        Vec::new(),
                    )),
                    Some(s) if s.phase == Phase::Failed => rows.push(upload_row(
                        &t,
                        "triangle-alert",
                        tr!("Export failed").into(),
                        None,
                        s.reason
                            .map(|r| {
                                crate::daemon_text::error_text(
                                    Some(cm_core::ErrorCode::NotFound),
                                    &r,
                                )
                            })
                            .unwrap_or_else(|| tr!("The export failed."))
                            .into(),
                        t.warning,
                        Vec::new(),
                    )),
                    _ => {}
                }
                rows.extend(upload_rows(&t, &d.id));
                rows.extend(finished_upload_rows(&t, d, &clock, &self.app));
                if rows.is_empty() {
                    rows.push(upload_row(
                        &t,
                        "file-output",
                        tr_ctx!("exports list", "None yet").into(),
                        None,
                        tr!("Press Export MP4 to make one file with the screen, camera bubble and audio.")
                            .into(),
                        t.mist,
                        Vec::new(),
                    ));
                }
                let export_button = (!exporting).then(|| {
                    // "Export again" re-renders with the current edits and captions (the automatic
                    // export after recording has no transcript yet).
                    let (app, id) = (self.app.clone(), d.id.clone());
                    small_button_with_icon(
                        &t,
                        "rec-export",
                        &if has_export {
                            tr!("Export again")
                        } else {
                            tr!("Export MP4")
                        },
                        Some("file-output"),
                    )
                    .on_click(move |_, _, cx| {
                        let (app, id) = (app.clone(), id.clone());
                        cx.defer(move |cx| export_ui::request(&app, &id, burn_choice, cx));
                    })
                });
                // VOC-2: the sound alone, with the same cuts (the Export audio sheet).
                let has_audio = d
                    .tracks
                    .iter()
                    .any(|t| matches!(t.name.as_str(), "mic" | "system") && !t.missing);
                let audio_button = has_audio.then(|| {
                    let (app, id) = (self.app.clone(), d.id.clone());
                    small_button_with_icon(
                        &t,
                        "rec-export-audio",
                        &tr!("Export audio only"),
                        Some("file-headphone"),
                    )
                    .on_click(move |_, _, cx| {
                        let (app, id) = (app.clone(), id.clone());
                        cx.defer(move |cx| crate::ui::ai::open_export_audio(&app, &id, cx));
                    })
                });
                let burn_row = burn_choice.filter(|_| !exporting).map(|on| {
                    let (app, id) = (self.app.clone(), d.id.clone());
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .pt(px(12.))
                        .border_t_1()
                        .border_color(rgba_of(t.border))
                        .child(icon("captions", 16., t.text_2))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(13.5))
                                .text_color(rgba_of(t.text_2))
                                .child(tr!("Burn in captions on next export")),
                        )
                        .child(
                            switch(
                                &t,
                                "rec-export-burn",
                                tr!("Burn captions into the exported video"),
                                on,
                                true,
                            )
                            .on_click(move |_, _, cx| {
                                let (app, id) = (app.clone(), id.clone());
                                cx.defer(move |cx| {
                                    crate::ui::ai::set_export_burn(&app, &id, !on, cx)
                                });
                            }),
                        )
                });
                let exports_card = card(&t)
                    .p(px(18.))
                    .rounded(px(16.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .mr_auto()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(tr!("Exports & uploads")),
                            )
                            .children(audio_button)
                            .children(export_button),
                    )
                    .children(rows)
                    .children(burn_row);
                let transcript_card = card(&t).p(px(18.)).rounded(px(16.)).child(
                    crate::ui::recording::transcript_card::transcript_section(
                        &t, &self.app, &d.id, "rec", cx,
                    ),
                );
                let right = div()
                    .flex_basis(px(400.))
                    .flex_grow(1.)
                    .flex_shrink(1.)
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .child(exports_card)
                    .child(transcript_card)
                    .child(
                        div()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.mist))
                            .child(tr!("Edit opens the trim, cut, split, speed, background, camera, zoom, cursor and reframe tools. The MP4 export applies them to a copy; the recording itself is never changed by looking at it, editing it or exporting it.")),
                    );
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .gap(px(24.))
                    .child(left)
                    .child(right)
                    .into_any_element()
            }
        };
        let title = match &self.load {
            Load::Ready { details, .. } => details.title.clone(),
            _ => tr_ctx!("page title", "Recording"),
        };
        let toolbar = page_toolbar_actions(
            &t,
            "recording-toolbar",
            tr!("Recording toolbar"),
            crumb,
            SharedString::from(title),
            actions,
        );
        div()
            .id("recording-details")
            .size_full()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(
                div()
                    .id("recording-details-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .px(px(28.))
                            .pt(px(24.))
                            .pb(px(28.))
                            .child(body),
                    ),
            )
    }
}

/// The uploads of this recording that are not finished (STO-3): "Uploading to Google Drive: 40%",
/// or the wait for a reconnect.
fn upload_rows(t: &Theme, capture_id: &str) -> Vec<gpui::AnyElement> {
    crate::ui::upload_ui::running_of(capture_id)
        .into_iter()
        .map(|(s, destination)| {
            let waiting = crate::ui::upload_ui::is_waiting(&s);
            upload_row(
                t,
                if waiting { "unplug" } else { "cloud-upload" },
                if waiting {
                    tr!("Upload waiting")
                } else {
                    tr_ctx!("upload row", "Uploading")
                }
                .into(),
                None,
                SharedString::from(crate::ui::upload_ui::status_line(&s, &destination)),
                if waiting { t.warning } else { t.text_2 },
                Vec::new(),
            )
        })
        .collect()
}

/// The copies that finished (STO-4.AC3): the destination with a check, "Video \u{b7} 12.3 MB \u{b7}
/// Oct 21, 2026", and Copy link and Open icon buttons when the upload has a share link. A local
/// folder copy has none (and shows a folder, not a drive).
fn finished_upload_rows(
    t: &Theme,
    d: &Details,
    clock: &LocalClock,
    app: &Entity<AppModel>,
) -> Vec<gpui::AnyElement> {
    d.uploads
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let when = u.at.map(|a| clock.format_date(a)).unwrap_or_default();
            let acts: Vec<gpui::AnyElement> = match u.share_link.clone() {
                None => Vec::new(),
                Some(link) => {
                    let (a1, l1, a2, l2) = (app.clone(), link.clone(), app.clone(), link);
                    vec![
                        small_icon_button(t, ("upload-copy-link", i), "link", tr!("Copy link"))
                            .on_click(move |_, _, cx| {
                                let (app, l) = (a1.clone(), l1.clone());
                                cx.defer(move |cx| crate::ui::upload_ui::copy_link(&app, &l, cx));
                            })
                            .into_any_element(),
                        small_icon_button(
                            t,
                            ("upload-open-link", i),
                            "external-link",
                            tr_ctx!("button", "Open"),
                        )
                        .on_click(move |_, _, cx| {
                            let (app, l) = (a2.clone(), l2.clone());
                            cx.defer(move |cx| crate::ui::upload_ui::open_link(&app, &l, cx));
                        })
                        .into_any_element(),
                    ]
                }
            };
            upload_row(
                t,
                if u.share_link.is_some() {
                    "hard-drive"
                } else {
                    "folder"
                },
                SharedString::from(u.destination.clone()),
                Some(("cloud-check", t.success)),
                SharedString::from(upload_meta(u, &when)),
                t.mist,
                acts,
            )
        })
        .collect()
}

fn file_name(rel: &str) -> String {
    rel.rsplit('/').next().unwrap_or(rel).to_owned()
}

fn clock_label(clock: &LocalClock, t: time::OffsetDateTime) -> String {
    clock.format_date(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::published::UploadRecord;

    fn export(file: &str, exists: bool) -> files::ExportLine {
        files::ExportLine {
            file: file.to_owned(),
            preset: "quick".to_owned(),
            bytes: 4_500_000,
            exists,
            encoder: Some("h264_videotoolbox".to_owned()),
            duration_ms: Some(4_000),
        }
    }

    /// Board R2: four tiles in order; Length and Video only when known; Pauses (not on the board)
    /// is kept, only when there were pauses.
    #[test]
    fn facts_follow_the_board_and_keep_pauses() {
        let all = facts(
            "Today 15:40".into(),
            Some("00:04".into()),
            "11.9 MB".into(),
            Some("3024x1964 \u{b7} 30 fps \u{b7} hevc".into()),
            0,
        );
        let labels: Vec<_> = all.iter().map(|f| f.label.clone()).collect();
        assert_eq!(labels, ["Recorded", "Length", "Size on disk", "Video"]);
        let glyphs: Vec<_> = all.iter().map(|f| f.glyph).collect();
        assert_eq!(glyphs, ["calendar", "clock", "hard-drive", "monitor"]);
        let sparse = facts("Today".into(), None, "1 MB".into(), None, 2);
        let labels: Vec<_> = sparse.iter().map(|f| f.label.clone()).collect();
        assert_eq!(labels, ["Recorded", "Size on disk", "Pauses"]);
        assert_eq!(sparse[2].value, "2");
    }

    /// Every track kind has an icon; unknown names fall back to a video file.
    #[test]
    fn track_pills_have_icons() {
        assert_eq!(track_glyph("screen"), "monitor");
        assert_eq!(track_glyph("cursor"), "mouse-pointer-2");
        assert_eq!(track_glyph("mic"), "mic");
        assert_eq!(track_glyph("camera"), "video");
        assert_eq!(track_glyph("system"), "volume-2");
        assert_eq!(track_glyph("whatever"), "file-video");
    }

    /// The export meta line keeps the file, size, length and encoder; a deleted file says so.
    #[test]
    fn export_meta_lines() {
        let e = export("exports/Rec - quick.mp4", true);
        assert_eq!(
            export_meta(&e),
            "Rec - quick.mp4 \u{b7} 4.5 MB \u{b7} 00:04 \u{b7} h264_videotoolbox"
        );
        assert_eq!(
            export_meta(&export("exports/gone.mp4", false)),
            "gone.mp4 (the file was deleted)"
        );
    }

    /// "Reveal export" belongs to the first export still on disk.
    #[test]
    fn reveal_export_goes_to_the_first_existing_export() {
        assert_eq!(reveal_export_row(&[]), None);
        let v = vec![
            export("a.mp4", false),
            export("b.mp4", true),
            export("c.mp4", true),
        ];
        assert_eq!(reveal_export_row(&v), Some(1));
        assert_eq!(reveal_export_row(&v[..1]), None);
    }

    /// The upload meta line leaves the destination to the row title.
    #[test]
    fn upload_meta_drops_the_destination() {
        let u = UploadRecord {
            destination_id: "d".into(),
            destination: "Google Drive".into(),
            what: "export".into(),
            bytes: 4_500_000,
            share_link: None,
            at: None,
        };
        assert_eq!(
            upload_meta(&u, "Today 15:40"),
            "Video \u{b7} 4.5 MB \u{b7} Today 15:40"
        );
        let o = UploadRecord {
            what: "original".into(),
            bytes: 0,
            ..u
        };
        assert_eq!(upload_meta(&o, ""), "Original recording");
    }
}
