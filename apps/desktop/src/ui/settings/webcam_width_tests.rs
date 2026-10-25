//! Width of the texts on the Webcam & audio page (R13) in English, Vietnamese and the
//! pseudo-locale, measured in the bundled Geist fonts like `layout_width_tests`. The widths come
//! from the sizes in `webcam_view.rs` and `widgets::settings_row`. English and Vietnamese must
//! fit; the pseudo-locale (30 percent longer) is only compared.

use cm_i18n::{with_language, Lang};

use super::webcam_model::{fallback_note_text, preview_caption, DeviceKind, Dropdown, Hint};
use crate::recording::geometry::{BubbleShape, BubbleSize};
use crate::text_metrics_tests::{lines, width, Face};
use crate::theme::type_scale;

/// The settings row: 818 px card, 18 px padding both sides, 34 px tile, two 14 px gaps and the
/// 300 px dropdown leave this much for the title and its hint.
const HINT_ROOM: f32 = 818. - 36. - 34. - 28. - 300.;
/// The caption pill: 10 px padding, a 12 px icon and a 6 px gap, in a 640 px row that also holds
/// the live-camera button (at most about 190 px).
const PILL_ROOM: f32 = 640. - 190. - 8. - 20. - 18.;
/// Inside the 300 px dropdown: 12 px padding twice, the arrow and two gaps.
const TRIGGER_ROOM: f32 = 300. - 24. - 14. - 16.;

fn each(f: impl Fn() -> Vec<String>) -> Vec<(Lang, String)> {
    let mut out = Vec::new();
    for lang in [Lang::En, Lang::Vi] {
        for s in with_language(lang, &f) {
            out.push((lang, s));
        }
    }
    out
}

fn assert_one_line(what: &str, room: f32, size: f32, face: Face, texts: Vec<(Lang, String)>) {
    for (lang, text) in texts {
        let w = width(&text, face, size);
        assert!(
            w <= room,
            "{what} ({lang:?}): {text:?} needs {w:.0} px, room {room:.0} px"
        );
    }
}

#[test]
fn the_caption_pill_fits_in_every_combination() {
    let texts = each(|| {
        let mut v = Vec::new();
        for size in BubbleSize::ALL {
            for shape in [BubbleShape::Circle, BubbleShape::Rounded] {
                for mirror in [true, false] {
                    v.push(preview_caption(size, shape, mirror));
                }
            }
        }
        v
    });
    assert_eq!(texts.len(), 24);
    assert_one_line(
        "caption pill",
        PILL_ROOM,
        type_scale::CAPTION,
        Face::Regular,
        texts,
    );
}

#[test]
fn the_hint_pills_and_the_live_button_fit() {
    assert_one_line(
        "device hint pill",
        110. - 16.,
        11.,
        Face::Regular,
        each(|| vec![Hint::Default.label(), Hint::NotConnected.label()]),
    );
    assert_one_line(
        "live camera button",
        190. - 24. - 20.,
        type_scale::CAPTION,
        Face::Regular,
        each(|| {
            vec![
                cm_i18n::tr!("Show live camera"),
                cm_i18n::tr!("Hide live camera"),
            ]
        }),
    );
    assert_one_line(
        "permission missing",
        300.,
        type_scale::CAPTION,
        Face::Regular,
        each(|| vec![cm_i18n::tr!("Camera permission is missing")]),
    );
}

#[test]
fn the_row_hints_wrap_to_at_most_two_lines() {
    for (lang, text) in each(|| {
        vec![
            cm_i18n::tr!("Used for the bubble and the camera track."),
            cm_i18n::tr!("System default input unless you pick one."),
        ]
    }) {
        let n = lines(&text, Face::Regular, 12.5, HINT_ROOM);
        assert!(
            n.is_some_and(|n| n <= 2),
            "row hint ({lang:?}) {text:?}: {n:?} lines in {HINT_ROOM:.0} px"
        );
    }
}

#[test]
fn the_fallback_note_fits_in_two_lines() {
    for (lang, text) in each(|| vec![fallback_note_text()]) {
        // 818 px card, 18 px padding, a 14 px icon and an 8 px gap.
        let room = 818. - 36. - 14. - 8.;
        let n = lines(&text, Face::Regular, type_scale::CAPTION, room);
        assert!(
            n.is_some_and(|n| n <= 2),
            "note ({lang:?}) {text:?}: {n:?} lines"
        );
    }
}

/// The closed dropdown shows the saved name and a "Not connected" tag; a long device name is cut
/// with an ellipsis by the layout (`truncate`), the tag always stays.
#[test]
fn the_not_connected_tag_leaves_room_for_a_name() {
    for lang in [Lang::En, Lang::Vi] {
        with_language(lang, || {
            let tag = width(&Hint::NotConnected.label(), Face::Regular, 11.) + 16. + 8.;
            let left = TRIGGER_ROOM - tag;
            assert!(
                left >= 90.,
                "{lang:?}: only {left:.0} px left for the device name"
            );
            let d = Dropdown::build(DeviceKind::Mic, &[], true, Some("x"), None);
            assert_eq!(d.trigger_hint, Some(Hint::NotConnected));
            assert_one_line(
                "generic saved name",
                left,
                type_scale::UI,
                Face::Regular,
                vec![(lang, d.trigger)],
            );
        });
    }
}

/// English is the longest in the table above, so the pseudo-locale is only reported.
#[test]
fn the_pseudo_locale_is_wider_than_english() {
    let en = with_language(Lang::En, || {
        width(
            &preview_caption(BubbleSize::M, BubbleShape::Circle, true),
            Face::Regular,
            12.,
        )
    });
    let xa = with_language(Lang::EnXa, || {
        width(
            &preview_caption(BubbleSize::M, BubbleShape::Circle, true),
            Face::Regular,
            12.,
        )
    });
    assert!(xa > en);
}
