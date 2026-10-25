//! Library view (LIB-1, LIB-2), redesign board R1: the title row (title, real count, search), the
//! Uploaded and Published chips and the sort bar, status banners, and a virtualized grid of
//! 16:10 thumbnails grouped under day headers when sorting by date (OD-20: no hero row).

use gpui::{
    anchored, deferred, div, img, list, point, prelude::*, px, Anchor, AnyElement, ClickEvent,
    Context, Entity, FocusHandle, FontWeight, ListAlignment, ListState, MouseButton, ObjectFit,
    Pixels, Point, Render, Role, SharedString, Subscription, Task, WeakEntity, Window,
};
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;

use cm_i18n::{tr, tr_ctx, trn};

use crate::ai::models::NoticeSite;
use crate::backend::Backend;
use crate::daemon_text::{AppNote, ErrorNote};
use crate::library::{
    build_rows, columns_for_width, format_duration, format_size, row_of_item, CaptureItem,
    CardAction, Chip, EmptyKind, Filter, Kind, LibraryState, LibraryStatus, LocalClock, Row,
    SortKey, GRID_GAP,
};
use crate::nav::{self, NavCounts};
use crate::overlay::CaptureMode;
use crate::shortcuts::ShortcutAction;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, AppModel, Command, Route};
use crate::ui::components::{
    button, button_with_icon, icon, icon_size, keycaps, status_pill, ButtonKind,
};
use crate::ui::kit::{callout, Tone};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::small_button;
use crate::ui::studio::{
    OpenSelected, SelectDown, SelectLeft, SelectRight, SelectUp, ThemeGlobal, SIDEBAR_WIDTH,
};
use crate::ui::text_field::{TextField, TextFieldEvent};
use crate::ui::thumbs::{Thumb, ThumbCache};

/// Side padding of the page (board: 26 top, 32 sides).
const PAGE_PADDING: f32 = 32.;
/// Page title size (board: 30 / 600).
const HEADLINE: f32 = 30.;
/// Thumbnail aspect (board: 16:10).
const THUMB_ASPECT: f32 = 10. / 16.;
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(150);

/// The context menu of one card (LIB-1), at a window position.
struct CardMenu {
    ix: usize,
    at: Point<Pixels>,
    /// Which corner of the menu sits at `at`: the top left for a right click, the top right for
    /// the "..." button (the menu opens below it, flush with its right edge).
    anchor: Anchor,
    /// The Rename box is showing instead of the actions.
    renaming: bool,
}

pub struct LibraryView {
    app: Entity<AppModel>,
    menu: Option<CardMenu>,
    rename_input: Entity<TextField>,
    state: LibraryState,
    items: Arc<Vec<CaptureItem>>,
    /// The rows of the list: day headers (date sort) and grid runs, rebuilt when the items, the
    /// sort or the column count change.
    rows: Arc<Vec<Row>>,
    search: Entity<TextField>,
    thumbs: Entity<ThumbCache>,
    list: ListState,
    list_focus: FocusHandle,
    cols: usize,
    rows_dirty: bool,
    seen_epoch: u64,
    /// The `cm_i18n` epoch the cached day headers and field labels were made in.
    seen_lang: u64,
    debounce: Option<Task<()>>,
    /// Move keyboard focus from the search box to the list on the next render.
    focus_list_next: bool,
    /// The library changed while another page was showing: reload when this one shows (R1-7).
    stale: bool,
    /// The next list rebuild comes from a library change (a watcher event, a rescan), not from
    /// the user's search or filter: keep the scroll position (the selection is kept by id).
    keep_scroll: bool,
    /// Captures whose thumbnails to drop when the next load lands (R1-7).
    pending_invalidate: Vec<String>,
    _subs: Vec<Subscription>,
}

impl LibraryView {
    pub fn new(app: Entity<AppModel>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let backend = app.read(cx).backend.clone();
        let status = backend.initial_status.clone();
        let search =
            cx.new(|cx| TextField::new(tr!("Search captures"), tr!("Search by title"), cx));
        let thumbs = cx.new(|_| ThumbCache::new(backend.clone()));
        let rename_input = cx.new(|cx| {
            TextField::new(tr!("New title"), tr_ctx!("field", "Title"), cx).without_icon()
        });
        let seen_epoch = app.read(cx).library_epoch;
        let subs = vec![
            cx.subscribe(&search, |this, _, ev: &TextFieldEvent, cx| match ev {
                TextFieldEvent::Changed(text) => {
                    this.keep_scroll = false;
                    this.state.query = text.clone();
                    this.reload_debounced(cx);
                }
                TextFieldEvent::Submitted => {
                    this.reload(cx);
                    this.focus_list_next = true;
                    cx.notify();
                }
                TextFieldEvent::Escaped => {
                    this.search.update(cx, |s, cx| s.clear(cx));
                    this.focus_list_next = true;
                    cx.notify();
                }
            }),
            cx.observe(&app, |this, app, cx| {
                let epoch = app.read(cx).library_epoch;
                if epoch != this.seen_epoch {
                    this.seen_epoch = epoch;
                    this.keep_scroll = true;
                    let changed = app.update(cx, |m, _| std::mem::take(&mut m.changed_captures));
                    this.pending_invalidate.extend(changed);
                    // Not on screen: remember, and reload when it is (R1-7).
                    if matches!(
                        app.read(cx).route,
                        Route::Library | Route::Screenshots | Route::Recordings
                    ) {
                        this.reload(cx);
                    } else {
                        this.stale = true;
                    }
                }
                cx.notify();
            }),
            cx.observe(&thumbs, |_, _, cx| cx.notify()),
            cx.subscribe(&rename_input, |this, _, ev: &TextFieldEvent, cx| match ev {
                TextFieldEvent::Submitted => this.commit_rename(cx),
                TextFieldEvent::Escaped => this.close_menu(cx),
                TextFieldEvent::Changed(_) => {}
            }),
        ];
        let mut view = LibraryView {
            menu: None,
            rename_input,
            app,
            state: LibraryState::new(match status {
                LibraryStatus::Loading => LibraryStatus::Loading,
                other => other,
            }),
            items: Arc::new(Vec::new()),
            rows: Arc::new(Vec::new()),
            search,
            thumbs,
            list: ListState::new(0, ListAlignment::Top, px(600.)),
            list_focus: cx.focus_handle(),
            cols: 1,
            rows_dirty: true,
            seen_epoch,
            seen_lang: cm_i18n::epoch(),
            debounce: None,
            focus_list_next: false,
            stale: false,
            keep_scroll: false,
            pending_invalidate: Vec::new(),
            _subs: subs,
        };
        view.reload(cx);
        view
    }

    pub fn search_focus(&self, cx: &gpui::App) -> FocusHandle {
        self.search.read(cx).focus.clone()
    }

    fn reload_debounced(&mut self, cx: &mut Context<Self>) {
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |this, cx| this.reload(cx)).ok();
        }));
    }

    /// Asks the daemon for the library (as the user) and applies the answer if it is still
    /// the latest request.
    fn reload(&mut self, cx: &mut Context<Self>) {
        let token = self.state.begin_load();
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            self.state
                .finish_load(token, Err(ErrorNote::app(AppNote::LibraryUnavailable)));
            self.sync_items();
            cx.notify();
            return;
        };
        let query = self.state.query.clone();
        let kind = self.state.filter.daemon_kind();
        let status = self.state.remote.map(|r| r.daemon_status());
        let rt = backend.rt.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move { Backend::list_library(&daemon, &query, kind, status).await })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r)
                .map_err(ErrorNote::plain);
            this.update(cx, |this, cx| {
                if this.state.finish_load(token, res) {
                    this.state.update_remote_counts();
                    this.publish_counts(cx);
                    this.sync_items();
                    // Only the captures that changed lose their poster; the rest keep theirs.
                    let changed = std::mem::take(&mut this.pending_invalidate);
                    this.thumbs.update(cx, |t, cx| {
                        for id in &changed {
                            t.invalidate(id, cx);
                        }
                        t.invalidate_missing(cx);
                    });
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Publishes the real per-kind totals to the sidebar after an unfiltered load.
    fn publish_counts(&self, cx: &mut Context<Self>) {
        if !self.state.query.is_empty()
            || self.state.filter != Filter::All
            || self.state.remote.is_some()
        {
            return;
        }
        let counts = if matches!(self.state.status, LibraryStatus::Ready) {
            let n = |k: Kind| self.state.items().iter().filter(|i| i.kind == k).count();
            NavCounts {
                screenshots: Some(n(Kind::Screenshot)),
                recordings: Some(n(Kind::Recording)),
                // Not this view's to know: the Published page counts its own.
                published: self.app.read(cx).counts.published,
            }
        } else {
            NavCounts {
                published: self.app.read(cx).counts.published,
                ..NavCounts::default()
            }
        };
        self.app.update(cx, |m, cx| {
            if m.counts != counts {
                m.counts = counts;
                cx.notify();
            }
        });
    }

    fn sync_items(&mut self) {
        self.items = Arc::new(self.state.items().to_vec());
        self.rows_dirty = true;
    }

    /// The sidebar section changed (Screenshots or Recordings): show that kind.
    pub fn show_kind(&mut self, f: Filter, cx: &mut Context<Self>) {
        if f != self.state.filter {
            self.state.filter = f;
            self.reload(cx);
        }
    }

    fn click_chip(&mut self, chip: Chip, cx: &mut Context<Self>) {
        if self.state.click_chip(chip) {
            self.keep_scroll = false;
            self.reload(cx);
        }
    }

    fn set_sort(&mut self, key: SortKey, cx: &mut Context<Self>) {
        self.state.set_sort(key);
        self.sync_items();
        cx.notify();
    }

    /// Scrolls the selected card's row into view. Moving back onto the first row of a day also
    /// shows that day's header above it.
    fn reveal_selected(&self, backwards: bool) {
        let Some(sel) = self.state.selected else {
            return;
        };
        let Some(r) = row_of_item(&self.rows, sel) else {
            return;
        };
        let header_above = r > 0 && matches!(self.rows[r - 1], Row::Header { .. });
        self.list
            .scroll_to_reveal_item(if backwards && header_above { r - 1 } else { r });
    }

    /// Left and right: one card, across rows and day groups.
    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.state.move_selection(delta);
        self.reveal_selected(delta < 0);
        cx.notify();
    }

    /// Up and down: one grid row, skipping the day headers.
    fn move_vertical(&mut self, dir: isize, cx: &mut Context<Self>) {
        let rows = self.rows.clone();
        self.state.move_vertical(&rows, dir);
        self.reveal_selected(dir < 0);
        cx.notify();
    }

    fn open_menu(&mut self, ix: usize, at: Point<Pixels>, anchor: Anchor, cx: &mut Context<Self>) {
        self.state.selected = Some(ix);
        self.menu = Some(CardMenu {
            ix,
            at,
            anchor,
            renaming: false,
        });
        cx.notify();
    }

    /// The "..." button: the same menu, below the button and flush with its right edge (at the
    /// button's bottom left for a keyboard activation, which has no pointer position).
    fn open_menu_from_button(&mut self, ix: usize, ev: &ClickEvent, cx: &mut Context<Self>) {
        match ev.mouse_position() {
            Some(p) => self.open_menu(
                ix,
                point(p.x + px(14.), p.y + px(20.)),
                Anchor::TopRight,
                cx,
            ),
            None => self.open_menu(ix, ev.position(), Anchor::TopLeft, cx),
        }
    }

    fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            cx.notify();
        }
    }

    /// Runs one entry of the card menu (LIB-1).
    fn run_card_action(&mut self, action: CardAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.menu.as_ref().map(|m| m.ix) else {
            return;
        };
        let Some(item) = self.items.get(ix).cloned() else {
            self.close_menu(cx);
            return;
        };
        match action {
            CardAction::Open => {
                self.close_menu(cx);
                self.open_index(ix, cx);
            }
            CardAction::Reveal => {
                self.close_menu(cx);
                let backend = self.app.read(cx).backend.clone();
                let Some(daemon) = backend.daemon.clone() else {
                    return;
                };
                let app = self.app.clone();
                cx.spawn(async move |_, cx| {
                    let id = item.id.clone();
                    let dir = backend
                        .rt
                        .spawn(async move { Backend::capture_dir(&daemon, &id).await })
                        .await;
                    cx.update(|cx| match dir {
                        Ok(Ok(dir)) => cx.reveal_path(&dir),
                        Ok(Err(e)) => {
                            app::note_open_failed(&e, cx);
                            app::push_toast(
                                &app,
                                crate::flows::error_toast_live(
                                    || tr!("Couldn\u{2019}t show the capture"),
                                    e,
                                ),
                                cx,
                            );
                        }
                        Err(_) => {}
                    });
                })
                .detach();
            }
            CardAction::CopyLink => {
                // STO-4.AC3: the link of the newest upload that has one.
                self.close_menu(cx);
                if let Some(link) = crate::published::share_link_of(&item.uploads) {
                    let (app, link) = (self.app.clone(), link.to_owned());
                    cx.defer(move |cx| crate::ui::upload_ui::copy_link(&app, &link, cx));
                }
            }
            CardAction::Rename => {
                if let Some(m) = &mut self.menu {
                    m.renaming = true;
                }
                let title = item.title.clone();
                self.rename_input.update(cx, |f, cx| f.set_text(&title, cx));
                let focus = self.rename_input.read(cx).focus.clone();
                window.focus(&focus, cx);
                cx.notify();
            }
            CardAction::Delete => {
                self.close_menu(cx);
                let (app, id, title) = (self.app.clone(), item.id.clone(), item.title.clone());
                cx.defer(move |cx| app::delete_capture(&app, &id, &title, cx));
            }
        }
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.menu.as_ref().map(|m| m.ix) else {
            return;
        };
        let title = self.rename_input.read(cx).text().to_owned();
        let id = self.items.get(ix).map(|i| i.id.clone());
        self.close_menu(cx);
        if let Some(id) = id {
            let app = self.app.clone();
            cx.defer(move |cx| app::rename_capture(&app, &id, &title, cx));
        }
    }

    fn open_index(&mut self, ix: usize, cx: &mut Context<Self>) {
        // A click that lands while the card menu is up belongs to the menu, never to the card below.
        if self.menu.is_some() {
            return;
        }
        if let Some(item) = self.items.get(ix) {
            self.state.selected = Some(ix);
            app::set_route(&self.app, app::open_route(&item.id), cx);
        }
    }

    /// The one-line "no speech model" hint of the Recordings page: text, Open AI, dismiss.
    fn model_hint(&self, t: &Theme) -> AnyElement {
        let (open, dismiss) = (self.app.clone(), self.app.clone());
        let actions = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .child(
                small_button(t, "library-ai-open", &tr!("Open AI"))
                    .on_click(move |_, _, cx| app::set_route(&open, Route::Ai, cx)),
            )
            .child(
                small_button(t, "library-ai-dismiss", &tr_ctx!("button", "Dismiss"))
                    .on_click(move |_, _, cx| crate::ui::ai::dismiss_notice(&dismiss, cx)),
            );
        callout(
            t,
            Tone::Neutral,
            "bot",
            tr!("Add a speech model on the AI page to transcribe your recordings."),
            None,
            Some(actions.into_any_element()),
        )
        .id("library-ai-hint")
        .into_any_element()
    }
}

struct RowCtx {
    t: Theme,
    items: Arc<Vec<CaptureItem>>,
    rows: Arc<Vec<Row>>,
    selected: Option<usize>,
    list_focused: bool,
    /// The card whose menu is open: its "..." button stays visible.
    menu_ix: Option<usize>,
    thumbs: Entity<ThumbCache>,
    view: WeakEntity<LibraryView>,
    clock: LocalClock,
    avail: f32,
    cols: usize,
}

/// The darkened overlay behind the pills on a thumbnail (the thumbnail is an image, so the
/// overlay does not follow the theme).
fn overlay_bg(alpha: u32) -> gpui::Rgba {
    rgba_of(Theme::DARK.ink.with_alpha(alpha))
}

/// The kind pill of a card: icon and "REC" or "SHOT" (a screenshot has no duration badge).
fn kind_pill(item: &CaptureItem) -> AnyElement {
    let (glyph, label) = match item.kind {
        Kind::Recording => ("film", tr_ctx!("card badge", "REC")),
        Kind::Screenshot => ("image", tr_ctx!("card badge", "SHOT")),
    };
    div()
        .absolute()
        .top(px(10.))
        .left(px(10.))
        .h(px(24.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(7.))
        .bg(overlay_bg(72))
        .text_size(px(11.))
        .text_color(rgba_of(Theme::DARK.text))
        .mono()
        .child(icon(glyph, icon_size::PILL, Theme::DARK.text))
        .child(label)
        .into_any_element()
}

fn thumb_frame(ctx: &RowCtx, item: &CaptureItem, width: f32, cx: &mut gpui::App) -> gpui::Div {
    let t = &ctx.t;
    let id = item.id.clone();
    let state = ctx.thumbs.update(cx, |c, cx| c.get(&id, cx));
    let mut frame = div()
        .relative()
        .w(px(width))
        .h(px((width * THUMB_ASPECT).round()))
        .flex_none()
        .overflow_hidden()
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .bg(rgba_of(t.raised));
    match state {
        Thumb::Ready(image) => {
            frame = frame.child(img(image).size_full().object_fit(ObjectFit::Cover));
        }
        Thumb::Loading => {}
        Thumb::Missing => {
            frame = frame.child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_3))
                    .child(if item.broken {
                        tr!("Unreadable")
                    } else {
                        tr!("No preview")
                    }),
            );
        }
    }
    frame = frame.child(kind_pill(item));
    if let (Kind::Recording, Some(ticks)) = (item.kind, item.duration_ticks) {
        frame = frame.child(
            div()
                .absolute()
                .bottom(px(10.))
                .right(px(10.))
                .h(px(22.))
                .px(px(7.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .bg(overlay_bg(78))
                .text_size(px(11.5))
                .text_color(rgba_of(Theme::DARK.text))
                .mono()
                .child(SharedString::from(format_duration(ticks))),
        );
    }
    if item.broken {
        frame =
            frame.child(div().absolute().bottom(px(10.)).left(px(10.)).child(
                status_pill(t, t.warning, tr_ctx!("status", "Broken")).bg(rgba_of(t.surface)),
            ));
    }
    frame
}

/// Three dots, the "more" glyph (the icon kit has none).
fn more_glyph(color: crate::theme::Color) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .children((0..3).map(|_| div().size(px(3.5)).rounded(px(2.)).bg(rgba_of(color))))
}

/// One card: the bordered thumbnail with the kind pill, the duration and the hover-only "..."
/// button, the title and the meta line under it, no surrounding card.
fn card(ctx: &RowCtx, ix: usize, width: f32, cx: &mut gpui::App) -> AnyElement {
    let t = ctx.t;
    let Some(item) = ctx.items.get(ix) else {
        return div().into_any_element();
    };
    let selected = ctx.selected == Some(ix);
    let label = SharedString::from(item.accessible_label(&ctx.clock));
    let view = ctx.view.clone();
    let meta = {
        let mut m = ctx.clock.format_date(item.created);
        if item.size_bytes > 0 {
            m.push_str(" \u{00B7} ");
            m.push_str(&format_size(item.size_bytes));
        }
        m
    };
    let border = if selected {
        if ctx.list_focused {
            t.text
        } else {
            t.line
        }
    } else {
        t.border
    };
    let group = SharedString::from(format!("card-{ix}"));
    let hover_border = rgba_of(t.line);
    let frame = thumb_frame(ctx, item, width, cx)
        .border_color(rgba_of(border))
        .group_hover(group.clone(), move |s| s.border_color(hover_border));
    // The "..." button shows on hover, on keyboard focus, on the selected card while the list has
    // focus, and while its menu is open. It sits beside the clickable card (not inside it) and
    // occludes what is under it, so a click on it never also opens the capture.
    let shown = (selected && ctx.list_focused) || ctx.menu_ix == Some(ix);
    let more_fg = Theme::DARK.text;
    let more = div()
        .id(("card-more", ix))
        .role(Role::Button)
        .aria_label(SharedString::from(tr!(
            "Actions for {title}",
            title = item.title
        )))
        .focusable()
        .tab_stop(selected)
        .occlude()
        .absolute()
        .top(px(10.))
        .right(px(10.))
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(rgba_of(t.line))
        .bg(overlay_bg(80))
        .cursor_pointer()
        .opacity(if shown { 1. } else { 0. })
        .group_hover(group.clone(), |s| s.opacity(1.))
        .hover(|s| s.opacity(1.))
        .focus_visible(move |s| s.opacity(1.).border_color(rgba_of(t.text)))
        .child(more_glyph(more_fg))
        .on_click({
            let view = ctx.view.clone();
            move |ev, _, cx| {
                view.update(cx, |this, cx| this.open_menu_from_button(ix, ev, cx))
                    .ok();
            }
        });
    let clickable = div()
        .id(("card", ix))
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(selected)
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.))
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            view.update(cx, |this, cx| this.open_index(ix, cx)).ok();
        })
        // LIB-1: right click opens the card's menu.
        .on_mouse_down(MouseButton::Right, {
            let view = ctx.view.clone();
            move |ev, _, cx| {
                view.update(cx, |this, cx| {
                    this.open_menu(ix, ev.position, Anchor::TopLeft, cx)
                })
                .ok();
            }
        })
        .child(frame)
        .child(
            div()
                .px(px(2.))
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .w_full()
                        .truncate()
                        .text_size(px(type_scale::UI))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba_of(t.text))
                        .child(SharedString::from(item.title.clone())),
                )
                .child(
                    div()
                        .w_full()
                        .truncate()
                        .text_size(px(12.5))
                        .text_color(rgba_of(t.mist))
                        .tnum()
                        .child(SharedString::from(meta)),
                ),
        );
    div()
        .group(group)
        .relative()
        .w(px(width))
        .flex_none()
        .child(clickable)
        .child(more)
        .into_any_element()
}

/// A day header row: the day, its count in mono, and a rule to the right.
fn day_header(t: &Theme, ix: usize, label: &str, count: usize) -> AnyElement {
    div()
        .id(("day", ix))
        .role(Role::Heading)
        .aria_level(2)
        .aria_label(SharedString::from(format!("{label}, {count}")))
        .w_full()
        // Space above every group but the first.
        .when(ix > 0, |d| d.pt(px(8.)))
        .pb(px(12.))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba_of(t.text_2))
                .child(SharedString::from(label.to_owned())),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(rgba_of(t.faint))
                .mono()
                .child(SharedString::from(count.to_string())),
        )
        .child(div().flex_1().h(px(1.)).bg(rgba_of(t.border)))
        .into_any_element()
}

fn render_row(ctx: &RowCtx, ix: usize, cx: &mut gpui::App) -> AnyElement {
    let Some(row) = ctx.rows.get(ix) else {
        return div().into_any_element();
    };
    match row {
        Row::Header { label, count } => day_header(&ctx.t, ix, label, *count),
        Row::Grid(range) => {
            let n = ctx.cols as f32;
            let w = ((ctx.avail - GRID_GAP * (n - 1.)) / n).floor();
            let mut r = div().flex().gap(px(GRID_GAP));
            for i in range.clone() {
                r = r.child(card(ctx, i, w, cx));
            }
            div().w_full().pb(px(GRID_GAP)).child(r).into_any_element()
        }
    }
}

/// An Uploaded or Published chip: a 32 px pill with a leading icon, the label and the live count
/// in mono. A toggle (the page reloads with that status), so it keeps the tab semantics it had.
fn filter_chip(
    t: &Theme,
    chip: Chip,
    count: Option<usize>,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    let fg = if active { t.text } else { t.text_3 };
    let aria = match count {
        Some(n) => tr!("{name}, {n}", name = chip.label(), n = n),
        None => chip.label(),
    };
    div()
        .id(SharedString::from(format!("filter-{}", chip.key())))
        .role(Role::Tab)
        .aria_label(SharedString::from(aria))
        .aria_selected(active)
        .focusable()
        .tab_stop(true)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.))
        .rounded(px(scale::RADIUS_PILL))
        .border_1()
        .border_color(rgba_of(if active { t.text_3 } else { t.line }))
        .text_size(px(13.))
        .text_color(rgba_of(fg))
        .cursor_pointer()
        .when(active, |d| d.bg(rgba_of(t.raised)))
        .when(!active, |d| d.hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
        .child(icon(chip.glyph(), icon_size::SMALL, fg))
        .child(chip.label())
        .children(count.map(|n| {
            div()
                .text_size(px(11.))
                .text_color(rgba_of(t.mist))
                .mono()
                .child(SharedString::from(n.to_string()))
        }))
}

/// One segment of the sort bar: an icon, the label, and on the active one the direction icon.
fn sort_segment(
    t: &Theme,
    key: SortKey,
    active: bool,
    descending: bool,
) -> gpui::Stateful<gpui::Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    let fg = if active { t.text } else { t.text_3 };
    let aria = if active {
        if descending {
            tr!("{key}, descending", key = key.label())
        } else {
            tr!("{key}, ascending", key = key.label())
        }
    } else {
        key.label()
    };
    div()
        .id(SharedString::from(format!("sort-{}", key.key())))
        .role(Role::Tab)
        .aria_label(SharedString::from(aria))
        .aria_selected(active)
        .focusable()
        .tab_stop(true)
        .h(px(30.))
        .px(px(11.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(gpui::transparent_black())
        .text_size(px(13.))
        .text_color(rgba_of(fg))
        .cursor_pointer()
        .when(active, |d| {
            d.bg(rgba_of(t.segment_selected()))
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!active, |d| d.hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
        .child(icon(key.glyph(), icon_size::SMALL, fg))
        .child(key.label())
        .when(active, |d| {
            d.child(icon(
                if descending { "arrow-down" } else { "arrow-up" },
                13.,
                fg,
            ))
        })
}

impl Render for LibraryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        // A language switch: the day headers are text made when the rows were built, and the
        // fields keep their labels, so rebuild the first and relabel the second.
        if self.seen_lang != cm_i18n::epoch() {
            self.seen_lang = cm_i18n::epoch();
            self.rows_dirty = true;
            self.search.update(cx, |s, cx| {
                s.set_labels(tr!("Search captures"), tr!("Search by title"), cx)
            });
            self.rename_input.update(cx, |s, cx| {
                s.set_labels(tr!("New title"), tr_ctx!("field", "Title"), cx)
            });
        }
        if std::mem::take(&mut self.stale) {
            self.reload(cx);
        }
        let (platform, shortcuts, clock, ipc_warning, perms, route, counts, hint) = {
            let m = self.app.read(cx);
            (
                m.platform,
                m.shortcuts.clone(),
                m.clock,
                m.backend.ipc_warning.clone(),
                m.perms.clone(),
                m.route.clone(),
                m.counts,
                m.ai.notice_visible(NoticeSite::Library),
            )
        };
        let avail = (f32::from(window.viewport_size().width) - SIDEBAR_WIDTH - PAGE_PADDING * 2.)
            .max(CARD_FLOOR);
        let cols = columns_for_width(avail);
        if self.rows_dirty || cols != self.cols {
            let days = self
                .state
                .group_by_day()
                .then(|| (&clock, OffsetDateTime::now_utc()));
            self.rows = Arc::new(build_rows(&self.items, cols, days));
            // A refresh after a library change keeps the scroll position (reset drops it).
            let top = std::mem::take(&mut self.keep_scroll)
                .then(|| self.list.logical_scroll_top())
                .filter(|_| cols == self.cols);
            self.list.reset(self.rows.len());
            if let Some(top) = top {
                self.list.scroll_to(top);
            }
            self.rows_dirty = false;
            self.cols = cols;
        }
        let rows = self.rows.clone();
        if std::mem::take(&mut self.focus_list_next) && !self.items.is_empty() {
            window.focus(&self.list_focus, cx);
        }
        let list_focused = self.list_focus.contains_focused(window, cx);

        // ---- title row: the page title, its real count, the search field ---------------------
        let view = cx.entity().downgrade();
        let (_, title) = nav::page_heading(&route);
        let title_row = div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.))
            .child(
                div()
                    .id("page-title")
                    .flex()
                    .items_baseline()
                    .gap(px(12.))
                    .mr_auto()
                    .child(
                        div()
                            .id("page-heading")
                            .role(Role::Heading)
                            .aria_level(1)
                            .aria_label(title.clone())
                            .text_size(px(HEADLINE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .children(nav::route_count(&route, &counts).map(|n| {
                        div()
                            .id("page-count")
                            .aria_label(SharedString::from(trn!("{n} item", "{n} items", n)))
                            .text_size(px(13.))
                            .text_color(rgba_of(t.mist))
                            .mono()
                            .child(SharedString::from(n.to_string()))
                    })),
            )
            .child(
                div()
                    .w(px(320.))
                    .max_w_full()
                    .flex_none()
                    .child(self.search.clone()),
            );

        // ---- toolbar: the chips and the sort bar -------------------------------------------
        let chips = Chip::ALL.iter().map(|c| {
            let v = view.clone();
            let c = *c;
            filter_chip(&t, c, self.state.chip_count(c), self.state.chip_active(c)).on_click(
                move |_, _, cx| {
                    v.update(cx, |this, cx| this.click_chip(c, cx)).ok();
                },
            )
        });
        let sorts = SortKey::ALL.iter().map(|k| {
            let v = view.clone();
            let k = *k;
            sort_segment(&t, k, self.state.sort == k, self.state.descending).on_click(
                move |_, _, cx| {
                    v.update(cx, |this, cx| this.set_sort(k, cx)).ok();
                },
            )
        });
        let toolbar = div()
            .flex_none()
            .pb(px(6.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.))
            .border_b_1()
            .border_color(rgba_of(t.border))
            .child(
                div()
                    .mr(px(2.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.faint))
                    .child(tr_ctx!("filter label", "Show")),
            )
            .child(
                div()
                    .id("filters")
                    .role(Role::TabList)
                    .aria_label(tr!("Filter captures"))
                    .flex()
                    .gap(px(10.))
                    .children(chips),
            )
            .child(
                div()
                    .id("sorts")
                    .role(Role::TabList)
                    .aria_label(tr!("Sort captures"))
                    .ml_auto()
                    .p(px(3.))
                    .flex()
                    .gap(px(2.))
                    .rounded(px(11.))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .bg(rgba_of(t.surface_2))
                    .children(sorts),
            );

        // ---- banners (callouts) -------------------------------------------------------------
        let mut banners: Vec<AnyElement> = Vec::new();
        if let LibraryStatus::Unavailable {
            hint,
            path,
            reason,
            fallback_in_use,
        } = &self.state.status
        {
            // LIB-2: "Library unavailable - connect <name>".
            let body = if *fallback_in_use {
                tr!("New captures are saved to a folder on this computer until it is back. {path} ({reason})", path = path, reason = reason)
            } else {
                tr!(
                    "Nothing can be saved or shown. {path} ({reason})",
                    path = path,
                    reason = reason
                )
            };
            banners.push(
                callout(
                    &t,
                    Tone::Brass,
                    "triangle-alert",
                    tr!("Library unavailable \u{2014} connect {hint}", hint = hint),
                    Some(body.into()),
                    None,
                )
                .id("banner-unavailable")
                .role(Role::Alert)
                .into_any_element(),
            );
        }
        // PRM-1.AC9: one non-blocking banner while Screen Recording is missing. No dialog opens
        // unless the user presses "Grant access…".
        if perms.screen_missing() {
            use crate::permissions::{self, Action, Id, State};
            let state = perms.state(Id::ScreenRecording);
            let (title, body) = if state == State::GrantedNeedsRestart {
                (
                    permissions::screen_restart_title(),
                    tr!("macOS usually needs the app restarted before it can capture."),
                )
            } else {
                (
                    permissions::screen_missing_title(),
                    permissions::screen_missing_hint(),
                )
            };
            let actions = permissions::actions(perms.os, Id::ScreenRecording, state);
            let buttons = actions.into_iter().map(|a| {
                let app = self.app.clone();
                let kind = if a == Action::Restart {
                    ButtonKind::Primary
                } else {
                    ButtonKind::Secondary
                };
                button(
                    &t,
                    format!("banner-screen-{}", a.label(perms.os)),
                    a.label(perms.os),
                    kind,
                )
                .on_click(move |_, _, cx| app::permission_action(&app, Id::ScreenRecording, a, cx))
            });
            let app = self.app.clone();
            let action = div()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .children(buttons)
                .child(
                    button(
                        &t,
                        "banner-open-permissions",
                        tr!("Open Permissions\u{2026}"),
                        ButtonKind::Ghost,
                    )
                    .on_click(move |_, _, cx| app::open_permissions_page(&app, cx)),
                );
            banners.push(
                callout(
                    &t,
                    Tone::Brass,
                    "shield-alert",
                    title.to_owned(),
                    Some(body.to_owned().into()),
                    Some(action.into_any_element()),
                )
                .id("banner-screen-recording")
                .role(Role::Alert)
                .into_any_element(),
            );
        }
        if let Some(w) = ipc_warning {
            banners.push(
                callout(
                    &t,
                    Tone::Agent,
                    "bot",
                    tr!("Command line and agents are offline"),
                    Some(w.into()),
                    None,
                )
                .into_any_element(),
            );
        }
        if self.state.truncated {
            banners.push(
                callout(
                    &t,
                    Tone::Neutral,
                    "info",
                    tr!("Showing the newest 5,000 captures"),
                    Some(tr!("Narrow the search to find older ones.").into()),
                    None,
                )
                .into_any_element(),
            );
        }
        // A quiet one-line hint while no speech model is installed (never a download).
        if route == Route::Recordings && hint {
            banners.push(self.model_hint(&t));
        }

        // ---- body --------------------------------------------------------------------
        let app = self.app.clone();
        let body: AnyElement = match (&self.state.status, self.state.empty_kind()) {
            (
                LibraryStatus::Unavailable {
                    fallback_in_use: false,
                    ..
                },
                _,
            ) => empty_state(
                &t,
                "hard-drive",
                &tr!("Library unavailable"),
                &tr!("Connect the drive or choose another folder in Settings."),
                None,
            ),
            (LibraryStatus::Failed(e), _) => {
                let v = view.clone();
                empty_state(
                    &t,
                    "triangle-alert",
                    &tr!("Couldn\u{2019}t load the library"),
                    &e.text(),
                    Some(
                        button_with_icon(
                            &t,
                            "retry",
                            tr!("Try again"),
                            ButtonKind::Secondary,
                            Some("refresh-cw"),
                        )
                        .on_click(move |_, _, cx| {
                            v.update(cx, |this, cx| this.reload(cx)).ok();
                        })
                        .into_any_element(),
                    ),
                )
            }
            (LibraryStatus::Loading, _) => empty_state(
                &t,
                "loader-circle",
                &tr!("Loading library\u{2026}"),
                "",
                None,
            ),
            (_, Some(EmptyKind::NoCaptures)) => {
                let keys = shortcuts
                    .get(ShortcutAction::CaptureRegion)
                    .keycaps(platform);
                let a = app.clone();
                empty_state(
                    &t,
                    "images",
                    &tr!("Nothing captured yet"),
                    &tr!("Your screenshots and recordings will show up here."),
                    Some(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                button_with_icon(
                                    &t,
                                    "empty-capture",
                                    tr!("Capture region"),
                                    ButtonKind::Primary,
                                    Some("cm-capture-region"),
                                )
                                .on_click(move |_, _, cx| {
                                    app::handle_command(
                                        &a,
                                        Command::Capture(CaptureMode::Region),
                                        cx,
                                    )
                                })
                                .child(keycaps(&t, &keys)),
                            )
                            .into_any_element(),
                    ),
                )
            }
            (_, Some(EmptyKind::NoResults { query })) => {
                let v = view.clone();
                let search = self.search.clone();
                empty_state(
                    &t,
                    "search-x",
                    &tr!("No matches"),
                    &if query.is_empty() {
                        tr!("Nothing in this filter yet.")
                    } else {
                        tr!("No captures match \u{201C}{query}\u{201D}.", query = query)
                    },
                    Some(
                        button(
                            &t,
                            "clear-search",
                            tr!("Clear search"),
                            ButtonKind::Secondary,
                        )
                        .on_click(move |_, _, cx| {
                            search.update(cx, |s, cx| s.clear(cx));
                            // The kind is the page's (the sidebar): only the chip goes.
                            v.update(cx, |this, cx| {
                                this.state.remote = None;
                                this.reload(cx);
                            })
                            .ok();
                        })
                        .into_any_element(),
                    ),
                )
            }
            _ => {
                let ctx = RowCtx {
                    t,
                    items: self.items.clone(),
                    rows: rows.clone(),
                    selected: self.state.selected,
                    list_focused,
                    menu_ix: self.menu.as_ref().map(|m| m.ix),
                    thumbs: self.thumbs.clone(),
                    view: view.clone(),
                    clock,
                    avail,
                    cols,
                };
                div()
                    .id("library-list")
                    .role(Role::List)
                    .aria_label(tr_ctx!("list", "Captures"))
                    .key_context("LibraryList")
                    .track_focus(&self.list_focus)
                    .tab_stop(true)
                    .on_action(
                        cx.listener(|this, _: &SelectLeft, _, cx| this.move_selection(-1, cx)),
                    )
                    .on_action(
                        cx.listener(|this, _: &SelectRight, _, cx| this.move_selection(1, cx)),
                    )
                    .on_action(cx.listener(|this, _: &SelectUp, _, cx| this.move_vertical(-1, cx)))
                    .on_action(cx.listener(|this, _: &SelectDown, _, cx| this.move_vertical(1, cx)))
                    .on_action(cx.listener(|this, _: &OpenSelected, _, cx| {
                        if let Some(i) = this.state.selected {
                            this.open_index(i, cx);
                        }
                    }))
                    .flex_1()
                    .min_h(px(0.))
                    .child(
                        list(self.list.clone(), move |ix, _window, cx| {
                            render_row(&ctx, ix, cx)
                        })
                        .size_full(),
                    )
                    .into_any_element()
            }
        };

        let menu = self
            .menu
            .as_ref()
            .map(|m| self.card_menu(&t, m, window, cx));
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .px(px(PAGE_PADDING))
            .pt(px(26.))
            .child(title_row)
            .children(banners)
            .child(toolbar)
            .child(body)
            .children(menu)
    }
}

impl LibraryView {
    /// The card's context menu (LIB-1): a 220 px popover with an icon on every row, a key hint
    /// where one exists and a rule before Delete. While renaming it holds the title box instead.
    fn card_menu(
        &self,
        t: &Theme,
        m: &CardMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mac = cfg!(target_os = "macos");
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        let panel = div()
            .id("card-menu")
            // The panel sits above the grid: without this a click on a menu entry also reaches the
            // card underneath and opens its detail (Rename jumped to the editor).
            .occlude()
            .role(Role::Menu)
            .aria_label(tr!("Capture actions"))
            .w(px(220.))
            .p(px(6.))
            .flex()
            .flex_col()
            .gap(px(1.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface_2))
            .shadow_lg()
            .text_size(px(13.5))
            .text_color(rgba_of(t.text_2))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_menu(cx)));
        let body = if m.renaming {
            panel
                .gap(px(8.))
                .p(px(8.))
                .child(self.rename_input.clone())
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(
                            button(
                                t,
                                "rename-cancel",
                                tr_ctx!("button", "Cancel"),
                                ButtonKind::Ghost,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.close_menu(cx))),
                        )
                        .child(
                            button(
                                t,
                                "rename-save",
                                tr_ctx!("button", "Rename"),
                                ButtonKind::Primary,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.commit_rename(cx))),
                        ),
                )
        } else {
            let actions = self
                .items
                .get(m.ix)
                .map_or_else(|| CardAction::ALL.to_vec(), CardAction::for_item);
            let mut panel = panel;
            for a in actions {
                let fg = if a == CardAction::Delete {
                    t.warning
                } else {
                    t.text_2
                };
                if a == CardAction::Delete {
                    // The rule before the destructive row.
                    panel =
                        panel.child(div().h(px(1.)).mx(px(6.)).my(px(4.)).bg(rgba_of(t.border)));
                }
                panel = panel.child(
                    div()
                        .id(SharedString::from(format!("card-menu-{}", a.key(mac))))
                        .role(Role::MenuItem)
                        .aria_label(SharedString::from(a.label(mac)))
                        .focusable()
                        .tab_stop(true)
                        .min_h(px(34.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .rounded(px(scale::RADIUS_SM))
                        .border_1()
                        .border_color(gpui::transparent_black())
                        .text_color(rgba_of(fg))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .focus_visible(move |s| s.border_color(focus))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_card_action(a, window, cx)
                        }))
                        .child(icon(a.glyph(), 15., fg))
                        .child(a.label(mac))
                        .children(a.hint().map(|h| {
                            div()
                                .ml_auto()
                                .text_size(px(11.))
                                .text_color(rgba_of(t.faint))
                                .mono()
                                .child(h)
                        })),
                );
            }
            panel
        };
        // Anchored at the pointer or the "..." button, drawn above the grid, kept inside the window.
        deferred(
            anchored()
                .position(point(m.at.x, m.at.y))
                .anchor(m.anchor)
                .snap_to_window_with_margin(px(8.))
                .child(body),
        )
        .with_priority(10)
        .into_any_element()
    }
}

/// Minimum usable content width (avoids negative widths in a tiny window).
const CARD_FLOOR: f32 = 200.;

/// A centred empty or error state: an icon tile, the title, a line of text and an action.
fn empty_state(
    t: &Theme,
    glyph: &'static str,
    title: &str,
    body: &str,
    action: Option<AnyElement>,
) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(12.))
        .child(
            div()
                .size(px(48.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_LG))
                .border_1()
                .border_color(rgba_of(t.border))
                .bg(rgba_of(t.surface))
                .child(icon(glyph, 22., t.text_3)),
        )
        .child(
            div()
                .text_size(px(type_scale::H2))
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(title.to_owned())),
        )
        .when(!body.is_empty(), |d| {
            d.child(
                div()
                    .max_w(px(420.))
                    .text_align(gpui::TextAlign::Center)
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(body.to_owned())),
            )
        })
        .children(action)
        .into_any_element()
}
