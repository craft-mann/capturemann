//! The outline drawn on the real screen around the window selected in the picker (R19, "the
//! selected window is also outlined on screen with its title").
//!
//! A borderless, transparent, click-through window the size of the target plus its ring, hidden
//! from screen capture like the webcam bubble and the HUD (`platform_window::apply`), one level
//! below the picker so the palette stays on top. It is replaced, not moved, when the selection
//! changes (GPUI 0.3.9 cannot resize a window).

use gpui::{
    div, prelude::*, px, Context, IntoElement, Render, SharedString, Window, WindowOptions,
};

use crate::platform_window::Tweaks;
use crate::recording::geometry::Rect;
use crate::theme::{derived, Theme};
use crate::ui::components::{icon, icon_size};
use crate::ui::paint::{rgba_of, ui_features, UI_FONT};

/// The ring outside the 3 px border (board: `box-shadow: 0 0 0 6px`).
pub const RING: f64 = 6.;
pub const BORDER: f64 = 3.;
/// The title chip: 24 px tall, 6 px above the ring.
pub const CHIP_H: f64 = 24.;
const CHIP_GAP: f64 = 6.;
/// Inset of the chip from the window's right edge.
const CHIP_INSET: f64 = 12.;
/// NSWindow level of the outline: above normal and floating windows, below the picker (101).
pub const LEVEL: isize = 90;

/// Where the outline window goes and how it is laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutlineFrame {
    /// The outline window's frame, global points.
    pub frame: Rect,
    /// The target window's rectangle inside that frame (the ring surrounds it).
    pub target: Rect,
    /// The chip sits above the ring; otherwise it sits inside the top right of the window.
    pub chip_above: bool,
}

/// Lays the outline out around `target` (global points). `visible` is the visible part of the
/// target's display (below the menu bar): the chip goes above the window only when it fits there.
pub fn outline_frame(target: Rect, visible: Rect) -> OutlineFrame {
    let room_above = target.y - RING - CHIP_GAP - CHIP_H >= visible.y;
    let top = if room_above {
        RING + CHIP_GAP + CHIP_H
    } else {
        RING
    };
    let frame = Rect::new(
        target.x - RING,
        target.y - top,
        target.w + 2. * RING,
        target.h + top + RING,
    );
    OutlineFrame {
        frame,
        target: Rect::new(RING, top, target.w, target.h),
        chip_above: room_above,
    }
}

/// The window options and capture hiding of the outline window.
pub fn options(display: (gpui::DisplayId, Rect), frame: Rect) -> WindowOptions {
    crate::ui::recording::panel_options(display, frame, false, false)
}

/// Hides the new outline window from capture and from the mouse, and puts it at [`LEVEL`].
pub fn harden(window: &Window) {
    if let Err(e) = crate::platform_window::apply(
        window,
        Tweaks {
            exclude_from_capture: !crate::recording::demo::capturable(),
            click_through: true,
            level: Some(LEVEL),
            no_shadow: true,
        },
    ) {
        eprintln!("cm-desktop: could not set up the window outline: {e}");
    }
}

pub struct OutlineView {
    layout: OutlineFrame,
    title: SharedString,
}

impl OutlineView {
    pub fn new(layout: OutlineFrame, title: impl Into<SharedString>) -> Self {
        OutlineView {
            layout,
            title: title.into(),
        }
    }
}

impl Render for OutlineView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let l = self.layout;
        let (tx, ty, tw, th) = (l.target.x, l.target.y, l.target.w, l.target.h);
        let chip = div()
            .absolute()
            .top(px(if l.chip_above {
                0.
            } else {
                (ty + CHIP_INSET) as f32
            }))
            .right(px((RING + CHIP_INSET) as f32))
            .max_w(px((tw - 2. * CHIP_INSET).max(40.) as f32))
            .h(px(CHIP_H as f32))
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(7.))
            .bg(rgba_of(t.text))
            .text_color(rgba_of(t.ink))
            .text_size(px(12.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(icon("locate", icon_size::PILL, t.ink))
            .child(div().min_w(px(0.)).truncate().child(self.title.clone()));
        div()
            .size_full()
            .relative()
            .font_family(UI_FONT)
            .font_features(ui_features())
            // The ring, then the border inside it, both around the target's rectangle.
            .child(
                div()
                    .absolute()
                    .left(px((tx - RING) as f32))
                    .top(px((ty - RING) as f32))
                    .w(px((tw + 2. * RING) as f32))
                    .h(px((th + 2. * RING) as f32))
                    .rounded(px(19.))
                    .border(px(RING as f32))
                    .border_color(rgba_of(derived::PICKER_OUTLINE_RING)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(tx as f32))
                    .top(px(ty as f32))
                    .w(px(tw as f32))
                    .h(px(th as f32))
                    .rounded(px(13.))
                    .border(px(BORDER as f32))
                    .border_color(rgba_of(t.text)),
            )
            .child(chip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible() -> Rect {
        Rect::new(0., 25., 1512., 957.)
    }

    #[test]
    fn the_frame_wraps_the_ring_and_the_chip_when_there_is_room_above() {
        let target = Rect::new(200., 200., 600., 400.);
        let o = outline_frame(target, visible());
        assert!(o.chip_above);
        assert_eq!(o.frame, Rect::new(194., 164., 612., 442.));
        // The target sits where it was, relative to the frame.
        assert_eq!(o.target, Rect::new(6., 36., 600., 400.));
        assert_eq!(o.frame.x + o.target.x, target.x);
        assert_eq!(o.frame.y + o.target.y, target.y);
    }

    #[test]
    fn a_window_at_the_top_gets_its_chip_inside() {
        let target = Rect::new(100., 30., 600., 400.);
        let o = outline_frame(target, visible());
        assert!(!o.chip_above);
        assert_eq!(o.frame.y, target.y - RING);
        assert_eq!(o.frame.h, target.h + 2. * RING);
        assert_eq!(o.frame.y + o.target.y, target.y);
    }

    #[test]
    fn the_chip_needs_the_whole_strip_above() {
        // Exactly enough room: ring 6 + gap 6 + chip 24 = 36 below the visible top.
        let fits = outline_frame(Rect::new(0., 25. + 36., 100., 100.), visible());
        assert!(fits.chip_above);
        let tight = outline_frame(Rect::new(0., 25. + 35., 100., 100.), visible());
        assert!(!tight.chip_above);
    }

    #[test]
    fn a_window_on_a_display_above_the_primary_uses_that_displays_menu_bar() {
        let upper = Rect::new(0., -900., 1440., 875.);
        let o = outline_frame(Rect::new(10., -800., 300., 200.), upper);
        assert!(o.chip_above);
        assert!(o.frame.y >= upper.y);
    }
}
