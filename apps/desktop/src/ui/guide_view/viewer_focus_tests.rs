//! QA V-esc: Esc closes the full-size picture viewer. A small view with the same pattern as the
//! guide editor (a focusable picture whose click opens an overlay that tracks its own focus
//! handle and closes on Esc) runs in the headless GPUI platform, so the focus order is checked
//! without a screen.

use gpui::{
    div, px, Context, FocusHandle, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, TestAppContext, VisualTestContext, Window,
};

use super::viewer_should_take_focus;

struct Pattern {
    viewer: bool,
    viewer_focus: FocusHandle,
    /// The click moves the focus to the viewer (false: the focus stays on the picture).
    focus_on_open: bool,
}

impl Render for Pattern {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // What the real view does on every render while the viewer is open.
        if viewer_should_take_focus(self.viewer, self.viewer_focus.contains_focused(window, cx)) {
            let handle = self.viewer_focus.clone();
            window.defer(cx, move |window, cx| window.focus(&handle, cx));
        }
        div()
            .size(px(400.))
            .id("root")
            .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                if this.viewer && ev.keystroke.key == "escape" {
                    this.viewer = false;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("picture")
                    .size(px(100.))
                    .focusable()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.viewer = true;
                        if this.focus_on_open {
                            window.focus(&this.viewer_focus, cx);
                        }
                        cx.notify();
                    })),
            )
            .children(self.viewer.then(|| {
                div()
                    .id("viewer")
                    .track_focus(&self.viewer_focus)
                    .on_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, _, cx| {
                        if ev.keystroke.key == "escape" {
                            this.viewer = false;
                            cx.notify();
                        }
                    }))
                    .absolute()
                    .size(px(400.))
            }))
    }
}

fn open_then_esc(cx: &mut TestAppContext, focus_on_open: bool) -> bool {
    let window = cx.add_window(|_, cx| Pattern {
        viewer: false,
        viewer_focus: cx.focus_handle(),
        focus_on_open,
    });
    let view = window.root(cx).unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx).into_mut();
    vcx.run_until_parked();
    vcx.simulate_click(gpui::point(px(10.), px(10.)), gpui::Modifiers::none());
    vcx.run_until_parked();
    assert!(view.read_with(vcx, |v, _| v.viewer), "the click opens it");
    vcx.simulate_keystrokes("escape");
    vcx.run_until_parked();
    !view.read_with(vcx, |v, _| v.viewer)
}

#[gpui::test]
fn esc_closes_the_viewer_after_a_click_on_the_picture(cx: &mut TestAppContext) {
    assert!(open_then_esc(cx, true), "Esc did not close the viewer");
}

/// The focus stayed on the picture: the root's handler (and the render's re-focus) still close it.
#[gpui::test]
fn esc_closes_the_viewer_when_the_focus_stayed_on_the_picture(cx: &mut TestAppContext) {
    assert!(open_then_esc(cx, false), "Esc did not close the viewer");
}
