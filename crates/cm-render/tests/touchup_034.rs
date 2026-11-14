//! 0.3.4 ops: Zoom Lens (ANN-8), Retype (ANN-9), Move area (ANN-10), Blueprint (ANN-11) and the
//! base-edit order. Regenerate goldens with
//! `UPDATE_GOLDEN=1 cargo test -p cm-render --test touchup_034`, then review the PNGs.
#![allow(clippy::unwrap_used)]

mod common;
use cm_core::annotation::{validate_list, Fill, FillKind};
use cm_core::{Color, ErrorCode};
use cm_render::{
    blueprint_bars, measure_blueprint, measure_fill, measure_text_style, parse_ops_str,
    render_annotations, ring_error, Rc, Renderer,
};
use common::*;
use image::{imageops, Rgba, RgbaImage};

const LIMIT: f64 = 0.01;

fn white(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]))
}

/// White picture with one-pixel black vertical lines every 6 px and horizontal ones every 9.
fn lines(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        if x % 6 == 0 || y % 9 == 0 {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    })
}

fn lens(source: &str, at: &str, zoom: u32, extra: &str) -> String {
    format!(
        r##"{{"id":"lens","op":"magnify","source":{source},"at":{at},"zoom":{zoom},"shape":"rect","border_width":0,"connector":false,"badge":false{extra}}}"##
    )
}

fn crop_of(img: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
    imageops::crop_imm(img, x, y, w, h).to_image()
}

/// Number of pixels in a row that are neither dark nor light (a 10 % to 90 % edge width).
fn edge_width(img: &RgbaImage, y: u32, x0: u32, x1: u32) -> u32 {
    (x0..x1)
        .filter(|&x| {
            let v = img.get_pixel(x, y).0[0];
            (26..=230).contains(&v)
        })
        .count() as u32
}

/// Sub-pixel edge width of a black and white row: the sum of how far each pixel is from pure
/// black or white (a linear ramp of width W adds W / 2 per edge).
fn soft_edge(img: &RgbaImage, y: u32, x0: u32, x1: u32) -> f64 {
    (x0..x1)
        .map(|x| 1.0 - (f64::from(img.get_pixel(x, y).0[0]) / 127.5 - 1.0).abs())
        .sum()
}

fn stddev(img: &RgbaImage) -> f64 {
    let v: Vec<f64> = img.pixels().map(|p| f64::from(p.0[0])).collect();
    let m = v.iter().sum::<f64>() / v.len() as f64;
    (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / v.len() as f64).sqrt()
}

// ---------------------------------------------------------------- ANN-8 Zoom Lens

/// ANN-8.AC1: the loupe equals a direct 4x render of the region, and is sharper than a bilinear
/// enlargement.
#[test]
fn ann8_ac1_loupe_is_a_direct_render_and_sharp() {
    let base = lines(400, 300);
    let o = ops(&format!("[{}]", lens("[20,20,100,50]", "[300,200]", 4, "")));
    let out = render_annotations(&base, &o).unwrap();
    assert_eq!(out.dimensions(), base.dimensions());
    // Loupe rect: 400x200 centred on (300, 200).
    let got = crop_of(&out, 100, 100, 300, 200);
    let want_full = imageops::resize(
        &crop_of(&base, 20, 20, 100, 50),
        400,
        200,
        imageops::FilterType::CatmullRom,
    );
    let want = crop_of(&want_full, 0, 0, 300, 200);
    assert!(diff_fraction(&got, &want, 8) <= LIMIT);
    // Edge width on a vertical line: sharper than bilinear.
    let bilinear = imageops::resize(
        &crop_of(&base, 20, 20, 100, 50),
        400,
        200,
        imageops::FilterType::Triangle,
    );
    let loupe = crop_of(&out, 100, 100, 300, 200);
    let ours = edge_width(&loupe, 20, 0, 300);
    let theirs = edge_width(&bilinear, 20, 0, 300);
    println!("edge width: ours {ours}, bilinear {theirs}");
    assert!(ours < theirs, "{ours} !< {theirs}");
    assert_golden("lens_rect_x4", &out, LIMIT);
}

/// ANN-8: circle, rounded and rect shapes with border, connector and badge.
#[test]
fn ann8_shapes_with_border_connector_and_badge() {
    let base = base_image(420, 260);
    for (shape, name) in [
        ("circle", "lens_circle"),
        ("rounded", "lens_rounded"),
        ("rect", "lens_rect_badge"),
    ] {
        let j = format!(
            r##"[{{"op":"magnify","source":[60,135,60,60],"at":[300,100],"zoom":3,"shape":"{shape}","border":"#FFFFFF","border_width":4,"connector":true,"badge":true}}]"##
        );
        let out = render_annotations(&base, &ops(&j)).unwrap();
        assert_golden(name, &out, LIMIT);
    }
}

/// ANN-8.AC2: an arrow listed before the lens is in the loupe at 4x; one after is not.
#[test]
fn ann8_ac2_vector_ops_before_the_lens_are_in_it() {
    let base = white(400, 300);
    let arrow = r##"{"op":"arrow","from":[15,40],"to":[100,40],"color":"#FF0000","width":4}"##;
    let l = lens("[10,20,100,50]", "[300,200]", 4, "");
    let reds = |img: &RgbaImage| {
        crop_of(img, 100, 100, 300, 200)
            .pixels()
            .filter(|p| p.0[0] > 200 && p.0[1] < 80)
            .count()
    };
    let before = render_annotations(&base, &ops(&format!("[{arrow},{l}]"))).unwrap();
    let after = render_annotations(&base, &ops(&format!("[{l},{arrow}]"))).unwrap();
    let n_before = reds(&before);
    assert!(
        n_before > 16 * 50,
        "arrow is in the loupe ({n_before} red px)"
    );
    assert_eq!(reds(&after), 0, "an arrow after the lens is not in it");
    // The arrow shaft in the loupe is about 4 x 4 = 16 px thick.
    let col = 150;
    let thick = (100..300)
        .filter(|&y| {
            let p = before.get_pixel(col, y).0;
            p[0] > 200 && p[1] < 80
        })
        .count();
    assert!((14..=18).contains(&thick), "thickness {thick}");
}

/// ANN-8.AC3 (pixel form): a blur or pixelate hides every copy of the pixels, listed before or
/// after the lens, at zoom 2, 4 and 8. (The OCR form is in `ocr_034_real` below.)
#[test]
fn ann8_ac3_redactions_are_never_revealed_in_the_loupe() {
    let base = noise_image(200, 120);
    for zoom in [2u32, 4, 8] {
        let l = lens("[20,20,60,30]", "[100,60]", zoom, "");
        let blur = r##"{"op":"blur","rect":[20,20,60,30],"strength":12}"##;
        let pix = r##"{"op":"pixelate","rect":[20,20,60,30],"block":12}"##;
        let plain = render_annotations(&base, &ops(&format!("[{l}]"))).unwrap();
        let (lw, lh) = (60 * zoom, 30 * zoom);
        // Part of the loupe that stays inside the image: its top left quarter.
        let win = |img: &RgbaImage| {
            crop_of(
                img,
                (100 - lw as i32 / 2).max(0) as u32,
                (60 - lh as i32 / 2).max(0) as u32,
                lw.min(60) / 2,
                lh.min(40) / 2,
            )
        };
        let sd_plain = stddev(&win(&plain));
        for (name, list) in [
            ("blur before", format!("[{blur},{l}]")),
            ("blur after", format!("[{l},{blur}]")),
            ("pixelate before", format!("[{pix},{l}]")),
            ("pixelate after", format!("[{l},{pix}]")),
        ] {
            let out = render_annotations(&base, &ops(&list)).unwrap();
            let sd = stddev(&win(&out));
            println!("zoom {zoom} {name}: stddev {sd:.1} (plain {sd_plain:.1})");
            assert!(
                sd < 0.5 * sd_plain,
                "zoom {zoom} {name}: {sd:.1} vs plain {sd_plain:.1}"
            );
        }
    }
}

/// ANN-8.AC4: at 2x the loupe is rendered again at 2 x zoom from the original; its edge width
/// (per output pixel of scale) is not worse than at 1x.
#[test]
fn ann8_ac4_export_at_2x_renders_the_loupe_again() {
    // Wide blocks, so a step edge shows its full width at both scales.
    let base = RgbaImage::from_fn(400, 300, |x, _| {
        if (x / 10) % 2 == 0 {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    });
    let o = ops(&format!("[{}]", lens("[20,20,100,50]", "[300,200]", 4, "")));
    let mut r = Renderer::new();
    let one = r.render_scaled(&base, &o, 1.0).unwrap();
    let two = r.render_scaled(&base, &o, 2.0).unwrap();
    assert_eq!(two.dimensions(), (800, 600));
    let e1 = soft_edge(&crop_of(&one, 100, 100, 300, 200), 20, 0, 300);
    let e2 = soft_edge(&crop_of(&two, 200, 200, 600, 400), 40, 0, 600);
    println!(
        "soft edge width 1x {e1:.1}, 2x {e2:.1} (per unit of scale: {:.1})",
        e2 / 2.0
    );
    assert!(e2 / 2.0 <= e1 * 1.1, "the 2x loupe is blurrier");
    // Not an enlargement of the 1x picture: the 2x loupe equals a direct 8x render.
    let want = imageops::resize(
        &crop_of(&base, 20, 20, 100, 50),
        800,
        400,
        imageops::FilterType::CatmullRom,
    );
    let got = crop_of(&two, 200, 200, 600, 400);
    assert!(diff_fraction(&got, &crop_of(&want, 0, 0, 600, 400), 8) <= LIMIT);
}

/// ANN-8.AC5: out-of-range lens parameters and a fifth lens are `invalid_input` naming `ops[i]`.
#[test]
fn ann8_ac5_limits() {
    let bad = [
        lens("[10,10,50,50]", "[100,100]", 1, ""),
        lens("[10,10,50,50]", "[100,100]", 9, ""),
        lens("[10,10,7,50]", "[100,100]", 3, ""),
        lens("[10,10,50,50]", "[100,100]", 3, r#","border_width":33"#),
        lens("[10,10,2800,2800]", "[100,100]", 8, ""),
    ];
    for b in &bad {
        let e = parse_ops_str(&format!("[{b}]")).and_then(|o| {
            // Both the parser and the renderer refuse it.
            render_annotations(&white(50, 50), &o).map(|_| ())
        });
        let e = e.unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidInput, "{b}");
        assert!(e.to_string().contains("ops[0]"), "{b}: {e}");
    }
    let one = lens("[10,10,50,50]", "[100,100]", 3, "");
    let five = format!("[{}]", [one.as_str(); 5].join(","));
    // Distinct ids are not required: the id field is the same here, so drop it.
    let five = five.replace(r#""id":"lens","#, "");
    let e = parse_ops_str(&five).unwrap_err();
    assert!(e.to_string().contains("ops[4]"), "{e}");
    parse_ops_str(&format!(
        "[{}]",
        vec![one.replace(r#""id":"lens","#, ""); 4].join(",")
    ))
    .unwrap();
}

/// ANN-8.AC6: a loupe partly outside the image is clipped; a source partly outside is clamped.
#[test]
fn ann8_ac6_clipping_and_clamping() {
    let base = base_image(200, 150);
    let o = ops(&format!(
        "[{}]",
        lens("[-20,-10,60,40]", "[190,140]", 3, "")
    ));
    let out = render_annotations(&base, &o).unwrap();
    assert_eq!(out.dimensions(), (200, 150));
    // Nothing at all to show: the source lies wholly outside the image.
    let o = ops(&format!(
        "[{}]",
        lens("[500,500,60,40]", "[100,100]", 3, "")
    ));
    assert_eq!(render_annotations(&base, &o).unwrap(), base);
}

/// ANN-8.AC10 and base-edit order: a Retype inside the lens source shows in the lens.
#[test]
fn ann8_ac10_the_lens_shows_base_edits() {
    let base = white(300, 200);
    let retype = r##"{"op":"retype","rect":[20,20,80,30],"fill":{"kind":"solid","colors":["#000000"]},"text":"","size":20}"##;
    let l = lens("[10,10,100,50]", "[220,130]", 2, "");
    // The retype turns its rect black; the lens must show it black although it is listed
    // after the lens (a stored list is sorted by the renderer; `annotate` is the strict check).
    let mut v = ops(&format!("[{l}]"));
    v.extend(ops(&format!("[{retype}]")));
    let out = render_annotations(&base, &v).unwrap();
    // Loupe 200 x 100 centred on (220, 130): the black rect starts at loupe (20, 20).
    assert_eq!(out.get_pixel(150, 110).0, [0, 0, 0, 255]);
    assert_eq!(out.get_pixel(130, 90).0, [255, 255, 255, 255]);
}

// ---------------------------------------------------------------- base-edit order

/// A list as a 0.3.3 app would save it: a base edit after an arrow.
fn unsorted() -> Vec<cm_render::AnnotationOp> {
    let arrow =
        r##"{"id":"a","op":"arrow","from":[10,10],"to":[100,100],"color":"#FF0000","width":4}"##;
    let retype = r##"{"id":"r","op":"retype","rect":[20,20,80,30],"fill":{"kind":"solid","colors":["#000000"]},"text":"","size":20}"##;
    let mut v = ops(&format!("[{arrow}]"));
    v.extend(ops(&format!("[{retype}]")));
    v
}

/// ANN-9.AC4: `annotate` input is strict.
#[test]
fn ann9_ac4_a_base_edit_after_another_op_is_refused() {
    let arrow = r##"{"op":"arrow","from":[10,10],"to":[100,100]}"##;
    let retype = r##"{"op":"retype","rect":[20,20,80,30],"fill":{"kind":"solid","colors":["#000000"]},"text":"x"}"##;
    let e = parse_ops_str(&format!("[{arrow},{retype}]")).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("ops[1]"), "{e}");
    parse_ops_str(&format!("[{retype},{arrow}]")).unwrap();
    assert!(validate_list(&unsorted()).is_err());
}

/// Plan problem 12: a stored list with a base edit after an arrow renders as the sorted list.
#[test]
fn problem12_unsorted_stored_lists_render_like_sorted_ones() {
    let base = white(200, 150);
    let u = unsorted();
    let mut s = u.clone();
    assert!(cm_core::annotation::sort_base_edits_first(&mut s));
    assert_eq!(s[0].id, "r");
    let a = render_annotations(&base, &u).unwrap();
    let b = render_annotations(&base, &s).unwrap();
    assert_eq!(a, b);
    // The arrow is drawn over the retype fill (black rect at 20..100 x 20..50).
    assert!(a.get_pixel(60, 60).0 != [255, 255, 255, 255]);
}

// ---------------------------------------------------------------- fills (S2 helpers)

fn vgradient(w: u32, h: u32, top: [u8; 3], bottom: [u8; 3]) -> RgbaImage {
    RgbaImage::from_fn(w, h, |_, y| {
        let t = f64::from(y) / f64::from(h - 1);
        let c = |i: usize| {
            (f64::from(top[i]) + (f64::from(bottom[i]) - f64::from(top[i])) * t).round() as u8
        };
        Rgba([c(0), c(1), c(2), 255])
    })
}

/// A synthetic text line drawn with the project's own renderer.
fn with_text(base: &RgbaImage, x: u32, y: u32, text: &str, size: u32, color: &str) -> RgbaImage {
    let j = format!(
        r##"[{{"op":"text","at":[{x},{y}],"text":"{text}","size":{size},"color":"{color}","outline":null}}]"##
    );
    render_annotations(base, &ops(&j)).unwrap()
}

/// ANN-9.AC2: a vertical gradient gives a `linear` fill within 6 levels on the ring check, and
/// the filled box matches the gradient it replaces.
#[test]
fn ann9_ac2_gradient_background_gets_a_linear_fill() {
    let bg = vgradient(300, 120, [250, 250, 252], [180, 190, 215]);
    let shot = with_text(&bg, 40, 40, "Save", 28, "#101010");
    let rect = Rc::new(36.0, 36.0, 110.0, 44.0);
    let m = measure_fill(&shot, rect);
    println!("{m:?}");
    assert_eq!(m.fill.kind, FillKind::Linear);
    assert!(!m.busy);
    let (max, p98) = ring_error(&shot, rect, &m.fill).unwrap();
    assert!(max <= 6, "ring max error {max} (p98 {p98})");
    // The painted fill equals the clean gradient inside the box.
    let retype = format!(
        r##"[{{"op":"retype","rect":[36,36,110,44],"fill":{},"text":"","size":28}}]"##,
        serde_json::to_string(&m.fill).unwrap()
    );
    let out = render_annotations(&shot, &ops(&retype)).unwrap();
    let mut worst = 0u8;
    for y in 36..80 {
        for x in 36..146 {
            let (a, b) = (out.get_pixel(x, y).0, bg.get_pixel(x, y).0);
            worst = worst.max((0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap());
        }
    }
    println!("worst difference inside the filled box: {worst}");
    assert!(worst <= 6, "{worst}");
}

/// ANN-9.AC3: a photo-like background is "busy" (the tool warns before it makes the op).
#[test]
fn ann9_ac3_a_busy_background_is_flagged() {
    let m = measure_fill(&noise_image(200, 120), Rc::new(40.0, 40.0, 80.0, 30.0));
    assert!(m.busy, "{m:?}");
    // And a flat one is not.
    let m = measure_fill(&white(200, 120), Rc::new(40.0, 40.0, 80.0, 30.0));
    assert!(!m.busy);
    assert_eq!(m.fill, Fill::solid(Color::WHITE));
}

/// ANN-9.AC1 (pixel form): "Save" retyped to "Save all" on a flat dialog; nothing outside `rect`
/// changes; the measured style recovers the size and colour; the old word is gone.
#[test]
fn ann9_ac1_retype_on_a_flat_dialog() {
    let bg = RgbaImage::from_pixel(300, 100, Rgba([236, 236, 240, 255]));
    let shot = with_text(&bg, 40, 30, "Save", 28, "#1A1A1A");
    let rect = Rc::new(34.0, 28.0, 120.0, 40.0);
    let m = measure_fill(&shot, rect);
    assert_eq!(m.fill.kind, FillKind::Solid);
    let t = measure_text_style(&shot, rect, &m.fill, Some("Save"));
    println!("measured {t:?}");
    assert!((t.size - 28.0).abs() <= 2.0, "size {}", t.size);
    assert!(t.color.r < 70 && t.color.g < 70, "colour {}", t.color);
    // Retyping the same word with the measured style reproduces the shot closely.
    let json = |text: &str| {
        format!(
            r##"[{{"op":"retype","rect":[34,28,120,40],"fill":{},"text":"{text}","size":{},"color":"{}"}}]"##,
            serde_json::to_string(&m.fill).unwrap(),
            t.size,
            t.color
        )
    };
    let all = render_annotations(&shot, &ops(&json("Save all"))).unwrap();
    for y in 0..100 {
        for x in 0..300 {
            let inside = (34..154).contains(&x) && (28..68).contains(&y);
            if !inside {
                assert_eq!(all.get_pixel(x, y), shot.get_pixel(x, y), "({x},{y})");
            }
        }
    }
    assert_ne!(all, shot);
    assert_golden("retype_save_all", &all, LIMIT);
    // Right-aligned and bold variants stay inside the rect.
    let j = json("Save all").replace(r#""size""#, r#""weight":700,"align":"right","size""#);
    let bold = render_annotations(&shot, &ops(&j)).unwrap();
    assert_golden("retype_bold_right", &bold, LIMIT);
    // `weight` 400 is not written; the JSON round-trips.
    let o = ops(&json("x"));
    assert!(serde_json::to_value(&o).unwrap()[0].get("weight").is_none());
}

/// ANN-9.AC5 and ANN-1.AC6: the JSON an agent sends renders the same as the structs.
#[test]
fn ann9_ac5_json_round_trip_renders_identically() {
    let base = vgradient(200, 100, [240, 240, 240], [200, 200, 220]);
    let j = r##"[{"id":"op_1","op":"retype","rect":[10,10,100,30],"fill":{"kind":"linear","colors":["#F0F0F0","#D0D0E0"],"angle":90},"text":"Hello","size":22,"color":"#000000","align":"center"},
        {"id":"op_2","op":"move_area","source":[120,20,40,40],"to":[150,50],"fill":{"kind":"solid","colors":["#E0E0E8"]}},
        {"id":"op_3","op":"blueprint","boxes":[[10,60,100,12]],"style":"bar","color":"auto","radius":3}]"##;
    let a = ops(j);
    let b: Vec<cm_render::AnnotationOp> =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(a, b);
    assert_eq!(
        render_annotations(&base, &a).unwrap(),
        render_annotations(&base, &b).unwrap()
    );
}

// ---------------------------------------------------------------- ANN-10 Move area

fn toolbar() -> RgbaImage {
    let mut img = RgbaImage::from_pixel(400, 60, Rgba([60, 64, 72, 255]));
    // An "icon": a 24x24 light square with a dark dot.
    for y in 18..42 {
        for x in 30..54 {
            img.put_pixel(x, y, Rgba([240, 240, 240, 255]));
        }
    }
    for y in 27..33 {
        for x in 39..45 {
            img.put_pixel(x, y, Rgba([200, 40, 40, 255]));
        }
    }
    img
}

/// ANN-10.AC1, AC2: the icon moves 120 px to the right pixel for pixel, the hole matches the bar;
/// `to: null` removes it.
#[test]
fn ann10_ac1_ac2_move_and_remove() {
    let base = toolbar();
    let fill = r##"{"kind":"solid","colors":["#3C4048"]}"##;
    let j =
        format!(r##"[{{"op":"move_area","source":[30,18,24,24],"to":[150,18],"fill":{fill}}}]"##);
    let out = render_annotations(&base, &ops(&j)).unwrap();
    for y in 0..24 {
        for x in 0..24 {
            assert_eq!(
                out.get_pixel(150 + x, 18 + y),
                base.get_pixel(30 + x, 18 + y)
            );
        }
    }
    for y in 18..42 {
        for x in 30..54 {
            let (a, b) = (out.get_pixel(x, y).0, base.get_pixel(5, 5).0);
            assert!(
                (0..3).all(|i| a[i].abs_diff(b[i]) <= 6),
                "hole at ({x},{y})"
            );
        }
    }
    assert_golden("move_area_icon", &out, LIMIT);
    let j = format!(r##"[{{"op":"move_area","source":[30,18,24,24],"to":null,"fill":{fill}}}]"##);
    let removed = render_annotations(&base, &ops(&j)).unwrap();
    assert!(removed.get_pixel(40, 28).0[0] < 70);
    assert_eq!(removed.get_pixel(5, 5), base.get_pixel(5, 5));
    // `to: null` is written, not dropped.
    assert!(serde_json::to_value(ops(&j)).unwrap()[0]["to"].is_null());
}

/// ANN-10.AC3: overlapping source and destination give "cut, fill, paste".
#[test]
fn ann10_ac3_overlap_is_cut_fill_paste() {
    let base = toolbar();
    let fill = r##"{"kind":"solid","colors":["#3C4048"]}"##;
    let j =
        format!(r##"[{{"op":"move_area","source":[30,18,24,24],"to":[42,18],"fill":{fill}}}]"##);
    let out = render_annotations(&base, &ops(&j)).unwrap();
    // The reference: copy, fill, paste by hand.
    let mut want = base.clone();
    let block = crop_of(&base, 30, 18, 24, 24);
    for y in 18..42 {
        for x in 30..54 {
            want.put_pixel(x, y, Rgba([0x3C, 0x40, 0x48, 255]));
        }
    }
    imageops::replace(&mut want, &block, 42, 18);
    assert_eq!(out, want);
    assert_golden("move_area_overlap", &out, LIMIT);
}

// ---------------------------------------------------------------- ANN-11 Blueprint

fn settings_window() -> (RgbaImage, Vec<Rc>) {
    let mut img = RgbaImage::from_pixel(360, 420, Rgba([245, 245, 247, 255]));
    let mut boxes = Vec::new();
    for i in 0..20u32 {
        let y = 14 + i * 20;
        let w = 100 + (i * 37) % 160;
        img = with_text(
            &img,
            20,
            y,
            &"Setting label"
                .chars()
                .cycle()
                .take((w / 9) as usize)
                .collect::<String>(),
            14,
            "#202020",
        );
        boxes.push(Rc::new(18.0, f64::from(y) - 1.0, f64::from(w), 18.0));
    }
    (img, boxes)
}

/// ANN-11.AC2: each bar is inside its box; the bars of one paragraph share a height and a pitch.
#[test]
fn ann11_ac2_bars_are_inside_and_even() {
    let (_, boxes) = settings_window();
    let bars = blueprint_bars(&boxes, cm_core::annotation::BlueprintStyle::Bar);
    for (b, bar) in boxes.iter().zip(&bars) {
        assert!(bar[0] as f64 >= b.x - 0.5 && bar[2] as f64 <= b.x + b.w + 0.5);
        assert!(
            bar[1] as f64 >= b.y - 0.5 && bar[3] as f64 <= b.y + b.h + 0.5,
            "{b:?} {bar:?}"
        );
    }
    let hs: Vec<i64> = bars.iter().map(|b| b[3] - b[1]).collect();
    assert!(hs.iter().all(|h| *h == hs[0]), "{hs:?}");
    let gaps: Vec<i64> = bars.windows(2).map(|w| w[1][1] - w[0][1]).collect();
    assert!(gaps.iter().all(|g| *g == gaps[0]), "{gaps:?}");
}

/// ANN-11.AC1 (pixel form): with Blueprint applied, no ink of the lines is left except in the
/// kept ones; the golden shows the look. (The OCR form is in `ocr_034_real`.)
#[test]
fn ann11_ac1_text_is_gone_except_the_kept_lines() {
    let (img, boxes) = settings_window();
    let styles = measure_blueprint(&img, &boxes);
    assert_eq!(styles.len(), 20);
    let keep = [3usize, 11];
    let kept: Vec<Rc> = boxes
        .iter()
        .enumerate()
        .filter(|(i, _)| !keep.contains(i))
        .map(|(_, b)| *b)
        .collect();
    let fills: Vec<Fill> = styles
        .iter()
        .enumerate()
        .filter(|(i, _)| !keep.contains(i))
        .map(|(_, s)| s.fill.clone())
        .collect();
    let colors: Vec<Color> = styles
        .iter()
        .enumerate()
        .filter(|(i, _)| !keep.contains(i))
        .map(|(_, s)| s.color)
        .collect();
    let j = serde_json::json!([{
        "op": "blueprint", "boxes": kept, "style": "bar", "color": "auto", "radius": 3,
        "colors": colors, "fills": fills,
    }]);
    let out = render_annotations(&img, &parse_ops_str(&j.to_string()).unwrap()).unwrap();
    // Dark ink (the text colour) remains only inside the kept boxes.
    for (i, b) in boxes.iter().enumerate() {
        let dark = crop_of(&out, b.x as u32, b.y as u32, b.w as u32, b.h as u32)
            .pixels()
            .filter(|p| p.0[0] < 90)
            .count();
        if keep.contains(&i) {
            assert!(dark > 20, "kept line {i} lost its text");
        } else {
            assert_eq!(dark, 0, "line {i} still has dark pixels");
        }
    }
    assert_golden("blueprint_settings", &out, LIMIT);
}

// ---------------------------------------------------------------- S2 / S5

/// S5: the largest side this machine's CPU renderer handles (the GPU limit is the compositor's
/// business). Prints sizes; never fails on memory. Ignored: allocates up to ~1 GB.
#[test]
#[ignore = "allocates large images; run by hand for spike S5"]
fn s5_largest_renderable_side() {
    let t = std::time::Instant::now();
    let base = RgbaImage::from_pixel(16_384, 3_000, Rgba([250, 250, 250, 255]));
    let o = ops(
        r##"[{"op":"retype","rect":[200,200,400,60],"fill":{"kind":"solid","colors":["#FAFAFA"]},"text":"Long","size":40},
        {"op":"magnify","source":[8000,1000,200,100],"at":[8000,1500],"zoom":8},
        {"op":"rect","rect":[100,100,16000,2800],"color":"#FF0000","width":8}]"##,
    );
    let out = render_annotations(&base, &o).unwrap();
    println!(
        "S5 cpu render 16384x3000: {:?}, {:?}",
        out.dimensions(),
        t.elapsed()
    );
}

/// S2: fill quality on real screenshots from the repo (`verification/` and `tests/data`) with the
/// real OS recognizer finding the boxes (every text line). Prints, per picture, how many boxes
/// pass the ring check (98th percentile within 6 levels) and how many the measurer calls busy.
/// Ignored: `CM_OCR_REAL=1 cargo test -p cm-render --test touchup_034 s2_ -- --ignored --nocapture`.
#[test]
#[ignore = "runs the real OS text recognizer on repo screenshots; set CM_OCR_REAL=1"]
fn s2_fill_quality_on_repo_screenshots() {
    use cm_render::ocr::system_recognizer;
    if std::env::var("CM_OCR_REAL").as_deref() != Ok("1") || !system_recognizer().is_available() {
        println!("SKIPPED: set CM_OCR_REAL=1 with a text recognizer");
        return;
    }
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for dir in [
        "verification/L1-editor-linux-2026-10-31-shots",
        "verification/L1-desktop-shell-linux-2026-10-31-shots",
        "verification/L0-gpui-linux-2026-10-31-screenshots",
        "tests/data/regress",
    ] {
        if let Ok(rd) = std::fs::read_dir(root.join(dir)) {
            for e in rd.flatten() {
                if e.path().extension().is_some_and(|x| x == "png") {
                    files.push(e.path());
                }
            }
        }
    }
    files.sort();
    let (mut total, mut pass, mut busy, mut solid, mut linear) = (0, 0, 0, 0, 0);
    let mut worst: Vec<u8> = Vec::new();
    for f in &files {
        let img = image::open(f).unwrap().to_rgba8();
        let lines = system_recognizer().recognize(&img).unwrap();
        let (mut p, mut b, mut n) = (0, 0, 0);
        for l in lines.iter().filter(|l| !l.text.trim().is_empty()) {
            let r = Rc::new(
                l.rect.x.floor(),
                l.rect.y.floor(),
                l.rect.w.ceil() + 1.0,
                l.rect.h.ceil() + 1.0,
            );
            let m = measure_fill(&img, r);
            n += 1;
            if m.ring_p98_error <= 6 {
                p += 1;
            }
            if m.busy {
                b += 1;
            }
            match m.fill.kind {
                FillKind::Solid => solid += 1,
                FillKind::Linear => linear += 1,
            }
            worst.push(m.ring_p98_error);
        }
        println!(
            "{:<70} {:>5}x{:<5} boxes {:>3}  within6 {:>3}  busy {:>3}",
            f.file_name().unwrap().to_string_lossy(),
            img.width(),
            img.height(),
            n,
            p,
            b
        );
        total += n;
        pass += p;
        busy += b;
    }
    worst.sort_unstable();
    let pct = |q: f64| {
        worst
            .get(((worst.len() as f64 * q) as usize).min(worst.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0)
    };
    println!(
        "S2 TOTAL: {} pictures, {total} boxes, within 6 levels {pass} ({:.1} %), busy {busy}, solid {solid}, linear {linear}; p98 error median {} p90 {} max {}",
        files.len(),
        100.0 * f64::from(pass) / f64::from(total.max(1)),
        pct(0.5),
        pct(0.9),
        pct(1.0)
    );
}

/// S2 on a folder of real screenshots (`CM_S2_DIR`, searched recursively; nothing is copied into
/// the repository). Prints one `BOX` line per recognised text line and, with `CM_S2_CROPS=<dir>`,
/// writes before/after crops (Retype with an empty text: the fill alone) of every `CM_S2_STEP`-th
/// box and of some failing ones, for looking at seams. Counts only; no text is printed.
#[test]
#[ignore = "needs CM_S2_DIR, a recognizer"]
fn s2_real_set() {
    use cm_render::ocr::system_recognizer;
    let Ok(dir) = std::env::var("CM_S2_DIR") else {
        println!("SKIPPED: CM_S2_DIR");
        return;
    };
    if !system_recognizer().is_available() {
        println!("SKIPPED: no recognizer");
        return;
    }
    let crops = std::env::var("CM_S2_CROPS").ok();
    let step: usize = std::env::var("CM_S2_STEP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(25);
    let mut files = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(&dir)];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "png") {
                files.push(p);
            }
        }
    }
    files.sort();
    let mut k = 0usize;
    for f in &files {
        let img = image::open(f).unwrap().to_rgba8();
        let rel = f
            .strip_prefix(&dir)
            .unwrap()
            .to_string_lossy()
            .replace(' ', "_");
        let lines = system_recognizer().recognize(&img).unwrap();
        for l in lines.iter().filter(|l| !l.text.trim().is_empty()) {
            let r = Rc::new(
                l.rect.x.floor(),
                l.rect.y.floor(),
                l.rect.w.ceil() + 1.0,
                l.rect.h.ceil() + 1.0,
            );
            let m = measure_fill(&img, r);
            println!(
                "BOX {rel} {} {} {} {} err={} max={} kind={:?} busy={} sides={}",
                r.x,
                r.y,
                r.w,
                r.h,
                m.ring_p98_error,
                m.ring_max_error,
                m.fill.kind,
                m.busy,
                m.sides_used
            );
            k += 1;
            let failing = m.ring_p98_error > 6;
            let Some(out_dir) = &crops else { continue };
            if !k.is_multiple_of(step) && !(failing && k.is_multiple_of(4)) {
                continue;
            }
            let retype = format!(
                r##"[{{"op":"retype","rect":[{},{},{},{}],"fill":{},"text":"","size":12}}]"##,
                r.x,
                r.y,
                r.w,
                r.h,
                serde_json::to_string(&m.fill).unwrap()
            );
            let out = render_annotations(&img, &ops(&retype)).unwrap();
            let pad = 24.0;
            let x0 = (r.x - pad).max(0.0) as u32;
            let y0 = (r.y - pad).max(0.0) as u32;
            let w = ((r.w + 2.0 * pad) as u32).min(img.width() - x0).min(900);
            let h = ((r.h + 2.0 * pad) as u32).min(img.height() - y0);
            let mut sheet = RgbaImage::new(w * 2 + 6, h);
            imageops::replace(
                &mut sheet,
                &imageops::crop_imm(&img, x0, y0, w, h).to_image(),
                0,
                0,
            );
            imageops::replace(
                &mut sheet,
                &imageops::crop_imm(&out, x0, y0, w, h).to_image(),
                i64::from(w) + 6,
                0,
            );
            let name = format!("{out_dir}/crop_{k:04}_err{}.png", m.ring_p98_error);
            sheet.save(name).unwrap();
        }
    }
}

/// S5 (GPU part): the adapter's texture limit and whether a 16384 px texture can be created,
/// cleared and read back here (the same device settings as the compositor). Prints only; run by
/// hand: `cargo test -p cm-render --test touchup_034 s5_gpu -- --ignored --nocapture`.
#[test]
#[ignore = "opens the GPU and allocates up to ~1 GB; run by hand for spike S5"]
fn s5_gpu_texture_limits() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    })) else {
        println!("SKIPPED: no GPU adapter");
        return;
    };
    let info = adapter.get_info();
    let limits = adapter.limits();
    println!(
        "S5 adapter: {} ({:?}), max_texture_dimension_2d {}, max_buffer_size {}",
        info.name, info.backend, limits.max_texture_dimension_2d, limits.max_buffer_size
    );
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("s5"),
        required_limits: limits.clone(),
        ..Default::default()
    }))
    .unwrap();
    for (w, h) in [(16_384u32, 3_072u32), (3_072, 16_384), (16_384, 16_384)] {
        let t = std::time::Instant::now();
        let g_val = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let g_oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("s5"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        drop(enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        }));
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rb"),
            size: 256 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        // The far corner of the texture.
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: w - 256,
                    y: h - 1,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256 * 4),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(enc.finish()));
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let read = rx.recv().unwrap();
        let px = slice.get_mapped_range()[..4].to_vec();
        let oom = pollster::block_on(g_oom.pop());
        let val = pollster::block_on(g_val.pop());
        println!(
            "S5 gpu {w}x{h}: readback {:?} pixel {px:?}, oom {oom:?}, validation {val:?}, {:?}",
            read.is_ok(),
            t.elapsed()
        );
    }
}
