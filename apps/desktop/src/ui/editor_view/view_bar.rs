//! The view bar at the bottom centre of the stage (0.3.3, board N7):
//! `Undo Redo | Copy text Auto-redact | Zoom out [68% v] Zoom in | Images Keyboard ...`.
//! The zoom value opens the zoom menu; "Revert to original" lives in the `...` menu. The
//! logic (menu entries, dimming) is in `crate::editor::view_model`; this file only draws it.

use std::time::{Duration, Instant};

use gpui::Stateful;

use super::*;
use crate::editor::strip_model;
use crate::editor::view_model::{self, ZoomChoice};

/// Bar buttons are 34 px tall with 16 px icons (board N7).
const BAR_BTN: f32 = 34.;
/// A press right after a menu closed itself (a click outside) is the same press that closed it.
const REOPEN_GUARD: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BarMenu {
    Zoom,
    More,
}

#[derive(Default)]
pub(super) struct BarState {
    pub open: Option<BarMenu>,
    closed_by_click: Option<(BarMenu, Instant)>,
}

impl BarState {
    /// Opens `m`, or closes it when it is open. A press that lands right after the menu closed
    /// because of a click outside it (the same press, on the trigger) does not reopen it.
    fn toggle(&mut self, m: BarMenu) {
        if self.open == Some(m) {
            self.open = None;
        } else if self
            .closed_by_click
            .is_some_and(|(k, at)| k == m && at.elapsed() < REOPEN_GUARD)
        {
            self.closed_by_click = None;
        } else {
            self.open = Some(m);
        }
    }

    fn click_outside(&mut self, m: BarMenu) {
        if self.open == Some(m) {
            self.open = None;
            self.closed_by_click = Some((m, Instant::now()));
        }
    }
}

impl EditorView {
    fn bar_divider(t: &Theme) -> Div {
        div()
            .w(px(1.))
            .h(px(22.))
            .mx(px(5.))
            .flex_none()
            .bg(rgba_of(t.line))
    }

    /// One bar button: `content` (an icon, or an icon and a label) in a 34 px target.
    /// `dim` is the opacity of a button with nothing to do (undo and redo use 35%).
    #[allow(clippy::too_many_arguments)]
    fn bar_btn(
        &self,
        t: &Theme,
        id: &'static str,
        label: String,
        tip: String,
        content: AnyElement,
        keys: &[String],
        enabled: bool,
        on_state: bool,
        on: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        let b = div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .focusable()
            .tab_stop(true)
            .h(px(BAR_BTN))
            .min_w(px(BAR_BTN))
            .px(px(8.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(10.))
            .border_1()
            .border_color(gpui::transparent_black())
            .text_size(px(13.))
            .text_color(rgba_of(if on_state { t.text } else { t.text_2 }))
            .when(on_state, |d| d.bg(rgba_of(t.raised)))
            .when(!on_state, |d| d.hover(move |s| s.bg(hover)))
            .focus_visible(move |s| s.border_color(focus))
            .tooltip(tooltip::tip(tip, keys.to_vec()))
            .child(content);
        if enabled {
            b.cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| on(this, window, cx)))
        } else {
            b.opacity(view_model::history_opacity(false))
                .cursor_default()
        }
    }

    /// An icon and a label side by side (Copy text, Auto-redact, Images).
    fn bar_icon_label(glyph: &'static str, color: Color, label: String) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(icon(glyph, 16., color))
            .child(shrink_text(label))
            .into_any_element()
    }

    /// The view bar. `images` is `Some(on)` when the strip can be used (more than one image in
    /// the list), `on` being whether it is shown.
    pub(super) fn view_bar(
        &mut self,
        t: &Theme,
        s: &Snap,
        images: Option<bool>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (can_undo, can_redo) = (s.can_undo, s.can_redo);
        let m = mod_label().to_owned();
        let plain = |k: &str| vec![m.clone(), k.to_owned()];
        let shifted = |k: &str| {
            if cfg!(target_os = "macos") {
                vec![shift_label().to_owned(), m.clone(), k.to_owned()]
            } else {
                vec![m.clone(), shift_label().to_owned(), k.to_owned()]
            }
        };
        let c = t.text_2;
        let percent = self.viewport.zoom_percent();
        let zoom = view_model::zoom_label(percent);
        let open = self.shortcuts_open;

        let history = |this: &Self,
                       id: &'static str,
                       label: String,
                       glyph: &'static str,
                       keys: Vec<String>,
                       enabled: bool,
                       redo: bool,
                       cx: &mut Context<Self>| {
            this.bar_btn(
                t,
                id,
                label.clone(),
                label,
                icon(glyph, 16., c).into_any_element(),
                &keys,
                // Nothing to undo or redo: dimmed to 35% and the press does nothing.
                enabled,
                false,
                move |this, _, cx| {
                    if let Some(d) = &this.doc {
                        d.update(cx, |d, cx| if redo { d.redo(cx) } else { d.undo(cx) });
                    }
                },
                cx,
            )
        };
        let undo = history(
            self,
            "editor-undo",
            tr!("Undo"),
            "undo-2",
            plain("Z"),
            can_undo,
            false,
            cx,
        );
        let redo = history(
            self,
            "editor-redo",
            tr!("Redo"),
            "redo-2",
            shifted("Z"),
            can_redo,
            true,
            cx,
        );

        let copy_tip =
            tr!("Copies all the text in the image. Select text copies only the part you select.");
        let copy_text = if s.text_available {
            let running = s.text_running;
            Some(self.bar_btn(
                t,
                "editor-copy-text",
                tr!("Copy text"),
                copy_tip.clone(),
                Self::bar_icon_label(
                    if running {
                        "loader-circle"
                    } else {
                        "scan-text"
                    },
                    c,
                    if running {
                        tr!("Reading text\u{2026}")
                    } else {
                        tr!("Copy text")
                    },
                ),
                &[],
                !running,
                false,
                |this, _, cx| {
                    if let Some(d) = &this.doc {
                        d.update(cx, |d, cx| d.copy_text(None, cx));
                    }
                },
                cx,
            ))
        } else {
            s.text_hint.map(|hint| {
                self.bar_btn(
                    t,
                    "editor-copy-text-unavailable",
                    hint.to_owned(),
                    hint.to_owned(),
                    Self::bar_icon_label("scan-text", c, tr!("Copy text")),
                    &[],
                    false,
                    false,
                    |_, _, _| {},
                    cx,
                )
            })
        };
        let auto_redact = if s.redact_available {
            Some(self.bar_btn(
                t,
                "editor-auto-redact",
                tr!("Auto-redact"),
                tr!("Auto-redact"),
                Self::bar_icon_label("eye-off", c, tr!("Auto-redact")),
                &[],
                s.redact != RedactPhase::Running,
                false,
                |this, _, cx| {
                    if let Some(d) = &this.doc {
                        d.update(cx, |d, cx| d.auto_redact(cx));
                    }
                },
                cx,
            ))
        } else {
            s.redact_hint.map(|hint| {
                self.bar_btn(
                    t,
                    "editor-auto-redact-unavailable",
                    hint.to_owned(),
                    hint.to_owned(),
                    Self::bar_icon_label("eye-off", c, tr!("Auto-redact")),
                    &[],
                    false,
                    false,
                    |_, _, _| {},
                    cx,
                )
            })
        };

        let zoom_out = self.bar_btn(
            t,
            "editor-zoom-out",
            tr!("Zoom out"),
            tr!("Zoom out"),
            icon("zoom-out", 16., c).into_any_element(),
            &plain("-"),
            true,
            false,
            |this, _, _| this.viewport.zoom_step(false),
            cx,
        );
        let zoom_in = self.bar_btn(
            t,
            "editor-zoom-in",
            tr!("Zoom in"),
            tr!("Zoom in"),
            icon("zoom-in", 16., c).into_any_element(),
            &plain("+"),
            true,
            false,
            |this, _, _| this.viewport.zoom_step(true),
            cx,
        );
        let zoom_open = self.bar.open == Some(BarMenu::Zoom);
        let zoom_value = self
            .bar_btn(
                t,
                "editor-zoom",
                tr_ctx!("zoom level", "Zoom {zoom}", zoom = zoom),
                tr!("Zoom"),
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .min_w(px(34.))
                            .flex()
                            .justify_center()
                            .text_color(rgba_of(t.text))
                            .mono()
                            .tnum()
                            .child(SharedString::from(zoom)),
                    )
                    .child(icon("chevron-down", 14., c))
                    .into_any_element(),
                &[],
                true,
                zoom_open,
                |this, _, _| this.bar.toggle(BarMenu::Zoom),
                cx,
            )
            .aria_expanded(zoom_open);
        let zoom_cell = div()
            .relative()
            .flex_none()
            .child(zoom_value)
            .when(zoom_open, |d| d.child(self.zoom_menu(t, percent, cx)));

        let images_btn = images.map(|on| {
            self.bar_btn(
                t,
                "editor-images",
                tr!("Images"),
                tr!("Show or hide the image strip"),
                Self::bar_icon_label(
                    "gallery-horizontal-end",
                    if on { t.text } else { c },
                    tr!("Images"),
                ),
                &[],
                true,
                on,
                |this, _, cx| this.toggle_strip(cx),
                cx,
            )
            .aria_toggled(toggled(on))
        });

        let shortcuts = icon_button(
            t,
            "editor-shortcuts",
            "keyboard",
            tr!("Keyboard shortcuts"),
            open,
            Some(Vec::new()),
        )
        .size(px(BAR_BTN))
        .aria_expanded(open)
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.shortcuts_open = !this.shortcuts_open;
            cx.notify();
        }));

        let more_open = self.bar.open == Some(BarMenu::More);
        let more_btn = self
            .bar_btn(
                t,
                "editor-more",
                tr!("More"),
                tr!("More"),
                icon("ellipsis", 16., c).into_any_element(),
                &[],
                true,
                more_open,
                |this, _, _| this.bar.toggle(BarMenu::More),
                cx,
            )
            .aria_expanded(more_open);
        let more_cell = div()
            .relative()
            .flex_none()
            .child(more_btn)
            .when(more_open, |d| d.child(self.more_menu(t, s.has_ops, cx)));

        Self::pill_surface(t)
            .id("editor-transport")
            .role(Role::Toolbar)
            .aria_label(tr!("Undo and zoom"))
            .max_w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            .p(px(5.))
            .rounded(px(14.))
            .child(undo)
            .child(redo)
            .when(copy_text.is_some() || auto_redact.is_some(), |d| {
                d.child(Self::bar_divider(t))
            })
            .children(copy_text)
            .children(auto_redact)
            .child(Self::bar_divider(t))
            .child(zoom_out)
            .child(zoom_cell)
            .child(zoom_in)
            .child(Self::bar_divider(t))
            .children(images_btn)
            .child(shortcuts)
            .child(more_cell)
            .into_any_element()
    }

    /// A menu above its trigger: 220 px, padding 6, rows 34 px (board N6/N7).
    fn bar_menu_panel(
        t: &Theme,
        id: &'static str,
        label: String,
        menu: BarMenu,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .occlude()
            .role(Role::Menu)
            .aria_label(label)
            .w(px(220.))
            .p(px(6.))
            .flex()
            .flex_col()
            .rounded(px(13.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            .text_color(rgba_of(t.text))
            .text_size(px(13.))
            .on_mouse_down_out(cx.listener(move |this, _, _, cx| {
                this.bar.click_outside(menu);
                cx.notify();
            }))
    }

    /// Wraps a menu so it floats above its trigger, drawn over the canvas.
    fn bar_menu_float(menu: Stateful<Div>, right: bool) -> AnyElement {
        deferred(
            div()
                .absolute()
                .bottom(px(BAR_BTN + 12.))
                .when(right, |d| d.right_0())
                .when(!right, |d| d.left_0())
                .child(menu),
        )
        .with_priority(10)
        .into_any_element()
    }

    fn zoom_menu(&self, t: &Theme, percent: f64, cx: &mut Context<Self>) -> AnyElement {
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        let mut panel = Self::bar_menu_panel(t, "editor-zoom-menu", tr!("Zoom"), BarMenu::Zoom, cx);
        for (i, e) in view_model::zoom_entries(percent, self.viewport.fit)
            .into_iter()
            .enumerate()
        {
            let choice = e.choice;
            panel = panel.child(
                div()
                    .id(ElementId::from(("zoom-entry", i)))
                    .role(Role::MenuItemRadio)
                    .aria_label(SharedString::from(e.label.clone()))
                    .aria_toggled(toggled(e.checked))
                    .focusable()
                    .tab_stop(true)
                    .h(px(34.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(scale::RADIUS_SM))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .cursor_pointer()
                    .when(e.checked, |d| d.bg(hover))
                    .hover(move |s| s.bg(hover))
                    .focus_visible(move |s| s.border_color(focus))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        match choice {
                            ZoomChoice::Fit => this.viewport.fit_view(),
                            ZoomChoice::Percent(100) => this.viewport.actual_size(),
                            ZoomChoice::Percent(p) => this.viewport.zoom_to_percent(f64::from(p)),
                            ZoomChoice::Custom(_) => {}
                        }
                        this.bar.open = None;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(16.))
                            .flex_none()
                            .when(e.checked, |d| d.child(icon("check", 16., t.text))),
                    )
                    .child(shrink_text(e.label)),
            );
        }
        Self::bar_menu_float(panel, false)
    }

    fn more_menu(&self, t: &Theme, has_ops: bool, cx: &mut Context<Self>) -> AnyElement {
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        let panel = Self::bar_menu_panel(t, "editor-more-menu", tr!("More"), BarMenu::More, cx)
            .child(
                div()
                    .id("editor-revert")
                    .role(Role::MenuItem)
                    .aria_label(tr!("Revert to original"))
                    .aria_description(tr!("Removes every annotation, crop and redaction."))
                    .focusable()
                    .tab_stop(true)
                    .px(px(10.))
                    .py(px(6.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .rounded(px(scale::RADIUS_SM))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .text_color(rgba_of(t.warning))
                    .when(has_ops, |d| {
                        d.cursor_pointer()
                            .hover(move |s| s.bg(hover))
                            .focus_visible(move |s| s.border_color(focus))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.bar.open = None;
                                if let Some(d) = &this.doc {
                                    d.update(cx, |d, cx| d.revert(cx));
                                }
                                cx.notify();
                            }))
                    })
                    .when(!has_ops, |d| d.opacity(0.4).cursor_default())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(icon("rotate-ccw", 16., t.warning))
                            .child(shrink_text(tr!("Revert to original"))),
                    )
                    .child(
                        div()
                            .pl(px(26.))
                            .text_size(px(11.))
                            .text_color(rgba_of(t.mist))
                            .child(tr!("Removes every annotation, crop and redaction.")),
                    ),
            );
        Self::bar_menu_float(panel, true)
    }

    /// Whether the Images button is offered and on; `None` hides the button.
    pub(super) fn images_state(&self, list: &[strip_model::StripItem]) -> Option<bool> {
        strip_model::images_button_available(list, &self.capture_id)
            .then_some(self.strip.pref_shown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_toggles_and_a_click_outside_does_not_reopen_it_at_once() {
        let mut b = BarState::default();
        b.toggle(BarMenu::Zoom);
        assert_eq!(b.open, Some(BarMenu::Zoom));
        b.toggle(BarMenu::Zoom);
        assert_eq!(b.open, None);
        // The press on the trigger that also counts as a click outside the open menu.
        b.toggle(BarMenu::Zoom);
        b.click_outside(BarMenu::Zoom);
        assert_eq!(b.open, None);
        b.toggle(BarMenu::Zoom);
        assert_eq!(b.open, None, "same press: stays closed");
        b.toggle(BarMenu::Zoom);
        assert_eq!(b.open, Some(BarMenu::Zoom));
        // Opening the other menu while one is open switches.
        b.toggle(BarMenu::More);
        assert_eq!(b.open, Some(BarMenu::More));
    }
}
