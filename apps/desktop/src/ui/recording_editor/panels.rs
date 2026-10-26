//! The header, the preview column (picture, transport) and the inspector: a rail of nine icon
//! tabs and one panel card at a time (redesign R3, R3b, `UiInspectorTabs`). The Zoom, Cursor and
//! Reframe panels are in `view_cards`, Captions and Suggestions in `ai_cards`.

use cm_core::{BackgroundKind, BubbleKind, CameraLayoutSpec, CameraMode, LayoutTransition};
use cm_i18n::{tr, tr_ctx};
use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Role, SharedString};

use super::tabs::{
    hint, hint_box_keyed, micro, panel, tab_button, tab_rail, tile_choice, unavailable, Inspector,
    KEY_MARK,
};
use super::{Load, RecordingEditor, RowKind, Session};
use crate::recedit::camera::{self, Corner};
use crate::recedit::inspector::{audio_note, keep_audio_applies, BgSpec, Preset, SPEEDS, SWATCHES};
use crate::recedit::keys::Cmd;
use crate::recedit::model::timecode;
use crate::recedit::render::parse_hex;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::components::{button_with_icon, icon, switch, ButtonKind};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::{segment, segment_with_icon, segmented};

use super::tabs::field_row;

use crate::ai::caption_view::{inspector_width, INSPECTOR_MIN_W};

/// The glyph of a bubble corner.
fn corner_icon(c: Corner) -> &'static str {
    match c {
        Corner::TopLeft => "cm-corner-top-left",
        Corner::TopRight => "cm-corner-top-right",
        Corner::BottomLeft => "cm-corner-bottom-left",
        Corner::BottomRight => "cm-corner-bottom-right",
    }
}

/// The glyph of a camera layout.
fn mode_icon(m: CameraMode) -> &'static str {
    match m {
        CameraMode::Bubble => "cm-layout-bubble",
        CameraMode::Screen => "cm-layout-screen",
        CameraMode::Camera => "cm-layout-camera",
        CameraMode::SideBySide => "cm-layout-split",
    }
}

/// The visible name of a bubble size button ("S", "M", "L").
fn size_name(id: &str) -> String {
    match id {
        "S" => tr_ctx!("bubble size", "S"),
        "M" => tr_ctx!("bubble size", "M"),
        "L" => tr_ctx!("bubble size", "L"),
        other => other.to_owned(),
    }
}

type ClickFn = Box<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static>;

impl RecordingEditor {
    /// Everything under the header once the recording is loaded.
    pub(super) fn ready(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Load::Ready(s) = &self.load else {
            return div().into_any_element();
        };
        if s.voice.audio {
            return self.audio_ready(t, s, cx);
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.title_row(t, &s.title, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap(px(16.))
                    .px(px(24.))
                    .pt(px(16.))
                    .child(self.preview_column(t, s, cx))
                    .child(self.inspector(t, s, cx)),
            )
            .child(
                div()
                    .px(px(24.))
                    .py(px(12.))
                    .child(self.timeline_section(t, s, cx)),
            )
            .into_any_element()
    }

    /// Follows the timeline: a newly picked segment or zoom opens the matching tab (Q3), and
    /// "Open captions" from another screen opens the Captions tab.
    pub(super) fn sync_tab(&mut self, cx: &mut Context<Self>) {
        let wanted = self.app.read(cx).transcribe.captions_focus.clone();
        let open_captions = wanted.is_some() && wanted == self.id;
        if open_captions {
            self.tab = super::tabs::next_tab(self.tab, true, false, false, false);
            let app = self.app.clone();
            cx.defer(move |cx| {
                app.update(cx, |m, _| m.transcribe.captions_focus = None);
            });
        }
        let Load::Ready(s) = &self.load else { return };
        let has_camera = s.rows.iter().any(|r| r.kind == RowKind::Camera);
        let seg = matches!(
            s.model.selection,
            crate::recedit::Selection::Segment(_) | crate::recedit::Selection::Range(_)
        );
        let seg_changed = seg && Some(s.model.selection) != self.last_selection;
        let zoom_changed =
            s.model.zoom_selected.is_some() && s.model.zoom_selected != self.last_zoom;
        self.last_selection = Some(s.model.selection);
        self.last_zoom = s.model.zoom_selected;
        self.tab = super::tabs::next_tab(self.tab, false, seg_changed, zoom_changed, has_camera);
        if s.voice.audio && self.tab == Inspector::Captions {
            self.tab = Inspector::Transcript;
        }
    }

    /// The burn-in choice the Output panel shows: `None` when the recording has no transcript.
    pub(super) fn burn_choice(&self, cx: &mut Context<Self>) -> Option<bool> {
        let id = self.id.clone().unwrap_or_default();
        self.ai.has_transcript().then(|| {
            self.app
                .read(cx)
                .transcribe
                .export_burn(&id, self.ai.burn_in_default())
        })
    }

    pub(super) fn export_click(&self, burn: Option<bool>) -> ClickFn {
        let (app, id) = (self.app.clone(), self.id.clone().unwrap_or_default());
        Box::new(move |_, _, cx| {
            let (app, id) = (app.clone(), id.clone());
            cx.defer(move |cx| crate::ui::export_ui::request(&app, &id, burn, cx));
        })
    }

    pub(super) fn publish_click(&self) -> ClickFn {
        let (app, id) = (self.app.clone(), self.id.clone().unwrap_or_default());
        Box::new(move |_, _, cx| {
            let (app, id) = (app.clone(), id.clone());
            cx.defer(move |cx| crate::ui::publish_sheet::open(&app, &id, cx));
        })
    }

    fn preview_column(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let m = &s.model;
        let picture = self.presenter.element();
        let waiting = picture.is_none();
        let frame = div()
            .id("re-preview")
            .role(Role::Image)
            .aria_label(tr!("Preview of the edited recording"))
            .flex_1()
            .min_h(px(0.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.ink))
            .overflow_hidden()
            .children(picture)
            .when(waiting, |d| {
                d.child(
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.text_3))
                        .child(tr!("Preparing the preview\u{2026}")),
                )
            });

        let cur = timecode(m.output_position(), s.fps);
        let total = timecode(m.output_duration(), s.fps);
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
            .size(px(scale::CONTROL_H))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(scale::CONTROL_H / 2.))
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
                18.0,
                t.primary_fg,
            ))
            .on_click(cx.listener(|this, _, _, cx| this.run(Cmd::PlayPause, cx)));

        // The renderer label sits by the timecode; the warnings go under the row.
        let (label, warnings): (Option<String>, Vec<String>) = {
            let warn = [
                s.source_note.clone(),
                s.audio_note.clone(),
                s.preview_error.clone(),
                s.plain.then(|| {
                    tr!(
                        "No GPU adapter: the preview shows the screen without the background or the \
                         camera bubble."
                    )
                }),
            ];
            (
                Some(s.renderer.label().to_owned()),
                warn.into_iter().flatten().collect(),
            )
        };
        let transport = div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(play)
            .child(
                div()
                    .mono()
                    .tnum()
                    .text_size(px(17.))
                    .flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_color(rgba_of(t.text))
                            .child(SharedString::from(cur)),
                    )
                    .child(div().text_color(rgba_of(t.mist)).child("/"))
                    .child(
                        div()
                            .text_color(rgba_of(t.mist))
                            .child(SharedString::from(total)),
                    ),
            )
            .children(label.map(|l| {
                div()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(l))
            }))
            .child(div().flex_1())
            .child(
                div()
                    .mono()
                    .flex_none()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(s.stats.clone())),
            );

        div()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(frame)
            .child(transport)
            .children((!warnings.is_empty()).then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.warning))
                    .children(
                        warnings
                            .into_iter()
                            .map(|n| div().child(SharedString::from(n))),
                    )
            }))
            .into_any_element()
    }

    // ---- the header ----------------------------------------------------------------------

    pub(super) fn back_button(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.id.clone().unwrap_or_default();
        let app = self.app.clone();
        let _ = cx;
        button_with_icon(
            t,
            "re-back",
            tr!("Recording details"),
            ButtonKind::Ghost,
            Some("info"),
        )
        .on_click(move |_, _, cx| {
            let (app, id) = (app.clone(), id.clone());
            cx.defer(move |cx| {
                crate::ui::app::set_route(&app, crate::ui::app::Route::Recording(id), cx)
            });
        })
    }

    /// Crumb "‹ Recordings" (aria "Back to recordings").
    fn crumb(&self, t: &Theme) -> impl IntoElement {
        let app = self.app.clone();
        let (hover_fg, fg, focus) = (rgba_of(t.text), rgba_of(t.mist), rgba_of(t.text));
        div()
            .id("re-crumb")
            .role(Role::Button)
            .aria_label(tr!("Back to recordings"))
            .focusable()
            .tab_stop(true)
            .self_start()
            .flex()
            .items_center()
            .gap(px(4.))
            .border_1()
            .border_color(gpui::transparent_black())
            .rounded(px(scale::RADIUS_XS))
            .cursor_pointer()
            .text_size(px(type_scale::CAPTION))
            .text_color(fg)
            .hover(move |s| s.text_color(hover_fg))
            .focus_visible(move |s| s.border_color(focus))
            .child(icon("chevron-left", 14., t.mist))
            .child(tr_ctx!("breadcrumb", "Recordings"))
            .on_click(move |_, _, cx| {
                let app = app.clone();
                cx.defer(move |cx| {
                    crate::ui::app::set_route(&app, crate::ui::app::Route::Recordings, cx)
                });
            })
    }

    pub(super) fn title_row(
        &self,
        t: &Theme,
        title: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Export MP4 uses the Output panel's burn-in choice (Q1).
        let burn = self.burn_choice(cx);
        let export = button_with_icon(
            t,
            "re-export-top",
            tr!("Export MP4"),
            ButtonKind::Secondary,
            Some("file-output"),
        )
        .on_click(self.export_click(burn));
        let publish = button_with_icon(
            t,
            "re-publish-top",
            tr!("Publish\u{2026}"),
            ButtonKind::Primary,
            Some("send"),
        )
        .on_click(self.publish_click());
        crate::ui::components::page_toolbar_actions(
            t,
            "recording-editor-toolbar",
            tr!("Recording editor toolbar"),
            self.crumb(t),
            SharedString::from(title.to_owned()),
            vec![
                self.back_button(t, cx).into_any_element(),
                export.into_any_element(),
                publish.into_any_element(),
            ],
        )
    }

    // ---- the inspector -------------------------------------------------------------------

    fn do_speed(&mut self, factor: f64, keep: bool, cx: &mut Context<Self>) {
        let Some(s) = self.session() else { return };
        let reqs = s.model.set_speed(factor, keep);
        self.send(reqs, cx);
    }

    /// CAM-3: one change of the camera layout of the target segment.
    fn do_camera(
        &mut self,
        f: impl FnOnce(
            &CameraLayoutSpec,
            Option<&cm_media::layout::CameraLayout>,
            f64,
        ) -> CameraLayoutSpec,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.session() else { return };
        let Some(cur) = s.model.camera_spec() else {
            let reqs = s
                .model
                .set_camera_layout(CameraLayoutSpec::new(CameraMode::Bubble));
            self.send(reqs, cx);
            return;
        };
        let (w, h) = s.renderer.size();
        let base = s.model.camera_base();
        let next = f(&cur, base.as_ref(), f64::from(w) / f64::from(h.max(1)));
        let reqs = s.model.set_camera_layout(next);
        self.send(reqs, cx);
    }

    fn do_background(&mut self, f: impl FnOnce(&BgSpec) -> BgSpec, cx: &mut Context<Self>) {
        let Some(s) = self.session() else { return };
        let next = f(&s.model.background());
        let reqs = s.model.set_background(&next);
        self.send(reqs, cx);
    }

    /// The rail and the open panel.
    pub(super) fn inspector(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let has_camera = s.rows.iter().any(|r| r.kind == RowKind::Camera);
        let tabs: Vec<AnyElement> = Inspector::set(s.voice.audio)
            .iter()
            .map(|&tab| {
                tab_button(
                    t,
                    tab,
                    tab == self.tab,
                    unavailable(tab, has_camera, s.cursor_separate),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tab = tab;
                    cx.notify();
                }))
                .into_any_element()
            })
            .collect();
        let open = match self.tab {
            Inspector::Speed => self.speed_panel(t, s, cx),
            Inspector::Camera => self.camera_panel(t, s, cx),
            Inspector::Zoom => self.zoom_panel(t, s, cx),
            Inspector::Cursor => self.cursor_panel(t, s, cx),
            Inspector::Reframe => self.reframe_panel(t, s, cx),
            Inspector::Background => self.background_panel(t, s, cx),
            Inspector::Captions => self.captions_panel(t, s, cx),
            Inspector::Transcript => self.transcript_panel(t, s, cx),
            Inspector::Speakers => self.speakers_panel(t, s, cx),
            Inspector::Report => self.report_panel(t, s, cx),
            Inspector::Suggestions => self.suggest_panel(t, s, cx),
            Inspector::Output => self.output_panel(t, s, cx),
        };
        div()
            .id("re-inspector")
            // 400 px (board: 380 to 440 at 1440), 600 px on the Captions tab when the window
            // leaves the preview 360 px; it shrinks to `INSPECTOR_MIN_W` before the preview does.
            .w(px(inspector_width(
                self.tab.is_wide(),
                self.ai.view.window_w.get(),
            )))
            .min_w(px(INSPECTOR_MIN_W))
            .flex_shrink(1.)
            .h_full()
            .flex()
            .gap(px(10.))
            .pb(px(4.))
            .child(tab_rail(t, tabs))
            .child(open)
            .into_any_element()
    }

    fn speed_panel(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let m = &s.model;
        let chosen = m.selected_speed();
        let has_target = matches!(
            m.selection,
            crate::recedit::Selection::Segment(_) | crate::recedit::Selection::Range(_)
        );
        let mut row = segmented(t, "re-speeds", &tr!("Playback speed"));
        for f in SPEEDS {
            let active = chosen.is_some_and(|(c, _)| (c - f).abs() < 1e-9);
            let keep = chosen.is_some_and(|(_, k)| k);
            let label = crate::recedit::geom::speed_label(f);
            let seg = segment(
                t,
                SharedString::from(format!("re-speed-{f}")),
                &label,
                active,
            )
            .px(px(8.))
            .min_w(px(40.));
            row = row.child(if has_target {
                seg.on_click(cx.listener(move |this, _, _, cx| this.do_speed(f, keep, cx)))
                    .into_any_element()
            } else {
                seg.opacity(0.45).cursor_default().into_any_element()
            });
        }
        let note: SharedString = match chosen {
            Some((f, k)) => audio_note(f, k).into(),
            None if has_target => tr!("Pick a speed for the selected range.").into(),
            None => tr!("Select a segment on the timeline to change its speed.").into(),
        };
        let mut body: Vec<AnyElement> = vec![div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(micro(t, tr!("Playback speed")))
            .child(row)
            .into_any_element()];
        if let Some((f, k)) = chosen.filter(|(f, _)| keep_audio_applies(*f)) {
            body.push(
                field_row(
                    t,
                    "volume-2",
                    tr!("Keep audio"),
                    None,
                    switch(t, "re-keep-audio", tr!("Keep audio"), k, true)
                        .on_click(cx.listener(move |this, _, _, cx| this.do_speed(f, !k, cx))),
                )
                .into_any_element(),
            );
        }
        body.push(hint(t, "info", note).into_any_element());
        panel(
            t,
            Inspector::Speed,
            tr!("Selected segment or range"),
            body,
            Vec::new(),
        )
    }

    /// A stepper row: a glyph, the label, and `[-] value [+]`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn stepper(
        t: &Theme,
        id: &'static str,
        glyph: &'static str,
        label: String,
        (decrease, increase): (String, String),
        value: String,
        minus: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        plus: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let step = |suffix: &str, g: &'static str, aria: String| {
            let hover = rgba_of(t.raised);
            let focus = rgba_of(t.text);
            div()
                .id(SharedString::from(format!("{id}-{suffix}")))
                .role(Role::Button)
                .aria_label(SharedString::from(aria))
                .focusable()
                .tab_stop(true)
                .size(px(32.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_SM))
                .border_1()
                .border_color(gpui::transparent_black())
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .focus_visible(move |s| s.border_color(focus))
                .child(icon(g, 14., t.text_2))
        };
        let control = div()
            .flex()
            .items_center()
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.field))
            .child(
                step("minus", "minus", decrease)
                    .on_click(cx.listener(move |this, _, _, cx| minus(this, cx))),
            )
            .child(
                div()
                    .min_w(px(56.))
                    .flex()
                    .justify_center()
                    .mono()
                    .tnum()
                    .text_size(px(13.))
                    .child(SharedString::from(value)),
            )
            .child(
                step("plus", "plus", increase)
                    .on_click(cx.listener(move |this, _, _, cx| plus(this, cx))),
            );
        field_row(t, glyph, label, None, control).into_any_element()
    }

    fn background_panel(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let cur = s.model.background();
        let which = cur.which();
        let presets = Preset::ALL.map(|p| {
            let active = which == p;
            segment(
                t,
                SharedString::from(format!("re-bg-{}", p.label())),
                &p.label(),
                active,
            )
            .w_full()
            .justify_start()
            .on_click(
                cx.listener(move |this, _, _, cx| this.do_background(|c| BgSpec::preset(p, c), cx)),
            )
        });
        let swatches = (which == Preset::Custom && cur.kind == BackgroundKind::Color).then(|| {
            let current = cur.value.as_str().unwrap_or_default().to_owned();
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .children(SWATCHES.map(|hex| {
                    let active = current.eq_ignore_ascii_case(hex);
                    let rgb = parse_hex(hex).unwrap_or([0, 0, 0]);
                    div()
                        .id(SharedString::from(format!("re-swatch-{hex}")))
                        .role(Role::RadioButton)
                        .aria_label(SharedString::from(tr!("Background color {hex}", hex = hex)))
                        .focusable()
                        .tab_stop(true)
                        .size(px(28.))
                        .rounded(px(14.))
                        .border_2()
                        .border_color(rgba_of(if active { t.text } else { t.line }))
                        .bg(gpui::rgb(
                            (u32::from(rgb[0]) << 16)
                                | (u32::from(rgb[1]) << 8)
                                | u32::from(rgb[2]),
                        ))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.do_background(|c| c.with_color(hex), cx)
                        }))
                }))
        });
        let on = cur.kind != BackgroundKind::None;
        let mut body: Vec<AnyElement> = vec![div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(micro(t, tr_ctx!("background", "Preset")))
            .child(
                div()
                    .id("re-presets")
                    .role(Role::RadioGroup)
                    .aria_label(tr!("Background preset"))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(presets),
            )
            .into_any_element()];
        body.extend(swatches.map(IntoElement::into_any_element));
        if on {
            body.push(Self::stepper(
                t,
                "re-padding",
                "maximize",
                tr!("Padding"),
                (tr!("Decrease padding"), tr!("Increase padding")),
                format!("{:.0}%", cur.padding_pct),
                |this, cx| this.do_background(|c| c.with_padding(c.padding_pct - 1.0), cx),
                |this, cx| this.do_background(|c| c.with_padding(c.padding_pct + 1.0), cx),
                cx,
            ));
            body.push(Self::stepper(
                t,
                "re-radius",
                "square",
                tr!("Corner radius"),
                (tr!("Decrease corner radius"), tr!("Increase corner radius")),
                format!("{} px", cur.radius_px),
                |this, cx| this.do_background(|c| c.with_radius(c.radius_px.saturating_sub(2)), cx),
                |this, cx| this.do_background(|c| c.with_radius(c.radius_px + 2), cx),
                cx,
            ));
            let sh = cur.shadow;
            body.push(
                field_row(
                    t,
                    "droplet",
                    tr!("Shadow"),
                    None,
                    switch(t, "re-shadow", tr!("Shadow"), sh, true).on_click(cx.listener(
                        move |this, _, _, cx| this.do_background(|c| c.with_shadow(!sh), cx),
                    )),
                )
                .into_any_element(),
            );
        }
        panel(
            t,
            Inspector::Background,
            tr!("Whole recording"),
            body,
            Vec::new(),
        )
    }

    /// CAM-3: the camera layout of the selected segment (or the one under the playhead): screen
    /// with the bubble, screen only, camera full, side by side, and for the bubble its corner,
    /// size, shape and ring. A recording without a camera track has none of it.
    fn camera_panel(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let has_camera = s.rows.iter().any(|r| r.kind == RowKind::Camera);
        if !has_camera {
            return panel(
                t,
                Inspector::Camera,
                tr!("Not available for this recording"),
                vec![
                    hint(t, "video-off", tr!("This recording has no camera track."))
                        .into_any_element(),
                ],
                Vec::new(),
            );
        }
        let m = &s.model;
        let spec = m.camera_spec();
        let target = m.layout_target();
        let base = m.camera_base();
        let on = spec.is_some() && target.is_some();
        let cur = spec
            .clone()
            .unwrap_or_else(|| CameraLayoutSpec::new(CameraMode::Bubble));
        let eff = camera::effective(&cur, base.as_ref());

        // The four layouts as 2x2 tiles with their glyphs.
        let mut modes = div()
            .id("re-cam-modes")
            .role(Role::RadioGroup)
            .aria_label(tr!("Camera layout"))
            .grid()
            .grid_cols(2)
            .gap(px(8.));
        for mode in camera::MODES {
            let label = camera::mode_label(mode);
            let active = on && cur.mode == mode;
            let tile = tile_choice(
                t,
                SharedString::from(format!("re-cam-{}", mode.as_str())),
                &label,
                active,
            )
            .h(px(52.))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(10.))
            .text_size(px(type_scale::UI))
            .child(icon(
                mode_icon(mode),
                20.,
                if active { t.text } else { t.text_2 },
            ))
            // A long translation wraps inside the tile instead of running into the next one.
            .child(div().flex_1().min_w(px(0.)).child(label));
            modes = modes.child(if on {
                tile.on_click(cx.listener(move |this, _, _, cx| {
                    this.do_camera(|c, _, _| camera::with_mode(c, mode), cx)
                }))
                .into_any_element()
            } else {
                tile.opacity(0.45).cursor_default().into_any_element()
            });
        }

        let bubble = (on && cur.mode == CameraMode::Bubble).then(|| {
            let corner = camera::corner_of(eff.rect);
            let mut corners = div()
                .id("re-cam-corners")
                .role(Role::RadioGroup)
                .aria_label(tr!("Bubble position"))
                .grid()
                .grid_cols(2)
                .gap(px(6.));
            for c in Corner::ALL {
                corners = corners.child(
                    tile_choice(
                        t,
                        SharedString::from(format!("re-cam-corner-{}", c.label())),
                        &c.label(),
                        corner == c,
                    )
                    .size(px(44.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .tooltip(crate::ui::tooltip::tip(c.label(), Vec::new()))
                    .child(icon(
                        corner_icon(c),
                        18.,
                        if corner == c { t.text } else { t.text_2 },
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.do_camera(|s, b, a| camera::with_corner(s, b, c, a), cx)
                    })),
                );
            }
            let mut sizes = segmented(t, "re-cam-sizes", &tr!("Bubble size"));
            for (label, w) in camera::SIZES {
                sizes = sizes.child(
                    segment(
                        t,
                        SharedString::from(format!("re-cam-size-{label}")),
                        &size_name(label),
                        camera::size_label(eff.rect[2]) == Some(label),
                    )
                    .flex_1()
                    .px(px(10.))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.do_camera(|s, b, a| camera::with_size(s, b, w, a), cx)
                    })),
                );
            }
            let mut shapes = segmented(t, "re-cam-shapes", &tr!("Bubble shape"));
            for (kind, glyph) in [
                (BubbleKind::Circle, "circle"),
                (BubbleKind::Rounded, "square"),
            ] {
                let label = match kind {
                    BubbleKind::Circle => tr_ctx!("bubble shape", "Circle"),
                    _ => tr_ctx!("bubble shape", "Rounded"),
                };
                shapes = shapes.child(
                    segment_with_icon(
                        t,
                        SharedString::from(format!("re-cam-shape-{label}")),
                        &label,
                        eff.shape == kind,
                        Some(glyph),
                    )
                    .flex_1()
                    .px(px(10.))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.do_camera(|s, b, a| camera::with_shape(s, b, kind, a), cx)
                    })),
                );
            }
            let ring_on = eff.ring > 0.0;
            let ring = field_row(
                t,
                "circle-dot",
                tr_ctx!("camera bubble", "Ring"),
                None,
                switch(
                    t,
                    "re-cam-ring",
                    tr_ctx!("camera bubble", "Ring"),
                    ring_on,
                    true,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.do_camera(|s, _, _| camera::with_ring(s, !ring_on), cx)
                })),
            );
            (
                div()
                    .flex()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .flex_none()
                            .child(micro(t, tr_ctx!("camera bubble", "Position")))
                            .child(corners),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(micro(t, tr_ctx!("camera bubble", "Size")))
                            .child(sizes)
                            .child(micro(t, tr_ctx!("camera bubble", "Shape")))
                            .child(shapes),
                    ),
                ring,
            )
        });
        let mirror_on = eff.mirror;
        let smooth_on = cur.transition == LayoutTransition::Smooth;
        let mut body: Vec<AnyElement> = vec![div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(micro(t, tr_ctx!("camera", "Layout")))
            .child(modes)
            .into_any_element()];
        if let Some((pos, ring)) = bubble {
            body.push(pos.into_any_element());
            body.push(ring.into_any_element());
        }
        if on {
            body.push(
                field_row(
                    t,
                    "flip-horizontal-2",
                    tr!("Mirror camera"),
                    None,
                    switch(t, "re-cam-mirror", tr!("Mirror camera"), mirror_on, true).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.do_camera(|s, _, _| camera::with_mirror(s, !mirror_on), cx)
                        }),
                    ),
                )
                .into_any_element(),
            );
            body.push(
                field_row(
                    t,
                    "blend",
                    tr!("Smooth move (300 ms)"),
                    None,
                    switch(t, "re-cam-smooth", tr!("Smooth move"), smooth_on, true).on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.do_camera(|s, _, _| camera::with_smooth(s, !smooth_on), cx)
                        }),
                    ),
                )
                .into_any_element(),
            );
            body.push(
                hint_box_keyed(
                    t,
                    "cm-split",
                    tr!(
                        "Split the timeline {key} to set a different layout for each part.",
                        key = KEY_MARK
                    ),
                    "S",
                )
                .into_any_element(),
            );
        } else {
            body.push(
                hint(
                    t,
                    "cm-split",
                    tr!("Move the playhead into a kept segment to change its camera layout."),
                )
                .into_any_element(),
            );
        }
        panel(
            t,
            Inspector::Camera,
            tr!("Selected segment, or the one under the playhead"),
            body,
            Vec::new(),
        )
    }

    fn output_panel(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        if s.voice.audio {
            return self.audio_output_panel(t, s, cx);
        }
        let m = &s.model;
        let kept = m.output_duration();
        let id = self.id.clone().unwrap_or_default();
        // "Burn in captions" for this export: only when the recording has a transcript; it starts
        // at the recording's own setting (the Captions panel's "Include in exported video").
        let burn = self.burn_choice(cx);
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
                    .child(SharedString::from(timecode(kept, s.fps))),
            );
        // Design gap: the board draws "The MP4 applies" as chips (Trims, Cuts, Speed,
        // Background, Captions on/off). Which edits are non-default is new derived UI, so the
        // sentence of the shipped build stays.
        let mut body: Vec<AnyElement> = vec![
            length.into_any_element(),
            hint(
                t,
                "info",
                tr!(
                    "The MP4 export applies trims, cuts, speed, the background and, if you choose, \
                     the captions."
                ),
            )
            .into_any_element(),
        ];
        if let Some(on) = burn {
            let (app, id2) = (self.app.clone(), id.clone());
            body.push(
                field_row(
                    t,
                    "captions",
                    tr!("Burn in captions"),
                    None,
                    switch(
                        t,
                        "re-export-burn",
                        tr!("Burn captions into the exported video"),
                        on,
                        true,
                    )
                    .on_click(move |_, _, cx| {
                        let (app, id) = (app.clone(), id2.clone());
                        cx.defer(move |cx| crate::ui::ai::set_export_burn(&app, &id, !on, cx));
                    }),
                )
                .into_any_element(),
            );
        }
        // VOC-2: the sound of a screen recording on its own, with the same cuts (V7).
        if s.voice.available().any() {
            body.push(
                button_with_icon(
                    t,
                    "re-export-audio-only",
                    tr!("Export audio only"),
                    ButtonKind::Secondary,
                    Some("file-headphone"),
                )
                .on_click(cx.listener(|this, _, _, cx| this.open_export_sheet(cx)))
                .into_any_element(),
            );
        }
        body.push(
            hint(
                t,
                "lock",
                tr!("Edits are saved with the recording and never change the original tracks."),
            )
            .into_any_element(),
        );
        let export = button_with_icon(
            t,
            "re-export",
            tr!("Export MP4"),
            ButtonKind::Secondary,
            Some("file-output"),
        )
        .flex_1()
        .on_click(self.export_click(burn));
        let publish = button_with_icon(
            t,
            "re-publish",
            tr!("Publish\u{2026}"),
            ButtonKind::Primary,
            Some("send"),
        )
        .flex_1()
        .on_click(self.publish_click());
        panel(
            t,
            Inspector::Output,
            tr!("What Export MP4 and Publish\u{2026} will produce"),
            body,
            vec![export.into_any_element(), publish.into_any_element()],
        )
    }
}
