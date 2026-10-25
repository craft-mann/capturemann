//! Golden-image tests, one per op type (ANN-1, ANN-2). Fixtures live in `tests/golden/`.
//!
//! Regenerate after an intentional rendering change:
//! `UPDATE_GOLDEN=1 cargo test -p cm-render --test golden`, then review the PNG diffs.
#![allow(clippy::unwrap_used)]

mod common;
use cm_render::render_annotations;
use common::*;

const LIMIT: f64 = 0.01; // ANN-1.AC1: within 1% pixel difference

fn base() -> image::RgbaImage {
    base_image(200, 120)
}

fn check(name: &str, json: &str, max_frac: f64) -> image::RgbaImage {
    let out = render_annotations(&base(), &ops(json)).unwrap();
    assert_golden(name, &out, max_frac);
    out
}

/// ANN-1.AC1: the arrow from the spec example (scaled to the fixture) matches the golden.
#[test]
fn ann1_ac1_arrow() {
    let out = check(
        "arrow",
        r##"[{"id":"op_7","op":"arrow","from":[30,90],"to":[170,30],"color":"#FF3B3B","width":4}]"##,
        LIMIT,
    );
    assert_ne!(out, base());
}

#[test]
fn ann1_line() {
    check(
        "line",
        r##"[{"id":"op_10","op":"line","from":[10,60],"to":[190,60],"color":"#111111","width":2}]"##,
        LIMIT,
    );
}

#[test]
fn ann1_rect_rounded_filled() {
    check(
        "rect",
        r##"[{"op":"rect","rect":[20,30,80,50],"color":"#FF3B3B","width":4,"fill":null,"radius":6},
           {"op":"rect","rect":[110,30,70,50],"color":"#2E8BFF","width":3,"fill":"#FFD60A","radius":0}]"##,
        LIMIT,
    );
}

#[test]
fn ann1_ellipse() {
    check(
        "ellipse",
        r##"[{"op":"ellipse","rect":[30,25,140,70],"color":"#2E8BFF","width":4}]"##,
        LIMIT,
    );
}

#[test]
fn ann1_text_with_outline() {
    let out = check(
        "text",
        r##"[{"op":"text","at":[12,40],"text":"Click Save\nsecond line","size":24,"color":"#FFFFFF","outline":"#000000","bg":null}]"##,
        LIMIT,
    );
    assert!(
        diff_fraction(&out, &base(), 8) > 0.02,
        "text must be visible"
    );
}

#[test]
fn ann1_text_with_background() {
    check(
        "text_bg",
        r##"[{"op":"text","at":[16,50],"text":"Step one","size":20,"color":"#FFFFFF","outline":null,"bg":"#111111"}]"##,
        LIMIT,
    );
}

#[test]
fn ann1_highlight() {
    check(
        "highlight",
        r##"[{"op":"highlight","rect":[20,70,150,28],"color":"#FFD60A","opacity":0.35}]"##,
        LIMIT,
    );
}

#[test]
fn ann1_step_badges() {
    check(
        "step",
        r##"[{"op":"step","at":[30,40],"n":1,"size":28,"color":"#FF3B3B"},
           {"op":"step","at":[80,40],"n":2,"size":28,"color":"#2E8BFF"},
           {"op":"step","at":[130,40],"n":12,"size":28,"color":"#2FD158"}]"##,
        LIMIT,
    );
}

#[test]
fn ann1_ac4_crop_golden() {
    let out = check(
        "crop",
        r##"[{"op":"arrow","from":[10,100],"to":[150,60],"color":"#FF3B3B","width":4},
           {"op":"crop","rect":[0,0,120,80]}]"##,
        LIMIT,
    );
    assert_eq!(out.dimensions(), (120, 80));
}

#[test]
fn ann2_blur_golden() {
    check(
        "blur",
        r##"[{"op":"blur","rect":[16,66,80,40],"strength":12}]"##,
        LIMIT,
    );
}

#[test]
fn ann2_pixelate_golden() {
    check(
        "pixelate",
        r##"[{"op":"pixelate","rect":[16,66,80,40],"block":12}]"##,
        LIMIT,
    );
}

/// ANN-2.AC2: blur at strength 12 on a 2x image matches its golden.
#[test]
fn ann2_ac2_blur_2x_golden() {
    let base2 = base_image(400, 240);
    let out = render_annotations(
        &base2,
        &ops(r##"[{"op":"blur","rect":[32,132,160,80],"strength":12},
                 {"op":"rect","rect":[40,60,120,60],"color":"#FF3B3B","width":8}]"##),
    )
    .unwrap();
    assert_golden("blur_2x", &out, LIMIT);
}
