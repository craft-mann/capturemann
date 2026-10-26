//! Typing reaches a text field: the headless GPUI platform delivers keystrokes and input-method
//! text to the focused `TextField` the way macOS does. Covers the field the Publish page, Settings
//! and the caption editor all use.

#![allow(clippy::unwrap_used)]

use cm_desktop::theme::Theme;
use cm_desktop::ui::studio::ThemeGlobal;
use cm_desktop::ui::text_field::TextField;
use gpui::{Focusable, TestAppContext, VisualTestContext};

fn field(cx: &mut TestAppContext) -> (gpui::Entity<TextField>, &mut VisualTestContext) {
    cx.update(|cx| cx.set_global(ThemeGlobal(Theme::DARK)));
    let window = cx.add_window(|_, cx| TextField::new("Title", "A title", cx));
    let entity = window.root(cx).unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx).into_mut();
    entity.update_in(vcx, |f, window, cx| {
        let handle = f.focus_handle(cx);
        window.focus(&handle, cx);
    });
    vcx.run_until_parked();
    (entity, vcx)
}

#[gpui::test]
fn input_method_text_lands_in_the_focused_field(cx: &mut TestAppContext) {
    let (entity, vcx) = field(cx);
    vcx.simulate_input("hello");
    assert_eq!(entity.read_with(vcx, |f, _| f.text().to_owned()), "hello");
}

#[gpui::test]
fn keystrokes_edit_the_focused_field(cx: &mut TestAppContext) {
    let (entity, vcx) = field(cx);
    vcx.simulate_keystrokes("a b c backspace");
    assert_eq!(entity.read_with(vcx, |f, _| f.text().to_owned()), "ab");
}
