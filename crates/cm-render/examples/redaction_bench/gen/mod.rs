//! The synthetic set generator (RED-1). Writes `manifest.json` and PNG files as `schema.rs`
//! describes. Deterministic for a given seed, headless, pure Rust; all data comes from the rule
//! based generators in `data.rs` (formats and checksums over random digits, reserved ranges), so
//! no value points at a real person.
//!
//! ```text
//! redaction_bench gen --out <dir> [--seed N] [--count N] [--debug-boxes]
//! ```
//!
//! `--debug-boxes` outlines every truth (red) and decoy (blue) in the PNG, to check the boxes by
//! eye; never use it for a real set.

mod data;
mod layouts;
mod rng;
mod scene;

use std::collections::BTreeMap;
use std::io::BufWriter;
use std::path::Path;

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, RgbaImage};

use crate::schema::{ImageEntry, Manifest};
use layouts::{Spec, LAYOUTS};
use rng::Rng;
use scene::{Ctx, Fonts};

pub const DEFAULT_SEED: u64 = 20_261_030;
pub const DEFAULT_COUNT: usize = 300;
const MANIFEST_VERSION: u32 = 1;

pub fn main(args: &[String]) -> i32 {
    let mut out: Option<String> = None;
    let mut seed = DEFAULT_SEED;
    let mut count = DEFAULT_COUNT;
    let mut debug = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |name: &str| -> Option<String> {
            let v = it.next().cloned();
            if v.is_none() {
                eprintln!("gen: {name} needs a value");
            }
            v
        };
        match a.as_str() {
            "--out" => out = val("--out"),
            "--seed" => match val("--seed").and_then(|v| v.parse().ok()) {
                Some(v) => seed = v,
                None => return 2,
            },
            "--count" => match val("--count").and_then(|v| v.parse().ok()) {
                Some(v) => count = v,
                None => return 2,
            },
            "--debug-boxes" => debug = true,
            other => {
                eprintln!("gen: unknown argument `{other}`");
                return 2;
            }
        }
    }
    let Some(out) = out else {
        eprintln!("usage: redaction_bench gen --out <dir> [--seed N] [--count N] [--debug-boxes]");
        return 2;
    };
    match generate(Path::new(&out), seed, count, debug) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("gen: {e}");
            1
        }
    }
}

fn generate(out: &Path, seed: u64, count: usize, debug: bool) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("create {}: {e}", out.display()))?;
    let mut fonts = Fonts::new();
    let mut images = Vec::with_capacity(count);
    let mut bytes = 0u64;
    for i in 0..count {
        let (entry, img) = build_one(&mut fonts, seed, i, count, debug);
        let path = out.join(&entry.file);
        write_png(&path, &img)?;
        bytes += std::fs::metadata(&path).map_or(0, |m| m.len());
        images.push(entry);
        if (i + 1) % 25 == 0 || i + 1 == count {
            eprintln!("gen: {}/{count} images", i + 1);
        }
    }
    let manifest = Manifest {
        version: MANIFEST_VERSION,
        seed,
        images,
    };
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(out.join("manifest.json"), json).map_err(|e| format!("manifest: {e}"))?;
    summarize(&manifest, bytes);
    Ok(())
}

fn summarize(m: &Manifest, bytes: u64) {
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    let mut decoys: BTreeMap<&str, usize> = BTreeMap::new();
    let mut layouts: BTreeMap<&str, (usize, &str)> = BTreeMap::new();
    let (mut pos, mut neg, mut deg, mut vi) = (0, 0, 0, 0);
    for e in &m.images {
        if e.truths.is_empty() {
            neg += 1;
        } else {
            pos += 1;
        }
        deg += usize::from(e.degraded);
        vi += usize::from(e.language == "vi");
        layouts
            .entry(e.layout.as_str())
            .or_insert((0, e.split.as_str()))
            .0 += 1;
        for t in &e.truths {
            *kinds.entry(t.kind.as_str()).or_default() += 1;
        }
        for d in &e.decoys {
            *decoys.entry(d.what.as_str()).or_default() += 1;
        }
    }
    println!(
        "images {} (positive {pos}, negative {neg}, degraded {deg}, vi {vi})",
        m.images.len()
    );
    println!("png bytes {} ({:.1} MB)", bytes, bytes as f64 / 1e6);
    println!("truths per kind: {kinds:?}");
    println!("decoys per kind: {decoys:?}");
    println!("layouts (count, split): {layouts:?}");
}

fn write_png(path: &Path, img: &RgbaImage) -> Result<(), String> {
    let rgb = DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let f = std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    PngEncoder::new_with_quality(
        BufWriter::new(f),
        CompressionType::Best,
        FilterType::Adaptive,
    )
    .write_image(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        ExtendedColorType::Rgb8,
    )
    .map_err(|e| format!("write {}: {e}", path.display()))
}

/// The layout for picture `i`: round robin over all layouts, except that the Vietnamese pictures
/// (every `count / n_vi`-th) cycle through the layouts that have Vietnamese labels.
fn plan(i: usize, count: usize) -> (&'static Spec, bool) {
    let n_vi = if count >= 40 {
        (count * 5 + 50) / 100
    } else {
        0
    };
    if let Some(q) = count.checked_div(n_vi) {
        let stride = q.max(1);
        if i % stride == 7 % stride {
            let vi_layouts: Vec<&Spec> = LAYOUTS.iter().filter(|l| l.vi).collect();
            return (vi_layouts[(i / stride) % vi_layouts.len()], true);
        }
    }
    (&LAYOUTS[i % LAYOUTS.len()], false)
}

/// Builds picture `i` (independent of every other picture and of `count`, apart from the plan).
pub fn build_one(
    fonts: &mut Fonts,
    seed: u64,
    i: usize,
    count: usize,
    debug: bool,
) -> (ImageEntry, RgbaImage) {
    let (spec, vi) = plan(i, count);
    let mut r = Rng::derive(seed, i as u64);
    let positive = (i * 7 + 3) % 10 < 7;
    let dark = r.chance(0.5);
    let scale = if r.chance(0.5) { 2 } else { 1 };
    let px = *r.pick(&[10.0f32, 10.0, 11.0, 11.0, 12.0, 12.0, 13.0, 14.0, 16.0]);
    let degraded = r.chance(0.25);
    let pat = if r.chance(0.28) {
        (r.range(1, 5) as u8, r.range(5, 13) as i32)
    } else {
        (0, 0)
    };
    let card_world = positive && r.chance(0.6);
    let mut attempt = 0u64;
    loop {
        attempt += 1;
        let rng = Rng::derive(seed, (i as u64 + 1) * 1000 + attempt);
        let mut ctx = Ctx::new(
            fonts,
            rng,
            dark,
            spec.size.0,
            spec.size.1,
            scale,
            px,
            vi,
            positive,
            card_world,
        );
        if pat.0 != 0 {
            ctx.pattern(pat.0, pat.1);
        }
        (spec.draw)(&mut ctx);
        let fin = ctx.finish(debug);
        let ok = if positive {
            !fin.truths.is_empty()
        } else {
            fin.truths.is_empty() && !fin.decoys.is_empty()
        };
        if !ok && attempt < 40 {
            continue;
        }
        assert!(
            ok,
            "layout {} produced no usable picture for index {i}",
            spec.name
        );
        let mut img = fin.img;
        if degraded {
            let mut dr = Rng::derive(seed ^ 0xDE57, i as u64);
            img = degrade(img, &mut dr);
        }
        let file = format!("{i:04}_{}{}.png", spec.name, if vi { "_vi" } else { "" });
        let entry = ImageEntry {
            file,
            layout: spec.name.to_string(),
            theme: if dark { "dark" } else { "light" }.to_string(),
            scale,
            font_px: fin.min_px.floor() as u32,
            language: if vi { "vi" } else { "en" }.to_string(),
            degraded,
            split: if spec.held_out { "held_out" } else { "dev" }.to_string(),
            lines: fin.lines,
            truths: fin.truths,
            decoys: fin.decoys,
        };
        return (entry, img);
    }
}

/// Rescale down and up, slight blur, JPEG re-encode (quality 60 to 85): at least one of them.
/// The picture keeps its size, so the boxes stay valid.
fn degrade(img: RgbaImage, r: &mut Rng) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut rgb = DynamicImage::ImageRgba8(img).to_rgb8();
    let mode = r.below(4);
    if mode == 0 || mode == 2 || mode == 3 {
        let f = 0.55 + r.range(0, 30) as f32 / 100.0;
        let (sw, sh) = (
            ((w as f32 * f) as u32).max(8),
            ((h as f32 * f) as u32).max(8),
        );
        let small = image::imageops::resize(&rgb, sw, sh, image::imageops::FilterType::Triangle);
        rgb = image::imageops::resize(&small, w, h, image::imageops::FilterType::CatmullRom);
    }
    if mode == 1 || mode == 3 {
        rgb = image::imageops::blur(&rgb, 0.5 + r.range(0, 50) as f32 / 100.0);
    }
    if mode == 1 || mode == 2 || mode == 3 {
        let q = r.range(60, 85) as u8;
        let mut buf = Vec::new();
        let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, q);
        if enc
            .write_image(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)
            .is_ok()
        {
            if let Ok(back) = image::load_from_memory_with_format(&buf, ImageFormat::Jpeg) {
                rgb = back.to_rgb8();
            }
        }
    }
    DynamicImage::ImageRgb8(rgb).to_rgba8()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_manifest() {
        let mut fonts = Fonts::new();
        let run = |fonts: &mut Fonts, seed| {
            let images: Vec<ImageEntry> = (0..5)
                .map(|i| build_one(fonts, seed, i, 300, false).0)
                .collect();
            serde_json::to_string(&Manifest {
                version: MANIFEST_VERSION,
                seed,
                images,
            })
            .unwrap()
        };
        let a = run(&mut fonts, 42);
        let b = run(&mut fonts, 42);
        assert_eq!(a, b);
        assert_ne!(a, run(&mut fonts, 43));
    }

    #[test]
    fn every_truth_and_decoy_lies_inside_its_image() {
        let mut fonts = Fonts::new();
        // 26 pictures cover every layout twice, several scales and the Vietnamese ones.
        for i in 0..26 {
            let (e, img) = build_one(&mut fonts, 9, i, 26, false);
            let (w, h) = (f64::from(img.width()), f64::from(img.height()));
            let inside = |r: &[f64; 4]| {
                r[0] >= 0.0
                    && r[1] >= 0.0
                    && r[2] > 0.0
                    && r[3] > 0.0
                    && r[0] + r[2] <= w
                    && r[1] + r[3] <= h
            };
            for t in &e.truths {
                assert!(inside(&t.rect), "{} truth {:?}", e.file, t.rect);
                assert!(!t.standards.is_empty() || t.kind == "none");
            }
            for d in &e.decoys {
                assert!(inside(&d.rect), "{} decoy {:?}", e.file, d.rect);
            }
            for l in &e.lines {
                assert!(inside(&l.rect), "{} line {:?}", e.file, l.rect);
                for wd in &l.words {
                    assert!(inside(&wd.rect), "{} word {:?}", e.file, wd.rect);
                }
            }
            if e.truths.is_empty() {
                assert!(!e.decoys.is_empty());
            }
        }
    }

    #[test]
    fn negatives_carry_no_truth_and_plans_cover_every_layout() {
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..300 {
            let (s, _) = plan(i, 300);
            seen.insert(s.name);
        }
        assert_eq!(seen.len(), LAYOUTS.len());
        let vi = (0..300).filter(|i| plan(*i, 300).1).count();
        assert_eq!(vi, 15);
        let held = LAYOUTS.iter().filter(|l| l.held_out).count();
        assert!((3..=5).contains(&held));
    }
}
