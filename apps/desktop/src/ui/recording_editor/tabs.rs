//! The inspector's tab rail and the pieces every panel card is made of (redesign R3, R3b and
//! `UiInspectorTabs`): one panel at a time, picked from a vertical rail of nine icon tabs.
//!
//! The pure parts (tab order, which tab a timeline selection opens, which tabs are unavailable)
//! are unit tested here; the drawing helpers are the building blocks of `panels`, `view_cards`
//! and `ai_cards`.

use cm_core::ZOOM_EASE;
use cm_i18n::{tr, tr_ctx};
use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, Bounds, Div, FontWeight, Hsla, PathBuilder,
    Pixels, Role, SharedString, Stateful,
};

use crate::theme::{scale, type_scale, Theme};
use crate::ui::components::{icon, icon_size, shrink_text};
use crate::ui::paint::{rgba_of, TextExt};

/// The inspector tabs (`UiInspectorTabs`): [`Inspector::VIDEO`] is the rail of a screen recording,
/// [`Inspector::AUDIO`] the one of an audio recording (VOC-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Inspector {
    Speed,
    Camera,
    Zoom,
    Cursor,
    Reframe,
    Background,
    Captions,
    /// Audio recordings only: the transcript with its speakers (the Captions tab of a video).
    Transcript,
    Speakers,
    Report,
    Suggestions,
    Output,
}

impl Inspector {
    /// Every tab (for checks over all of them).
    #[cfg(test)]
    pub const ALL: [Inspector; 12] = [
        Inspector::Speed,
        Inspector::Camera,
        Inspector::Zoom,
        Inspector::Cursor,
        Inspector::Reframe,
        Inspector::Background,
        Inspector::Captions,
        Inspector::Transcript,
        Inspector::Speakers,
        Inspector::Report,
        Inspector::Suggestions,
        Inspector::Output,
    ];

    /// The rail of a screen recording (board `set="video"`, 11 tabs).
    pub const VIDEO: [Inspector; 11] = [
        Inspector::Speed,
        Inspector::Camera,
        Inspector::Zoom,
        Inspector::Cursor,
        Inspector::Reframe,
        Inspector::Background,
        Inspector::Captions,
        Inspector::Speakers,
        Inspector::Report,
        Inspector::Suggestions,
        Inspector::Output,
    ];

    /// The rail of an audio recording (board `set="audio"`, 6 tabs).
    pub const AUDIO: [Inspector; 6] = [
        Inspector::Speed,
        Inspector::Transcript,
        Inspector::Speakers,
        Inspector::Report,
        Inspector::Suggestions,
        Inspector::Output,
    ];

    /// The rail for a recording of the given kind.
    pub fn set(audio: bool) -> &'static [Inspector] {
        if audio {
            &Self::AUDIO
        } else {
            &Self::VIDEO
        }
    }

    /// A tab that is a heavy text editor on the whole transcript or a report: it gets the wide
    /// inspector (R3c, V4; the Report tab too, setup and report alike, maintainer Oct 27, 2026).
    pub fn is_wide(self) -> bool {
        matches!(
            self,
            Inspector::Captions | Inspector::Transcript | Inspector::Speakers | Inspector::Report
        )
    }

    /// The tab's name (its tooltip, its accessible name and the panel's title).
    pub fn label(self) -> String {
        match self {
            Inspector::Speed => tr_ctx!("inspector tab", "Speed"),
            Inspector::Camera => tr_ctx!("inspector tab", "Camera"),
            Inspector::Zoom => tr_ctx!("inspector tab", "Zoom"),
            Inspector::Cursor => tr_ctx!("inspector tab", "Cursor"),
            Inspector::Reframe => tr_ctx!("inspector tab", "Reframe"),
            Inspector::Background => tr_ctx!("inspector tab", "Background"),
            Inspector::Captions => tr_ctx!("inspector tab", "Captions"),
            Inspector::Transcript => tr_ctx!("inspector tab", "Transcript"),
            Inspector::Speakers => tr_ctx!("inspector tab", "Speakers"),
            Inspector::Report => tr_ctx!("inspector tab", "Report"),
            Inspector::Suggestions => tr_ctx!("inspector tab", "Suggestions"),
            Inspector::Output => tr_ctx!("inspector tab", "Output"),
        }
    }

    /// The board's glyph for the tab.
    pub fn icon(self) -> &'static str {
        match self {
            Inspector::Speed => "gauge",
            Inspector::Camera => "webcam",
            Inspector::Zoom => "zoom-in",
            Inspector::Cursor => "mouse-pointer-2",
            Inspector::Reframe => "ratio",
            Inspector::Background => "wallpaper",
            Inspector::Captions => "captions",
            Inspector::Transcript => "captions",
            Inspector::Speakers => "users",
            Inspector::Report => "file-text",
            Inspector::Suggestions => "sparkles",
            Inspector::Output => "file-output",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Inspector::Speed => "re-tab-speed",
            Inspector::Camera => "re-tab-camera",
            Inspector::Zoom => "re-tab-zoom",
            Inspector::Cursor => "re-tab-cursor",
            Inspector::Reframe => "re-tab-reframe",
            Inspector::Background => "re-tab-background",
            Inspector::Captions => "re-tab-captions",
            Inspector::Transcript => "re-tab-transcript",
            Inspector::Speakers => "re-tab-speakers",
            Inspector::Report => "re-tab-report",
            Inspector::Suggestions => "re-tab-suggestions",
            Inspector::Output => "re-tab-output",
        }
    }
}

/// What the user just selected on the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Picked {
    /// A kept segment or a range.
    Segment,
    /// A zoom chip on the zoom lane.
    Zoom,
}

/// The tab to show after the user picked something on the timeline (Q3). A zoom chip always opens
/// Zoom. A segment opens Camera (Speed when the recording has no camera track) unless the user
/// is already on Speed or Camera, and it never pulls the user off Captions, Suggestions or
/// Output: those are worked on while clicking around the timeline to seek.
pub(crate) fn auto_tab(current: Inspector, picked: Picked, has_camera: bool) -> Inspector {
    match picked {
        Picked::Zoom => Inspector::Zoom,
        Picked::Segment => match current {
            Inspector::Speed | Inspector::Camera => current,
            Inspector::Captions
            | Inspector::Transcript
            | Inspector::Speakers
            | Inspector::Report
            | Inspector::Suggestions
            | Inspector::Output => current,
            Inspector::Zoom | Inspector::Cursor | Inspector::Reframe | Inspector::Background => {
                if has_camera {
                    Inspector::Camera
                } else {
                    Inspector::Speed
                }
            }
        },
    }
}

/// The tab for the next frame: "Open captions" from another screen wins, then a zoom pick, then a
/// segment pick; otherwise the tab stays.
pub(crate) fn next_tab(
    current: Inspector,
    open_captions: bool,
    segment_picked: bool,
    zoom_picked: bool,
    has_camera: bool,
) -> Inspector {
    if open_captions {
        Inspector::Captions
    } else if zoom_picked {
        auto_tab(current, Picked::Zoom, has_camera)
    } else if segment_picked {
        auto_tab(current, Picked::Segment, has_camera)
    } else {
        current
    }
}

/// A tab whose card has nothing to edit for this recording (Q4): dimmed, still selectable, and the
/// panel says why.
pub(crate) fn unavailable(tab: Inspector, has_camera: bool, cursor_separate: bool) -> bool {
    match tab {
        Inspector::Camera => !has_camera,
        Inspector::Cursor => !cursor_separate,
        _ => false,
    }
}

// ---- the rail -----------------------------------------------------------------------------

/// Width of the tab rail, px (board: 54).
pub const RAIL_W: f32 = 54.;
const TILE: f32 = 40.;
const RAIL_PAD: f32 = 6.;

/// One tab: a 40 px tile with the glyph, the red 3 px indicator on the left when active. The
/// caller adds `.on_click`.
pub(crate) fn tab_button(
    t: &Theme,
    tab: Inspector,
    active: bool,
    unavailable: bool,
) -> Stateful<Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(tab.id())
        .role(Role::Tab)
        .aria_label(tab.label())
        .aria_selected(active)
        .when(unavailable, |d| {
            d.aria_description(tr!("Not available for this recording"))
        })
        .focusable()
        .tab_stop(true)
        .relative()
        .size(px(TILE))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(gpui::transparent_black())
        .cursor_pointer()
        .when(unavailable, |d| d.opacity(0.45))
        .when(active, |d| d.bg(rgba_of(t.raised)))
        .when(!active, |d| d.hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
        .tooltip(crate::ui::tooltip::tip(tab.label(), Vec::new()))
        .when(active, |d| {
            d.child(
                div()
                    .absolute()
                    .left(px(-RAIL_PAD - 1.))
                    .top(px(10.))
                    .w(px(3.))
                    .h(px(20.))
                    .rounded(px(2.))
                    .bg(rgba_of(t.rec)),
            )
        })
        .child(icon(
            tab.icon(),
            icon_size::ICON_BUTTON,
            if active { t.text } else { t.text_3 },
        ))
}

/// The vertical rail around the tabs.
pub(crate) fn tab_rail(t: &Theme, tabs: Vec<AnyElement>) -> Stateful<Div> {
    div()
        .id("re-tabs")
        .role(Role::TabList)
        .aria_label(tr!("Inspector tabs"))
        .aria_orientation(gpui::accesskit::Orientation::Vertical)
        .flex_none()
        .w(px(RAIL_W))
        .self_start()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .p(px(RAIL_PAD))
        .rounded(px(scale::RADIUS_XL))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.surface))
        .children(tabs)
}

// ---- the panel card ------------------------------------------------------------------------

/// A panel card: title 16/600 and a scope line, the controls (scrolling when they do not fit),
/// and the panel's own actions pinned at the foot.
pub(crate) fn panel(
    t: &Theme,
    tab: Inspector,
    scope: impl Into<SharedString>,
    body: Vec<AnyElement>,
    foot: Vec<AnyElement>,
) -> AnyElement {
    let title = tab.label();
    div()
        .id(SharedString::from(format!("re-panel-{}", tab.id())))
        .role(Role::Group)
        .aria_label(SharedString::from(tr!("{title} panel", title = title)))
        .flex_1()
        .min_w(px(0.))
        .h_full()
        .flex()
        .flex_col()
        .rounded(px(scale::RADIUS_XL))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.surface))
        .overflow_hidden()
        .child(
            div()
                .id(SharedString::from(format!("re-panel-body-{}", tab.id())))
                .flex_1()
                .min_h(px(0.))
                .overflow_y_scroll()
                .p(px(20.))
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .id(SharedString::from(format!("re-panel-title-{}", tab.id())))
                                .role(Role::Heading)
                                .aria_level(2)
                                .aria_label(title.clone())
                                .text_size(px(16.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(rgba_of(t.mist))
                                .child(scope.into()),
                        ),
                )
                .children(body),
        )
        .when(!foot.is_empty(), |d| {
            d.child(
                div()
                    .flex_none()
                    .px(px(20.))
                    .pb(px(20.))
                    .pt(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .children(foot),
            )
        })
        .into_any_element()
}

/// A micro label above a group of controls ("Layout", "Position").
pub(crate) fn micro(t: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(t.mist))
        .child(text.into())
}

/// A hint row: a 16 px glyph and a muted sentence.
pub(crate) fn hint(t: &Theme, glyph: &'static str, text: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(10.))
        .text_size(px(13.))
        .text_color(rgba_of(t.text_3))
        .child(div().pt(px(2.)).child(icon(glyph, 16., t.mist)))
        .child(div().flex_1().min_w(px(0.)).child(text.into()))
}

/// A hint in a bordered box.
pub(crate) fn hint_box(t: &Theme, glyph: &'static str, text: impl Into<SharedString>) -> Div {
    div()
        .p(px(12.))
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.ground))
        .child(hint(t, glyph, text))
}

/// Stands for the keycap inside the translated sentence of [`hint_box_keyed`].
pub(crate) const KEY_MARK: &str = "\u{E000}";

/// Splits a sentence built with `{key}` = [`KEY_MARK`] around the keycap. A translation that
/// dropped the mark puts the keycap at the end.
pub(crate) fn split_at_key(text: &str) -> (String, String) {
    match text.split_once(KEY_MARK) {
        Some((b, a)) => (b.trim_end().to_owned(), a.trim_start().to_owned()),
        None => (text.to_owned(), String::new()),
    }
}

/// A hint in a bordered box, with a keycap in the middle of the sentence ("Split the timeline
/// [S] to set ..."). `text` is one whole translated sentence built with `{key}` = [`KEY_MARK`];
/// the keycap is drawn where the mark is (at the end when a translation dropped the mark).
pub(crate) fn hint_box_keyed(
    t: &Theme,
    glyph: &'static str,
    text: String,
    key: &'static str,
) -> Div {
    let (before, after) = split_at_key(&text);
    div()
        .p(px(12.))
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.ground))
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .text_size(px(13.))
                .text_color(rgba_of(t.text_3))
                .child(icon(glyph, 16., t.mist))
                .child(before)
                .child(crate::ui::components::keycap(t, key))
                .child(after),
        )
}

/// A control row: a glyph, the label (with an optional mono suffix such as `2 s`) and the control
/// on the right, under a hairline.
pub(crate) fn field_row(
    t: &Theme,
    glyph: &'static str,
    label: impl Into<SharedString>,
    suffix: Option<&'static str>,
    control: impl IntoElement,
) -> Div {
    div()
        .py(px(8.))
        .border_t_1()
        .border_color(rgba_of(t.row_divider()))
        .flex()
        .items_center()
        .gap(px(12.))
        .child(icon(glyph, icon_size::SMALL, t.mist))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_baseline()
                .gap(px(8.))
                .text_size(px(type_scale::UI))
                .child(shrink_text(label))
                .children(suffix.map(|s| {
                    div()
                        .mono()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.mist))
                        .child(s)
                })),
        )
        .child(div().flex_none().child(control))
}

/// One choice drawn as a tile (the layouts, the preview shapes): a glyph or picture above or
/// beside a label. The caller adds `.on_click`.
pub(crate) fn tile_choice(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    aria: &str,
    active: bool,
) -> Stateful<Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::RadioButton)
        .aria_label(SharedString::from(aria.to_owned()))
        .aria_toggled(if active {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(if active { t.line } else { t.border }))
        .bg(rgba_of(if active {
            t.segment_selected()
        } else {
            t.ground
        }))
        .text_color(rgba_of(if active { t.text } else { t.text_2 }))
        .cursor_pointer()
        .when(!active, |d| d.hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
}

/// A 32 px icon-only button (Dismiss `x`): the aria label is required and is the tooltip. The
/// caller adds `.on_click`.
pub(crate) fn icon_only(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    aria: impl Into<SharedString>,
) -> Stateful<Div> {
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
        .border_color(gpui::transparent_black())
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
        .tooltip(crate::ui::tooltip::tip(aria, Vec::new()))
        .child(icon(glyph, icon_size::SMALL, t.text_2))
}

/// The ease of a zoom region as a picture: in over `ZOOM_EASE`, hold, out.
pub(crate) fn ease_diagram(t: &Theme) -> AnyElement {
    let line = Hsla::from(rgba_of(t.text));
    let guide = Hsla::from(rgba_of(t.mist.with_alpha(90)));
    let secs = ZOOM_EASE.as_secs_f64();
    let canvas_el = canvas(
        |bounds: Bounds<Pixels>, _, _| bounds,
        move |_, bounds: Bounds<Pixels>, window, _| {
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let (x0, x1, y_lo, y_hi) = (0.04 * w, 0.96 * w, h - 10.0, 10.0);
            let (a, b, c, d) = (0.25, 0.32, 0.68, 0.75);
            let at = |fx: f32, fy: f32| {
                point(
                    bounds.origin.x + px(fx * w),
                    bounds.origin.y + px(y_lo + (y_hi - y_lo) * fy),
                )
            };
            let _ = (x0, x1);
            let smooth = |u: f32| u * u * (3.0 - 2.0 * u);
            // The guides at the end of the ease in and the start of the ease out.
            for fx in [b, c] {
                let mut pb = PathBuilder::stroke(px(1.));
                pb.move_to(point(
                    bounds.origin.x + px(fx * w),
                    bounds.origin.y + px(4.),
                ));
                pb.line_to(point(bounds.origin.x + px(fx * w), bounds.origin.y + px(h)));
                if let Ok(p) = pb.build() {
                    window.paint_path(p, guide);
                }
            }
            let mut pb = PathBuilder::stroke(px(2.));
            pb.move_to(at(0.04, 0.0));
            pb.line_to(at(a, 0.0));
            for i in 1..=16 {
                let u = i as f32 / 16.0;
                pb.line_to(at(a + (b - a) * u, smooth(u)));
            }
            pb.line_to(at(c, 1.0));
            for i in 1..=16 {
                let u = i as f32 / 16.0;
                pb.line_to(at(c + (d - c) * u, 1.0 - smooth(u)));
            }
            pb.line_to(at(0.96, 0.0));
            if let Ok(p) = pb.build() {
                window.paint_path(p, line);
            }
        },
    )
    .w_full()
    .h(px(84.));
    div()
        .id("re-ease-diagram")
        .role(Role::Image)
        .aria_label(SharedString::from(tr!(
            "A zoom eases in over {secs} seconds, holds, and eases out",
            secs = cm_i18n::fmt::decimal(secs, 1)
        )))
        .px(px(12.))
        .pt(px(8.))
        .pb(px(8.))
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.field))
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(canvas_el)
        .child(
            div()
                .flex()
                .justify_between()
                .mono()
                .text_size(px(11.))
                .text_color(rgba_of(t.mist))
                .child(SharedString::from(tr!(
                    "ease in {secs} s",
                    secs = cm_i18n::fmt::decimal(secs, 1)
                )))
                .child(tr_ctx!("zoom phase", "hold"))
                .child(tr_ctx!("zoom phase", "ease out")),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_distinct(set: &[Inspector]) {
        let mut ids: Vec<_> = set.iter().map(|t| t.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), set.len());
        let mut glyphs: Vec<_> = set.iter().map(|t| t.icon()).collect();
        glyphs.sort_unstable();
        glyphs.dedup();
        assert_eq!(glyphs.len(), set.len());
    }

    /// UiInspectorTabs `set="video"`: eleven tabs in the board's order, each with its own id,
    /// name and glyph.
    #[test]
    fn eleven_video_tabs_in_the_boards_order() {
        let names: Vec<_> = Inspector::VIDEO.iter().map(|t| t.label()).collect();
        assert_eq!(
            names,
            [
                "Speed",
                "Camera",
                "Zoom",
                "Cursor",
                "Reframe",
                "Background",
                "Captions",
                "Speakers",
                "Report",
                "Suggestions",
                "Output"
            ]
        );
        assert_distinct(&Inspector::VIDEO);
    }

    /// UiInspectorTabs `set="audio"`: Speed, Transcript, Speakers, Report, Suggestions, Output.
    #[test]
    fn six_audio_tabs_in_the_boards_order() {
        let names: Vec<_> = Inspector::AUDIO.iter().map(|t| t.label()).collect();
        assert_eq!(
            names,
            [
                "Speed",
                "Transcript",
                "Speakers",
                "Report",
                "Suggestions",
                "Output"
            ]
        );
        assert_distinct(&Inspector::AUDIO);
        assert_eq!(Inspector::set(true), &Inspector::AUDIO);
        assert_eq!(Inspector::set(false), &Inspector::VIDEO);
        assert!(!Inspector::VIDEO.contains(&Inspector::Transcript));
        for gone in [
            Inspector::Camera,
            Inspector::Zoom,
            Inspector::Cursor,
            Inspector::Reframe,
            Inspector::Background,
            Inspector::Captions,
        ] {
            assert!(!Inspector::AUDIO.contains(&gone));
        }
        assert!(Inspector::ALL
            .iter()
            .all(|t| Inspector::VIDEO.contains(t) || Inspector::AUDIO.contains(t)));
    }

    /// V4: the wide tabs are the text editors: captions, transcript, speakers and report.
    #[test]
    fn captions_and_transcript_are_the_wide_tabs() {
        let wide: Vec<_> = Inspector::ALL.iter().filter(|t| t.is_wide()).collect();
        assert_eq!(
            wide,
            [
                &Inspector::Captions,
                &Inspector::Transcript,
                &Inspector::Speakers,
                &Inspector::Report
            ]
        );
    }

    /// Q3: a zoom chip opens Zoom; a segment opens Camera (Speed without a camera) from the
    /// view-only tabs, stays on Speed and Camera, and leaves Captions, Suggestions and Output
    /// alone.
    /// I18N P2: the keycap sits where the translation put `{key}`.
    #[test]
    fn keyed_hint_splits_around_the_key() {
        let line = tr!(
            "Split the timeline {key} to set a different layout for each part.",
            key = KEY_MARK
        );
        assert_eq!(
            split_at_key(&line),
            (
                "Split the timeline".to_owned(),
                "to set a different layout for each part.".to_owned()
            )
        );
        // A translation that lost the mark keeps the whole sentence.
        assert_eq!(
            split_at_key("no mark here"),
            ("no mark here".to_owned(), String::new())
        );
    }

    #[test]
    fn selecting_on_the_timeline_picks_the_matching_tab() {
        for cur in Inspector::ALL {
            assert_eq!(auto_tab(cur, Picked::Zoom, true), Inspector::Zoom);
            assert_eq!(auto_tab(cur, Picked::Zoom, false), Inspector::Zoom);
        }
        assert_eq!(
            auto_tab(Inspector::Zoom, Picked::Segment, true),
            Inspector::Camera
        );
        assert_eq!(
            auto_tab(Inspector::Cursor, Picked::Segment, false),
            Inspector::Speed
        );
        assert_eq!(
            auto_tab(Inspector::Speed, Picked::Segment, true),
            Inspector::Speed
        );
        assert_eq!(
            auto_tab(Inspector::Camera, Picked::Segment, true),
            Inspector::Camera
        );
        for keep in [
            Inspector::Captions,
            Inspector::Transcript,
            Inspector::Speakers,
            Inspector::Report,
            Inspector::Suggestions,
            Inspector::Output,
        ] {
            assert_eq!(auto_tab(keep, Picked::Segment, true), keep);
        }
    }

    /// "Open captions" selects the Captions tab whatever else happened; with nothing picked the tab
    /// stays; a zoom pick beats a segment pick in the same frame.
    #[test]
    fn next_tab_priorities() {
        use Inspector::*;
        assert_eq!(next_tab(Output, true, true, true, true), Captions);
        assert_eq!(next_tab(Output, false, false, false, true), Output);
        assert_eq!(next_tab(Camera, false, true, true, true), Zoom);
        assert_eq!(next_tab(Cursor, false, true, false, true), Camera);
    }

    /// Q4: Camera is unavailable without a camera track and Cursor when the cursor is baked in;
    /// every other tab always works.
    #[test]
    fn unavailable_tabs() {
        assert!(unavailable(Inspector::Camera, false, true));
        assert!(!unavailable(Inspector::Camera, true, false));
        assert!(unavailable(Inspector::Cursor, true, false));
        assert!(!unavailable(Inspector::Cursor, false, true));
        for t in Inspector::ALL {
            if !matches!(t, Inspector::Camera | Inspector::Cursor) {
                assert!(!unavailable(t, false, false), "{t:?}");
            }
        }
    }

    /// Every glyph the rail and the panels draw is in the registry.
    #[test]
    fn tab_glyphs_are_registered() {
        for t in Inspector::ALL {
            assert!(
                crate::ui::paint::ASSETS
                    .iter()
                    .any(|(p, _)| *p == format!("icons/{}.svg", t.icon())),
                "{}",
                t.icon()
            );
        }
    }
}
