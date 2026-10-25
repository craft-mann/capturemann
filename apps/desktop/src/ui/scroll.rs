//! A vertical scroll area for lists that sit inside a scrolling page, with a visible scrollbar.
//!
//! GPUI's own scroll handler never stops a wheel event, so a scrollable list inside a scrollable
//! page scrolls both at once. [`nested_scroll`] contains the wheel while its content overflows
//! (the list scrolls, the page does not) and lets it through when everything fits. Within one
//! element the `on_scroll_wheel` listener is registered before GPUI's built-in scroll listener and
//! bubble handlers run in reverse order, so the list has scrolled by the time the listener stops
//! the event from reaching the page.
//!
//! The thumb is painted from the scroll handle after layout, so it is right on the first frame
//! and needs no re-render. The pure geometry is [`thumb_geometry`].

use gpui::{
    canvas, div, prelude::*, px, quad, AnyElement, BorderStyle, Div, ScrollHandle, Stateful,
};

use crate::theme::Theme;
use crate::ui::paint::hsla_of;

/// Width of the scrollbar track and thumb in px.
pub const BAR_W: f32 = 6.;
/// The thumb never gets shorter than this, so it stays easy to see on long lists.
pub const MIN_THUMB: f32 = 24.;
/// Space kept free on the right of the list so the bar does not cover its content.
const GUTTER: f32 = 12.;

/// Where the thumb goes: `(top, height)` in px inside a track `viewport` px tall, for content
/// that overflows the viewport by `max_offset` px and is scrolled to `offset_y` (GPUI uses
/// `0` at the top and a negative number when scrolled down). `None` when nothing overflows.
pub fn thumb_geometry(viewport: f32, max_offset: f32, offset_y: f32) -> Option<(f32, f32)> {
    // `is_nan` first: NaN must give no thumb, and `>` on NaN is false either way.
    if viewport.is_nan() || max_offset.is_nan() || viewport <= 0.0 || max_offset <= 0.0 {
        return None;
    }
    let content = viewport + max_offset;
    let height = (viewport * viewport / content).max(MIN_THUMB).min(viewport);
    let progress = (-offset_y / max_offset).clamp(0.0, 1.0);
    Some((progress * (viewport - height), height))
}

/// Wraps `list` (the caller keeps its id, role, aria label, direction and children) in a
/// height-capped scroll area with a visible scrollbar. `handle` must live in the owning view.
pub fn nested_scroll(
    t: &Theme,
    handle: &ScrollHandle,
    max_h: f32,
    list: Stateful<Div>,
) -> AnyElement {
    scroll_area(t, handle, Some(max_h), list, None)
}

/// [`nested_scroll`] for a list that fills the height its parent gives it (a flex item with
/// `min_h(0)`): the captions list between the panel header and footer. The rows must be direct
/// children of `list`, so `ScrollHandle::scroll_to_item` can address them. `on_wheel` runs on
/// every wheel event over the list (the captions list notes the time of a manual scroll).
pub fn fill_scroll(
    t: &Theme,
    handle: &ScrollHandle,
    list: Stateful<Div>,
    on_wheel: impl Fn() + 'static,
) -> AnyElement {
    scroll_area(t, handle, None, list, Some(Box::new(on_wheel)))
}

fn scroll_area(
    t: &Theme,
    handle: &ScrollHandle,
    max_h: Option<f32>,
    list: Stateful<Div>,
    on_wheel: Option<Box<dyn Fn()>>,
) -> AnyElement {
    let contain = handle.clone();
    let bar = handle.clone();
    let track = hsla_of(t.raised).opacity(0.7);
    let thumb = hsla_of(t.mist).opacity(0.75);
    let list = list
        .track_scroll(handle)
        .overflow_y_scroll()
        .pr(px(GUTTER))
        .on_scroll_wheel(move |_, _, cx| {
            if let Some(f) = &on_wheel {
                f();
            }
            if contain.max_offset().y > px(0.) {
                cx.stop_propagation();
            }
        });
    let list = match max_h {
        Some(h) => list.max_h(px(h)),
        None => list.absolute().top_0().bottom_0().left_0().right_0(),
    };
    div()
        .relative()
        .w_full()
        .when(max_h.is_none(), |d| d.flex_1().min_h(px(0.)))
        .child(list)
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let max = f32::from(bar.max_offset().y);
                    let Some((top, height)) = thumb_geometry(
                        f32::from(bounds.size.height),
                        max,
                        f32::from(bar.offset().y),
                    ) else {
                        return;
                    };
                    let mut at = |y: f32, h: f32, color| {
                        let mut b = bounds;
                        b.origin.x += bounds.size.width - px(BAR_W);
                        b.origin.y += px(y);
                        b.size.width = px(BAR_W);
                        b.size.height = px(h);
                        window.paint_quad(quad(
                            b,
                            px(BAR_W / 2.),
                            color,
                            px(0.),
                            gpui::transparent_black(),
                            BorderStyle::default(),
                        ));
                    };
                    at(0., f32::from(bounds.size.height), track);
                    at(top, height, thumb);
                },
            )
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(BAR_W + 2.)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_overflow_means_no_thumb() {
        assert_eq!(thumb_geometry(300., 0., 0.), None);
        assert_eq!(thumb_geometry(0., 100., 0.), None);
        assert_eq!(thumb_geometry(300., -5., 0.), None);
        assert_eq!(thumb_geometry(f32::NAN, 100., 0.), None);
    }

    #[test]
    fn thumb_is_the_visible_share_of_the_content() {
        // 300 px viewport, 300 px more content: the thumb is half the track.
        let (top, h) = thumb_geometry(300., 300., 0.).unwrap();
        assert_eq!(top, 0.);
        assert!((h - 150.).abs() < 1e-3);
    }

    #[test]
    fn thumb_travels_from_the_top_to_the_bottom_of_the_track() {
        let (_, h) = thumb_geometry(300., 300., 0.).unwrap();
        let (mid, _) = thumb_geometry(300., 300., -150.).unwrap();
        let (end, _) = thumb_geometry(300., 300., -300.).unwrap();
        assert!((mid - (300. - h) / 2.).abs() < 1e-3);
        assert!((end - (300. - h)).abs() < 1e-3);
    }

    #[test]
    fn thumb_has_a_minimum_height_and_overscroll_is_clamped() {
        let (_, h) = thumb_geometry(300., 30_000., 0.).unwrap();
        assert_eq!(h, MIN_THUMB);
        let (past_end, _) = thumb_geometry(300., 100., -500.).unwrap();
        let (top, _) = thumb_geometry(300., 100., 40.).unwrap();
        assert!(past_end <= 300. - MIN_THUMB + 1e-3);
        assert_eq!(top, 0.);
        // A tiny viewport never gets a thumb taller than itself.
        assert_eq!(thumb_geometry(10., 100., 0.).unwrap().1, 10.);
    }
}
