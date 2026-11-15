//! 0.3.4 QA pass 5: Blueprint and Retype must cover the whole text line even when the recognizer
//! reports only the letters' bodies (Windows: 17 to 19 px boxes for 28 px text). Synthetic boxes,
//! no OS recognizer. Text is drawn with cm-render's own layout (a `text` op) at 28 px.
#![allow(clippy::unwrap_used)]

use cm_core::annotation::BlueprintStyle;
use cm_render::ocr::pad_line_boxes;
use cm_render::{blueprint_bars, parse_ops_str, render_annotations, Rc};
use image::{Rgba, RgbaImage};

fn white(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]))
}

/// Ink bounding box `[x0, y0, x1, y1)` of a picture on white.
fn ink_bbox(img: &RgbaImage) -> Option<[u32; 4]> {
    let mut b: Option<[u32; 4]> = None;
    for (x, y, p) in img.enumerate_pixels() {
        if p.0[0] < 200 {
            b = Some(match b {
                None => [x, y, x + 1, y + 1],
                Some(o) => [o[0].min(x), o[1].min(y), o[2].max(x + 1), o[3].max(y + 1)],
            });
        }
    }
    b
}

fn text_scene(lines: &[(&str, f64, f64)], size: f64) -> RgbaImage {
    let ops: Vec<String> = lines
        .iter()
        .map(|(t, x, y)| {
            format!(
                r##"{{"op":"text","at":[{x},{y}],"text":"{t}","size":{size},"color":"#000000"}}"##
            )
        })
        .collect();
    render_annotations(
        &white(420, 260),
        &parse_ops_str(&format!("[{}]", ops.join(","))).unwrap(),
    )
    .unwrap()
}

/// The box a Windows-like recognizer reports for the ink inside `region`: the letters' bodies,
/// 2 px off the top and 1 px off the bottom, 1 px off each side (17 to 19 px for 28 px text).
fn windows_box(img: &RgbaImage, region: [u32; 4]) -> Rc {
    let mut sub = white(img.width(), img.height());
    for y in region[1]..region[3] {
        for x in region[0]..region[2] {
            sub.put_pixel(x, y, *img.get_pixel(x, y));
        }
    }
    let b = ink_bbox(&sub).unwrap();
    Rc::new(
        f64::from(b[0] + 1),
        f64::from(b[1] + 2),
        f64::from(b[2] - b[0] - 2),
        f64::from(b[3] - b[1] - 3),
    )
}

fn blueprint(img: &RgbaImage, boxes: &[Rc]) -> (RgbaImage, Vec<[i64; 4]>) {
    let b: Vec<String> = boxes
        .iter()
        .map(|b| format!("[{},{},{},{}]", b.x, b.y, b.w, b.h))
        .collect();
    let json = format!(
        r##"[{{"op":"blueprint","boxes":[{}],"style":"bar","color":"#222222"}}]"##,
        b.join(",")
    );
    let out = render_annotations(img, &parse_ops_str(&json).unwrap()).unwrap();
    (out, blueprint_bars(boxes, BlueprintStyle::Bar))
}

/// (pixels not white outside every bar, pixels checked).
fn outside_bars(out: &RgbaImage, bars: &[[i64; 4]]) -> (usize, usize) {
    let (mut bad, mut n) = (0, 0);
    for (x, y, p) in out.enumerate_pixels() {
        let (x, y) = (i64::from(x), i64::from(y));
        // One pixel of slack for the bar's rounded corners and anti-aliasing.
        let inside = bars
            .iter()
            .any(|b| x >= b[0] - 1 && x < b[2] + 1 && y >= b[1] - 1 && y < b[3] + 1);
        if !inside {
            n += 1;
            if p.0[0] < 250 {
                bad += 1;
            }
        }
    }
    (bad, n)
}

/// Blueprint over padded 17 to 19 px boxes leaves no original ink outside the bars.
#[test]
fn blueprint_covers_every_glyph_pixel_of_short_boxes() {
    // Lines without descenders, as the QA images ("Team", "Plan:", "Amount: 99.00").
    let img = text_scene(
        &[("Team", 20.0, 20.0), ("Plan: Item 99.00", 20.0, 80.0)],
        28.0,
    );
    let regions = [[0, 0, 420, 70], [0, 70, 420, 130]];
    let tight: Vec<Rc> = regions.iter().map(|r| windows_box(&img, *r)).collect();
    for b in &tight {
        assert!((15.0..=24.0).contains(&b.h), "fixture box {b:?}");
    }
    // Without padding ink is left (the bug) ...
    let (out, bars) = blueprint(&img, &tight);
    assert!(
        outside_bars(&out, &bars).0 > 0,
        "the fixture must show the bug"
    );
    // ... with it nothing is.
    let padded = pad_line_boxes(&tight, img.width(), img.height());
    for (p, t) in padded.iter().zip(&tight) {
        assert!(p.h > t.h && p.x < t.x && p.y < t.y, "{p:?} vs {t:?}");
    }
    let (out, bars) = blueprint(&img, &padded);
    let (left, total) = outside_bars(&out, &bars);
    assert_eq!(
        left, 0,
        "{left} of {total} px are not white outside the bars"
    );
}

#[test]
fn padding_rules_clamp_to_the_image_and_neighbours() {
    // 18 px box: +3.6 top, +2.7 bottom, +2 each side.
    let p = pad_line_boxes(&[Rc::new(50.0, 50.0, 100.0, 18.0)], 400, 300);
    assert_eq!(p[0], Rc::new(48.0, 46.0, 104.0, 25.0));
    // At the image edge.
    let p = pad_line_boxes(&[Rc::new(0.0, 0.0, 400.0, 18.0)], 400, 20);
    assert_eq!(p[0], Rc::new(0.0, 0.0, 400.0, 20.0));
    // Two lines 2 px apart: the padded boxes overlap by at most 1 px.
    let two = pad_line_boxes(
        &[
            Rc::new(10.0, 10.0, 100.0, 18.0),
            Rc::new(10.0, 30.0, 100.0, 18.0),
        ],
        400,
        300,
    );
    assert!(two[0].y + two[0].h - two[1].y <= 1.0 + 1e-9, "{two:?}");
    // Boxes side by side are not neighbours: no shrinking.
    let side = pad_line_boxes(
        &[
            Rc::new(10.0, 10.0, 40.0, 18.0),
            Rc::new(80.0, 11.0, 40.0, 18.0),
        ],
        400,
        300,
    );
    assert_eq!(side[0].y, 6.0);
}

/// Retype in a box about as tall as the font clips nothing: the result matches the same text in
/// a 56 px box centred on the same line, for Vietnamese marks below the line and for tails.
#[test]
fn retype_in_a_short_box_keeps_marks_and_tails() {
    for text in ["ạ ệ ị ọ ụ ỵ", "gypqj", "Ế Ộ ặ"] {
        let img = white(420, 120);
        let run = |h: f64| {
            let y = 60.0 - h / 2.0;
            let j = format!(
                r##"[{{"op":"retype","rect":[20,{y},380,{h}],"fill":{{"kind":"solid","colors":["#ffffff"]}},"text":"{text}","size":28,"color":"#000000"}}]"##
            );
            render_annotations(&img, &parse_ops_str(&j).unwrap()).unwrap()
        };
        let small = run(18.0);
        let big = run(56.0);
        assert_eq!(
            ink_bbox(&small),
            ink_bbox(&big),
            "{text}: ink differs between an 18 px and a 56 px box"
        );
    }
}

/// The old text's tails and marks below a short recognized box are erased with the padded box.
#[test]
fn retype_erases_the_old_tails_with_a_padded_source_box() {
    let img = text_scene(&[("Example", 20.0, 20.0), ("Plan", 20.0, 80.0)], 28.0);
    let tight = windows_box(&img, [0, 0, 420, 70]);
    let padded = pad_line_boxes(&[tight], img.width(), img.height())[0];
    let j = format!(
        r##"[{{"op":"retype","rect":[{},{},{},{}],"fill":{{"kind":"solid","colors":["#ffffff"]}},"text":"","size":28}}]"##,
        padded.x, padded.y, padded.w, padded.h
    );
    let out = render_annotations(&img, &parse_ops_str(&j).unwrap()).unwrap();
    let first_line = ink_bbox(&{
        let mut top = out.clone();
        for y in 70..top.height() {
            for x in 0..top.width() {
                top.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        top
    });
    assert_eq!(first_line, None, "old ink left under a padded source box");
}
