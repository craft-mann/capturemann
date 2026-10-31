//! ANN-2 Auto-redact through the system `tesseract` helper program (Linux, and anywhere it is
//! installed). Prints SKIPPED when `tesseract` or its English data is absent. The pictures are
//! synthetic (text drawn by the project's own renderer); no screen, camera or microphone is
//! touched and nothing is downloaded.
//!
//! `cargo test -p cm-render --test ocr_tesseract -- --nocapture`

#![allow(clippy::unwrap_used)]

use cm_render::ocr::tesseract::TesseractRecognizer;
use cm_render::ocr::{suggest_redactions, RedactionKind, SuggestOptions, TextRecognizer};
use cm_render::{parse_ops_str, render_annotations};
use image::{Rgba, RgbaImage};

fn text_op(x: u32, y: u32, text: &str, size: u32, color: &str) -> String {
    format!(
        r##"{{"op":"text","at":[{x},{y}],"text":"{text}","size":{size},"color":"{color}","outline":null}}"##
    )
}

fn picture(bg: [u8; 4], fg: &str) -> RgbaImage {
    let base = RgbaImage::from_pixel(900, 260, Rgba(bg));
    let ops = format!(
        "[{},{},{}]",
        text_op(30, 30, "Owner: jane.doe@example.com", 16, fg),
        text_op(30, 90, "Call 415-555-0132 or 212-555-0147 today", 16, fg),
        text_op(30, 150, "Card 4111 1111 1111 1111 exp 04/29", 16, fg)
    );
    render_annotations(&base, &parse_ops_str(&ops).unwrap()).unwrap()
}

fn engine() -> Option<TesseractRecognizer> {
    let rec = TesseractRecognizer::system();
    match rec.engine() {
        Ok(p) => {
            println!("using {}", p.display());
            Some(rec)
        }
        Err(e) => {
            println!("SKIPPED: {e}");
            None
        }
    }
}

fn kinds_found(img: &RgbaImage, rec: &TesseractRecognizer) -> Vec<RedactionKind> {
    let lines = rec.recognize(img).unwrap();
    for l in &lines {
        println!("{:?} conf={:.2} rect={:?}", l.text, l.confidence, l.rect);
    }
    assert!(!lines.is_empty(), "tesseract found no text at all");
    let found = suggest_redactions(
        &lines,
        (img.width(), img.height()),
        &SuggestOptions::default(),
    );
    for s in &found {
        println!("{:?} {:?} {:.2}", s.kind, s.rect, s.confidence);
    }
    found.iter().map(|s| s.kind).collect()
}

#[test]
fn light_and_dark_pictures_give_an_email_a_phone_and_a_card() {
    let Some(rec) = engine() else { return };
    for (name, bg, fg) in [
        ("light", [255, 255, 255, 255], "#202020"),
        ("dark", [24, 26, 32, 255], "#E6E6E6"),
    ] {
        println!("--- {name}");
        let kinds = kinds_found(&picture(bg, fg), &rec);
        assert!(kinds.contains(&RedactionKind::Email), "{name}: {kinds:?}");
        assert!(kinds.contains(&RedactionKind::Phone), "{name}: {kinds:?}");
        assert!(kinds.contains(&RedactionKind::Card), "{name}: {kinds:?}");
    }
}

#[test]
fn a_picture_without_text_gives_no_lines() {
    let Some(rec) = engine() else { return };
    let blank = RgbaImage::from_pixel(300, 120, Rgba([255, 255, 255, 255]));
    assert!(rec.recognize(&blank).unwrap().is_empty());
}
