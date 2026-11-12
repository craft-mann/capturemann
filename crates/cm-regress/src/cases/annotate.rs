//! The 0.3.3 annotation pack through the CLI (callout, spotlight, stamps, step label/style/start) on
//! a seeded screenshot, and image import (IMP-6). No screen, camera or microphone is involved.
//!
//! Both cases need a 0.3.3 build. With an older package they end SKIPPED with a message that says
//! so, never FAIL, so the same suite can still be pointed at a 0.3.2 package.

use super::common::{
    check_png, import_file, library_entry, manifest, seed_screenshot, SEEDED_SHOT,
};
use crate::check;
use crate::ctx::Ctx;
use crate::fixtures;
use crate::model::{fail, skip, Case, Stop, Tier, ALL, R};
use serde_json::Value;
use std::path::Path;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "annotate.pack",
            title: "callout, spotlight, stamps and a step with style, start and text render with the right pixels; a second spotlight is invalid_input naming ops[1]",
            area: "annotate",
            specs: &["ANN-4", "ANN-6", "ANN-7", "AGT-3"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 180,
            run: pack,
        },
        Case {
            id: "import.images",
            title: "a PNG and a JPEG import as screenshots; annotate and export work on them; a GIF named .png is refused by its content",
            area: "import",
            specs: &["IMP-6", "ANN-1"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: import_images,
        },
    ]
}

/// A picture decoded to 8-bit RGBA.
pub struct Rgba {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Rgba {
    /// Decodes a PNG (any 8-bit or 16-bit colour type) with the `png` crate.
    pub fn decode(bytes: &[u8]) -> Result<Rgba, String> {
        let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = dec.read_info().map_err(|e| format!("PNG header: {e}"))?;
        let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| format!("PNG data: {e}"))?;
        buf.truncate(info.buffer_size());
        let px = match info.color_type {
            png::ColorType::Rgba => buf,
            png::ColorType::Rgb => buf
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[1], c[2], 255])
                .collect(),
            png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::GrayscaleAlpha => buf
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|c| [c[0], c[0], c[0], c[1]])
                .collect(),
            other => return Err(format!("unexpected PNG colour type {other:?}")),
        };
        Ok(Rgba {
            w: info.width,
            h: info.height,
            px,
        })
    }

    pub fn at(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * self.w + x) * 4) as usize;
        [self.px[i], self.px[i + 1], self.px[i + 2]]
    }

    /// How many pixels inside `rect` (x, y, w, h) are within `tol` of `colour` on every channel.
    pub fn count_near(&self, rect: [u32; 4], colour: [u8; 3], tol: u8) -> usize {
        let mut n = 0;
        for y in rect[1]..(rect[1] + rect[3]).min(self.h) {
            for x in rect[0]..(rect[0] + rect[2]).min(self.w) {
                let p = self.at(x, y);
                if (0..3).all(|c| p[c].abs_diff(colour[c]) <= tol) {
                    n += 1;
                }
            }
        }
        n
    }
}

/// Over the given sample points: (mean of rendered/original over the points where the original is
/// white, the number of those points). The "outside the spotlight keeps 40 %" check.
pub fn white_ratio(original: &Rgba, rendered: &Rgba, points: &[(u32, u32)]) -> (f64, usize) {
    let mut sum = 0.0;
    let mut n = 0;
    for &(x, y) in points {
        let o = original.at(x, y);
        if o.iter().all(|&c| c >= 250) {
            let r = rendered.at(x, y);
            sum += f64::from(r[0]) / f64::from(o[0]);
            n += 1;
        }
    }
    (if n == 0 { 0.0 } else { sum / n as f64 }, n)
}

/// Sample points on a grid.
pub fn grid(xs: &[u32], ys: &[u32]) -> Vec<(u32, u32)> {
    xs.iter()
        .flat_map(|&x| ys.iter().map(move |&y| (x, y)))
        .collect()
}

/// The largest per-channel difference between two pictures over the points.
pub fn max_diff(a: &Rgba, b: &Rgba, points: &[(u32, u32)]) -> u8 {
    points
        .iter()
        .flat_map(|&(x, y)| {
            let (p, q) = (a.at(x, y), b.at(x, y));
            (0..3).map(move |c| p[c].abs_diff(q[c]))
        })
        .max()
        .unwrap_or(0)
}

/// A refusal of the 0.3.3 ops by an older build: the first call that names an op or field this
/// build does not know.
fn old_build_words(text: &str) -> bool {
    let t = text.to_lowercase();
    ["callout", "spotlight", "stamp", "unknown op", "unsupported"]
        .iter()
        .any(|w| t.contains(w))
}

fn error_text(r: &crate::ctx::CliOut) -> String {
    format!(
        "{} {}",
        r.error_message(),
        r.error.as_ref().map(Value::to_string).unwrap_or_default()
    )
}

const PACK_OPS: &str = r##"[
  {"op":"spotlight","shapes":[{"kind":"ellipse","rect":[100,20,300,120]}]},
  {"op":"callout","rect":[600,20,250,60],"tail":[560,130],"text":"Click here","bg":"#1E90FF","style":"bubble"},
  {"op":"callout","rect":[600,100,200,40],"tail":[880,185],"text":"Box","bg":"#1E90FF","style":"box"},
  {"op":"stamp","at":[650,225],"kind":"check","size":34,"color":"#2EB872"},
  {"op":"stamp","at":[700,225],"kind":"cross","size":34,"color":"#2EB872"},
  {"op":"stamp","at":[750,225],"kind":"exclaim","size":34,"color":"#2EB872"},
  {"op":"stamp","at":[800,225],"kind":"question","size":34,"color":"#2EB872"},
  {"op":"stamp","at":[900,225],"kind":"label","size":34,"color":"#8E44AD","text":"Before"},
  {"op":"step","at":[300,225],"n":1,"size":28,"style":"upper","start":3,"text":"Open Settings"}
]"##;

fn pack(ctx: &mut Ctx) -> R {
    let bundle = seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let original = Rgba::decode(fixtures::REDACT_PNG)?;
    let ops = ctx.write_file("pack-ops.json", PACK_OPS.as_bytes())?;
    let ops = ops.to_string_lossy().into_owned();
    let out = ctx.root.join("pack.png").to_string_lossy().into_owned();

    let r = ctx.cli(&["annotate", SEEDED_SHOT, "--ops", &ops, "--out", &out])?;
    if !r.ok() {
        let text = error_text(&r);
        if r.error_code().as_deref() == Some("invalid_input") && old_build_words(&text) {
            return skip(format!(
                "this package does not know the 0.3.3 ops (needs 0.3.3 or later): {}",
                crate::util::cut(&text, 160)
            ));
        }
        return fail(format!(
            "annotate with the 0.3.3 ops failed ({}): {}",
            crate::proc::exit_text(r.code()),
            crate::util::cut(&text, 300)
        ));
    }
    let (w, h) = check_png(Path::new(&out))?;
    check!(
        (w, h) == (1000, 260),
        "the render is {w}x{h}, expected 1000x260"
    );
    let m = manifest(&bundle)?;
    let stored = m["ops"].as_array().cloned().unwrap_or_default();
    check!(
        stored.len() == 9 && m["revisions"].as_array().map_or(0, Vec::len) == 1,
        "expected 9 ops in 1 revision, got {} and {}",
        stored.len(),
        m["revisions"]
    );
    let step = stored.iter().find(|o| o["op"] == "step");
    check!(
        step.is_some_and(|s| s["style"] == "upper"
            && s["start"] == 3
            && s["text"] == "Open Settings"),
        "the step lost its style, start or text: {:?}",
        step
    );
    ctx.note("9 ops stored in one revision; step keeps style, start and text");

    let rendered = Rgba::decode(&std::fs::read(&out)?)?;

    // Spotlight: dim 0.6 by default, ellipse hole [100,20,300,120] (centre 250,80).
    let outside = grid(&[430, 460, 490, 520, 540], &[30, 60, 100, 130, 170, 200]);
    let (ratio, n) = white_ratio(&original, &rendered, &outside);
    ctx.note(format!(
        "outside the hole: white pixels keep {:.0} % of their brightness ({n} points)",
        ratio * 100.0
    ));
    check!(n >= 8, "only {n} white sample points outside the hole");
    check!(
        (0.33..=0.47).contains(&ratio),
        "outside the spotlight the brightness ratio is {ratio:.2}, expected about 0.40"
    );
    let inside = grid(&[180, 220, 260, 300, 320], &[50, 70, 90, 110]);
    let diff = max_diff(&original, &rendered, &inside);
    check!(
        diff <= 3,
        "inside the hole the picture changed by up to {diff}"
    );

    // Colours that only the new ops draw. They sit above the spotlight, so they stay bright.
    let blue = rendered.count_near([600, 20, 250, 120], [0x1E, 0x90, 0xFF], 12);
    let green = rendered.count_near([630, 205, 190, 40], [0x2E, 0xB8, 0x72], 12);
    let purple = rendered.count_near([840, 200, 120, 50], [0x8E, 0x44, 0xAD], 12);
    let red = rendered.count_near([270, 195, 60, 60], [0xFF, 0x3B, 0x3B], 12);
    ctx.note(format!(
        "colour pixels: callout {blue}, stamps {green}, label stamp {purple}, step badge {red}"
    ));
    check!(blue >= 3000, "callout colour found in only {blue} pixels");
    check!(green >= 100, "stamp colour found in only {green} pixels");
    check!(
        purple >= 50,
        "label stamp colour found in only {purple} pixels"
    );
    check!(red >= 150, "step badge colour found in only {red} pixels");

    // One undo step takes the whole call back.
    ctx.cli_ok(&["undo", SEEDED_SHOT])?;
    check!(
        manifest(&bundle)?["ops"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "undo left ops behind"
    );

    // One spotlight per image: a second is invalid_input naming ops[1], nothing stored.
    let two = ctx.write_file(
        "two-spots.json",
        br#"[{"op":"spotlight","shapes":[{"kind":"rect","rect":[10,10,100,50]}]},{"op":"spotlight","shapes":[{"kind":"rect","rect":[200,10,100,50]}]}]"#,
    )?;
    let two = two.to_string_lossy().into_owned();
    let before = manifest(&bundle)?;
    let r = ctx.cli(&["annotate", SEEDED_SHOT, "--ops", &two, "--replace"])?;
    let text = error_text(&r);
    ctx.note(format!(
        "two spotlights: {} {:?} {}",
        crate::proc::exit_text(r.code()),
        r.error_code(),
        crate::util::cut(&text, 140)
    ));
    check!(
        !r.ok() && r.error_code().as_deref() == Some("invalid_input"),
        "two spotlight ops were not answered with invalid_input"
    );
    check!(
        text.contains("ops[1]"),
        "the error does not name ops[1]: {text}"
    );
    let after = manifest(&bundle)?;
    check!(
        after["ops"] == before["ops"] && after["revisions"] == before["revisions"],
        "a refused annotate changed the manifest"
    );

    // Other invalid input names its op too.
    let bad = ctx.write_file(
        "bad-ops.json",
        br#"[{"op":"stamp","at":[50,50],"kind":"label"}]"#,
    )?;
    let bad = bad.to_string_lossy().into_owned();
    let r = ctx.cli(&["annotate", SEEDED_SHOT, "--ops", &bad])?;
    check!(
        !r.ok()
            && r.error_code().as_deref() == Some("invalid_input")
            && error_text(&r).contains("ops[0]"),
        "a label stamp without text: {} {:?} {}",
        crate::proc::exit_text(r.code()),
        r.error_code(),
        crate::util::cut(&error_text(&r), 160)
    );
    let nine = format!(
        r#"[{{"op":"spotlight","shapes":[{}]}}]"#,
        [r#"{"kind":"rect","rect":[10,10,20,20]}"#; 9].join(",")
    );
    let r = ctx.cli(&["annotate", SEEDED_SHOT, "--ops", &nine])?;
    check!(
        !r.ok()
            && r.error_code().as_deref() == Some("invalid_input")
            && error_text(&r).contains("ops[0]"),
        "a spotlight with 9 holes: {} {:?}",
        crate::proc::exit_text(r.code()),
        r.error_code()
    );
    Ok(())
}

/// A small RGB picture with a gradient and a red square in the corner.
fn generated_png() -> Vec<u8> {
    fixtures::make_png(120, 80, |x, y| {
        if x < 20 && y < 20 {
            [255, 0, 0]
        } else {
            [(x * 2) as u8, (y * 3) as u8, 128]
        }
    })
}

/// Finds the `.cmshot` bundle of a capture id.
fn shot_bundle(ctx: &Ctx, id: &str) -> R<std::path::PathBuf> {
    for b in ctx.bundles("cmshot") {
        if manifest(&b)?["id"] == id {
            return Ok(b);
        }
    }
    fail(format!(
        "no screenshot bundle of {id} in the library folder"
    ))
}

fn import_images(ctx: &mut Ctx) -> R {
    let dir = ctx.dir("images");
    std::fs::write(dir.join("generated.png"), generated_png())?;
    std::fs::write(dir.join("shot.jpg"), fixtures::SHOT_JPG)?;
    // A GIF header under a .png name: the content decides.
    std::fs::write(
        dir.join("not-a-png.png"),
        b"GIF89a\x01\x00\x01\x00\x00\x00\x00;",
    )?;
    ctx.start_daemon()?;

    for (name, size) in [
        ("generated.png", (120u32, 80u32)),
        ("shot.jpg", (1000, 260)),
    ] {
        let (id, _) = match import_file(ctx, &dir.join(name)) {
            Ok(v) => v,
            Err(Stop::Fail(msg)) if msg.contains("unsupported_format") => {
                return skip(format!(
                    "this package does not import images (needs 0.3.3 or later): {}",
                    crate::util::cut(&msg, 200)
                ))
            }
            Err(e) => return Err(e),
        };
        let e = library_entry(ctx, &id)?;
        check!(
            e.get("kind").and_then(Value::as_str) == Some("screenshot"),
            "{name}: kind {:?}, expected screenshot",
            e.get("kind")
        );
        let bundle = shot_bundle(ctx, &id)?;
        let original = bundle.join("original.png");
        let bytes = std::fs::read(&original)
            .map_err(|e| Stop::Fail(format!("{name}: no original.png: {e}")))?;
        let dims = fixtures::png_info(&bytes).map_err(|e| Stop::Fail(format!("{name}: {e}")))?;
        let m = manifest(&bundle)?;
        ctx.note(format!(
            "{name}: {id}, original.png {}x{}, source {}",
            dims.0, dims.1, m["source"]
        ));
        check!(
            dims == size,
            "{name}: original.png is {dims:?}, expected {size:?}"
        );
        check!(
            m["ops"].as_array().is_some_and(Vec::is_empty),
            "{name}: the new bundle already has ops"
        );
        check!(
            m.pointer("/source/original_name").and_then(Value::as_str) == Some(name),
            "{name}: source.original_name is {:?}",
            m.pointer("/source/original_name")
        );
        // No ancillary metadata chunk (EXIF, text, ICC) is carried over from the source.
        for tag in [&b"eXIf"[..], b"tEXt", b"iTXt", b"zTXt", b"iCCP"] {
            check!(
                !bytes.windows(4).any(|w| w == tag),
                "{name}: original.png has a {} chunk",
                String::from_utf8_lossy(tag)
            );
        }

        // Annotate and export like a screenshot taken here.
        let ops = ctx.write_file(
            "image-ops.json",
            br##"[{"op":"arrow","from":[5,5],"to":[100,60],"color":"#FF3B3B","width":4},{"op":"text","at":[10,70],"text":"imported","size":16,"color":"#FFFFFF","outline":null}]"##,
        )?;
        let ops = ops.to_string_lossy().into_owned();
        let out = ctx
            .root
            .join(format!("annotated-{name}.png"))
            .to_string_lossy()
            .into_owned();
        ctx.cli_ok(&["annotate", &id, "--ops", &ops, "--out", &out])?;
        let (w, h) = check_png(Path::new(&out))?;
        check!((w, h) == size, "{name}: annotated render is {w}x{h}");
        let got = ctx.root.join(format!("exported-{name}.png"));
        let got_s = got.to_string_lossy().into_owned();
        ctx.cli_ok(&["capture", "get", &id, "--out", &got_s])?;
        let (w, h) = check_png(&got)?;
        check!((w, h) == size, "{name}: exported image is {w}x{h}");
        check!(
            std::fs::read(&got)? != bytes,
            "{name}: the export equals the unannotated original"
        );
    }

    // Refused by its content, nothing created.
    let before = ctx.bundles("cmshot").len();
    let p = dir.join("not-a-png.png").to_string_lossy().into_owned();
    let r = ctx.cli_long(&["import", &p, "--wait", "--timeout", "60"], 90)?;
    let text = r.out.text().to_lowercase();
    ctx.note(format!(
        "GIF named .png: {} {}",
        crate::proc::exit_text(r.code()),
        crate::util::cut(&crate::util::one_line(&text), 160)
    ));
    check!(
        text.contains("unsupported") || text.contains("unreadable") || !r.ok(),
        "a GIF named .png was not refused"
    );
    check!(
        ctx.bundles("cmshot").len() == before,
        "a refused image left a bundle behind"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(f: impl Fn(u32, u32) -> [u8; 3]) -> Rgba {
        Rgba::decode(&fixtures::make_png(40, 20, f)).expect("decodes")
    }

    #[test]
    fn decode_reads_a_generated_png() {
        let p = picture(|x, _| [x as u8, 7, 9]);
        assert_eq!((p.w, p.h), (40, 20));
        assert_eq!(p.at(5, 3), [5, 7, 9]);
    }

    #[test]
    fn dim_ratio_is_measured_on_white_points_only() {
        let o = picture(|x, _| if x < 10 { [0, 0, 0] } else { [255, 255, 255] });
        let r = picture(|x, _| if x < 10 { [0, 0, 0] } else { [102, 102, 102] });
        let pts = grid(&[5, 20, 30], &[2, 10]);
        let (ratio, n) = white_ratio(&o, &r, &pts);
        assert_eq!(n, 4);
        assert!((ratio - 0.4).abs() < 0.01);
    }

    #[test]
    fn colour_count_and_diff() {
        let a = picture(|x, _| if x < 4 { [30, 144, 255] } else { [1, 2, 3] });
        assert_eq!(a.count_near([0, 0, 10, 20], [30, 144, 255], 5), 80);
        let b = picture(|_, _| [1, 2, 3]);
        assert_eq!(max_diff(&a, &b, &grid(&[30], &[5])), 0);
        assert!(max_diff(&a, &b, &grid(&[1], &[5])) > 200);
    }

    #[test]
    fn old_builds_are_recognised_by_their_words() {
        assert!(old_build_words("invalid op `callout` at ops[0]"));
        assert!(!old_build_words("rect is outside the image"));
    }

    #[test]
    fn the_ops_file_is_valid_json_with_nine_ops() {
        let v: Value = serde_json::from_str(PACK_OPS).expect("json");
        assert_eq!(v.as_array().map(Vec::len), Some(9));
    }

    #[test]
    fn the_jpeg_fixture_is_a_jpeg() {
        assert_eq!(&fixtures::SHOT_JPG[..3], &[0xFF, 0xD8, 0xFF]);
        assert!(fixtures::SHOT_JPG.len() < 200_000);
    }
}
