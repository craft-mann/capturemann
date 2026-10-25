//! ANN-1 edge case: CJK and emoji text must render real glyphs (not missing-glyph boxes) using
//! only bundled fonts, so output is identical on every machine.
#![allow(clippy::unwrap_used)]

mod common;
use cm_render::{render_annotations, Renderer};
use common::*;

fn text_json(text: &str) -> String {
    format!(
        r##"[{{"op":"text","at":[10,10],"text":{},"size":28,"color":"#FFFFFF","outline":null,"bg":null}}]"##,
        serde_json::to_string(text).unwrap()
    )
}

fn render(text: &str) -> image::RgbaImage {
    let base = image::RgbaImage::from_pixel(240, 60, image::Rgba([0, 0, 0, 255]));
    render_annotations(&base, &ops(&text_json(text))).unwrap()
}

fn ink(img: &image::RgbaImage) -> usize {
    img.pixels().filter(|p| p.0[0] > 64).count()
}

/// U+10FFFD is in no bundled font, so it draws the primary font's missing-glyph box.
fn tofu() -> image::RgbaImage {
    render("\u{10FFFD}")
}

#[test]
fn cjk_glyphs_render_without_missing_glyph_boxes() {
    let tofu = tofu();
    assert!(ink(&tofu) > 0, "sanity: tofu draws something");
    let mut seen = vec![];
    for ch in ['你', '好', 'こ', 'ん', 'に', 'ち', 'は'] {
        let img = render(&ch.to_string());
        assert!(ink(&img) > 20, "{ch} rendered no pixels");
        assert_ne!(img, tofu, "{ch} rendered as the missing-glyph box");
        assert!(!seen.contains(&img), "{ch} identical to another glyph");
        seen.push(img);
    }
}

#[test]
fn emoji_glyphs_render_without_missing_glyph_boxes() {
    let tofu = tofu();
    let mut seen = vec![];
    for ch in ["😀", "🎉", "👍", "❤"] {
        let img = render(ch);
        assert!(ink(&img) > 20, "{ch} rendered no pixels");
        assert_ne!(img, tofu, "{ch} rendered as the missing-glyph box");
        assert!(!seen.contains(&img), "{ch} identical to another glyph");
        seen.push(img);
    }
}

#[test]
fn bundled_fallback_is_deterministic() {
    let base = image::RgbaImage::from_pixel(240, 60, image::Rgba([0, 0, 0, 255]));
    let o = ops(&text_json("你好 こんにちは 😀"));
    let a = Renderer::new().render(&base, &o).unwrap();
    let b = Renderer::new().render(&base, &o).unwrap();
    assert_eq!(a, b);
}

const LIMIT: f64 = 0.01;

#[test]
fn ann1_text_cjk_golden() {
    let base = base_image(200, 120);
    let out = render_annotations(
        &base,
        &ops(
            r##"[{"op":"text","at":[10,20],"text":"你好 こんにちは","size":24,"color":"#FFFFFF","outline":"#000000","bg":null}]"##,
        ),
    )
    .unwrap();
    assert_golden("text_cjk", &out, LIMIT);
    assert!(diff_fraction(&out, &base, 8) > 0.02);
}

#[test]
fn ann1_text_emoji_golden() {
    let base = base_image(200, 120);
    let out = render_annotations(
        &base,
        &ops(
            r##"[{"op":"text","at":[10,20],"text":"Done 😀🎉👍","size":24,"color":"#FFFFFF","outline":"#000000","bg":null}]"##,
        ),
    )
    .unwrap();
    assert_golden("text_emoji", &out, LIMIT);
    assert!(diff_fraction(&out, &base, 8) > 0.02);
}
