//! Client-side window decorations, for desktops whose compositor draws no title bar.
//!
//! GNOME on Wayland has no server-side decorations: a GPUI window there has no title bar, so it
//! could not be moved, minimised, maximised or closed with the mouse, and had no resize edge.
//! GPUI reports what the compositor decided (`Window::window_decorations()`): `Server` when the
//! compositor or window manager draws the frame (KDE, X11 with a window manager, macOS,
//! Windows) and `Client { tiling }` when the app must draw it. [`frame`] wraps a window's content
//! for the second case only; in the first it returns the content untouched, so macOS and
//! Windows keep their native frame and nothing here runs.
//!
//! The bar follows the flat Studio mode (DESIGN section 1): the sidebar tone, a hairline below,
//! the app name at the left, 36 px tall, and three flat buttons at the right. It is not drawn
//! rounded and has no shadow (GPUI asks for an invisible inset for a shadow; a flat 1 px border
//! is enough to tell the window from the desktop and costs nothing).

use gpui::{
    div, prelude::*, px, AnyElement, CursorStyle, Decorations, Div, MouseButton, ResizeEdge, Role,
    SharedString, Stateful, Tiling, Window, WindowControls,
};

use cm_i18n::tr;

use crate::theme::{type_scale, Theme};
use crate::ui::components::{icon, logo, shrink_text};
use crate::ui::paint::rgba_of;

/// Height of the title bar in logical px.
pub const TITLEBAR_H: f32 = 36.;
/// Width of the invisible resize strip along each edge, in logical px.
pub const RESIZE_BORDER: f32 = 6.;
/// Width of one title bar button.
const BUTTON_W: f32 = 44.;

/// Whether the app has to draw the title bar itself: the compositor asked for client-side
/// decorations and the window is not full screen (a full-screen window has no frame at all).
pub fn needs_client_decorations(decorations: Decorations, fullscreen: bool) -> bool {
    matches!(decorations, Decorations::Client { .. }) && !fullscreen
}

/// The resize edge under a point `(x, y)` of a window of `size` (all logical px, origin top-left)
/// when the pointer is within `border` of an edge that is not tiled or maximised; corners win
/// over edges. `None` inside the window or on a tiled edge.
pub fn resize_edge_at(
    pos: (f32, f32),
    size: (f32, f32),
    border: f32,
    tiling: Tiling,
) -> Option<ResizeEdge> {
    let (x, y) = pos;
    let left = x < border && !tiling.left;
    let right = x >= size.0 - border && !tiling.right;
    let top = y < border && !tiling.top;
    let bottom = y >= size.1 - border && !tiling.bottom;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeEdge::TopLeft),
        (_, true, true, _) => Some(ResizeEdge::TopRight),
        (true, _, _, true) => Some(ResizeEdge::BottomLeft),
        (_, true, _, true) => Some(ResizeEdge::BottomRight),
        (true, ..) => Some(ResizeEdge::Left),
        (_, true, ..) => Some(ResizeEdge::Right),
        (_, _, true, _) => Some(ResizeEdge::Top),
        (_, _, _, true) => Some(ResizeEdge::Bottom),
        _ => None,
    }
}

/// The mouse cursor to show over a resize edge.
pub fn edge_cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

/// Which of the three buttons the platform supports (a compositor can say it has no minimise
/// or maximise, for example in a tiling session). Close is always there.
pub fn visible_buttons(c: WindowControls, resizable: bool) -> (bool, bool) {
    (c.minimize, c.maximize && resizable)
}

/// `content` inside the app-drawn frame when the compositor wants one, else `content` itself.
/// Call it at the end of the window root's `render`.
pub fn frame(window: &mut Window, t: &Theme, content: AnyElement) -> AnyElement {
    let decorations = window.window_decorations();
    if !needs_client_decorations(decorations, window.is_fullscreen()) {
        return content;
    }
    let tiling = match decorations {
        Decorations::Client { tiling } => tiling,
        Decorations::Server => Tiling::default(),
    };
    let maximized = window.is_maximized();
    let (can_min, can_max) = visible_buttons(window.window_controls(), window.is_resizable());
    let size = window.bounds().size;
    let (w, h) = (size.width.as_f32(), size.height.as_f32());
    let border = rgba_of(t.border);

    let bar = titlebar(t, can_min, can_max, maximized);
    let handles = (!maximized).then(|| resize_handles(tiling, w, h));
    div()
        .id("csd-frame")
        .size_full()
        .relative()
        .flex()
        .flex_col()
        .bg(rgba_of(t.ground))
        // A hairline around the window: the compositor draws no edge and no shadow here.
        .when(!maximized, |d| d.border_1().border_color(border))
        .child(bar)
        .child(div().flex_1().min_h(px(0.)).child(content))
        .children(handles.into_iter().flatten())
        .into_any_element()
}

fn titlebar(t: &Theme, can_min: bool, can_max: bool, maximized: bool) -> Stateful<Div> {
    let text = rgba_of(t.text_2);
    div()
        .id("csd-titlebar")
        .role(Role::TitleBar)
        .flex_none()
        .h(px(TITLEBAR_H))
        .w_full()
        .flex()
        .items_center()
        .bg(rgba_of(t.sidebar))
        .border_b_1()
        .border_color(rgba_of(t.border))
        .text_size(px(type_scale::CAPTION))
        .text_color(text)
        // Drag to move; double-click toggles maximise; right-click opens the window menu.
        .on_mouse_down(MouseButton::Left, |e, window, _| {
            if e.click_count >= 2 {
                if window.is_resizable() {
                    window.zoom_window();
                }
            } else {
                window.start_window_move();
            }
        })
        .on_mouse_down(MouseButton::Right, |e, window, _| {
            window.show_window_menu(e.position);
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .items_center()
                .gap(px(8.))
                .pl(px(12.))
                .child(logo(t, 16.))
                .child(shrink_text("CaptureMann")),
        )
        .child(
            div()
                .flex_none()
                .h_full()
                .flex()
                .when(can_min, |d| {
                    d.child(control_button(
                        t,
                        "csd-minimize",
                        "minus",
                        tr!("Minimize"),
                        |window| window.minimize_window(),
                    ))
                })
                .when(can_max, |d| {
                    d.child(control_button(
                        t,
                        "csd-maximize",
                        if maximized { "copy" } else { "square" },
                        if maximized {
                            tr!("Restore")
                        } else {
                            tr!("Maximize")
                        },
                        |window| window.zoom_window(),
                    ))
                })
                .child(control_button(
                    t,
                    "csd-close",
                    "x",
                    tr!("Close window"),
                    |window| window.remove_window(),
                )),
        )
}

fn control_button(
    t: &Theme,
    id: &'static str,
    glyph: &'static str,
    aria: String,
    action: fn(&mut Window),
) -> Stateful<Div> {
    let (hover_bg, focus) = (rgba_of(t.raised), rgba_of(t.text));
    let aria: SharedString = aria.into();
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(aria)
        .focusable()
        .tab_stop(true)
        .w(px(BUTTON_W))
        .h_full()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(gpui::transparent_black())
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .focus_visible(move |s| s.border_color(focus))
        // A press on a button must not start a window move.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, _| action(window))
        .child(icon(glyph, 14., t.text))
}

/// Eight transparent strips along the edges and corners that start a compositor resize.
fn resize_handles(tiling: Tiling, w: f32, h: f32) -> Vec<AnyElement> {
    use ResizeEdge::*;
    let b = RESIZE_BORDER;
    let c = b * 2.;
    // (edge, origin, size) of each strip.
    type Spot = (ResizeEdge, (f32, f32), (f32, f32));
    let spots: [Spot; 8] = [
        (Top, (c, 0.), (w - 2. * c, b)),
        (Bottom, (c, h - b), (w - 2. * c, b)),
        (Left, (0., c), (b, h - 2. * c)),
        (Right, (w - b, c), (b, h - 2. * c)),
        (TopLeft, (0., 0.), (c, c)),
        (TopRight, (w - c, 0.), (c, c)),
        (BottomLeft, (0., h - c), (c, c)),
        (BottomRight, (w - c, h - c), (c, c)),
    ];
    spots
        .into_iter()
        .filter(|(edge, ..)| {
            resize_edge_at(probe(*edge, w, h, b), (w, h), b, tiling) == Some(*edge)
        })
        .map(|(edge, (x, y), (sw, sh))| {
            div()
                .id(SharedString::from(format!("csd-resize-{edge:?}")))
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(sw.max(0.)))
                .h(px(sh.max(0.)))
                .cursor(edge_cursor(edge))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    window.start_window_resize(edge);
                    cx.stop_propagation();
                })
                .into_any_element()
        })
        .collect()
}

/// A point inside the strip of `edge`, to ask [`resize_edge_at`] whether that edge is tiled.
fn probe(edge: ResizeEdge, w: f32, h: f32, b: f32) -> (f32, f32) {
    let (mid_x, mid_y) = (w / 2., h / 2.);
    let (near, far_x, far_y) = (b / 2., w - b / 2., h - b / 2.);
    match edge {
        ResizeEdge::Top => (mid_x, near),
        ResizeEdge::Bottom => (mid_x, far_y),
        ResizeEdge::Left => (near, mid_y),
        ResizeEdge::Right => (far_x, mid_y),
        ResizeEdge::TopLeft => (near, near),
        ResizeEdge::TopRight => (far_x, near),
        ResizeEdge::BottomLeft => (near, far_y),
        ResizeEdge::BottomRight => (far_x, far_y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn free() -> Tiling {
        Tiling::default()
    }

    #[test]
    fn server_decorations_need_nothing_from_the_app() {
        assert!(!needs_client_decorations(Decorations::Server, false));
    }

    #[test]
    fn client_decorations_are_drawn_unless_full_screen() {
        let client = Decorations::Client { tiling: free() };
        assert!(needs_client_decorations(client, false));
        assert!(!needs_client_decorations(client, true));
    }

    #[test]
    fn corners_win_over_edges() {
        let s = (800., 600.);
        assert_eq!(
            resize_edge_at((2., 2.), s, 6., free()),
            Some(ResizeEdge::TopLeft)
        );
        assert_eq!(
            resize_edge_at((797., 3.), s, 6., free()),
            Some(ResizeEdge::TopRight)
        );
        assert_eq!(
            resize_edge_at((1., 598.), s, 6., free()),
            Some(ResizeEdge::BottomLeft)
        );
        assert_eq!(
            resize_edge_at((799., 599.), s, 6., free()),
            Some(ResizeEdge::BottomRight)
        );
    }

    #[test]
    fn edges_and_the_inside() {
        let s = (800., 600.);
        assert_eq!(
            resize_edge_at((400., 1.), s, 6., free()),
            Some(ResizeEdge::Top)
        );
        assert_eq!(
            resize_edge_at((400., 599.), s, 6., free()),
            Some(ResizeEdge::Bottom)
        );
        assert_eq!(
            resize_edge_at((0., 300.), s, 6., free()),
            Some(ResizeEdge::Left)
        );
        assert_eq!(
            resize_edge_at((795., 300.), s, 6., free()),
            Some(ResizeEdge::Right)
        );
        assert_eq!(resize_edge_at((400., 300.), s, 6., free()), None);
        assert_eq!(resize_edge_at((6., 6.), s, 6., free()), None);
    }

    #[test]
    fn a_tiled_edge_does_not_resize() {
        let s = (800., 600.);
        let tiled_left = Tiling {
            left: true,
            ..free()
        };
        assert_eq!(resize_edge_at((1., 300.), s, 6., tiled_left), None);
        // The top-left corner degrades to the top edge.
        assert_eq!(
            resize_edge_at((1., 1.), s, 6., tiled_left),
            Some(ResizeEdge::Top)
        );
        assert_eq!(
            resize_edge_at((799., 300.), s, 6., tiled_left),
            Some(ResizeEdge::Right)
        );
    }

    #[test]
    fn every_handle_is_listed_for_a_free_window_and_none_for_a_fully_tiled_one() {
        assert_eq!(resize_handles(free(), 800., 600.).len(), 8);
        assert_eq!(resize_handles(Tiling::tiled(), 800., 600.).len(), 0);
    }

    #[test]
    fn buttons_follow_the_compositor_and_the_window() {
        let all = WindowControls::default();
        assert_eq!(visible_buttons(all, true), (true, true));
        assert_eq!(visible_buttons(all, false), (true, false));
        let none = WindowControls {
            minimize: false,
            maximize: false,
            ..all
        };
        assert_eq!(visible_buttons(none, true), (false, false));
    }

    #[test]
    fn each_edge_has_a_resize_cursor() {
        assert_eq!(edge_cursor(ResizeEdge::Left), CursorStyle::ResizeLeftRight);
        assert_eq!(edge_cursor(ResizeEdge::Top), CursorStyle::ResizeUpDown);
    }
}
