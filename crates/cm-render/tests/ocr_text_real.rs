//! ANN-5 copy text against the real OS recognizer on SYNTHETIC PNGs (text drawn by the project's
//! own renderer on white; no screen, camera or microphone, no clipboard). Ignored: prints
//! SKIPPED unless `CM_OCR_REAL=1`, and when no recognizer is installed.
//!
//! `CM_OCR_REAL=1 cargo test -p cm-render --test ocr_text_real -- --ignored --nocapture`

#![allow(clippy::unwrap_used)]

use cm_render::ocr::{layout_text, system_recognizer, LayoutOptions};
use cm_render::{parse_ops_str, render_annotations};
use image::{Rgba, RgbaImage};

fn text_op(x: u32, y: u32, text: &str) -> String {
    format!(
        r##"{{"op":"text","at":[{x},{y}],"text":{},"size":34,"color":"#000000","outline":null}}"##,
        serde_json::to_string(text).unwrap()
    )
}

fn page(rows: &[&str]) -> RgbaImage {
    let base = RgbaImage::from_pixel(1500, 100 + 80 * rows.len() as u32, Rgba([255; 4]));
    let ops: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(i, t)| text_op(40, 40 + 80 * i as u32, t))
        .collect();
    render_annotations(
        &base,
        &parse_ops_str(&format!("[{}]", ops.join(","))).unwrap(),
    )
    .unwrap()
}

fn engine_ready() -> bool {
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

#[test]
#[ignore = "runs the real OS text recognizer; set CM_OCR_REAL=1"]
fn english_page_is_read_in_order() {
    if !engine_ready() {
        return;
    }
    let img = page(&["Save the file before closing", "Error 404: page not found"]);
    let lines = system_recognizer().recognize(&img).unwrap();
    let text = layout_text(&lines, &LayoutOptions::default());
    println!("english: {text:?}");
    let expected = "Save the file before closing\n\nError 404: page not found";
    println!("english character error rate: {:.3}", cer(expected, &text));
    assert!(text.contains("Save the file"), "{text}");
    assert!(text.find("Save").unwrap() < text.find("Error").unwrap());
}

/// Vietnamese with tone marks: prints what the engine returned and the character error rate
/// against the drawn text; it records, it does not assert a minimum, because whether the engine
/// reads Vietnamese is exactly what is being found out. It only asserts the engine returned text.
#[test]
#[ignore = "runs the real OS text recognizer; set CM_OCR_REAL=1"]
fn vietnamese_with_tone_marks_is_recorded() {
    if !engine_ready() {
        return;
    }
    let rows = [
        "Xin chào, đây là ảnh chụp màn hình",
        "Tiếng Việt có dấu: ắ ằ ẳ ẵ ặ ế ề ể ễ ệ",
        "Người dùng không thể lưu tệp",
    ];
    let img = page(&rows);
    let lines = system_recognizer().recognize(&img).unwrap();
    let text = layout_text(&lines, &LayoutOptions::default());
    println!("vietnamese expected: {rows:?}");
    println!("vietnamese got: {text:?}");
    let expected = rows.join("\n");
    println!(
        "vietnamese character error rate: {:.3}",
        cer(&expected, &text)
    );
    assert!(!lines.is_empty(), "no text at all");
}

/// Levenshtein distance over characters divided by the expected length.
fn cer(expected: &str, got: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (expected.chars().collect(), got.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i];
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur.push(sub.min(prev[j] + 1).min(cur[j - 1] + 1));
        }
        prev = cur;
    }
    prev[b.len()] as f64 / a.len().max(1) as f64
}
