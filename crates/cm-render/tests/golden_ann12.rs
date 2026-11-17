//! Golden-image tests for line styles (ANN-12): dashed and dotted `arrow`, `line`, `rect`,
//! `ellipse` at 1x and 2x. Regenerate with
//! `UPDATE_GOLDEN=1 cargo test -p cm-render --test golden_ann12`, then review the PNGs.
#![allow(clippy::unwrap_used)]

mod common;
use cm_render::render_annotations;
use common::*;
use image::{Rgba, RgbaImage};

const LIMIT: f64 = 0.01; // ANN-12.AC1: within 1 % of pixels

/// One op of `kind` in `style` at scale `k` (1 or 2): every coordinate and the width scale.
fn shape(kind: &str, style: &str, k: u32) -> String {
    let f = f64::from(k);
    let w = 3.0 * f;
    let st = if style == "solid" {
        String::new()
    } else {
        format!(r#","style":"{style}""#)
    };
    match kind {
        "line" => format!(
            r##"[{{"op":"line","from":[{},{}],"to":[{},{}],"color":"#111111","width":{w}{st}}}]"##,
            10.0 * f,
            60.0 * f,
            190.0 * f,
            60.0 * f
        ),
        "arrow" => format!(
            r##"[{{"op":"arrow","from":[{},{}],"to":[{},{}],"color":"#FF3B3B","width":{w}{st}}}]"##,
            20.0 * f,
            95.0 * f,
            180.0 * f,
            40.0 * f
        ),
        "rect" => format!(
            r##"[{{"op":"rect","rect":[{},{},{},{}],"color":"#2E8BFF","width":{w},"radius":{}{st}}}]"##,
            20.0 * f,
            25.0 * f,
            150.0 * f,
            70.0 * f,
            8.0 * f
        ),
        _ => format!(
            r##"[{{"op":"ellipse","rect":[{},{},{},{}],"color":"#2E8BFF","width":{w}{st}}}]"##,
            25.0 * f,
            25.0 * f,
            150.0 * f,
            70.0 * f
        ),
    }
}

fn render(kind: &str, style: &str, k: u32) -> RgbaImage {
    let base = base_image(200 * k, 120 * k);
    render_annotations(&base, &ops(&shape(kind, style, k))).unwrap()
}

/// ANN-12.AC1: every shape and style matches its golden at 1x and 2x.
#[test]
fn ann12_ac1_goldens_1x_2x() {
    for kind in ["line", "arrow", "rect", "ellipse"] {
        for style in ["dashed", "dotted"] {
            for k in [1, 2] {
                let out = render(kind, style, k);
                assert_golden(&format!("ann12_{kind}_{style}_{k}x"), &out, LIMIT);
            }
        }
    }
}

/// ANN-12.AC4: no `style`, or `"solid"`, renders exactly as before and `solid` is not written.
#[test]
fn ann12_ac4_solid_is_unchanged() {
    for kind in ["line", "arrow", "rect", "ellipse"] {
        let base = base_image(200, 120);
        let plain = shape(kind, "solid", 1);
        let explicit = plain.replacen('}', r#","style":"solid"}"#, 1);
        let a = render_annotations(&base, &ops(&plain)).unwrap();
        let b = render_annotations(&base, &ops(&explicit)).unwrap();
        assert_eq!(a, b, "{kind}");
        let written = serde_json::to_string(&ops(&explicit)).unwrap();
        assert!(!written.contains("style"), "{kind}: {written}");
        assert_ne!(a, render(kind, "dashed", 1), "{kind}");
    }
}

fn is_ink(p: &Rgba<u8>) -> bool {
    p.0[0] < 60 && p.0[1] < 60 && p.0[2] < 60
}

/// ANN-12.AC2: a dotted line is round dots, spaced 2 x width between dots (centre to centre
/// 2 x width, since a dot has no length of its own).
#[test]
fn ann12_ac2_dotted_line_spacing() {
    let base = RgbaImage::from_pixel(200, 40, Rgba([255, 255, 255, 255]));
    let out = render_annotations(
        &base,
        &ops(
            r##"[{"op":"line","from":[10,20],"to":[190,20],"color":"#000000","width":4,"style":"dotted"}]"##,
        ),
    )
    .unwrap();
    let mut starts = vec![];
    let mut prev = false;
    for x in 0..200 {
        let ink = is_ink(out.get_pixel(x, 20));
        if ink && !prev {
            starts.push(x as i32);
        }
        prev = ink;
    }
    assert!(starts.len() >= 15, "{starts:?}");
    for w in starts.windows(2).skip(1).take(10) {
        assert!((w[1] - w[0] - 8).abs() <= 1, "{starts:?}");
    }
    // Round: a dot is as tall as it is wide (4 px), not a long bar.
    let x = starts[3] as u32;
    let col = (0..40)
        .filter(|&y| out.get_pixel(x + 2, y).0[0] < 128)
        .count();
    assert!((3..=5).contains(&col), "{col}");
}

/// ANN-12.AC2 (minimum width): a 1 px dotted line is drawn 2 px wide.
#[test]
fn ann12_dotted_minimum_width() {
    let base = RgbaImage::from_pixel(100, 20, Rgba([255, 255, 255, 255]));
    let out = render_annotations(
        &base,
        &ops(
            r##"[{"op":"line","from":[5,10],"to":[95,10],"color":"#000000","width":1,"style":"dotted"}]"##,
        ),
    )
    .unwrap();
    let ink_rows = (0..20)
        .filter(|&y| (0..100).any(|x| out.get_pixel(x, y).0[0] < 100))
        .count();
    assert!(ink_rows >= 2, "{ink_rows}");
}

/// ANN-12.AC3: a dashed arrow's head is solid, the shaft is not.
#[test]
fn ann12_ac3_arrow_head_is_solid() {
    let base = RgbaImage::from_pixel(200, 60, Rgba([255, 255, 255, 255]));
    let out = render_annotations(
        &base,
        &ops(
            r##"[{"op":"arrow","from":[10,30],"to":[190,30],"color":"#000000","width":4,"style":"dashed"}]"##,
        ),
    )
    .unwrap();
    // Head: length 16, so x in 176..184 is solid on the axis.
    assert!((176..184).all(|x| is_ink(out.get_pixel(x, 30))));
    // Shaft: dash 16 + gap 10 leaves gaps before the head.
    let gaps = (10..170).filter(|&x| !is_ink(out.get_pixel(x, 30))).count();
    assert!(gaps > 20, "{gaps}");
}

/// ANN-12.AC8: a bad value is `invalid_input` and names the op index.
#[test]
fn ann12_ac8_bad_style_names_the_op() {
    let err = cm_render::parse_ops_str(
        r##"[{"op":"line","from":[0,0],"to":[5,5]},{"op":"rect","rect":[0,0,5,5],"style":"wavy"}]"##,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("ops[1]"), "{msg}");
}
