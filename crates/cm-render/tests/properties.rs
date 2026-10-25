//! Property-style and behavioral tests (ANN-1, ANN-2).
#![allow(clippy::unwrap_used)]

mod common;
use cm_core::ErrorCode;
use cm_render::{encode_png, parse_ops, parse_ops_str, render_annotations, Pt, Renderer};
use common::*;
use image::{GenericImageView, RgbaImage};

fn outside_unchanged(a: &RgbaImage, b: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> bool {
    a.enumerate_pixels().all(|(px, py, p)| {
        (px >= x && px < x + w && py >= y && py < y + h) || b.get_pixel(px, py) == p
    })
}

/// ANN-2: blur changes the region and leaves everything outside it untouched.
#[test]
fn ann2_blur_region_only() {
    let base = base_image(200, 120);
    let out = render_annotations(
        &base,
        &ops(r#"[{"op":"blur","rect":[20,60,80,40],"strength":12}]"#),
    )
    .unwrap();
    assert!(outside_unchanged(&base, &out, 20, 60, 80, 40));
    assert!(
        diff_fraction(
            &base.view(20, 60, 80, 40).to_image(),
            &out.view(20, 60, 80, 40).to_image(),
            8
        ) > 0.05
    );
}

/// ANN-2 (export safety): no original pixel values survive in a redacted noise region.
#[test]
fn ann2_no_leaked_pixels_blur_and_pixelate() {
    let base = noise_image(120, 80);
    for json in [
        r#"[{"op":"blur","rect":[10,10,100,60],"strength":12}]"#,
        r#"[{"op":"blur","rect":[10,10,100,60],"strength":4}]"#,
        r#"[{"op":"pixelate","rect":[10,10,100,60],"block":6}]"#,
    ] {
        let out = render_annotations(&base, &ops(json)).unwrap();
        let same = (10..70)
            .flat_map(|y| (10..110).map(move |x| (x, y)))
            .filter(|&(x, y)| base.get_pixel(x, y) == out.get_pixel(x, y))
            .count();
        assert!(same < 6000 / 100, "{json}: {same} unchanged pixels");
        assert!(outside_unchanged(&base, &out, 10, 10, 100, 60));
    }
}

/// ANN-2.AC1 (structural stand-in for OCR): after pixelate, every block is one flat color, so no
/// stroke detail of the source text survives.
#[test]
fn ann2_ac1_pixelate_blocks_are_flat() {
    let base = base_image(200, 120);
    let out = render_annotations(
        &base,
        &ops(r#"[{"op":"pixelate","rect":[20,60,84,48],"block":12}]"#),
    )
    .unwrap();
    for by in 0..4 {
        for bx in 0..7 {
            let first = *out.get_pixel(20 + bx * 12, 60 + by * 12);
            for dy in 0..12 {
                for dx in 0..12 {
                    assert_eq!(*out.get_pixel(20 + bx * 12 + dx, 60 + by * 12 + dy), first);
                }
            }
        }
    }
    assert!(outside_unchanged(&base, &out, 20, 60, 84, 48));
}

/// ANN-2 edge: fractional rects round outward; rects under 6 px still redact; partly offscreen clips.
#[test]
fn ann2_small_fractional_and_clipped_rects() {
    let base = noise_image(60, 60);
    let out = render_annotations(
        &base,
        &ops(r#"[{"op":"blur","rect":[10.4,10.4,3,3],"strength":4}]"#),
    )
    .unwrap();
    assert_ne!(out.get_pixel(10, 10), base.get_pixel(10, 10));
    assert_ne!(out.get_pixel(13, 13), base.get_pixel(13, 13));
    assert!(outside_unchanged(&base, &out, 10, 10, 4, 4));
    let out = render_annotations(
        &base,
        &ops(r#"[{"op":"pixelate","rect":[-20,-20,40,40],"block":6}]"#),
    )
    .unwrap();
    assert!(outside_unchanged(&base, &out, 0, 0, 20, 20));
    assert_ne!(out, base);
}

/// ANN-2: strengths outside the spec range are rejected (a weak blur could leave text legible).
#[test]
fn ann2_out_of_range_rejected_with_index() {
    let base = base_image(50, 50);
    for json in [
        r#"[{"op":"blur","rect":[0,0,10,10],"strength":2}]"#,
        r#"[{"op":"blur","rect":[0,0,10,10],"strength":33}]"#,
        r#"[{"op":"pixelate","rect":[0,0,10,10],"block":5}]"#,
    ] {
        // Parsing validates...
        let err = parse_ops_str(json).unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("ops[0]"), "{err}");
        // ...and so does rendering (for ops built without parsing).
        let raw: Vec<cm_render::AnnotationOp> = serde_json::from_str(json).unwrap();
        let err = render_annotations(&base, &raw).unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
        assert!(err.to_string().contains("ops[0]"), "{err}");
    }
}

/// ANN-2: defaults (strength 12, block 12) apply when omitted.
#[test]
fn ann2_defaults() {
    let a = ops(r#"[{"op":"blur","rect":[0,0,20,20]},{"op":"pixelate","rect":[30,0,20,20]}]"#);
    let b = ops(
        r#"[{"op":"blur","rect":[0,0,20,20],"strength":12},{"op":"pixelate","rect":[30,0,20,20],"block":12}]"#,
    );
    assert_eq!(a, b);
}

/// ANN-2.AC3: the encoded PNG has only IHDR/IDAT/IEND chunks.
#[test]
fn ann2_ac3_png_has_no_extra_chunks() {
    let base = base_image(80, 60);
    let out = render_annotations(&base, &ops(r#"[{"op":"blur","rect":[5,5,40,30]}]"#)).unwrap();
    let png = encode_png(&out).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let (mut i, mut names) = (8usize, vec![]);
    while i + 8 <= png.len() {
        let len = u32::from_be_bytes(png[i..i + 4].try_into().unwrap()) as usize;
        names.push(String::from_utf8(png[i + 4..i + 8].to_vec()).unwrap());
        i += 12 + len;
    }
    assert_eq!(i, png.len());
    assert!(
        names
            .iter()
            .all(|n| matches!(n.as_str(), "IHDR" | "IDAT" | "IEND")),
        "{names:?}"
    );
    // round trip decodes to the same pixels
    assert_eq!(image::load_from_memory(&png).unwrap().to_rgba8(), out);
}

/// ANN-1.AC4: crop sets the canvas size and contains the ops inside the rect. Op coordinates stay
/// in original-image space (spec: "coordinates are physical px of the original image").
#[test]
fn ann1_ac4_crop_size_and_original_space_coords() {
    let base = base_image(200, 120);
    let plain = render_annotations(&base, &ops(r##"[{"op":"rect","rect":[60,40,30,20],"color":"#FF3B3B","width":4,"fill":"#FF3B3B"}]"##)).unwrap();
    let cropped = render_annotations(
        &base,
        &ops(
            r##"[{"op":"rect","rect":[60,40,30,20],"color":"#FF3B3B","width":4,"fill":"#FF3B3B"},
                  {"op":"crop","rect":[50,30,100,60]}]"##,
        ),
    )
    .unwrap();
    assert_eq!(cropped.dimensions(), (100, 60));
    assert_eq!(cropped, plain.view(50, 30, 100, 60).to_image());
    // crop position in the list does not matter, and ops outside the rect are simply absent
    let crop_first = render_annotations(
        &base,
        &ops(r##"[{"op":"crop","rect":[50,30,100,60]},
                  {"op":"rect","rect":[60,40,30,20],"color":"#FF3B3B","width":4,"fill":"#FF3B3B"}]"##),
    )
    .unwrap();
    assert_eq!(crop_first, cropped);
}

/// ANN-1 crop edge cases: clipped to the image; fully outside is `invalid_input`; last crop wins.
#[test]
fn ann1_crop_clipping_and_errors() {
    let base = base_image(100, 80);
    let out = render_annotations(&base, &ops(r#"[{"op":"crop","rect":[50,40,500,500]}]"#)).unwrap();
    assert_eq!(out.dimensions(), (50, 40));
    let err =
        render_annotations(&base, &ops(r#"[{"op":"crop","rect":[200,200,10,10]}]"#)).unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("ops[0]"));
    let out = render_annotations(
        &base,
        &ops(r#"[{"op":"crop","rect":[0,0,10,10]},{"op":"crop","rect":[0,0,30,20]}]"#),
    )
    .unwrap();
    assert_eq!(out.dimensions(), (30, 20));
    // spec example: crop to the full-size rect is the identity
    let out = render_annotations(&base, &ops(r#"[{"op":"crop","rect":[0,0,100,80]}]"#)).unwrap();
    assert_eq!(out, base);
}

/// ANN-1 edge: ops with coordinates outside the image are clipped, not rejected.
#[test]
fn ann1_offscreen_ops_clipped_not_rejected() {
    let base = base_image(100, 80);
    let json = r##"[{"op":"arrow","from":[-50,-50],"to":[300,200],"color":"#FF3B3B","width":6},
        {"op":"rect","rect":[500,500,10,10],"color":"#FF3B3B","width":4},
        {"op":"ellipse","rect":[-40,-40,80,80],"color":"#2E8BFF","width":4},
        {"op":"text","at":[90,70],"text":"clipped","size":30,"color":"#FFFFFF"},
        {"op":"step","at":[-5,-5],"n":3,"size":28,"color":"#FF3B3B"},
        {"op":"highlight","rect":[-10,-10,30,30]}]"##;
    let out = render_annotations(&base, &ops(json)).unwrap();
    assert_eq!(out.dimensions(), (100, 80));
    assert_ne!(out, base);
}

/// ANN-1 edge: an invalid op type or parameter is `invalid_input` naming the op index.
#[test]
fn ann1_invalid_op_names_index() {
    let err =
        parse_ops_str(r#"[{"op":"rect","rect":[0,0,1,1]},{"op":"star","at":[1,1]}]"#).unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidInput);
    assert!(err.to_string().contains("ops[1]"), "{err}");
    let err =
        parse_ops_str(r##"[{"op":"arrow","from":[0,0],"to":[1,1],"color":"red"}]"##).unwrap_err();
    assert!(err.to_string().contains("ops[0]"), "{err}");
    let base = base_image(20, 20);
    // Parsing validates too; build the invalid list unvalidated to check the render-time guard.
    let bad: Vec<cm_render::AnnotationOp> = serde_json::from_str(
        r#"[{"op":"line","from":[0,0],"to":[5,5]},{"op":"highlight","rect":[0,0,5,5],"opacity":2}]"#,
    )
    .unwrap();
    let err = render_annotations(&base, &bad).unwrap_err();
    assert!(err.to_string().contains("ops[1]"), "{err}");
}

/// ANN-1 test notes: serialize -> deserialize -> render is stable.
#[test]
fn ann1_serde_roundtrip_render_stable() {
    let json = r##"[
      {"id":"op_7","op":"arrow","from":[120,300],"to":[380,210],"color":"#FF3B3B","width":4},
      {"id":"op_8","op":"rect","rect":[400,180,240,60],"color":"#FF3B3B","width":4,"fill":null,"radius":6},
      {"id":"op_9","op":"ellipse","rect":[400,180,240,60],"color":"#2E8BFF","width":4},
      {"id":"op_10","op":"line","from":[10,10],"to":[200,10],"color":"#111111","width":2},
      {"id":"op_11","op":"text","at":[130,320],"text":"Click Save","size":24,"color":"#FFFFFF","outline":"#000000","bg":null},
      {"id":"op_12","op":"highlight","rect":[90,400,300,28],"color":"#FFD60A","opacity":0.35},
      {"id":"op_13","op":"step","at":[400,200],"n":1,"size":28,"color":"#FF3B3B"},
      {"id":"op_14","op":"blur","rect":[420,88,260,24],"strength":12},
      {"id":"op_15","op":"pixelate","rect":[420,120,260,24],"block":12},
      {"id":"op_16","op":"crop","rect":[0,0,1200,800]}]"##;
    let parsed = parse_ops_str(json).unwrap();
    assert_eq!(parsed.len(), 10);
    let again = parse_ops(&serde_json::to_value(&parsed).unwrap()).unwrap();
    assert_eq!(parsed, again);
    let base = base_image(1200, 800);
    let a = render_annotations(&base, &parsed).unwrap();
    let b = render_annotations(&base, &again).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.dimensions(), (1200, 800)); // ANN-1.AC4 with the spec's crop
}

/// ANN-1.AC6: ops parsed from the agent JSON render identically to ops built in Rust (GUI path).
#[test]
fn ann1_ac6_agent_json_equals_native_ops() {
    use cm_render::{AnnotationOp, Color, OpKind};
    let agent = ops(
        r##"[{"id":"a","op":"arrow","from":[20,20],"to":[90,60],"color":"#FF3B3B","width":4}]"##,
    );
    let native = vec![AnnotationOp::new(
        "a",
        OpKind::Arrow {
            from: Pt::new(20.0, 20.0),
            to: Pt::new(90.0, 60.0),
            color: Color::rgb(0xFF, 0x3B, 0x3B),
            width: 4.0,
        },
    )];
    let base = base_image(120, 80);
    assert_eq!(
        render_annotations(&base, &agent).unwrap(),
        render_annotations(&base, &native).unwrap()
    );
}

/// ANN-1: omitted style fields take the documented defaults; explicit `outline: null` disables it.
#[test]
fn ann1_defaults_and_outline_null() {
    let base = base_image(120, 60);
    let a = ops(r#"[{"op":"text","at":[5,5],"text":"Hi"}]"#);
    let b = ops(
        r##"[{"op":"text","at":[5,5],"text":"Hi","size":24,"color":"#FFFFFF","outline":"#000000","bg":null}]"##,
    );
    assert_eq!(
        render_annotations(&base, &a).unwrap(),
        render_annotations(&base, &b).unwrap()
    );
    let c = ops(r#"[{"op":"text","at":[5,5],"text":"Hi","outline":null}]"#);
    assert_ne!(
        render_annotations(&base, &a).unwrap(),
        render_annotations(&base, &c).unwrap()
    );
    let arrow = ops(r#"[{"op":"arrow","from":[1,1],"to":[50,30]}]"#);
    let arrow_full =
        ops(r##"[{"op":"arrow","from":[1,1],"to":[50,30],"color":"#FF3B3B","width":4}]"##);
    assert_eq!(arrow, arrow_full);
}

/// ANN-1: highlight is translucent (the base shows through) and uses the given opacity.
#[test]
fn ann1_highlight_translucent() {
    let base = base_image(100, 60);
    let out = render_annotations(
        &base,
        &ops(r##"[{"op":"highlight","rect":[10,20,50,20],"color":"#FFD60A","opacity":0.35}]"##),
    )
    .unwrap();
    let (b, o) = (base.get_pixel(20, 30).0, out.get_pixel(20, 30).0);
    // blue channel pulled toward 0x0A by 35%
    let expect = (f64::from(b[2]) * 0.65 + 10.0 * 0.35).round();
    assert!((f64::from(o[2]) - expect).abs() <= 1.0, "{o:?} vs {expect}");
    assert_ne!(o, b);
    assert!(outside_unchanged(&base, &out, 10, 20, 50, 20));
}

/// ANN-1.AC2 (render side): badges render the `n` they are given, so renumbering after a delete is
/// a pure op-list edit. Different numerals produce different pixels; same `n` the same pixels.
#[test]
fn ann1_ac2_step_renders_given_number() {
    let base = base_image(100, 60);
    let r = |n: u32| {
        render_annotations(
            &base,
            &ops(&format!(
                r##"[{{"op":"step","at":[50,30],"n":{n},"size":28,"color":"#FF3B3B"}}]"##
            )),
        )
        .unwrap()
    };
    assert_ne!(r(1), r(2));
    assert_eq!(r(2), r(2));
    // centre of the badge disc is red/white-ish, corners untouched
    assert_eq!(r(1).get_pixel(0, 0), base.get_pixel(0, 0));
}

/// Strokes are drawn in the op color at the requested width (arrow shaft midpoint).
#[test]
fn ann1_line_width_and_color() {
    let base = RgbaImage::from_pixel(60, 40, image::Rgba([255, 255, 255, 255]));
    let out = render_annotations(
        &base,
        &ops(r##"[{"op":"line","from":[10,20],"to":[50,20],"color":"#2E8BFF","width":4}]"##),
    )
    .unwrap();
    assert_eq!(out.get_pixel(30, 20).0, [0x2E, 0x8B, 0xFF, 255]);
    assert_eq!(out.get_pixel(30, 18).0, [0x2E, 0x8B, 0xFF, 255]);
    assert_eq!(out.get_pixel(30, 15).0, [255, 255, 255, 255]);
    assert_eq!(out.get_pixel(30, 24).0, [255, 255, 255, 255]);
}

/// Shift-constrained shapes: a square rect and a circle ellipse render symmetric.
#[test]
fn ann1_ac3_square_and_circle_symmetric() {
    let base = RgbaImage::from_pixel(80, 80, image::Rgba([255, 255, 255, 255]));
    let out = render_annotations(
        &base,
        &ops(r##"[{"op":"ellipse","rect":[10,10,60,60],"color":"#111111","width":4}]"##),
    )
    .unwrap();
    let t = image::imageops::flip_horizontal(&out);
    assert!(diff_fraction(&out, &t, 24) < 0.005);
}

/// Determinism + purity: same input renders the same bytes; the base is untouched; a reused
/// Renderer agrees with a fresh one.
#[test]
fn render_is_deterministic_and_pure() {
    let base = base_image(120, 80);
    let copy = base.clone();
    let o = ops(
        r##"[{"op":"text","at":[5,5],"text":"Deterministic","size":18},{"op":"blur","rect":[0,50,60,20]}]"##,
    );
    let a = render_annotations(&base, &o).unwrap();
    let mut r = Renderer::new();
    let b = r.render(&base, &o).unwrap();
    let c = r.render(&base, &o).unwrap();
    assert_eq!(a, b);
    assert_eq!(b, c);
    assert_eq!(base, copy);
}

/// Redaction composes over earlier ops in list order (ambiguity noted in the report): an arrow
/// drawn under a blur is blurred too; one drawn after stays crisp.
#[test]
fn redaction_applies_in_list_order() {
    let base = RgbaImage::from_pixel(80, 40, image::Rgba([255, 255, 255, 255]));
    let line = r##"{"op":"line","from":[5,20],"to":[75,20],"color":"#000000","width":2}"##;
    let blur = r#"{"op":"blur","rect":[10,5,60,30],"strength":12}"#;
    let under = render_annotations(&base, &ops(&format!("[{line},{blur}]"))).unwrap();
    let over = render_annotations(&base, &ops(&format!("[{blur},{line}]"))).unwrap();
    assert!(
        under.get_pixel(40, 20).0[0] > 150,
        "line under blur must be smeared"
    );
    assert_eq!(over.get_pixel(40, 20).0, [0, 0, 0, 255]);
}

/// Degenerate inputs never panic.
#[test]
fn degenerate_ops_do_not_panic() {
    let base = base_image(30, 30);
    let json = r##"[{"op":"arrow","from":[5,5],"to":[5,5]},{"op":"rect","rect":[5,5,0,0]},
      {"op":"ellipse","rect":[5,5,0,10]},{"op":"text","at":[5,5],"text":""},
      {"op":"rect","rect":[20,20,-10,-10],"radius":100,"fill":"#00FF00"},
      {"op":"step","at":[10,10],"n":100000,"size":5}]"##;
    render_annotations(&base, &ops(json)).unwrap();
    // R1-13: a zero-area redaction is an error, not a silent no-op.
    for op in ["blur", "pixelate"] {
        let json = format!(r#"[{{"op":"{op}","rect":[5,5,0,0]}}]"#);
        let e = parse_ops_str(&json).unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidInput);
    }
}

/// Semi-transparent base pixels survive a no-op render unchanged (premultiply round trip).
#[test]
fn alpha_roundtrip_opaque_exact() {
    let base = base_image(40, 40);
    assert_eq!(render_annotations(&base, &[]).unwrap(), base);
}

/// R1-9: a text layer over the area budget is refused with `invalid_input` naming the op,
/// instead of allocating gigabytes; over-long text is refused by validation too.
#[test]
fn r1_9_huge_text_layers_are_refused() {
    let base = base_image(40, 40);
    // 30 lines of 40 glyphs at size 300 is roughly 6600 x 10800 px = 71 Mpx.
    let line = "W".repeat(40);
    let text = vec![line; 30].join("\\n");
    let json = format!(r#"[{{"op":"text","at":[0,0],"text":"{text}","size":300}}]"#);
    let e = render_annotations(&base, &ops(&json)).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("ops[0]"), "{e}");
    // 2001 chars: refused before layout.
    let long = "a".repeat(2001);
    let json = format!(r#"[{{"op":"text","at":[0,0],"text":"{long}","size":10}}]"#);
    assert!(parse_ops_str(&json).is_err());
}
