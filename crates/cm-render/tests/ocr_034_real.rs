//! The OCR forms of ANN-8.AC3, ANN-9.AC1 and ANN-11.AC1 against the real OS recognizer on
//! SYNTHETIC pictures (text drawn by the project's own renderer; no screen, camera or
//! microphone). Ignored: prints SKIPPED unless `CM_OCR_REAL=1` and a recognizer is installed.
//!
//! `CM_OCR_REAL=1 cargo test -p cm-render --test ocr_034_real -- --ignored --nocapture`

#![allow(clippy::unwrap_used)]

use cm_render::ocr::system_recognizer;
use cm_render::{
    measure_blueprint, measure_fill, measure_text_style, parse_ops_str, render_annotations, Rc,
};
use image::{Rgba, RgbaImage};

fn ready() -> bool {
    if std::env::var("CM_OCR_REAL").as_deref() != Ok("1") {
        println!("SKIPPED: set CM_OCR_REAL=1 to run the real text recognizer");
        return false;
    }
    if !system_recognizer().is_available() {
        println!("SKIPPED: no text recognizer on this machine");
        return false;
    }
    true
}

fn text(img: &RgbaImage, x: u32, y: u32, t: &str, size: u32) -> RgbaImage {
    let j = format!(
        r##"[{{"op":"text","at":[{x},{y}],"text":"{t}","size":{size},"color":"#161616","outline":null}}]"##
    );
    render_annotations(img, &parse_ops_str(&j).unwrap()).unwrap()
}

fn read(img: &RgbaImage) -> Vec<String> {
    system_recognizer()
        .recognize(img)
        .unwrap()
        .into_iter()
        .map(|l| l.text)
        .collect()
}

/// ANN-9.AC1: OCR reads "Save all", not "Save".
#[test]
#[ignore = "runs the real OS text recognizer; set CM_OCR_REAL=1"]
fn retype_is_read_back_by_ocr() {
    if !ready() {
        return;
    }
    let bg = RgbaImage::from_pixel(420, 120, Rgba([236, 236, 240, 255]));
    let shot = text(&bg, 40, 34, "Save", 40);
    println!("before: {:?}", read(&shot));
    let rect = Rc::new(32.0, 28.0, 320.0, 60.0);
    let m = measure_fill(&shot, rect);
    let t = measure_text_style(&shot, rect, &m.fill, Some("Save"));
    let j = format!(
        r##"[{{"op":"retype","rect":[32,28,320,60],"fill":{},"text":"Save all","size":{},"color":"{}"}}]"##,
        serde_json::to_string(&m.fill).unwrap(),
        t.size,
        t.color
    );
    let out = render_annotations(&shot, &parse_ops_str(&j).unwrap()).unwrap();
    let lines = read(&out);
    println!(
        "after: {lines:?} (measured size {}, colour {})",
        t.size, t.color
    );
    assert!(lines.iter().any(|l| l.trim() == "Save all"), "{lines:?}");
    assert!(!lines.iter().any(|l| l.trim() == "Save"), "{lines:?}");
}

/// ANN-8.AC3: no characters of the blurred text in the render or in the loupe, at zoom 2, 4, 8.
#[test]
#[ignore = "runs the real OS text recognizer; set CM_OCR_REAL=1"]
fn blurred_text_is_not_readable_in_the_loupe() {
    if !ready() {
        return;
    }
    let bg = RgbaImage::from_pixel(3000, 1000, Rgba([250, 250, 250, 255]));
    let shot = text(&bg, 30, 40, "SECRETWORD 4111", 36);
    println!("plain: {:?}", read(&shot));
    for zoom in [2u32, 4, 8] {
        let lens = format!(
            r##"{{"op":"magnify","source":[24,30,330,60],"at":[1500,650],"zoom":{zoom},"border_width":0,"connector":false,"badge":false}}"##
        );
        let blur = r##"{"op":"blur","rect":[24,30,330,60],"strength":24}"##;
        for list in [format!("[{lens},{blur}]"), format!("[{blur},{lens}]")] {
            let out = render_annotations(&shot, &parse_ops_str(&list).unwrap()).unwrap();
            let found = read(&out).join(" ");
            println!("zoom {zoom}: {found:?}");
            assert!(
                !found.contains("SECRET") && !found.contains("4111"),
                "zoom {zoom}: {found}"
            );
        }
        // Without the blur the loupe is readable, so the test can fail.
        let out = render_annotations(&shot, &parse_ops_str(&format!("[{lens}]")).unwrap()).unwrap();
        let found = read(&out).join(" ");
        assert!(found.contains("SECRET"), "control at zoom {zoom}: {found}");
    }
}

/// ANN-11.AC1: with Blueprint applied and 2 lines kept, OCR finds only the 2 kept lines.
#[test]
#[ignore = "runs the real OS text recognizer; set CM_OCR_REAL=1"]
fn blueprint_leaves_only_the_kept_lines_for_ocr() {
    if !ready() {
        return;
    }
    let mut img = RgbaImage::from_pixel(520, 20 * 44 + 30, Rgba([245, 245, 247, 255]));
    let mut boxes = Vec::new();
    for i in 0..20u32 {
        let y = 14 + i * 44;
        img = text(&img, 20, y, &format!("Option {i:02} label"), 26);
        boxes.push(Rc::new(14.0, f64::from(y) - 2.0, 300.0, 36.0));
    }
    assert!(read(&img).len() >= 18, "{:?}", read(&img));
    let keep = [4usize, 13];
    let styles = measure_blueprint(&img, &boxes);
    let pick = |f: &dyn Fn(usize) -> serde_json::Value| -> Vec<serde_json::Value> {
        (0..20).filter(|i| !keep.contains(i)).map(f).collect()
    };
    let j = serde_json::json!([{
        "op": "blueprint",
        "boxes": pick(&|i| serde_json::json!(boxes[i])),
        "fills": pick(&|i| serde_json::json!(styles[i].fill)),
        "colors": pick(&|i| serde_json::json!(styles[i].color)),
    }]);
    let out = render_annotations(&img, &parse_ops_str(&j.to_string()).unwrap()).unwrap();
    let found = read(&out);
    println!("found: {found:?}");
    assert!(found.iter().any(|l| l.contains("Option 04")), "{found:?}");
    assert!(found.iter().any(|l| l.contains("Option 13")), "{found:?}");
    let others = found
        .iter()
        .filter(|l| l.contains("Option") && !l.contains("04") && !l.contains("13"))
        .count();
    assert_eq!(others, 0, "{found:?}");
    let _ = measure_text_style; // (kept for the editor-side flow in the doc)
}
