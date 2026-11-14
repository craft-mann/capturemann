//! Spike S4: can Click Guide write a PDF with no new native dependency?
//!
//! Pass if: a 10-step guide (A4, two steps per page, Vietnamese titles and notes, a 1200 x 800
//! screenshot per step) is written in under 2 s with the fonts embedded (subset), and every
//! character of the text has a glyph in the font.
//!
//! ```text
//! cargo run --release --manifest-path spikes/Cargo.toml -p spike-pdf -- [font.ttf] [out.pdf]
//! ```
//!
//! The writer is `krilla` (MIT OR Apache-2.0; pure Rust). The check of the licences of the whole
//! tree is `cargo deny` (see the report in `docs/development/spike-s4-pdf.md`).

use std::time::Instant;

use anyhow::{bail, Context, Result};
use krilla::geom::{Point, Size, Transform};
use krilla::page::PageSettings;
use krilla::text::{Font, TextDirection};
use krilla::Document;

const A4: (f32, f32) = (595.0, 842.0);
const MARGIN: f32 = 48.0;

const TITLES: [&str; 10] = [
    "Mở trang Cài đặt",
    "Chọn mục Thanh toán",
    "Nhấp “Thêm thẻ”",
    "Nhập số thẻ và ngày hết hạn",
    "Nhấp “Lưu”",
    "Quay lại Tổng quan",
    "Mở menu Người dùng",
    "Chọn “Mời đồng nghiệp”",
    "Nhập địa chỉ email",
    "Nhấn Gửi lời mời",
];
const NOTE: &str = "Nếu không thấy nút, hãy kéo cửa sổ rộng hơn. Ế ề ể ễ ệ ạ ả ấ ầ ẩ ẫ ậ ơ ư ờ ở ỡ ợ ừ ử ữ ự ỳ ỷ ỹ ỵ Đ đ.";

fn screenshot(n: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(1200, 800, |x, y| {
        let band = ((y / 40 + n) % 2) as u8 * 12;
        if (400..560).contains(&x) && (300..360).contains(&y) {
            image::Rgba([31, 111, 235, 255])
        } else {
            image::Rgba([(x / 6) as u8 / 2 + 100 + band, 235 - band, (y / 4) as u8 / 2 + 80, 255])
        }
    });
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("png");
    out
}

fn advance(face: &ttf_parser::Face<'_>, text: &str, size: f32) -> f32 {
    let upm = f32::from(face.units_per_em());
    text.chars()
        .map(|c| {
            face.glyph_index(c)
                .and_then(|g| face.glyph_hor_advance(g))
                .map_or(0.5 * upm, f32::from)
        })
        .sum::<f32>()
        * size
        / upm
}

fn wrap(face: &ttf_parser::Face<'_>, text: &str, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for w in text.split_whitespace() {
        let t = if cur.is_empty() { w.to_owned() } else { format!("{cur} {w}") };
        if advance(face, &t, size) > width && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur = w.to_owned();
        } else {
            cur = t;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let font_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "design/fonts/Geist-Regular.ttf".to_owned());
    let out_path = args.get(2).cloned().unwrap_or_else(|| "guide.pdf".to_owned());
    let font_bytes = std::fs::read(&font_path).with_context(|| format!("font {font_path}"))?;

    // Coverage: every character that will be drawn must have a glyph.
    let face = ttf_parser::Face::parse(&font_bytes, 0)?;
    let mut text = String::from(NOTE);
    TITLES.iter().for_each(|t| text.push_str(t));
    let missing: Vec<char> = text
        .chars()
        .filter(|c| !c.is_whitespace() && face.glyph_index(*c).is_none())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    println!("font: {font_path}; characters without a glyph: {missing:?}");

    let pngs: Vec<Vec<u8>> = (0..10).map(screenshot).collect();
    let t0 = Instant::now();
    let font = Font::new(font_bytes.clone().into(), 0).context("font rejected by krilla")?;
    let mut doc = Document::new();
    let content_w = A4.0 - 2.0 * MARGIN;
    let img_w = content_w;
    let img_h = img_w * 800.0 / 1200.0 * 0.62;
    let img_w = img_h * 1200.0 / 800.0;
    for (pi, pair) in (0..10usize).collect::<Vec<_>>().chunks(2).enumerate() {
        let mut page = doc.start_page_with(PageSettings::from_wh(A4.0, A4.1).context("page")?);
        let mut s = page.surface();
        let _ = pi;
        for (slot, &i) in pair.iter().enumerate() {
            let top = MARGIN + slot as f32 * (A4.1 - 2.0 * MARGIN) / 2.0;
            let heading = format!("{}. {}", i + 1, TITLES[i]);
            s.draw_text(Point::from_xy(MARGIN, top + 18.0), font.clone(), 18.0, &heading, false, TextDirection::Auto);
            let mut y = top + 40.0;
            for line in wrap(&face, NOTE, 11.0, content_w) {
                s.draw_text(Point::from_xy(MARGIN, y), font.clone(), 11.0, &line, false, TextDirection::Auto);
                y += 15.0;
            }
            let image = krilla::image::Image::from_png(pngs[i].clone().into(), false)
                .map_err(|e| anyhow::anyhow!("png: {e}"))?;
            s.push_transform(&Transform::from_translate(MARGIN, y + 6.0));
            s.draw_image(image, Size::from_wh(img_w, img_h).context("size")?);
            s.pop();
        }
        s.finish();
        page.finish();
    }
    let pdf = doc.finish().map_err(|e| anyhow::anyhow!("finish: {e:?}"))?;
    let took = t0.elapsed();
    std::fs::write(&out_path, &pdf)?;
    println!(
        "wrote {out_path}: {} bytes, 5 pages, 10 steps, {:.0} ms",
        pdf.len(),
        took.as_secs_f64() * 1000.0
    );
    if !missing.is_empty() {
        bail!("FAIL: the font lacks glyphs for {missing:?}");
    }
    if took.as_secs_f64() >= 2.0 {
        bail!("FAIL: took {:.2} s (limit 2 s)", took.as_secs_f64());
    }
    println!("PASS");
    Ok(())
}
