//! The Published page: every video published from a recording, newest first, with its link, the
//! privacy the platform reports, the status in plain words and the warnings. All decisions live
//! in `published` (tested without a display); this view loads `list_library` and draws rows.
//!
//! It loads when the app starts (so the sidebar number is real before the page is opened) and
//! again whenever the library changes (`library_epoch`: a publish finished, a capture was
//! deleted).

use gpui::{
    div, img, prelude::*, px, Context, Entity, FontWeight, ObjectFit, Render, Role, SharedString,
    Task, Window,
};

use cm_i18n::{tr, tr_ctx};

use crate::backend::Backend;
use crate::daemon_text::ErrorNote;
use crate::library::LocalClock;
use crate::published::{self, PublishedRow};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, AppModel, Route};
use crate::ui::components::{button_with_icon, card, icon, icon_button, ButtonKind};
use crate::ui::kit;
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::studio::ThemeGlobal;
use crate::ui::thumbs::{Thumb, ThumbCache};
use crate::ui::upload_ui;

enum Load {
    Loading,
    Ready,
    /// `None`: the library is not available (no daemon); `Some`: what the daemon answered.
    Failed(Option<ErrorNote>),
}

pub struct PublishedView {
    app: Entity<AppModel>,
    rows: Vec<PublishedRow>,
    load: Load,
    /// Bumped by every request so a slow earlier answer cannot overwrite a later one.
    generation: u64,
    seen_epoch: u64,
    thumbs: Entity<ThumbCache>,
    /// The row whose Copy link button shows "Copied".
    copied: Option<usize>,
    copied_timer: Option<Task<()>>,
}

impl PublishedView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |this, app, cx| {
            let epoch = app.read(cx).library_epoch;
            if epoch != this.seen_epoch {
                this.seen_epoch = epoch;
                this.reload(cx);
            }
        })
        .detach();
        let seen_epoch = app.read(cx).library_epoch;
        let backend = app.read(cx).backend.clone();
        let thumbs = cx.new(|_| ThumbCache::new(backend));
        cx.observe(&thumbs, |_, _, cx| cx.notify()).detach();
        let mut view = PublishedView {
            app,
            rows: Vec::new(),
            load: Load::Loading,
            generation: 0,
            seen_epoch,
            thumbs,
            copied: None,
            copied_timer: None,
        };
        view.reload(cx);
        view
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let token = self.generation;
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            self.finish(token, Err(None), cx);
            return;
        };
        let rt = backend.rt.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { Backend::list_library(&daemon, "", None, None).await })
                .await
                .map_err(|e| Some(ErrorNote::plain(e.to_string())))
                .and_then(|r| r.map_err(|e| Some(ErrorNote::plain(e))));
            this.update(cx, |this, cx| this.finish(token, res, cx)).ok();
        })
        .detach();
    }

    fn finish(
        &mut self,
        token: u64,
        res: Result<crate::library::PageCollector, Option<ErrorNote>>,
        cx: &mut Context<Self>,
    ) {
        if token != self.generation {
            return;
        }
        let count = match res {
            Ok(c) => {
                self.rows = published::rows(&c.items);
                self.load = Load::Ready;
                Some(published::count(&c.items))
            }
            Err(e) => {
                self.load = Load::Failed(e);
                None
            }
        };
        self.app.update(cx, |m, cx| {
            if m.counts.published != count {
                m.counts.published = count;
                cx.notify();
            }
        });
        cx.notify();
    }

    fn row_view(
        &self,
        t: &Theme,
        ix: usize,
        row: &PublishedRow,
        clock: &LocalClock,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let r = &row.record;
        let when = r.at.map(|a| clock.format_date(a)).unwrap_or_default();
        let (app_open, id, kind) = (self.app.clone(), row.capture_id.clone(), row.kind);
        let focus = rgba_of(t.text);
        let open_details = div()
            .id(("published-open", ix))
            .role(Role::Link)
            .aria_label(SharedString::from(published::accessible_label(row, &when)))
            .focusable()
            .tab_stop(true)
            .min_w(px(0.))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(2.))
            .cursor_pointer()
            .rounded(px(scale::RADIUS_XS))
            .border_1()
            .border_color(gpui::transparent_black())
            .focus_visible(move |s| s.border_color(focus))
            .child(
                div()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(row.capture_title.clone())),
            )
            .child(
                div()
                    .truncate()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(if when.is_empty() {
                        published::platform_account(r)
                    } else {
                        tr!(
                            "{platform} \u{00B7} {when}",
                            platform = published::platform_account(r),
                            when = when
                        )
                    })),
            )
            .on_click(move |_, _, cx| {
                let (app, id) = (app_open.clone(), id.clone());
                cx.defer(move |cx| {
                    let route = match kind {
                        crate::library::Kind::Recording => Route::Recording(id),
                        crate::library::Kind::Screenshot => Route::Editor(id),
                    };
                    app::set_route(&app, route, cx)
                });
            });
        let (app_copy, link_copy) = (self.app.clone(), r.url.clone());
        let (app_web, link_web) = (self.app.clone(), r.url.clone());
        let copied = self.copied == Some(ix);
        let (status_glyph, status_tone) = published::status_icon(r.state);
        let status_color = kit::tone_colors(t, status_tone).icon;
        let locked = r.state == published::PublishState::PrivacyLocked;
        let platform = published::platform_label(&r.platform);
        let thumb = self.thumb_frame(t, row, &platform, cx);
        let body = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.))
                    .child(open_details)
                    .child(kit::pill(
                        t,
                        published::privacy_tone(r),
                        Some(published::privacy_icon(&r.privacy)),
                        published::privacy_line(r),
                    ))
                    .child(
                        icon_button(
                            t,
                            ("published-open-link", ix),
                            "external-link",
                            published::open_label(r),
                            false,
                            Some(Vec::new()),
                        )
                        .on_click(move |_, _, cx| {
                            let (app, l) = (app_web.clone(), link_web.clone());
                            cx.defer(move |cx| upload_ui::open_link(&app, &l, cx));
                        }),
                    )
                    .child(
                        button_with_icon(
                            t,
                            ("published-copy-link", ix),
                            &if copied {
                                tr_ctx!("button, after the link was copied", "Copied")
                            } else {
                                tr!("Copy link")
                            },
                            ButtonKind::Secondary,
                            Some(if copied { "check" } else { "link" }),
                        )
                        .aria_label(SharedString::from(tr!("Copy link")))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let (app, l) = (app_copy.clone(), link_copy.clone());
                            cx.defer(move |cx| upload_ui::copy_link(&app, &l, cx));
                            this.mark_copied(ix, cx);
                        })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icon("link", 14., t.mist))
                    .child(
                        div()
                            .min_w(px(0.))
                            .text_size(px(13.))
                            .text_color(rgba_of(t.text_2))
                            .mono()
                            .truncate()
                            .child(SharedString::from(r.url.clone())),
                    ),
            )
            .child(if locked {
                // The platform changed what was asked for: a boxed warning, only for this status.
                kit::callout(
                    t,
                    kit::Tone::Brass,
                    "triangle-alert",
                    published::status_text(r),
                    Some(SharedString::from(published::privacy_line(r))),
                    None,
                )
                .into_any_element()
            } else {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icon(status_glyph, 14., status_color))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgba_of(t.text_2))
                            .child(SharedString::from(published::status_text(r))),
                    )
                    .into_any_element()
            })
            .children(r.warnings.iter().map(|w| {
                kit::callout(
                    t,
                    kit::Tone::Brass,
                    "triangle-alert",
                    SharedString::from(w.clone()),
                    None,
                    None,
                )
            }));
        card(t)
            .id(("published-row", ix))
            .role(Role::ListItem)
            .flex()
            .gap(px(18.))
            .child(thumb)
            .child(body)
            .into_any_element()
    }

    /// The poster of the recording with a platform chip bottom-left; a placeholder tile with the
    /// platform glyph until (or unless) the poster exists.
    fn thumb_frame(
        &self,
        t: &Theme,
        row: &PublishedRow,
        platform: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let id = row.capture_id.clone();
        let state = self.thumbs.update(cx, |c, cx| c.get(&id, cx));
        let mut frame = div()
            .relative()
            .w(px(THUMB_W))
            .h(px(THUMB_W * 9. / 16.))
            .flex_none()
            .overflow_hidden()
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.raised));
        match state {
            Thumb::Ready(image) => {
                frame = frame.child(img(image).size_full().object_fit(ObjectFit::Cover));
            }
            Thumb::Loading | Thumb::Missing => {
                frame = frame.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(platform_glyph(platform, 24., t.faint)),
                );
            }
        }
        frame
            .child(
                div()
                    .absolute()
                    .left(px(8.))
                    .bottom(px(8.))
                    .h(px(24.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(scale::RADIUS_SM))
                    .bg(rgba_of(crate::theme::Theme::DARK.ink.with_alpha(80)))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(crate::theme::Theme::DARK.text))
                    .child(platform_glyph(
                        platform,
                        12.,
                        crate::theme::Theme::DARK.text,
                    ))
                    .child(SharedString::from(platform.to_owned())),
            )
            .into_any_element()
    }

    /// Shows "Copied" on one row's button for a moment.
    fn mark_copied(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.copied = Some(ix);
        cx.notify();
        self.copied_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1500))
                .await;
            this.update(cx, |this, cx| {
                this.copied = None;
                cx.notify();
            })
            .ok();
        }));
    }
}

/// Width of a row's thumbnail, px (the board: about 220).
const THUMB_W: f32 = 220.;

impl Render for PublishedView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let clock = self.app.read(cx).clock;
        let body: gpui::AnyElement = match &self.load {
            Load::Loading => div()
                .p(px(24.))
                .text_color(rgba_of(t.text_3))
                .child(SharedString::from(tr!("Loading\u{2026}")))
                .into_any_element(),
            Load::Failed(e) => div()
                .p(px(24.))
                .text_color(rgba_of(t.text_2))
                .child(SharedString::from(match e {
                    Some(note) => note.text(),
                    None => tr!("The library is not available."),
                }))
                .into_any_element(),
            Load::Ready if self.rows.is_empty() => {
                let (title, text) = published::empty_text();
                div()
                    .id("published-empty")
                    .role(Role::Group)
                    .aria_label(SharedString::from(tr!(
                        "{title}. {text}",
                        title = title,
                        text = text
                    )))
                    .size_full()
                    .min_h(px(320.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(56.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(scale::RADIUS_LG))
                            .border_1()
                            .border_color(rgba_of(t.border))
                            .bg(rgba_of(t.surface))
                            .child(icon("send", 24., t.text_3)),
                    )
                    .child(
                        div()
                            .text_size(px(type_scale::H2))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(title)),
                    )
                    .child(
                        div()
                            .max_w(px(420.))
                            .text_align(gpui::TextAlign::Center)
                            .text_color(rgba_of(t.text_3))
                            .child(SharedString::from(text)),
                    )
                    .into_any_element()
            }
            Load::Ready => div()
                .id("published-list")
                .role(Role::List)
                .aria_label(SharedString::from(tr!("Published videos")))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.mist))
                        .mono()
                        .child(SharedString::from(tr!("newest first"))),
                )
                .children({
                    let rows = self.rows.clone();
                    rows.iter()
                        .enumerate()
                        .map(|(i, r)| self.row_view(&t, i, r, &clock, cx))
                        .collect::<Vec<_>>()
                })
                .into_any_element(),
        };
        div()
            .id("published-page")
            .size_full()
            .overflow_y_scroll()
            .p(px(24.))
            .child(body)
    }
}

/// The glyph of a platform on the Published list: its mark, or the generic send glyph for a platform
/// we have no mark for.
fn platform_glyph(platform: &str, size: f32, color: crate::theme::Color) -> gpui::Svg {
    match crate::ui::brand::platform(&platform.to_lowercase()) {
        Some(b) => icon(b.icon, size, color),
        None => icon("send", size, color),
    }
}
