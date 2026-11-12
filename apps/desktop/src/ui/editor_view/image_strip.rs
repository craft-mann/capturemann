//! The image strip under the canvas (0.3.3, boards N8 and N8b): thumbnails of the images of the
//! Library view the editor was opened from, for quick navigation. Which images, the hide rules,
//! the names and the lazy range are in `crate::editor::strip_model` (unit tested); this file
//! draws them and routes the clicks and keys. UI only: no agent or daemon change.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui::{point, AnyView, ScrollHandle};

use super::*;
use crate::editor::strip_model::{self as sm, StripItem};
use crate::ui::thumbs::{Thumb, ThumbCache};

pub(super) struct StripState {
    /// The Images button: shown unless the user hid it (remembered, `editor.image_strip`).
    pub pref_shown: bool,
    scroll: ScrollHandle,
    thumbs: Entity<ThumbCache>,
    /// Annotation counts of the other images, read lazily; the open image uses its live list.
    counts: HashMap<String, usize>,
    asked: HashSet<String>,
    /// Neighbours whose original file was read ahead.
    warmed: HashSet<String>,
    focus: FocusHandle,
    /// The option the keyboard is on while the strip has focus.
    active: usize,
    /// The image the strip last scrolled to.
    shown_for: String,
    _sub: Subscription,
}

impl StripState {
    pub fn new(app: &Entity<AppModel>, cx: &mut Context<EditorView>) -> Self {
        let backend = app.read(cx).backend.clone();
        let pref_shown = backend
            .settings
            .snapshot()
            .get(sm::STRIP_PREF_KEY)
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let thumbs = cx.new(|_| ThumbCache::new(backend));
        let sub = cx.observe(&thumbs, |_, _, cx| cx.notify());
        StripState {
            pref_shown,
            scroll: ScrollHandle::new(),
            thumbs,
            counts: HashMap::new(),
            asked: HashSet::new(),
            warmed: HashSet::new(),
            focus: cx.focus_handle(),
            active: 0,
            shown_for: String::new(),
            _sub: sub,
        }
    }

    /// Another client may have changed other images: ask for the counts again (the old ones
    /// stay on screen until the new ones arrive).
    pub fn forget_asked(&mut self) {
        self.asked.clear();
    }
}

/// The two-line tooltip of a thumbnail: the full name, then the muted facts.
struct StripTip {
    name: SharedString,
    meta: SharedString,
}

impl Render for StripTip {
    fn render(&mut self, _w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        div()
            .max_w(px(280.))
            .px(px(10.))
            .py(px(6.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .rounded(px(9.))
            .bg(rgba_of(t.raised))
            .border_1()
            .border_color(rgba_of(t.line))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgba_of(t.text))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(self.name.clone()),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgba_of(t.mist))
                    .child(self.meta.clone()),
            )
    }
}

impl EditorView {
    /// The list the strip shows, or an empty one.
    pub(super) fn strip_list(&self, cx: &Context<Self>) -> Arc<Vec<StripItem>> {
        self.app.read(cx).editor_strip.clone()
    }

    /// The Images button: shows or hides the strip and remembers the choice.
    pub(super) fn toggle_strip(&mut self, cx: &mut Context<Self>) {
        self.strip.pref_shown = !self.strip.pref_shown;
        app::set_setting(
            &self.app,
            sm::STRIP_PREF_KEY,
            serde_json::Value::Bool(self.strip.pref_shown),
            cx,
        );
        cx.notify();
    }

    /// Opens another image of the list. Edits are saved as they are made and each image keeps
    /// its own history in the daemon, so there is no prompt.
    fn open_image(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.canvas_focus, cx);
        if id != self.capture_id {
            app::set_route(&self.app, Route::Editor(id.to_owned()), cx);
        }
    }

    /// Remembers what the image that is being left looks like now.
    pub(super) fn strip_leaving(&mut self, id: &str, annotations: usize, cx: &mut Context<Self>) {
        self.strip.counts.insert(id.to_owned(), annotations);
        self.strip.thumbs.update(cx, |t, cx| t.invalidate(id, cx));
    }

    /// Reads the previous and next originals ahead, so opening them finds the file cached.
    pub(super) fn warm_neighbors(&mut self, list: &[StripItem], cx: &mut Context<Self>) {
        let ids: Vec<String> = sm::preload_ids(list, &self.capture_id)
            .into_iter()
            .filter(|id| self.strip.warmed.insert(id.clone()))
            .collect();
        if ids.is_empty() {
            return;
        }
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let rt = backend.rt.clone();
        rt.spawn(async move {
            for id in ids {
                if let Ok(snap) = crate::editor::session::fetch_snapshot(&daemon, &id).await {
                    let path = snap.dir.join("original.png");
                    let _ = tokio::task::spawn_blocking(move || std::fs::read(path)).await;
                }
            }
        });
    }

    /// Reads the annotation counts of images that came into view.
    fn ask_counts(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        let ids: Vec<String> = ids
            .into_iter()
            .filter(|id| *id != self.capture_id && self.strip.asked.insert(id.clone()))
            .collect();
        if ids.is_empty() {
            return;
        }
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let rt = backend.rt.clone();
        cx.spawn(async move |this, cx| {
            for id in ids {
                let d = daemon.clone();
                let i = id.clone();
                let n = rt
                    .spawn(async move { crate::editor::session::fetch_snapshot(&d, &i).await })
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .map(|s| s.ops.len());
                if let Some(n) = n {
                    this.update(cx, |this, cx| {
                        this.strip.counts.insert(id, n);
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn strip_key(
        &mut self,
        ev: &KeyDownEvent,
        list: &[StripItem],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let n = list.len();
        let delta = match ev.keystroke.key.as_str() {
            "left" => Some(-1),
            "right" => Some(1),
            _ => None,
        };
        if let Some(d) = delta {
            self.strip.active = sm::move_focus(self.strip.active, d, n);
            self.scroll_to_active(n);
        } else {
            match ev.keystroke.key.as_str() {
                "enter" | "space" => {
                    if let Some(item) = list.get(self.strip.active) {
                        let id = item.id.clone();
                        self.open_image(&id, window, cx);
                    }
                }
                "escape" => window.focus(&self.canvas_focus, cx),
                _ => return,
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn scroll_to_active(&self, n: usize) {
        self.scroll_to_active_index(self.strip.active, n);
    }

    /// The strip: previous and next buttons around a scrolling row of thumbnails. `live` is the
    /// open image's annotation count.
    pub(super) fn image_strip(
        &mut self,
        t: &Theme,
        list: &Arc<Vec<StripItem>>,
        live: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let n = list.len();
        let current = sm::index_of(list, &self.capture_id);
        // Keep the open image in view when it changes.
        if self.strip.shown_for != self.capture_id {
            self.strip.shown_for = self.capture_id.clone();
            if let Some(i) = current {
                self.strip.active = i;
                self.scroll_to_active_index(i, n);
            }
        }
        let vw = f32::from(self.strip.scroll.bounds().size.width);
        let vw = if vw < 1. { 640. } else { vw };
        let off = -f32::from(self.strip.scroll.offset().x);
        let range = sm::visible_range(off, vw, n);
        self.ask_counts(
            list[range.clone()].iter().map(|i| i.id.clone()).collect(),
            cx,
        );

        let focused = self.strip.focus.is_focused(window);
        let clock = self.app.read(cx).clock;
        let (hover_ring, focus_ring, rec) = (rgba_of(t.mist), rgba_of(t.text), rgba_of(t.rec));
        let mut row = div()
            .id("editor-strip-list")
            .role(Role::ListBox)
            .aria_label(tr!("Images"))
            .aria_orientation(gpui::Orientation::Horizontal)
            .track_focus(&self.strip.focus)
            .tab_stop(true)
            .flex_1()
            .min_w(px(0.))
            .h(px(sm::THUMB_H + 2.))
            .flex()
            .items_center()
            .gap(px(sm::THUMB_GAP))
            .overflow_x_scroll()
            .track_scroll(&self.strip.scroll)
            .on_key_down({
                let list = list.clone();
                cx.listener(move |this, ev: &KeyDownEvent, window, cx| {
                    this.strip_key(ev, &list, window, cx)
                })
            });
        for (i, item) in list.iter().enumerate() {
            let is_current = Some(i) == current;
            let count = if is_current {
                Some(live)
            } else {
                self.strip.counts.get(&item.id).copied()
            };
            let name = sm::option_label(&item.title, count, i + 1, n);
            let meta = sm::tooltip_meta(&clock.format_date(item.created), count, i + 1, n);
            let thumb = range.contains(&i).then(|| {
                self.strip.thumbs.update(cx, |c, cx| {
                    c.get_cover(&item.id, sm::THUMB_W / sm::THUMB_H, cx)
                })
            });
            let id = item.id.clone();
            let title = SharedString::from(item.title.clone());
            let keyed = focused && i == self.strip.active;
            let mut frame = div()
                .id(ElementId::from(("strip-option", i)))
                .role(Role::ListBoxOption)
                .aria_label(SharedString::from(name))
                .aria_selected(is_current)
                .aria_position_in_set(i + 1)
                .aria_size_of_set(n)
                .relative()
                .w(px(sm::THUMB_W))
                .h(px(sm::THUMB_H))
                .flex_none()
                .rounded(px(7.))
                .overflow_hidden()
                .bg(rgba_of(t.raised))
                .border_2()
                .border_color(if is_current {
                    rec
                } else if keyed {
                    focus_ring
                } else {
                    gpui::rgba(0)
                })
                .cursor_pointer()
                .when(!is_current && !keyed, |d| {
                    d.hover(move |s| s.border_color(hover_ring))
                })
                .tooltip(move |_, cx| -> AnyView {
                    let (name, meta) = (title.clone(), SharedString::from(meta.clone()));
                    cx.new(|_| StripTip { name, meta }).into()
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.strip.active = i;
                    this.open_image(&id, window, cx);
                }));
            if let Some(Thumb::Ready(image)) = thumb {
                // 80% until hovered, 100% for the open image.
                frame = frame.child(
                    img(image)
                        .size_full()
                        .object_fit(ObjectFit::Fill)
                        .opacity(if is_current { 1.0 } else { 0.8 }),
                );
            }
            if let Some(c) = count.filter(|c| *c > 0) {
                frame = frame.child(
                    div()
                        .absolute()
                        .top(px(2.))
                        .right(px(2.))
                        .h(px(15.))
                        .min_w(px(15.))
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(sm::THUMB_H))
                        .bg(rec)
                        .text_color(rgba_of(t.on_rec))
                        .text_size(px(9.5))
                        .font_weight(FontWeight::BOLD)
                        .child(SharedString::from(c.to_string())),
                );
            }
            row = row.child(frame);
        }

        let step_btn = |this: &Self,
                        id: &'static str,
                        glyph: &'static str,
                        label: String,
                        delta: isize,
                        cx: &mut Context<Self>| {
            let target = sm::neighbor(list, &this.capture_id, delta).map(|i| i.id.clone());
            let (bg, focus) = (rgba_of(t.raised), rgba_of(t.text));
            let b = div()
                .id(id)
                .role(Role::Button)
                .aria_label(label.clone())
                .focusable()
                .tab_stop(true)
                .w(px(28.))
                .h(px(58.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .bg(bg)
                .border_1()
                .border_color(gpui::transparent_black())
                .focus_visible(move |s| s.border_color(focus))
                .tooltip(tooltip::tip(label, Vec::new()))
                .child(icon(glyph, 15., t.text_2));
            match target {
                Some(id) => b
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_image(&id, window, cx)
                    }))
                    .into_any_element(),
                None => b.opacity(0.35).cursor_default().into_any_element(),
            }
        };
        let prev = step_btn(
            self,
            "editor-strip-prev",
            "chevron-left",
            tr!("Previous image"),
            -1,
            cx,
        );
        let next = step_btn(
            self,
            "editor-strip-next",
            "chevron-right",
            tr!("Next image"),
            1,
            cx,
        );

        div()
            .id("editor-strip")
            .role(Role::Group)
            .aria_label(tr!("Image strip"))
            .w_full()
            .h(px(sm::STRIP_H))
            .flex_none()
            .p(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .child(prev)
            .child(row)
            .child(next)
            .into_any_element()
    }

    fn scroll_to_active_index(&self, i: usize, n: usize) {
        let vw = f32::from(self.strip.scroll.bounds().size.width);
        let vw = if vw < 1. { 640. } else { vw };
        let off = -f32::from(self.strip.scroll.offset().x);
        let to = sm::scroll_into_view(i, off, vw, n);
        self.strip.scroll.set_offset(point(px(-to), px(0.)));
    }
}
