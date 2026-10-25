//! ANN-2 Auto-redact against the real OS recognizer (Apple Vision on macOS). Ignored: it
//! prints SKIPPED unless `CM_OCR_REAL=1` is set. The picture is synthetic (text drawn by the
//! project's own text renderer on white); no screen, camera or microphone is touched.
//!
//! `CM_OCR_REAL=1 cargo test -p cm-render --test ocr_real -- --ignored --nocapture`

#![allow(clippy::unwrap_used)]

use cm_render::ocr::{suggest_redactions, system_recognizer, RedactionKind, SuggestOptions};
use cm_render::{parse_ops_str, render_annotations};
use image::{Rgba, RgbaImage};

fn text_op(x: u32, y: u32, text: &str) -> String {
    format!(
        r##"{{"op":"text","at":[{x},{y}],"text":"{text}","size":30,"color":"#000000","outline":null}}"##
    )
}

#[test]
#[ignore = "runs the real OS text recognizer; set CM_OCR_REAL=1"]
fn real_recognizer_finds_two_emails_and_a_key_on_a_synthetic_png() {
    if std::env::var("CM_OCR_REAL").as_deref() != Ok("1") {
        println!("SKIPPED: set CM_OCR_REAL=1 to run the real text recognizer");
        return;
    }
    let rec = system_recognizer();
    if !rec.is_available() {
        println!("SKIPPED: no text recognizer on this machine");
        return;
    }
    let base = RgbaImage::from_pixel(1400, 420, Rgba([255, 255, 255, 255]));
    let ops = format!(
        "[{},{},{}]",
        text_op(40, 40, "Owner: jane.doe@example.com"),
        text_op(40, 140, "Backup contact is ops-team@mail.example.org today"),
        text_op(40, 240, "key sk-proj-A1b2C3d4E5f6G7h8I9j0K1l2")
    );
    let img = render_annotations(&base, &parse_ops_str(&ops).unwrap()).unwrap();

    let lines = rec.recognize(&img).unwrap();
    for l in &lines {
        println!("{:?} conf={:.2} rect={:?}", l.text, l.confidence, l.rect);
    }
    assert!(!lines.is_empty(), "the recognizer found no text at all");
    let found = suggest_redactions(
        &lines,
        (img.width(), img.height()),
        &SuggestOptions::default(),
    );
    for s in &found {
        println!("{:?} {:?} {:.2}", s.kind, s.rect, s.confidence);
    }
    let emails: Vec<_> = found
        .iter()
        .filter(|s| s.kind == RedactionKind::Email)
        .collect();
    assert_eq!(emails.len(), 2, "two emails expected: {found:?}");
    // The first address sits on the first text row (y 40..80), the second on the second.
    assert!(emails[0].rect.y < 100.0 && emails[1].rect.y > 100.0);
    assert!(
        found.iter().any(|s| s.kind == RedactionKind::Token),
        "{found:?}"
    );
}
