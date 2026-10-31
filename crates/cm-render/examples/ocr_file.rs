//! Runs the system text recognizer and the Auto-redact detector over picture files and prints
//! the recognized lines and the proposals (masked previews only, as `suggest_redactions` does).
//! A debugging aid for a recognizer on a new platform; reads files, writes nothing.
//!
//! `cargo run -p cm-render --example ocr_file --release -- <picture.png>...`

#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::print_stderr)]

use cm_render::ocr::{suggest_redactions, system_recognizer, SuggestOptions};

fn main() {
    let rec = system_recognizer();
    println!("{}", cm_render::ocr::describe_system());
    if !rec.is_available() {
        eprintln!(
            "no text recognizer here: {}",
            rec.install_hint()
                .unwrap_or_else(|| "none on this platform".into())
        );
        std::process::exit(1);
    }
    println!("recognizer: {}", rec.name());
    for path in std::env::args().skip(1) {
        let img = match image::open(&path) {
            Ok(i) => i.to_rgba8(),
            Err(e) => {
                eprintln!("{path}: {e}");
                continue;
            }
        };
        let lines = rec.recognize(&img).unwrap();
        println!(
            "== {path} ({}x{}), {} lines",
            img.width(),
            img.height(),
            lines.len()
        );
        for l in &lines {
            println!("  line {:?} conf={:.2}", l.text, l.confidence);
        }
        let found = suggest_redactions(
            &lines,
            (img.width(), img.height()),
            &SuggestOptions::default(),
        );
        for s in &found {
            println!(
                "  -> {} {:?} conf={:.2} preview={}",
                s.kind.as_str(),
                [
                    s.rect.x.round(),
                    s.rect.y.round(),
                    s.rect.w.round(),
                    s.rect.h.round()
                ],
                s.confidence,
                s.text_preview
            );
        }
    }
}
