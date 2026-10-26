//! Width of the texts on the Webcam & audio page (R13, R13b) in English and Vietnamese, measured
//! in the bundled Geist fonts like `layout_width_tests`. The widths come from the sizes in
//! `webcam_view.rs`, `audio_check.rs`, `check_widgets.rs` and `widgets::settings_row`. Labels that
//! cannot wrap (buttons, chips, tags) must fit one line; sentences must stay within a line count
//! the cards have room for. The audio card is the narrow one: its basis is 300 px.

use cm_i18n::{tr, tr_ctx, with_language, Lang};

use super::webcam_model::{fallback_note_text, DeviceKind, Dropdown, Hint};
use crate::text_metrics_tests::{lines, width, Face};
use crate::theme::type_scale;

/// Inside the audio card at its narrowest: 300 px, 16 px padding on both sides, a 1 px border.
const AUDIO_INNER: f32 = 300. - 32. - 2.;
/// A state block inside it: 14 px padding on both sides and a 1 px border.
const BLOCK_INNER: f32 = AUDIO_INNER - 28. - 2.;
/// The microphone dropdown on its own row: 12 px padding twice, the arrow and two 8 px gaps.
const TRIGGER_ROOM: f32 = AUDIO_INNER - 24. - 13. - 16.;
/// The settings card in a column at its narrowest (380 px basis): the 18 px padding twice, the
/// 34 px tile, a 14 px gap, then the switch (44 px) and another 14 px gap.
const HINT_ROOM: f32 = 380. - 36. - 34. - 14. - 44. - 14.;

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

fn assert_max_lines(
    what: &str,
    room: f32,
    size: f32,
    face: Face,
    max: usize,
    texts: Vec<(Lang, String)>,
) {
    for (lang, text) in texts {
        let n = lines(&text, face, size, room);
        assert!(
            n.is_some_and(|n| n <= max),
            "{what} ({lang:?}): {text:?} takes {n:?} lines in {room:.0} px, at most {max}"
        );
    }
}

/// Buttons never wrap: a 30 px action button (10 px padding twice, a 13 px icon, a 6 px gap) must
/// fit the block it sits in; a 34 px control button (12 px padding twice, a 14 px icon, a 7 px
/// gap) the card.
#[test]
fn the_buttons_fit_on_one_line() {
    assert_one_line(
        "action button",
        BLOCK_INNER - 20. - 13. - 6.,
        12.5,
        Face::Regular,
        each(|| {
            vec![
                tr!("Turn on camera"),
                tr!("Allow access"),
                tr!("Try again"),
                tr!("Look again"),
                tr!("Test microphone"),
                tr!("Choose mic"),
                tr!("Sound settings"),
            ]
        }),
    );
    assert_one_line(
        "control button",
        AUDIO_INNER - 24. - 14. - 7.,
        13.,
        Face::Regular,
        each(|| {
            vec![
                tr!("Turn off camera"),
                tr!("Turn off microphone"),
                tr!("Play test sound"),
            ]
        }),
    );
}

/// The chips are one line next to their heading: the card minus the heading ("Microphone" or
/// "System audio", at most 100 px), the chip's padding and icon, and the gaps.
#[test]
fn the_chips_fit_beside_their_heading() {
    assert_one_line(
        "status chip",
        AUDIO_INNER - 100. - 8. - 16. - 12. - 6. - 2. - 14.,
        11.5,
        Face::Regular,
        each(|| {
            vec![
                tr_ctx!("mic verdict", "Too quiet"),
                tr_ctx!("mic verdict", "Good level"),
                tr_ctx!("mic verdict", "Too loud"),
                tr_ctx!("system audio status", "Heard"),
                tr_ctx!("system audio status", "Listening\u{2026}"),
            ]
        }),
    );
    // The camera chip sits in the 24 px chip of the card header, beside the mono readout.
    assert_one_line(
        "camera chip",
        120.,
        12.,
        Face::Regular,
        each(|| {
            vec![
                tr_ctx!("camera status", "Live"),
                tr_ctx!("camera status", "Starting\u{2026}"),
            ]
        }),
    );
}

#[test]
fn the_camera_readout_fits_its_header() {
    let w = width(
        "1920 \u{00D7} 1080 \u{00B7} 30 fps",
        Face::MonoRegular,
        11.5,
    );
    // The header of the camera card at its narrowest (420 px): 16 px padding twice, the icon, the
    // title (about 110 px), the chip (about 70 px) and the gaps.
    assert!(w <= 420. - 32. - 16. - 110. - 70. - 30., "{w:.0}");
}

#[test]
fn the_mirrored_tag_fits_the_feed() {
    assert_one_line(
        "mirrored tag",
        300. - 18. - 12. - 6.,
        11.5,
        Face::Regular,
        each(|| vec![tr!("Mirrored, like the bubble")]),
    );
}

#[test]
fn the_block_titles_take_at_most_two_lines() {
    // The title shares its row with the 30 px tile and a 10 px gap.
    assert_max_lines(
        "block title",
        BLOCK_INNER - 30. - 10.,
        14.,
        Face::SemiBold,
        2,
        each(|| {
            vec![
                tr!("Camera is off"),
                tr!("Camera access needed"),
                tr!("Camera is in use"),
                tr!("No camera found"),
                tr!("The camera didn\u{2019}t start"),
                tr!("Recording in progress"),
                tr!("Microphone is off"),
                tr!("Microphone access needed"),
                tr!("Too quiet"),
                tr!("Too loud, clipping"),
                tr!("No sound coming in"),
                tr!("The microphone didn\u{2019}t start"),
                tr!("Chime not heard"),
                tr!("Screen Recording access needed"),
                tr!("The test sound didn\u{2019}t play"),
            ]
        }),
    );
}

#[test]
fn the_block_sentences_take_at_most_four_lines() {
    assert_max_lines(
        "block sentence",
        BLOCK_INNER,
        12.5,
        Face::Regular,
        4,
        each(|| {
            vec![
                tr!("Nothing is shown until you ask. The light on your camera stays off."),
                tr!("CaptureMann isn\u{2019}t allowed to use the camera yet."),
                tr!(
                    "Another app is using {camera}. Close it, or pick another camera.",
                    camera = "Logitech BRIO Ultra HD Webcam"
                ),
                tr!("Plug in a camera, or record without the bubble."),
                tr!("CaptureMann couldn\u{2019}t open the camera. Try again, or pick another camera."),
                tr!("Recording in progress, the check is paused."),
                tr!("Nothing is heard until you ask. The orange microphone dot stays off."),
                tr!("CaptureMann isn\u{2019}t allowed to use the microphone yet."),
                tr!("Your voice barely reaches the good range. Move closer or pick another mic."),
                tr!("Peaks hit the top. Lower the input volume in System Settings."),
                tr!("The mic is selected but silent. It may be muted with a hardware switch."),
                tr!("CaptureMann couldn\u{2019}t open the microphone. Pick another one, or try again."),
                tr!("System audio needs Screen Recording access. Check your output is not muted."),
                tr!("System audio comes with Screen Recording, and CaptureMann isn\u{2019}t allowed to use it yet."),
                tr!("The check could not run. Try again."),
            ]
        }),
    );
}

#[test]
fn the_footnotes_and_the_hint_wrap_to_few_lines() {
    // A 12 px icon and a 7 px gap come first.
    assert_max_lines(
        "footnote",
        AUDIO_INNER - 19.,
        type_scale::CAPTION,
        Face::Regular,
        3,
        each(|| {
            vec![
                tr!("Preview only. Nothing is recorded, and the camera turns off when you leave this page."),
                tr!("Level only. Nothing is recorded; the mic turns off when you leave."),
            ]
        }),
    );
    assert_max_lines(
        "system audio hint",
        AUDIO_INNER,
        12.,
        Face::Regular,
        4,
        each(|| {
            vec![tr!(
                "Plays a short chime and checks that CaptureMann can hear it. Uses the Screen Recording permission."
            )]
        }),
    );
    // The position hint: 14 px padding twice, a 14 px icon and a 10 px gap, in a 380 px column.
    assert_max_lines(
        "position hint",
        380. - 28. - 2. - 14. - 10.,
        12.5,
        Face::Regular,
        3,
        each(|| {
            vec![tr!(
                "Drag the bubble anywhere before or during a recording. It snaps to corners and edge midpoints."
            )]
        }),
    );
}

#[test]
fn the_meter_texts_fit() {
    // The prompt shares its row with the 18 px mono readout ("-60 dB" at most about 70 px).
    assert_max_lines(
        "meter prompt",
        AUDIO_INNER - 28. - 2. - 70. - 8.,
        12.,
        Face::Regular,
        2,
        each(|| vec![tr!("Speak as you would while recording")]),
    );
    // The band is 36.7 percent of the meter; its label has 4 px padding twice and a 6 px offset.
    let band = (super::audio_check_model::band().1) * (AUDIO_INNER - 28. - 2.);
    assert_one_line(
        "good range label",
        band - 6. - 8.,
        10.,
        Face::Regular,
        each(|| vec![tr!("good range")]),
    );
    assert_one_line(
        "meter ends",
        (AUDIO_INNER - 30.) / 2.,
        10.,
        Face::MonoRegular,
        each(|| vec![tr_ctx!("meter end", "quiet"), tr_ctx!("meter end", "loud")]),
    );
}

#[test]
fn the_device_hint_pills_fit() {
    assert_one_line(
        "device hint pill",
        110. - 16.,
        11.,
        Face::Regular,
        each(|| vec![Hint::Default.label(), Hint::NotConnected.label()]),
    );
}

#[test]
fn the_settings_row_hints_take_at_most_two_lines() {
    assert_max_lines(
        "row hint",
        HINT_ROOM,
        12.5,
        Face::Regular,
        2,
        each(|| {
            vec![
                tr!("The ring is always the record color."),
                tr!("Also on the bubble\u{2019}s hover toolbar."),
                tr!("The recorded camera track is never flipped."),
                tr!("Off: every recording starts bottom-left."),
                tr!("Reduces steady background noise. The original sound is kept too."),
            ]
        }),
    );
}

#[test]
fn the_fallback_note_fits_in_two_lines() {
    for (lang, text) in each(|| vec![fallback_note_text()]) {
        // The audio card, a 14 px icon and an 8 px gap.
        let room = AUDIO_INNER - 14. - 8.;
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
                13.,
                Face::Regular,
                vec![(lang, d.trigger)],
            );
        });
    }
}
