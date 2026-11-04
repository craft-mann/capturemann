//! The pure half of the Windows recognizer: picture preparation, mapping `Windows.Media.Ocr`
//! words to [`TextLine`]s and the sentences about languages. No Windows API is called here, so
//! it is tested on every platform; the WinRT calls are in `winrt` (Windows only).
//!
//! `Windows.Media.Ocr` reads only the languages that are installed as OCR capabilities
//! (`Language.OCR`) and, with `TryCreateFromUserProfileLanguages`, only the ones in the user's
//! language list. Which ones those are is shown in diagnostics and in the install hint, never
//! assumed.

use super::{TextLine, TextWord};
use cm_core::Rc;
use image::imageops::FilterType;
use image::RgbaImage;

/// Upper bound for the up-scaled picture (pixels), to bound memory on 5K screens.
const MAX_SCALED_PIXELS: u64 = 40_000_000;
/// The scale factor tried first: UI text is 9 to 14 px high and the engine reads best at about
/// twice that.
const PREFERRED_SCALE: f64 = 2.0;

/// One word as the engine reports it: its text and box in the pixels of the picture it was given.
#[derive(Debug, Clone, PartialEq)]
pub struct RawWord {
    pub text: String,
    pub rect: Rc,
}

/// One line as the engine reports it: only its words (the engine gives a line no box).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawLine {
    pub words: Vec<RawWord>,
}

/// The factor to scale a `w` x `h` picture by before recognition: up to [`PREFERRED_SCALE`] for
/// small pictures, less when the result would pass the engine's longest side (`max_dim`) or the
/// pixel bound, and below 1 when the picture itself is longer than `max_dim`.
pub fn plan_scale(w: u32, h: u32, max_dim: u32) -> f64 {
    let (w, h) = (f64::from(w.max(1)), f64::from(h.max(1)));
    let longest = w.max(h);
    let max_dim = f64::from(max_dim.max(1));
    let by_side = PREFERRED_SCALE.min(max_dim / longest);
    let by_pixels = (MAX_SCALED_PIXELS as f64 / (w * h)).sqrt();
    let s = by_side.min(by_pixels);
    // Small enlargements are not worth a resample.
    if (1.0..1.25).contains(&s) {
        1.0
    } else {
        s
    }
}

/// The picture as the engine wants it: BGRA8 bytes, size, and the factor used.
pub struct Prepared {
    pub bgra: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// Scales the picture by [`plan_scale`] and swaps its channels to BGRA (alpha forced opaque:
/// the engine reads brightness, and a transparent corner must not turn black).
pub fn prepare(image: &RgbaImage, max_dim: u32) -> Prepared {
    let (w, h) = image.dimensions();
    let scale = plan_scale(w, h, max_dim);
    let scaled;
    let src = if (scale - 1.0).abs() > f64::EPSILON && w > 0 && h > 0 {
        let nw = ((f64::from(w) * scale).round() as u32).max(1);
        let nh = ((f64::from(h) * scale).round() as u32).max(1);
        scaled = image::imageops::resize(image, nw, nh, FilterType::CatmullRom);
        &scaled
    } else {
        image
    };
    let mut bgra = Vec::with_capacity(src.as_raw().len());
    for p in src.pixels() {
        bgra.extend_from_slice(&[p[2], p[1], p[0], 255]);
    }
    Prepared {
        bgra,
        width: src.width(),
        height: src.height(),
        scale: if w == 0 {
            1.0
        } else {
            f64::from(src.width()) / f64::from(w)
        },
    }
}

/// Engine lines to [`TextLine`]s in the original picture's pixels: boxes divided by the scale,
/// a line's box is the union of its word boxes, empty words and lines are dropped. The engine
/// gives no confidence, so a line counts as fully confident (the detectors' own thresholds
/// decide what is shown).
pub fn lines_from_raw(raw: &[RawLine], scale: f64) -> Vec<TextLine> {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let mut out = Vec::with_capacity(raw.len());
    for line in raw {
        let words: Vec<TextWord> = line
            .words
            .iter()
            .filter(|w| !w.text.trim().is_empty())
            .map(|w| TextWord {
                text: w.text.trim().to_owned(),
                rect: Rc::new(
                    w.rect.x / scale,
                    w.rect.y / scale,
                    w.rect.w / scale,
                    w.rect.h / scale,
                ),
            })
            .collect();
        let Some(first) = words.first() else {
            continue;
        };
        let (mut x0, mut y0) = (first.rect.x, first.rect.y);
        let (mut x1, mut y1) = (first.rect.x + first.rect.w, first.rect.y + first.rect.h);
        for w in &words[1..] {
            x0 = x0.min(w.rect.x);
            y0 = y0.min(w.rect.y);
            x1 = x1.max(w.rect.x + w.rect.w);
            y1 = y1.max(w.rect.y + w.rect.h);
        }
        let text = words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        out.push(TextLine {
            text,
            rect: Rc::new(x0, y0, x1 - x0, y1 - y0),
            confidence: 1.0,
            words,
        });
    }
    out
}

/// "en-US, vi" for diagnostics; `none` when the list is empty.
pub fn language_list(tags: &[String]) -> String {
    if tags.is_empty() {
        "none".to_owned()
    } else {
        tags.join(", ")
    }
}

/// The detail for diagnostics and settings: the language the engine reads for this user and the
/// OCR languages installed on the machine.
pub fn describe_languages(engine: Option<&str>, installed: &[String]) -> String {
    match engine {
        Some(tag) => format!(
            "reads {tag} (the first language of your Windows language list with OCR); OCR languages installed: {}",
            language_list(installed)
        ),
        None => format!(
            "no OCR language for your Windows language list; OCR languages installed: {}",
            language_list(installed)
        ),
    }
}

/// What to do when the engine cannot be created (no OCR language matches the user's list).
pub fn install_hint(installed: &[String]) -> String {
    let have = if installed.is_empty() {
        "none is installed".to_owned()
    } else {
        format!(
            "installed: {}, but none is in your language list",
            language_list(installed)
        )
    };
    format!(
        "text recognition needs a Windows OCR language ({have}): add the language in Settings > \
         Time & language > Language & region (it installs the text recognition part), or run \
         `Add-WindowsCapability -Online -Name Language.OCR~~~en-US~~~~0.0.1.0` in an Administrator \
         PowerShell (replace en-US with your language)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(t: &str, x: f64, y: f64, w: f64, h: f64) -> RawWord {
        RawWord {
            text: t.to_owned(),
            rect: Rc::new(x, y, w, h),
        }
    }

    #[test]
    fn small_pictures_double_and_big_ones_shrink() {
        assert_eq!(plan_scale(800, 600, 10_000), 2.0);
        // Over the longest side: scaled down to fit.
        assert!((plan_scale(20_000, 1000, 10_000) - 0.5).abs() < 1e-9);
        // Close to the limit: the enlargement is skipped, not a 1.1 resample.
        assert_eq!(plan_scale(9_000, 100, 10_000), 1.0);
        // 3840x2160 doubled is 33M pixels, inside the bound; 8K doubled is not.
        assert_eq!(plan_scale(3840, 2160, 10_000), 2.0);
        assert!(plan_scale(7680, 4320, 10_000) < 1.25);
    }

    #[test]
    fn prepare_swaps_to_bgra_and_reports_the_real_scale() {
        let img = RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 0]));
        let p = prepare(&img, 10_000);
        assert_eq!((p.width, p.height), (8, 6));
        assert_eq!(p.scale, 2.0);
        assert_eq!(p.bgra.len(), 8 * 6 * 4);
        // Alpha is forced opaque and the channels are swapped.
        assert_eq!(&p.bgra[..4], &[30, 20, 10, 255]);
        assert!(p.bgra.chunks(4).all(|c| c[3] == 255));
        // No scaling: same size, same swap.
        let big = RgbaImage::from_pixel(9_000, 1, image::Rgba([5, 6, 7, 255]));
        let p = prepare(&big, 10_000);
        assert_eq!((p.width, p.height, p.scale), (9_000, 1, 1.0));
        assert_eq!(&p.bgra[..4], &[7, 6, 5, 255]);
    }

    #[test]
    fn words_become_lines_in_original_pixels() {
        let raw = vec![
            RawLine {
                words: vec![
                    word("mail", 20.0, 40.0, 60.0, 20.0),
                    word("a@b.co", 100.0, 38.0, 80.0, 24.0),
                ],
            },
            RawLine { words: vec![] },
            RawLine {
                words: vec![word("  ", 0.0, 0.0, 1.0, 1.0)],
            },
        ];
        let lines = lines_from_raw(&raw, 2.0);
        assert_eq!(lines.len(), 1);
        let l = &lines[0];
        assert_eq!(l.text, "mail a@b.co");
        assert_eq!(l.confidence, 1.0);
        assert_eq!(l.words.len(), 2);
        assert_eq!(l.words[1].rect, Rc::new(50.0, 19.0, 40.0, 12.0));
        // Union of the word boxes, divided by the scale: x 10..90, y 19..31.
        assert_eq!(l.rect, Rc::new(10.0, 19.0, 80.0, 12.0));
    }

    #[test]
    fn a_zero_scale_is_treated_as_one() {
        let raw = vec![RawLine {
            words: vec![word("x", 5.0, 6.0, 7.0, 8.0)],
        }];
        assert_eq!(
            lines_from_raw(&raw, 0.0)[0].rect,
            Rc::new(5.0, 6.0, 7.0, 8.0)
        );
    }

    #[test]
    fn language_sentences_name_what_is_installed() {
        let tags = vec!["en-US".to_owned(), "vi".to_owned()];
        assert_eq!(language_list(&tags), "en-US, vi");
        assert_eq!(language_list(&[]), "none");
        let d = describe_languages(Some("vi"), &tags);
        assert!(d.contains("reads vi") && d.contains("en-US, vi"), "{d}");
        let d = describe_languages(None, &[]);
        assert!(d.contains("no OCR language") && d.contains("none"), "{d}");
        assert!(install_hint(&[]).contains("none is installed"));
        let h = install_hint(&tags);
        assert!(h.contains("none is in your language list") && h.contains("Language.OCR"));
    }
}
