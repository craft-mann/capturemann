//! The text field spec's keys, run through the headless GPUI platform: selection with Shift and
//! the opening selections of `load`, the Escape ladder of a search field, read-only and disabled
//! fields (spec `capturemann-text-fields/text-field-spec.md`, "Behaviour").

#![allow(clippy::unwrap_used)]

use cm_desktop::theme::Theme;
use cm_desktop::ui::studio::ThemeGlobal;
use cm_desktop::ui::text_field::{CaretAt, TextField, TextFieldEvent};
use gpui::{Focusable, TestAppContext, VisualTestContext};
use std::cell::RefCell;
use std::rc::Rc;

fn field(
    cx: &mut TestAppContext,
    build: impl FnOnce(TextField) -> TextField + 'static,
) -> (gpui::Entity<TextField>, &mut VisualTestContext) {
    cx.update(|cx| cx.set_global(ThemeGlobal(Theme::DARK)));
    let window = cx.add_window(|_, cx| build(TextField::new("Title", "A title", cx)));
    let entity = window.root(cx).unwrap();
    let vcx = VisualTestContext::from_window(window.into(), cx).into_mut();
    entity.update_in(vcx, |f, window, cx| {
        let handle = f.focus_handle(cx);
        window.focus(&handle, cx);
    });
    vcx.run_until_parked();
    (entity, vcx)
}

fn text(e: &gpui::Entity<TextField>, vcx: &mut VisualTestContext) -> String {
    e.read_with(vcx, |f, _| f.text().to_owned())
}

#[gpui::test]
fn shift_arrows_select_and_typing_replaces_the_selection(cx: &mut TestAppContext) {
    let (e, vcx) = field(cx, |f| f);
    e.update(vcx, |f, cx| f.load("hello", CaretAt::End, cx));
    vcx.simulate_keystrokes("shift-left shift-left");
    vcx.simulate_input("p!");
    assert_eq!(text(&e, vcx), "help!");
}

#[gpui::test]
fn load_can_select_everything_or_the_name_before_the_extension(cx: &mut TestAppContext) {
    let (e, vcx) = field(cx, |f| f);
    e.update(vcx, |f, cx| f.load("clip one.mp4", CaretAt::SelectStem, cx));
    vcx.simulate_input("x");
    assert_eq!(text(&e, vcx), "x.mp4");
    e.update(vcx, |f, cx| f.load("clip one.mp4", CaretAt::SelectAll, cx));
    vcx.simulate_input("y");
    assert_eq!(text(&e, vcx), "y");
}

#[gpui::test]
fn a_search_field_clears_on_the_first_escape_and_leaves_on_the_second(cx: &mut TestAppContext) {
    let (e, vcx) = field(cx, |f| f.clearable());
    let seen = Rc::new(RefCell::new(Vec::<&'static str>::new()));
    let log = seen.clone();
    let _sub = vcx.update(|_, cx| {
        cx.subscribe(&e, move |_, ev: &TextFieldEvent, _| {
            log.borrow_mut().push(match ev {
                TextFieldEvent::Changed(_) => "changed",
                TextFieldEvent::Submitted => "submitted",
                TextFieldEvent::Escaped => "escaped",
            })
        })
    });
    vcx.simulate_input("query");
    vcx.simulate_keystrokes("escape");
    assert_eq!(text(&e, vcx), "");
    assert!(
        !seen.borrow().contains(&"escaped"),
        "the first Escape only clears"
    );
    vcx.simulate_keystrokes("escape");
    assert!(seen.borrow().contains(&"escaped"));
}

#[gpui::test]
fn escape_puts_the_loaded_text_back(cx: &mut TestAppContext) {
    let (e, vcx) = field(cx, |f| f);
    e.update(vcx, |f, cx| f.load("Old title", CaretAt::SelectAll, cx));
    vcx.simulate_input("New");
    assert_eq!(text(&e, vcx), "New");
    vcx.simulate_keystrokes("escape");
    assert_eq!(text(&e, vcx), "Old title");
}

#[gpui::test]
fn a_read_only_field_takes_no_typing_but_keeps_its_text(cx: &mut TestAppContext) {
    let (e, vcx) = field(cx, |f| f.read_only(true));
    e.update(vcx, |f, cx| f.load("fixed", CaretAt::End, cx));
    vcx.simulate_input("x");
    vcx.simulate_keystrokes("backspace");
    assert_eq!(text(&e, vcx), "fixed");
}
