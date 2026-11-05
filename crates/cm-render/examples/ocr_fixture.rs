//! Writes `tests/data/regress/redact-text.png`: black text on white drawn by the project's own text
//! renderer (an e-mail address and an IPv4 address, 30 px). `cm-regress` uses it to check Auto-redact
//! against the OS text recogniser of a shipped build. Synthetic; no real person or address.
//!
//! `cargo run -p cm-render --example ocr_fixture -- tests/data/regress/redact-text.png`

use cm_render::{parse_ops_str, render_annotations};
use image::{Rgba, RgbaImage};

fn text_op(x: u32, y: u32, text: &str) -> String {
    format!(
        r##"{{"op":"text","at":[{x},{y}],"text":"{text}","size":30,"color":"#000000","outline":null}}"##
    )
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "redact-text.png".to_owned());
    let base = RgbaImage::from_pixel(1000, 260, Rgba([255, 255, 255, 255]));
    let ops = format!(
        "[{},{}]",
        text_op(40, 40, "Owner: jane.doe@example.com"),
        text_op(40, 140, "Server address 203.0.113.45 is down")
    );
    let ops = parse_ops_str(&ops).expect("ops");
    let img = render_annotations(&base, &ops).expect("render");
    img.save(&out).expect("save");
    println!("wrote {out}");
}
