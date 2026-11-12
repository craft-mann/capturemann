//! Golden-image tests for the 0.3.3 ops: callout and stamp (ANN-4), step label and styles
//! (ANN-6), spotlight (ANN-7). Regenerate with
//! `UPDATE_GOLDEN=1 cargo test -p cm-render --test golden_033`, then review the PNGs.
#![allow(clippy::unwrap_used)]

mod common;
use cm_core::ErrorCode;
use cm_render::{callout_box, render_annotations};
use common::*;
use image::Rgba;

const LIMIT: f64 = 0.01; // 1 % of pixels

fn base() -> image::RgbaImage {
    base_image(300, 180)
}

fn check(name: &str, json: &str) -> image::RgbaImage {
    let out = render_annotations(&base(), &ops(json)).unwrap();
    assert_golden(name, &out, LIMIT);
    out
}

const BUBBLE: &str = r##"{"op":"callout","rect":[40,30,150,50],"tail":[70,140],"text":"Click here","size":24,"color":"#FFFFFF","bg":"#2E8BFF","radius":12,"style":"bubble"}"##;
const BOXC: &str = r##"{"op":"callout","rect":[40,30,150,50],"tail":[250,100],"text":"Click here","size":24,"color":"#FFFFFF","bg":"#FF3B3B","radius":12,"style":"box"}"##;

/// ANN-4.AC3: the bubble and the box styles at 1x.
#[test]
fn ann4_ac3_callout_bubble_1x() {
    check("callout_bubble", &format!("[{BUBBLE}]"));
}

#[test]
fn ann4_ac3_callout_box_1x() {
    check("callout_box", &format!("[{BOXC}]"));
}

/// ANN-4.AC3: and at 2x (every coordinate and size doubled).
#[test]
fn ann4_ac3_callout_2x() {
    let json = r##"[{"op":"callout","rect":[80,60,300,100],"tail":[140,280],"text":"Click here","size":48,"color":"#FFFFFF","bg":"#2E8BFF","radius":24,"style":"bubble"},
        {"op":"callout","rect":[80,60,300,100],"tail":[500,200],"text":"Click here","size":48,"color":"#FFFFFF","bg":"#FF3B3B","radius":24,"style":"box"}]"##;
    let out = render_annotations(&base_image(600, 360), &ops(json)).unwrap();
    assert_golden("callout_2x", &out, LIMIT);
}

/// ANN-4.AC1: a box smaller than its text grows; `callout_box` returns the drawn box.
#[test]
fn ann4_ac1_box_grows_to_fit_text() {
    let json = r##"[{"id":"c","op":"callout","rect":[20,40,30,20],"tail":[60,150],"text":"A much longer text","size":24,"color":"#FFFFFF","bg":"#333333","radius":10,"style":"bubble"}]"##;
    let o = ops(json);
    let b = callout_box(&o[0].kind).unwrap();
    assert!(b.w > 100.0 && b.h > 20.0, "box did not grow: {b:?}");
    assert_eq!((b.x, b.y), (20.0, 40.0));
    let out = render_annotations(&base(), &o).unwrap();
    // The centre of the grown box is the callout fill (the text is white, the centre column
    // between glyphs is not guaranteed, so test the left padding area).
    let px = out.get_pixel((b.x + 6.0) as u32, (b.y + b.h / 2.0) as u32);
    assert_eq!(px.0[..3], [0x33, 0x33, 0x33]);
    // Outside the grown box the image is untouched.
    let far = (b.x + b.w + 6.0) as u32;
    assert_eq!(out.get_pixel(far, 50), base().get_pixel(far, 50));
    assert_golden("callout_grows", &out, LIMIT);
}

/// `callout_box` is `None` for other ops and the stored (normalized) rect when it is large enough.
#[test]
fn ann4_callout_box_other_ops_and_large_rect() {
    let o = ops(
        r##"[{"op":"rect","rect":[1,2,3,4]},{"op":"callout","rect":[10,10,200,100],"tail":[0,0],"text":"hi"}]"##,
    );
    assert!(callout_box(&o[0].kind).is_none());
    let b = callout_box(&o[1].kind).unwrap();
    assert_eq!((b.x, b.y, b.w, b.h), (10.0, 10.0, 200.0, 100.0));
}

/// ANN-4.AC2: moving the box leaves the tail tip on the same pixel.
#[test]
fn ann4_ac2_tail_fixed_when_box_moves() {
    for style in ["bubble", "box"] {
        let at = |dy: i32| {
            let json = format!(
                r##"[{{"op":"callout","rect":[60,{}, 120,50],"tail":[100,150],"text":"Hi","size":20,"color":"#FFFFFF","bg":"#2E8BFF","radius":10,"style":"{style}"}}]"##,
                30 + dy
            );
            render_annotations(&base(), &ops(&json)).unwrap()
        };
        // 25 px above the tip, on the tail's axis: tail colour in both renders.
        for dy in [0, -10, 12] {
            let out = at(dy);
            assert_eq!(
                out.get_pixel(100, 125).0[..3],
                [0x2E, 0x8B, 0xFF],
                "{style} dy={dy}"
            );
        }
    }
}

/// ANN-4 edge cases: a tip inside the box draws no tail; a tip outside the image is clipped.
#[test]
fn ann4_edges_tip_inside_and_outside_image() {
    let inside = ops(
        r##"[{"op":"callout","rect":[40,40,120,60],"tail":[80,70],"text":"","bg":"#2E8BFF"}]"##,
    );
    let out = render_annotations(&base(), &inside).unwrap();
    assert_eq!(out.get_pixel(80, 120), base().get_pixel(80, 120));
    let outside = ops(r##"[{"op":"callout","rect":[40,40,120,60],"tail":[400,300],"text":"x"}]"##);
    render_annotations(&base(), &outside).unwrap();
}

/// ANN-4.AC4: each stamp kind.
#[test]
fn ann4_ac4_stamps() {
    check(
        "stamps",
        r##"[{"op":"stamp","at":[40,40],"kind":"check","size":50,"color":"#2EB872"},
            {"op":"stamp","at":[110,40],"kind":"cross","size":50,"color":"#FF3B3B"},
            {"op":"stamp","at":[180,40],"kind":"exclaim","size":50,"color":"#FF9F0A"},
            {"op":"stamp","at":[250,40],"kind":"question","size":50,"color":"#2E8BFF"},
            {"op":"stamp","at":[70,120],"kind":"label","size":34,"color":"#FF3B3B","text":"Before"},
            {"op":"stamp","at":[200,120],"kind":"label","size":34,"color":"#2EB872","text":"After"}]"##,
    );
}

/// ANN-4.AC4: a label stamp without text is invalid_input naming ops[i].
#[test]
fn ann4_ac4_label_without_text_is_invalid() {
    let v: serde_json::Value = serde_json::from_str(
        r##"[{"op":"rect","rect":[1,1,5,5]},{"op":"stamp","at":[10,10],"kind":"label"}]"##,
    )
    .unwrap();
    let e = cm_render::parse_ops(&v).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("ops[1]"), "{e}");
}

fn dim_json(extra: &str) -> String {
    format!(
        r##"[{extra}{{"op":"spotlight","shapes":[{{"kind":"rect","rect":[60,40,100,60]}}],"dim":0.6}}]"##
    )
}

/// ANN-7.AC1: outside the hole 40 % of the brightness, inside unchanged; golden.
#[test]
fn ann7_ac1_dim() {
    let out = check("spotlight_dim", &dim_json(""));
    let b = base();
    let (o, i) = (out.get_pixel(10, 150), out.get_pixel(100, 70));
    let src = b.get_pixel(10, 150);
    for c in 0..3 {
        let want = (f64::from(src.0[c]) * 0.4).round() as i32;
        assert!(
            (i32::from(o.0[c]) - want).abs() <= 1,
            "c{c}: {o:?} vs {src:?}"
        );
    }
    assert_eq!(i, b.get_pixel(100, 70));
}

/// ANN-7: ellipse and rounded holes together (two shapes in one op).
#[test]
fn ann7_ellipse_and_rounded() {
    check(
        "spotlight_shapes",
        r##"[{"op":"spotlight","shapes":[{"kind":"ellipse","rect":[20,30,100,80]},{"kind":"rect","rect":[160,60,110,80],"radius":20}],"dim":0.7}]"##,
    );
}

/// ANN-7.AC5: blur mode blurs the outside with `strength`, the hole stays sharp.
#[test]
fn ann7_ac5_blur_mode() {
    let out = check(
        "spotlight_blur",
        r##"[{"op":"spotlight","shapes":[{"kind":"ellipse","rect":[90,50,120,80]}],"mode":"blur","strength":12}]"##,
    );
    let b = base();
    assert_eq!(out.get_pixel(150, 90), b.get_pixel(150, 90));
    assert_ne!(out, b);
}

/// ANN-7 / ANN-2: a spotlight never un-redacts; a blur drawn before it is identical inside the
/// hole to the blur alone, and the dimmed rest keeps the blur.
#[test]
fn ann7_spotlight_does_not_unredact() {
    let blur = r##"{"op":"blur","rect":[20,90,100,40],"strength":12}"##;
    let only_blur = render_annotations(&base(), &ops(&format!("[{blur}]"))).unwrap();
    for mode in [r##""mode":"dim""##, r##""mode":"blur","strength":8"##] {
        let json = format!(
            r##"[{blur},{{"op":"spotlight","shapes":[{{"kind":"rect","rect":[0,80,150,60]}}],{mode}}}]"##
        );
        let out = render_annotations(&base(), &ops(&json)).unwrap();
        for (x, y) in [(30, 100), (60, 110), (110, 120)] {
            assert_eq!(out.get_pixel(x, y), only_blur.get_pixel(x, y), "{mode}");
        }
    }
}

/// ANN-7.AC2: two spotlights are invalid_input naming ops[1], from the parser and the renderer.
#[test]
fn ann7_ac2_two_spotlights() {
    let one = r##"{"op":"spotlight","shapes":[{"rect":[10,10,50,50]}]}"##;
    let json = format!("[{one},{one}]");
    let e = cm_render::parse_ops_str(&json).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("ops[1]"), "{e}");
    // A list built in code (not parsed) is refused by render() too.
    let mut o = ops(&format!("[{one}]"));
    o.push(o[0].clone());
    let e = render_annotations(&base(), &o).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("ops[1]"), "{e}");
}

/// ANN-7.AC6: 9 shapes, no shape, no area, dim outside 0..1.
#[test]
fn ann7_ac6_invalid_spotlights() {
    let sh = r##"{"rect":[1,1,5,5]}"##;
    let nine = vec![sh; 9].join(",");
    for bad in [
        format!(r##"[{{"op":"spotlight","shapes":[{nine}]}}]"##),
        r##"[{"op":"spotlight","shapes":[]}]"##.to_string(),
        r##"[{"op":"spotlight","shapes":[{"rect":[1,1,0,5]}]}]"##.to_string(),
        format!(r##"[{{"op":"spotlight","shapes":[{sh}],"dim":1.2}}]"##),
    ] {
        let e = cm_render::parse_ops_str(&bad).unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidInput, "{bad}");
        assert!(e.to_string().contains("ops[0]"), "{e}");
    }
}

/// ANN-6.AC1: upper style, start 3: C D E.
#[test]
fn ann6_ac1_upper_start_3() {
    check(
        "step_upper_c_d_e",
        r##"[{"op":"step","at":[40,40],"n":1,"style":"upper","start":3,"color":"#FF3B3B"},
            {"op":"step","at":[100,40],"n":2,"style":"upper","start":3,"color":"#FF3B3B"},
            {"op":"step","at":[160,40],"n":3,"style":"upper","start":3,"color":"#FF3B3B"},
            {"op":"step","at":[40,100],"n":1,"style":"lower","color":"#2E8BFF"},
            {"op":"step","at":[100,100],"n":2,"style":"lower","color":"#2E8BFF"}]"##,
    );
}

/// ANN-6.AC2: the 26th reads Z, the 27th AA (labels), and AA fits in the disc (golden).
#[test]
fn ann6_ac2_z_then_aa() {
    use cm_core::annotation::{step_label, StepStyle};
    assert_eq!(step_label(StepStyle::Upper, 1, 26), "Z");
    assert_eq!(step_label(StepStyle::Upper, 1, 27), "AA");
    check(
        "step_z_aa",
        r##"[{"op":"step","at":[50,60],"n":26,"style":"upper","color":"#FF3B3B"},
            {"op":"step","at":[120,60],"n":27,"style":"upper","color":"#FF3B3B"},
            {"op":"step","at":[190,60],"n":28,"style":"lower","color":"#2E8BFF"}]"##,
    );
}

/// ANN-6.AC3: the label pill sits right of the badge and flips left near the right edge.
#[test]
fn ann6_ac3_label_flips_near_right_edge() {
    let out = check(
        "step_label_flip",
        r##"[{"op":"step","at":[30,40],"n":1,"text":"Open Settings"},
            {"op":"step","at":[270,110],"n":2,"text":"Choose Network"}]"##,
    );
    let b = base();
    // Row 40: pixels right of badge 1 changed (pill), pixels left of badge 2 changed.
    assert_ne!(out.get_pixel(80, 40), b.get_pixel(80, 40));
    assert_ne!(out.get_pixel(215, 110), b.get_pixel(215, 110));
    // Nothing is drawn right of badge 2 (the pill flipped to the left).
    assert_eq!(out.get_pixel(295, 110), b.get_pixel(295, 110));
}

/// ANN-6.AC5: a 0.3.2 step (no new fields) renders the same as one with the defaults spelled out.
#[test]
fn ann6_ac5_old_step_unchanged() {
    let old = ops(r##"[{"op":"step","at":[30,40],"n":12,"size":28,"color":"#FF3B3B"}]"##);
    let new = ops(
        r##"[{"op":"step","at":[30,40],"n":12,"size":28,"color":"#FF3B3B","style":"number","start":1}]"##,
    );
    assert_eq!(
        render_annotations(&base(), &old).unwrap(),
        render_annotations(&base(), &new).unwrap()
    );
}

/// Serialize -> parse -> render is stable for the new ops.
#[test]
fn new_ops_roundtrip_render_stable() {
    let json = format!(
        r##"[{BUBBLE},{BOXC},
          {{"op":"spotlight","shapes":[{{"kind":"ellipse","rect":[10,10,80,60],"radius":0}},{{"rect":[120,90,60,50],"radius":8}}],"dim":0.5,"mode":"blur","strength":10}},
          {{"op":"step","at":[200,40],"n":2,"style":"upper","start":3,"text":"Go"}},
          {{"op":"stamp","at":[250,150],"kind":"label","size":30,"color":"#2EB872","text":"After"}},
          {{"op":"stamp","at":[250,100],"kind":"question"}}]"##
    );
    let a = ops(&json);
    let ser = serde_json::to_string(&a).unwrap();
    let b = ops(&ser);
    assert_eq!(a, b);
    assert_eq!(
        render_annotations(&base(), &a).unwrap(),
        render_annotations(&base(), &b).unwrap()
    );
}

/// Op list order is kept: the spotlight dims what was drawn before it and not what follows.
#[test]
fn ann7_ac4_order_arrow_after_spotlight_stays_bright() {
    let json = r##"[{"op":"spotlight","shapes":[{"rect":[200,100,50,50]}],"dim":0.8},
        {"op":"rect","rect":[10,10,60,40],"color":"#FFFFFF","width":4,"fill":"#FFFFFF","radius":0}]"##;
    let out = render_annotations(&base(), &ops(json)).unwrap();
    assert_eq!(out.get_pixel(40, 30), &Rgba([255, 255, 255, 255]));
}
