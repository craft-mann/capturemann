//! The preset edit sheet (QC-4): name, kind and target, the toggles, the tool, the folder, the
//! shortcut with Change, and Save preset, Cancel and Delete. A floating sheet like the Record
//! sheet; it changes `presets.json` only through the preset actions.

use cm_core::preset::{Preset, PresetKind, PresetTarget};
use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, App, Context, Entity, FocusHandle, FontWeight, Global, KeyDownEvent,
    MouseButton, Render, Role, ScrollHandle, SharedString, Window, WindowHandle,
};
use serde_json::json;

use crate::quick::presets::{check_preset_chord, chord_of};
use crate::quick::state::{kind_label, target_label, SHOT_TARGETS, VIDEO_TARGETS};
use crate::shortcuts::Chord;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, AppModel};
use crate::ui::components::{keycaps, switch};
use crate::ui::paint::{rgba_of, ui_features, UI_FONT};
use crate::ui::recording::{panel_options, pointer_display, visible_part};
use crate::ui::text_field::{TextField, TextFieldEvent};

pub const WIDTH: f32 = 440.;
pub const HEIGHT: f32 = 640.;

/// The sheet's height on a display whose visible part is `visible_h` logical px tall: the usual
/// height, never more than the display leaves (QC-4.AC8). The body scrolls and the footer stays.
pub fn sheet_height(visible_h: f64) -> f64 {
    f64::from(HEIGHT).min((visible_h - 48.0).max(MIN_HEIGHT.min(visible_h - 16.0)))
}

/// The least height the sheet is drawn at when the display is very short (the title, one row of
/// the body and the footer); below it the sheet takes what the display has.
pub const MIN_HEIGHT: f64 = 360.0;

/// The editor tools a preset can open with (the first is "none").
pub const TOOLS: [&str; 8] = [
    "",
    "step",
    "arrow",
    "rect",
    "text",
    "callout",
    "highlight",
    "blur",
];

struct SheetSlot(Option<WindowHandle<PresetSheet>>);
impl Global for SheetSlot {}

/// Opens the sheet for `preset`, or for a new one (`None`). One sheet at a time.
pub fn open(app: &Entity<AppModel>, preset: Option<Preset>, cx: &mut App) {
    if let Some(h) = cx.try_global::<SheetSlot>().and_then(|s| s.0) {
        if h.update(cx, |_, window, _| window.activate_window())
            .is_ok()
        {
            return;
        }
    }
    let Some(display) = pointer_display(cx) else {
        return;
    };
    let visible = visible_part(&display.1);
    let h = sheet_height(visible.h);
    let global = crate::recording::geometry::Rect::new(
        visible.x + (visible.w - f64::from(WIDTH)) / 2.0,
        visible.y + (visible.h - h) / 2.0,
        f64::from(WIDTH),
        h,
    );
    let is_new = preset.is_none();
    let p = preset.unwrap_or_else(crate::ui::quick_flow::blank_preset);
    let app2 = app.clone();
    let opened = cx.open_window(
        panel_options(display, global, true, false),
        move |window, cx| {
            crate::platform_window::exclude_from_capture(window);
            cx.new(|cx| PresetSheet::new(app2, p, is_new, window, cx))
        },
    );
    if let Ok(handle) = opened {
        crate::surfaces::opened(crate::surfaces::Surface::QuickSheet, handle);
        cx.set_global(SheetSlot(Some(handle)));
        crate::surfaces::activate_app(cx, "preset sheet opened");
        crate::surfaces::take_keyboard_x11(cx, handle);
    }
}

pub struct PresetSheet {
    app: Entity<AppModel>,
    p: Preset,
    is_new: bool,
    name: Entity<TextField>,
    save_to: Entity<TextField>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    waiting_key: bool,
    key_error: Option<String>,
    error: Option<String>,
    confirm_delete: bool,
}

impl PresetSheet {
    fn new(
        app: Entity<AppModel>,
        p: Preset,
        is_new: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let name = cx.new(|cx| {
            TextField::new(
                tr_ctx!("field", "Preset name"),
                tr!("Preset name, e.g. Bug report"),
                cx,
            )
            .without_icon()
        });
        name.update(cx, |f, cx| f.set_text(&p.name, cx));
        let save_to = cx.new(|cx| {
            TextField::new(
                tr_ctx!("field", "Save to folder"),
                tr!("Folder, e.g. ~/Bugs/{date}"),
                cx,
            )
            .without_icon()
        });
        save_to.update(cx, |f, cx| {
            f.set_text(p.save_to.as_deref().unwrap_or(""), cx)
        });
        for field in [&name, &save_to] {
            cx.subscribe_in(
                field,
                window,
                |this, _, ev: &TextFieldEvent, window, cx| match ev {
                    TextFieldEvent::Submitted => this.save(window, cx),
                    TextFieldEvent::Escaped => this.close(window, cx),
                    TextFieldEvent::Changed(_) => {
                        this.error = None;
                        cx.notify();
                    }
                },
            )
            .detach();
        }
        let handle = name.read(cx).focus.clone();
        window.focus(&handle, cx);
        PresetSheet {
            app,
            p,
            is_new,
            name,
            save_to,
            focus,
            scroll: ScrollHandle::new(),
            waiting_key: false,
            key_error: None,
            error: None,
            confirm_delete: false,
        }
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_key(cx);
        crate::surfaces::close_self(window);
        cx.set_global(SheetSlot(None));
    }

    fn set_kind(&mut self, k: PresetKind, cx: &mut Context<Self>) {
        if self.p.kind == k {
            return;
        }
        // The fields of the other kind do not carry over: a preset only holds what its kind has.
        let (shortcut, save_to, extra) = (
            self.p.shortcut.clone(),
            self.p.save_to.clone(),
            self.p.extra.clone(),
        );
        let mut fresh = crate::ui::quick_flow::blank_preset();
        fresh.id = self.p.id.clone();
        fresh.name = self.p.name.clone();
        fresh.kind = k;
        fresh.target = match k {
            PresetKind::Audio => None,
            PresetKind::Video => Some(PresetTarget::Display),
            PresetKind::Screenshot => Some(PresetTarget::Region),
        };
        if k == PresetKind::Video {
            fresh.options.mic = Some(true);
            fresh.options.countdown_s = Some(3);
        }
        if k == PresetKind::Audio {
            fresh.options.mic = Some(true);
        }
        fresh.shortcut = shortcut;
        fresh.save_to = save_to.filter(|_| k == PresetKind::Screenshot);
        fresh.extra = extra;
        self.p = fresh;
        cx.notify();
    }

    fn begin_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.waiting_key = true;
        self.key_error = None;
        // The global shortcuts are released while a new one is recorded, so a press of a
        // registered combination reaches the sheet and its owner can be named.
        app::suspend_shortcuts(&self.app.clone(), cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn end_key(&mut self, cx: &mut Context<Self>) {
        if self.waiting_key {
            self.waiting_key = false;
            app::resume_shortcuts(&self.app.clone(), cx);
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        if !self.waiting_key {
            if k.key == "escape" {
                self.close(window, cx);
                cx.stop_propagation();
            }
            return;
        }
        cx.stop_propagation();
        if k.key == "escape" {
            self.end_key(cx);
            cx.notify();
            return;
        }
        let m = k.modifiers;
        let chord = match Chord::from_pressed(m.control, m.alt, m.shift, m.platform, &k.key) {
            Ok(c) => c,
            Err(crate::shortcuts::ChordError::NoModifier) => {
                self.key_error = Some(crate::ui::app::RebindError::NeedsModifier.text());
                cx.notify();
                return;
            }
            Err(_) => {
                self.key_error = Some(crate::ui::app::RebindError::BadKey.text());
                cx.notify();
                return;
            }
        };
        let (map, presets) = {
            let a = self.app.read(cx);
            (a.shortcuts.clone(), a.quick.presets.clone())
        };
        let editing = (!self.is_new).then_some(self.p.id.as_str());
        match check_preset_chord(&map, &presets, editing, &chord) {
            Ok(()) => {
                self.p.shortcut = Some(chord.to_setting());
                self.key_error = None;
                self.end_key(cx);
            }
            // QC-4.AC5: refused, with the owner named.
            Err(owner) => {
                self.key_error = Some(owner.to_string());
                // The note sits under the shortcut row at the end of the body: bring it into view.
                self.scroll.scroll_to_bottom();
            }
        }
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).text().trim().to_owned();
        if name.is_empty() {
            self.error = Some(tr!("Give the preset a name."));
            cx.notify();
            return;
        }
        let mut p = self.p.clone();
        p.name = name;
        let folder = self.save_to.read(cx).text().trim().to_owned();
        p.save_to = (!folder.is_empty() && p.kind == PresetKind::Screenshot).then_some(folder);
        if !p.after.open_in_editor {
            p.after.tool = None;
        }
        let mut v = serde_json::to_value(&p).unwrap_or_default();
        if self.is_new {
            if let Some(o) = v.as_object_mut() {
                o.remove("id");
            }
        }
        let this = cx.entity();
        let _ = window;
        crate::ui::quick_flow::preset_call(
            &self.app,
            "save_preset",
            json!({ "preset": v }),
            move |_, res, cx| {
                this.update(cx, |this, cx| match res {
                    Ok(_) => this.finish(cx),
                    Err(e) => {
                        this.error = Some(e);
                        cx.notify();
                    }
                });
            },
            cx,
        );
    }

    fn delete(&mut self, cx: &mut Context<Self>) {
        if !self.confirm_delete {
            self.confirm_delete = true;
            cx.notify();
            return;
        }
        let this = cx.entity();
        crate::ui::quick_flow::preset_call(
            &self.app,
            "delete_preset",
            json!({ "preset": self.p.id }),
            move |_, res, cx| {
                this.update(cx, |this, cx| match res {
                    Ok(_) => this.finish(cx),
                    Err(e) => {
                        this.error = Some(e);
                        cx.notify();
                    }
                });
            },
            cx,
        );
    }

    /// The preset was saved or deleted: the sheet closes.
    fn finish(&mut self, cx: &mut Context<Self>) {
        self.end_key(cx);
        let handle = cx.try_global::<SheetSlot>().and_then(|s| s.0);
        cx.set_global(SheetSlot(None));
        // The window is closed from outside, after this update.
        if let Some(h) = handle {
            cx.defer(move |cx| crate::surfaces::close(cx, h));
        }
    }
}

// ---- drawing ----------------------------------------------------------------------------

fn section(t: &Theme, title: String) -> gpui::Div {
    div()
        .text_size(px(type_scale::CAPTION))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba_of(t.mist))
        .child(SharedString::from(title))
}

fn chip(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    label: String,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::RadioButton)
        .aria_label(SharedString::from(label.clone()))
        .aria_selected(on)
        .focusable()
        .tab_stop(true)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(rgba_of(if on { t.line } else { t.border }))
        .bg(rgba_of(if on { t.raised } else { t.surface_2 }))
        .text_size(px(type_scale::UI - 1.))
        .text_color(rgba_of(if on { t.text } else { t.text_2 }))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
        .child(SharedString::from(label))
}

impl PresetSheet {
    fn toggle_row(
        &self,
        t: &Theme,
        id: &'static str,
        label: String,
        on: bool,
        cx: &mut Context<Self>,
        set: impl Fn(&mut Preset, bool) + 'static,
    ) -> gpui::Div {
        div()
            .flex()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(px(type_scale::UI))
                    .child(SharedString::from(label.clone())),
            )
            .child(
                switch(t, id, label, on, true).on_click(cx.listener(move |this, _, _, cx| {
                    set(&mut this.p, !on);
                    cx.notify();
                })),
            )
    }

    fn toggles(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let (o, a) = (&self.p.options, &self.p.after);
        let col = div().flex().flex_col();
        match self.p.kind {
            PresetKind::Screenshot => col
                .child(self.toggle_row(
                    t,
                    "ps-copy",
                    tr_ctx!("preset sheet", "Copy to clipboard"),
                    a.copy_to_clipboard,
                    cx,
                    |p, v| p.after.copy_to_clipboard = v,
                ))
                .child(self.toggle_row(
                    t,
                    "ps-redact",
                    tr_ctx!("preset sheet", "Auto-redact"),
                    a.auto_redact,
                    cx,
                    |p, v| p.after.auto_redact = v,
                ))
                .child(self.toggle_row(
                    t,
                    "ps-editor",
                    tr_ctx!("preset sheet", "Open in editor"),
                    a.open_in_editor,
                    cx,
                    |p, v| {
                        p.after.open_in_editor = v;
                        if !v {
                            p.after.tool = None;
                        }
                    },
                )),
            PresetKind::Video => col
                .child(self.toggle_row(
                    t,
                    "ps-mic",
                    tr_ctx!("preset sheet", "Microphone"),
                    o.mic == Some(true),
                    cx,
                    |p, v| p.options.mic = Some(v),
                ))
                .child(self.toggle_row(
                    t,
                    "ps-system",
                    tr_ctx!("preset sheet", "System audio"),
                    o.system_audio == Some(true),
                    cx,
                    |p, v| p.options.system_audio = Some(v),
                ))
                .child(self.toggle_row(
                    t,
                    "ps-camera",
                    tr_ctx!("preset sheet", "Webcam"),
                    o.camera == Some(true),
                    cx,
                    |p, v| p.options.camera = Some(v),
                )),
            PresetKind::Audio => col
                .child(self.toggle_row(
                    t,
                    "ps-mic",
                    tr_ctx!("preset sheet", "Microphone"),
                    o.mic == Some(true),
                    cx,
                    |p, v| p.options.mic = Some(v),
                ))
                .child(self.toggle_row(
                    t,
                    "ps-system",
                    tr_ctx!("preset sheet", "System audio"),
                    o.system_audio == Some(true),
                    cx,
                    |p, v| p.options.system_audio = Some(v),
                ))
                .child(self.toggle_row(
                    t,
                    "ps-transcribe",
                    tr_ctx!("preset sheet", "Transcribe after"),
                    a.transcribe,
                    cx,
                    |p, v| p.after.transcribe = v,
                )),
        }
    }

    fn shortcut_row(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        let platform = self.app.read(cx).platform;
        let chord = chord_of(&self.p);
        let shown: gpui::AnyElement = if self.waiting_key {
            div()
                .text_size(px(type_scale::UI - 1.))
                .child(SharedString::from(tr!("Press the new shortcut")))
                .into_any_element()
        } else {
            match &chord {
                Some(c) => keycaps(t, &c.keycaps(platform)).into_any_element(),
                None => div()
                    .text_size(px(type_scale::UI - 1.))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(tr_ctx!("preset sheet", "Not set")))
                    .into_any_element(),
            }
        };
        let button = |id: &'static str, label: String| {
            crate::ui::settings::widgets::small_button(t, id, &label)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().flex_1().min_w(px(0.)).child(shown))
                    .child(
                        button("ps-key-change", tr_ctx!("button", "Change")).on_click(
                            cx.listener(|this, _, window, cx| this.begin_key(window, cx)),
                        ),
                    )
                    .children(chord.is_some().then(|| {
                        button("ps-key-clear", tr_ctx!("button", "Clear")).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.p.shortcut = None;
                                this.key_error = None;
                                cx.notify();
                            },
                        ))
                    })),
            )
            .children(self.key_error.clone().map(|e| {
                div()
                    .text_size(px(type_scale::UI - 1.))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(e))
            }))
    }
}

impl Render for PresetSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let kinds = crate::quick::presets::kinds().into_iter().map(|k| {
            chip(
                &t,
                format!("ps-kind-{}", k.as_str()),
                kind_label(k),
                self.p.kind == k,
            )
            .on_click(cx.listener(move |this, _, _, cx| this.set_kind(k, cx)))
        });
        let kinds: Vec<_> = kinds.collect();
        let targets: &[PresetTarget] = match self.p.kind {
            PresetKind::Screenshot => &SHOT_TARGETS,
            PresetKind::Video => &VIDEO_TARGETS,
            PresetKind::Audio => &[],
        };
        let target_chips = targets.iter().map(|&tg| {
            chip(
                &t,
                format!("ps-target-{}", tg.as_str()),
                target_label(tg),
                self.p.target == Some(tg),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.p.target = Some(tg);
                cx.notify();
            }))
        });
        let target_chips: Vec<_> = target_chips.collect();
        let tool_chips = (self.p.kind == PresetKind::Screenshot && self.p.after.open_in_editor)
            .then(|| {
                TOOLS
                    .iter()
                    .map(|&id| {
                        let label = crate::editor::Tool::from_id(id).map_or_else(
                            || tr_ctx!("preset sheet", "No tool"),
                            |tool| tool.label(),
                        );
                        let on = self.p.after.tool.as_deref().unwrap_or("") == id;
                        chip(&t, format!("ps-tool-{id}"), label, on).on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.p.after.tool = (!id.is_empty()).then(|| id.to_owned());
                                cx.notify();
                            },
                        ))
                    })
                    .collect::<Vec<_>>()
            });
        let list = div()
            .id("ps-body")
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(section(&t, tr_ctx!("preset sheet", "Name")))
            .child(div().flex_none().child(self.name.clone()))
            .child(section(&t, tr_ctx!("preset sheet", "Kind and target")))
            .child(div().flex().flex_wrap().gap(px(6.)).children(kinds))
            .children((!targets.is_empty()).then(|| {
                div().flex().flex_wrap().gap(px(6.)).children(target_chips)
            }))
            .child(section(&t, tr_ctx!("preset sheet", "Options")))
            .child(self.toggles(&t, cx))
            .children(tool_chips.map(|chips| {
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(section(&t, tr_ctx!("preset sheet", "Then open the tool")))
                    .child(div().flex().flex_wrap().gap(px(6.)).children(chips))
            }))
            .children((self.p.kind == PresetKind::Screenshot).then(|| {
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(section(&t, tr_ctx!("preset sheet", "Save to")))
                    .child(div().flex_none().child(self.save_to.clone()))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgba_of(t.text_3))
                            .child(SharedString::from(tr!(
                                "{date} becomes today\u{2019}s date. The capture is also in your library."
                            ))),
                    )
            }))
            .child(section(&t, tr_ctx!("preset sheet", "Shortcut")))
            .child(self.shortcut_row(&t, cx))
            .children(self.error.clone().map(|e| {
                div()
                    .text_size(px(type_scale::UI - 1.))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(e))
            }));
        // The body fills what the title and the footer leave and scrolls inside it, with a
        // scrollbar as the cue; the footer never moves over a field (QC-4.AC8).
        let body = crate::ui::scroll::fill_scroll(&t, &self.scroll, list, || {});
        let delete_label = if self.confirm_delete {
            tr_ctx!("button", "Delete for good")
        } else {
            tr_ctx!("button", "Delete")
        };
        let footer = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .children((!self.is_new).then(|| {
                crate::ui::components::button_with_icon(
                    &t,
                    "ps-delete",
                    delete_label,
                    crate::ui::components::ButtonKind::Danger,
                    Some("trash-2"),
                )
                .on_click(cx.listener(|this, _, _, cx| this.delete(cx)))
            }))
            .child(div().flex_1())
            .child(
                crate::ui::components::button(
                    &t,
                    "ps-cancel",
                    tr_ctx!("button", "Cancel"),
                    crate::ui::components::ButtonKind::Secondary,
                )
                .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
            .child(
                crate::ui::components::button(
                    &t,
                    "ps-save",
                    tr_ctx!("button", "Save preset"),
                    crate::ui::components::ButtonKind::Primary,
                )
                .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
            );
        div()
            .id("preset-sheet")
            .role(Role::Dialog)
            .aria_label(if self.is_new {
                tr_ctx!("preset sheet", "New preset")
            } else {
                tr_ctx!("preset sheet", "Edit preset")
            })
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::UI))
            .child(
                div()
                    .size_full()
                    .p(px(18.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .rounded(px(scale::RADIUS_DIALOG))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(type_scale::TITLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(if self.is_new {
                                tr_ctx!("preset sheet", "New preset")
                            } else {
                                tr_ctx!("preset sheet", "Edit preset")
                            })),
                    )
                    .child(body)
                    .child(footer),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// QC-4.AC8: a 1080p display at 150 % is 720 logical px tall (less the taskbar): the sheet
    /// fits with a margin, and the body keeps room for several rows between title and footer.
    #[test]
    fn the_sheet_fits_a_1080p_display_at_150_percent() {
        for visible in [720.0, 680.0, 672.0, 640.0] {
            let h = sheet_height(visible);
            assert!(h <= visible - 16.0, "{h} px on a {visible} px display");
            // title 28 + footer 36 + padding 36 + two gaps 28 leave the rest to the body.
            let body = h - 128.0;
            assert!(body >= 220.0, "body {body} px on a {visible} px display");
        }
        assert_eq!(sheet_height(1400.0), f64::from(HEIGHT));
        // A very short display: the sheet takes what is there, not more.
        assert!(sheet_height(300.0) <= 300.0);
    }
}
