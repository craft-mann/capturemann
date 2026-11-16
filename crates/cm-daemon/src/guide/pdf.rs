//! PDF export of a guide (GDE-3, spike S4): A4 or Letter, the steps flowing one after another so
//! one or two fit on a page, the Geist fonts embedded as subsets (Vietnamese included), the
//! step pictures as they are exported to Markdown and HTML (rendered: annotations and blurs
//! applied). Pure Rust (`krilla`, MIT OR Apache-2.0); the S4 run wrote 10 steps in 65 ms.

use cm_core::CmError;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font, TextDirection};
use krilla::Document;

use super::export::ExportDoc;
use super::Headings;

const REGULAR: &[u8] = include_bytes!("../../../../design/fonts/Geist-Regular.ttf");
const SEMIBOLD: &[u8] = include_bytes!("../../../../design/fonts/Geist-SemiBold.ttf");

/// Paper size in PDF points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Paper {
    #[default]
    A4,
    Letter,
}

impl Paper {
    fn size(self) -> (f32, f32) {
        match self {
            Paper::A4 => (595.0, 842.0),
            Paper::Letter => (612.0, 792.0),
        }
    }
}

const MARGIN: f32 = 48.0;
const TITLE_PT: f32 = 24.0;
const HEAD_PT: f32 = 16.0;
const BODY_PT: f32 = 11.0;
/// Space between a step heading (or its note) and its picture.
const HEAD_TO_PICTURE: f32 = 6.0;
/// The tallest a picture is drawn, so two steps fit on most pages.
const MAX_IMAGE_H: f32 = 280.0;

fn err(what: &str, e: impl std::fmt::Debug) -> CmError {
    CmError::internal(format!("PDF export: {what}: {e:?}"))
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
    for para in text.lines() {
        let mut cur = String::new();
        for w in para.split_whitespace() {
            let t = if cur.is_empty() {
                w.to_owned()
            } else {
                format!("{cur} {w}")
            };
            if advance(face, &t, size) > width && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
                cur = w.to_owned();
            } else {
                cur = t;
            }
        }
        lines.push(cur);
    }
    lines
}

/// Writes the PDF.
pub fn pdf(
    doc: &ExportDoc,
    headings: Headings,
    intro: bool,
    notes: bool,
    paper: Paper,
) -> Result<Vec<u8>, CmError> {
    let regular = Font::new(REGULAR.to_vec().into(), 0).ok_or_else(|| err("font", "rejected"))?;
    let bold = Font::new(SEMIBOLD.to_vec().into(), 0).ok_or_else(|| err("font", "rejected"))?;
    let face = ttf_parser::Face::parse(REGULAR, 0).map_err(|e| err("font", e))?;
    let face_b = ttf_parser::Face::parse(SEMIBOLD, 0).map_err(|e| err("font", e))?;
    let mut document = Document::new();
    let (pw, ph) = paper.size();
    let content_w = pw - 2.0 * MARGIN;

    // Everything to draw, as blocks: (height, what).
    enum Block {
        Text(Vec<String>, bool, f32),
        Image(Vec<u8>, f32, f32),
        Gap,
    }
    let mut blocks: Vec<(f32, Block, bool)> = Vec::new();
    let line_h = |pt: f32| pt * 1.4;
    let push_text =
        |blocks: &mut Vec<(f32, Block, bool)>, text: &str, pt: f32, strong: bool, keep: bool| {
            let f = if strong { &face_b } else { &face };
            let lines = wrap(f, text, pt, content_w);
            blocks.push((
                line_h(pt) * lines.len() as f32,
                Block::Text(lines, strong, pt),
                keep,
            ));
        };
    push_text(&mut blocks, &doc.title, TITLE_PT, true, true);
    if intro && !doc.intro.trim().is_empty() {
        blocks.push((6.0, Block::Gap, true));
        push_text(&mut blocks, doc.intro.trim(), BODY_PT, false, false);
    }
    blocks.push((14.0, Block::Gap, false));
    for (i, s) in doc.steps.iter().enumerate() {
        let h = super::export::heading(headings, &s.label, &s.title, i + 1);
        // A heading stays with what follows it (`keep`).
        push_text(&mut blocks, &h, HEAD_PT, true, true);
        if notes && !s.note.trim().is_empty() {
            push_text(&mut blocks, s.note.trim(), BODY_PT, false, true);
        }
        if let Some(img) = &s.image {
            blocks.push((HEAD_TO_PICTURE, Block::Gap, true));
            let w = (content_w).min(img.width as f32);
            let ratio = img.height as f32 / img.width.max(1) as f32;
            let (mut dw, mut dh) = (w, w * ratio);
            if dh > MAX_IMAGE_H {
                dh = MAX_IMAGE_H;
                dw = dh / ratio;
            }
            blocks.push((dh + 6.0, Block::Image(img.png.clone(), dw, dh), false));
        }
        blocks.push((18.0, Block::Gap, false));
    }

    // Lay out: a block that does not fit starts a new page, and a `keep` block goes with the next.
    let bottom = ph - MARGIN;
    let mut pages: Vec<Vec<(f32, usize)>> = vec![Vec::new()];
    let mut y = MARGIN;
    let mut i = 0;
    while i < blocks.len() {
        let (h, _, keep) = &blocks[i];
        // A run of `keep` blocks (heading, note, gap) goes with the block after it (the picture).
        let mut need = *h;
        if *keep {
            let mut j = i + 1;
            while j < blocks.len() {
                need += blocks[j].0;
                if !blocks[j].2 {
                    break;
                }
                j += 1;
            }
        }
        if y + need > bottom && y > MARGIN {
            pages.push(Vec::new());
            y = MARGIN;
        }
        if let Some(p) = pages.last_mut() {
            p.push((y, i));
        }
        y += h;
        i += 1;
    }

    for page_blocks in &pages {
        let mut page = document
            .start_page_with(PageSettings::from_wh(pw, ph).ok_or_else(|| err("page", "size"))?);
        let mut s = page.surface();
        for &(top, bi) in page_blocks {
            match &blocks[bi].1 {
                Block::Text(lines, strong, pt) => {
                    let font = if *strong { &bold } else { &regular };
                    for (k, l) in lines.iter().enumerate() {
                        if l.is_empty() {
                            continue;
                        }
                        s.draw_text(
                            Point::from_xy(MARGIN, top + line_h(*pt) * (k as f32 + 0.8)),
                            font.clone(),
                            *pt,
                            l,
                            false,
                            TextDirection::Auto,
                        );
                    }
                }
                Block::Image(png, w, h) => {
                    let image = Image::from_png(png.clone().into(), false)
                        .map_err(|e| err("picture", e))?;
                    s.push_transform(&Transform::from_translate(MARGIN, top));
                    s.draw_image(
                        image,
                        Size::from_wh(*w, *h).ok_or_else(|| err("picture", "size"))?,
                    );
                    // A hairline frame, so a light screenshot does not melt into the paper.
                    let mut pb = PathBuilder::new();
                    pb.push_rect(
                        Rect::from_xywh(0.0, 0.0, *w, *h).ok_or_else(|| err("frame", "size"))?,
                    );
                    if let Some(path) = pb.finish() {
                        s.set_fill(None);
                        s.set_stroke(Some(Stroke {
                            paint: rgb::Color::new(205, 207, 214).into(),
                            width: 0.75,
                            ..Stroke::default()
                        }));
                        s.draw_path(&path);
                        // The text that follows is filled and not outlined.
                        s.set_stroke(None);
                        s.set_fill(Some(Fill::default()));
                    }
                    s.pop();
                }
                Block::Gap => {}
            }
        }
        s.finish();
        page.finish();
    }
    document.finish().map_err(|e| err("finish", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guide::export::{ExportImage, ExportStep};

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([40, 120, 220, 255]));
        let mut out = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    fn doc(n: usize) -> ExportDoc {
        ExportDoc {
            title: "Cài đặt thanh toán".into(),
            intro: "Làm theo các bước sau. ".repeat(6),
            steps: (0..n)
                .map(|i| ExportStep {
                    label: (i + 1).to_string(),
                    title: format!("Nhấp “Lưu” {}", i + 1),
                    note: "Ế ề ể ễ ệ ạ ả ấ ầ ẩ ẫ ậ ơ ư ờ ở ỡ ợ ừ ử ữ ự".into(),
                    image: Some(ExportImage {
                        file: format!("images/step-{:02}.png", i + 1),
                        png: png(1200, 800),
                        width: 1200,
                        height: 800,
                    }),
                })
                .collect(),
        }
    }

    /// S4: ten steps in well under two seconds, fonts embedded, pages for both papers.
    #[test]
    fn ten_vietnamese_steps_are_written_fast_in_both_sizes() {
        for paper in [Paper::A4, Paper::Letter] {
            let t = std::time::Instant::now();
            let bytes = pdf(&doc(10), Headings::Numbered, true, true, paper).unwrap();
            // 2 s in a release build; a debug build on a shared CI runner took 2.3 s (Nov 16, 2026).
            let limit = if cfg!(debug_assertions) { 10.0 } else { 2.0 };
            assert!(t.elapsed().as_secs_f64() < limit, "{:?}", t.elapsed());
            assert!(bytes.starts_with(b"%PDF-"));
            let text = String::from_utf8_lossy(&bytes);
            assert!(text.contains("Geist-Regular") && text.contains("Geist-SemiBold"));
        }
    }
}
