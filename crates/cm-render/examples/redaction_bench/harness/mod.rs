//! The measuring harness: reads a set (see `schema.rs`), runs the detector behind any
//! [`TextRecognizer`], and writes a Markdown report (see `report.rs` for the definitions).
//!
//! ```text
//! run --set <dir> [--report <file.md>] [--split dev|held_out|all] [--detector-only]
//!     [--recognizer system|truth] [--kinds a,b,c]
//! ```
//!
//! `--detector-only` is `--recognizer truth` without decoding any picture: the detector gets the
//! manifest's drawn lines (perfect text), so rule errors show apart from OCR errors.

mod geom;
mod report;
mod score;

use crate::schema::{DrawnLine, ImageEntry, Manifest};
use cm_core::{CmError, Rc};
use cm_render::ocr::{
    suggest_redactions, system_recognizer, RedactionKind, SuggestOptions, TextLine, TextRecognizer,
    TextWord,
};
use cm_render::{AnnotationOp, OpKind, Renderer};
use report::{Header, Timing};
use score::{ImageResult, Meta, Proposal};
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Dev,
    HeldOut,
    All,
}

impl Split {
    fn name(self) -> &'static str {
        match self {
            Split::Dev => "dev",
            Split::HeldOut => "held_out",
            Split::All => "all",
        }
    }

    fn takes(self, split: &str) -> bool {
        match self {
            Split::All => true,
            Split::Dev => split == "dev",
            Split::HeldOut => split == "held_out",
        }
    }
}

/// Where the text of a picture comes from.
pub enum Source<'a> {
    /// The manifest's drawn lines; pictures are not decoded.
    Truth,
    /// A recognizer reading the decoded PNG.
    Recognizer(&'a dyn TextRecognizer),
}

pub struct Options {
    pub split: Split,
    pub kinds: Vec<RedactionKind>,
    /// Run the blur-and-read-again pass (needs a real recognizer).
    pub leak: bool,
}

pub struct Evaluation {
    pub header: Header,
    pub aggregate: report::Aggregate,
    pub timing: Timing,
}

fn to_text_lines(lines: &[DrawnLine]) -> Vec<TextLine> {
    lines
        .iter()
        .map(|l| TextLine {
            text: l.text.clone(),
            rect: rc(l.rect),
            confidence: 1.0,
            words: l
                .words
                .iter()
                .map(|w| TextWord {
                    text: w.text.clone(),
                    rect: rc(w.rect),
                })
                .collect(),
        })
        .collect()
}

fn rc(r: [f64; 4]) -> Rc {
    Rc::new(r[0], r[1], r[2], r[3])
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn propose(lines: &[TextLine], size: (u32, u32), kinds: &[RedactionKind]) -> Vec<Proposal> {
    let opts = SuggestOptions {
        kinds: kinds.to_vec(),
    };
    suggest_redactions(lines, size, &opts)
        .into_iter()
        .map(|s| Proposal {
            rect: [s.rect.x, s.rect.y, s.rect.w, s.rect.h],
            kind: s.kind.as_str().to_owned(),
        })
        .collect()
}

/// Runs the detector (and, with `opts.leak`, the end-to-end pass) over the chosen images.
pub fn evaluate(
    set: &Path,
    m: &Manifest,
    opts: &Options,
    source: &Source,
    recognizer_name: &str,
    progress: bool,
) -> Result<Evaluation, String> {
    if opts.leak && matches!(source, Source::Truth) {
        return Err("the end-to-end pass needs a real recognizer, not the manifest's lines".into());
    }
    let selected: Vec<&ImageEntry> = m
        .images
        .iter()
        .filter(|e| opts.split.takes(&e.split))
        .collect();
    if selected.is_empty() {
        return Err(format!("no images in split `{}`", opts.split.name()));
    }
    let kind_names: Vec<&str> = opts.kinds.iter().map(|k| k.as_str()).collect();
    let mut renderer = Renderer::new();
    let mut timing = Timing::default();
    let mut results = Vec::new();
    for (i, e) in selected.iter().enumerate() {
        if progress && i % 50 == 0 {
            eprintln!("redaction_bench: image {} of {}", i + 1, selected.len());
        }
        let path = set.join(&e.file);
        let (lines, size, image) = match source {
            Source::Truth => {
                let size = image::image_dimensions(&path)
                    .map_err(|err| format!("{}: {err}", path.display()))?;
                (to_text_lines(&e.lines), size, None)
            }
            Source::Recognizer(r) => {
                let img = image::open(&path)
                    .map_err(|err| format!("{}: {err}", path.display()))?
                    .to_rgba8();
                let t = Instant::now();
                let lines = r
                    .recognize(&img)
                    .map_err(|err: CmError| format!("{}: {err}", e.file))?;
                timing.recognize_ms.push(ms(t));
                let size = img.dimensions();
                (lines, size, Some(img))
            }
        };
        let t = Instant::now();
        let props = propose(&lines, size, &opts.kinds);
        timing.detect_ms.push(ms(t));

        let truths: Vec<_> = e
            .truths
            .iter()
            .filter(|t| kind_names.contains(&t.kind.as_str()))
            .cloned()
            .collect();
        let (mut tr, pr) = score::score_image(&truths, &e.decoys, &props);

        if let (true, Source::Recognizer(r), Some(img)) = (opts.leak, source, &image) {
            // No proposal: the output is the input, so the first reading stands.
            let after = if props.is_empty() {
                lines.clone()
            } else {
                let ops: Vec<AnnotationOp> = props
                    .iter()
                    .enumerate()
                    .map(|(n, p)| {
                        AnnotationOp::new(
                            format!("op_{n}"),
                            OpKind::Blur {
                                rect: rc(p.rect),
                                strength: score::LEAK_BLUR_STRENGTH,
                            },
                        )
                    })
                    .collect();
                let blurred = renderer
                    .render(img, &ops)
                    .map_err(|err| format!("{}: blur failed: {err}", e.file))?;
                r.recognize(&blurred)
                    .map_err(|err| format!("{}: {err}", e.file))?
            };
            score::apply_leak(&truths, &mut tr, &lines, &after);
        }
        results.push(ImageResult {
            meta: Meta {
                split: e.split.clone(),
                layout: e.layout.clone(),
                theme: e.theme.clone(),
                scale: e.scale,
                font_px: e.font_px,
                degraded: e.degraded,
                language: e.language.clone(),
                negative: e.truths.is_empty(),
            },
            truths: tr,
            props: pr,
            decoys: e.decoys.iter().map(|d| d.what.clone()).collect(),
        });
    }
    let header = Header {
        date: String::new(),
        commit: String::new(),
        seed: m.seed,
        manifest_version: m.version,
        images_in_manifest: m.images.len(),
        images_run: selected.len(),
        negatives_run: selected.iter().filter(|e| e.truths.is_empty()).count(),
        truths_run: results.iter().map(|r| r.truths.len()).sum(),
        decoys_run: selected.iter().map(|e| e.decoys.len()).sum(),
        recognizer: recognizer_name.to_owned(),
        kinds: kind_names.iter().map(|s| (*s).to_owned()).collect(),
        split: opts.split.name().to_owned(),
        detector_only: matches!(source, Source::Truth),
        leak: opts.leak,
    };
    Ok(Evaluation {
        aggregate: report::aggregate(&results, &opts.kinds),
        header,
        timing,
    })
}

fn usage() -> i32 {
    eprintln!(
        "usage: redaction_bench run --set <dir> [--report <file.md>] [--split dev|held_out|all] \
         [--detector-only] [--recognizer system|truth] [--kinds a,b,c]"
    );
    2
}

fn commit_hash() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(
            || "unknown".to_owned(),
            |o| String::from_utf8_lossy(&o.stdout).trim().to_owned(),
        )
}

pub fn main(args: &[String]) -> i32 {
    let mut set = None;
    let mut report_path = None;
    let mut split = Split::Dev;
    let mut detector_only = false;
    let mut recognizer = "system".to_owned();
    let mut kinds: Vec<RedactionKind> = RedactionKind::DEFAULT.to_vec();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        let r: Result<(), String> = match a.as_str() {
            "--set" => value("--set").map(|v| set = Some(v)),
            "--report" => value("--report").map(|v| report_path = Some(v)),
            "--split" => value("--split").and_then(|v| {
                split = match v.as_str() {
                    "dev" => Split::Dev,
                    "held_out" => Split::HeldOut,
                    "all" => Split::All,
                    _ => return Err(format!("unknown split `{v}` (dev, held_out, all)")),
                };
                Ok(())
            }),
            "--detector-only" => {
                detector_only = true;
                Ok(())
            }
            "--recognizer" => value("--recognizer").and_then(|v| match v.as_str() {
                "system" | "truth" => {
                    recognizer = v;
                    Ok(())
                }
                _ => Err(format!("unknown recognizer `{v}` (system, truth)")),
            }),
            "--kinds" => value("--kinds").and_then(|v| {
                kinds = v
                    .split(',')
                    .map(|n| {
                        RedactionKind::parse(n.trim()).ok_or_else(|| {
                            format!(
                                "unknown kind `{n}`; known: {}",
                                RedactionKind::names().join(", ")
                            )
                        })
                    })
                    .collect::<Result<_, _>>()?;
                kinds.sort();
                kinds.dedup();
                Ok(())
            }),
            _ => Err(format!("unknown argument `{a}`")),
        };
        if let Err(e) = r {
            eprintln!("redaction_bench run: {e}");
            return usage();
        }
    }
    let Some(set) = set else {
        return usage();
    };
    let set = Path::new(&set);
    if split != Split::Dev {
        eprintln!(
            "WARNING: reading the `{}` split. Thresholds are tuned on dev only; held_out is read once per release.",
            split.name()
        );
    }
    let manifest: Manifest = match std::fs::read_to_string(set.join("manifest.json"))
        .map_err(|e| e.to_string())
        .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "redaction_bench run: cannot read manifest.json in {}: {e}",
                set.display()
            );
            return 1;
        }
    };
    let use_truth = detector_only || recognizer == "truth";
    let sys = system_recognizer();
    if !use_truth && !sys.is_available() {
        eprintln!(
            "redaction_bench run: the system text recognizer is not available on this machine \
             (Apple Vision on macOS; elsewhere the `tesseract` program, see CM_TESSERACT, or \
             `sudo apt install tesseract-ocr`). Use --detector-only to test the rules with the \
             manifest's text, or plug in a recognizer."
        );
        return 1;
    }
    let (source, name) = if use_truth {
        (Source::Truth, "truth")
    } else {
        (Source::Recognizer(&*sys), sys.name())
    };
    let opts = Options {
        split,
        kinds,
        leak: !use_truth,
    };
    let mut ev = match evaluate(set, &manifest, &opts, &source, name, true) {
        Ok(ev) => ev,
        Err(e) => {
            eprintln!("redaction_bench run: {e}");
            return 1;
        }
    };
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    ev.header.date = report::date_of(secs);
    ev.header.commit = commit_hash();
    let text = report::render(&ev.header, &ev.aggregate, &ev.timing);
    println!("{text}");
    if let Some(p) = report_path {
        if let Err(e) = std::fs::write(&p, &text) {
            eprintln!("redaction_bench run: cannot write {p}: {e}");
            return 1;
        }
        eprintln!("report written to {p}");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Decoy, DrawnWord, Truth};
    use cm_render::ocr::ScriptedRecognizer;
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::Mutex;

    const EMAIL: &str = "jane.doe@example.com";
    const OTHER: &str = "support.desk@example.org";

    fn tmp(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("redaction_bench_test_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn drawn(text: &str, x: f64, y: f64) -> DrawnLine {
        let l = TextLine::laid_out(text, x, y, 8.0, 14.0);
        DrawnLine {
            text: l.text.clone(),
            rect: [l.rect.x, l.rect.y, l.rect.w, l.rect.h],
            words: l
                .words
                .iter()
                .map(|w| DrawnWord {
                    text: w.text.clone(),
                    rect: [w.rect.x, w.rect.y, w.rect.w, w.rect.h],
                })
                .collect(),
        }
    }

    fn truth_of(text: &str, x: f64, y: f64, kind: &str, std: &str) -> Truth {
        let l = drawn(text, x, y);
        Truth {
            kind: kind.into(),
            text: text.into(),
            rect: l.rect,
            standards: vec![std.into()],
        }
    }

    fn entry(
        file: &str,
        split: &str,
        lines: Vec<DrawnLine>,
        truths: Vec<Truth>,
        decoys: Vec<Decoy>,
    ) -> ImageEntry {
        ImageEntry {
            file: file.into(),
            layout: "mail".into(),
            theme: "light".into(),
            scale: 1,
            font_px: 12,
            language: "en".into(),
            degraded: false,
            split: split.into(),
            lines,
            truths,
            decoys,
        }
    }

    /// Writes the PNGs and returns the set folder.
    fn make_set(tag: &str, images: Vec<ImageEntry>) -> (PathBuf, Manifest) {
        let dir = tmp(tag);
        for e in &images {
            image::RgbaImage::from_pixel(260, 80, image::Rgba([255, 255, 255, 255]))
                .save(dir.join(&e.file))
                .unwrap();
        }
        let m = Manifest {
            version: 1,
            seed: 7,
            images,
        };
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&m).unwrap(),
        )
        .unwrap();
        (dir, m)
    }

    fn opts(split: Split, leak: bool) -> Options {
        Options {
            split,
            kinds: vec![RedactionKind::Email],
            leak,
        }
    }

    fn lines_of(text: &str) -> Vec<TextLine> {
        vec![TextLine::laid_out(text, 10.0, 10.0, 8.0, 14.0)]
    }

    /// Answers the queued line sets in order, one per `recognize` call.
    struct Seq(Mutex<VecDeque<Vec<TextLine>>>);

    impl TextRecognizer for Seq {
        fn name(&self) -> &'static str {
            "seq"
        }
        fn is_available(&self) -> bool {
            true
        }
        fn recognize(&self, _: &image::RgbaImage) -> Result<Vec<TextLine>, CmError> {
            Ok(self.0.lock().unwrap().pop_front().unwrap_or_default())
        }
    }

    fn email_image(file: &str, split: &str) -> ImageEntry {
        entry(
            file,
            split,
            vec![drawn(EMAIL, 10.0, 10.0)],
            vec![truth_of(EMAIL, 10.0, 10.0, "email", "HIPAA 6")],
            vec![],
        )
    }

    #[test]
    fn perfect_recall_and_precision() {
        let (dir, m) = make_set("perfect", vec![email_image("a.png", "dev")]);
        let rec = ScriptedRecognizer::new(lines_of(EMAIL));
        let ev = evaluate(
            &dir,
            &m,
            &opts(Split::Dev, false),
            &Source::Recognizer(&rec),
            "scripted",
            false,
        )
        .unwrap();
        let t = &ev.aggregate.overall;
        assert_eq!((t.truths, t.found, t.found_kind), (1, 1, 1));
        assert_eq!((t.props, t.tp, t.fa()), (1, 1, 0));
        assert_eq!(rec.calls(), 1);
        assert_eq!(ev.aggregate.by_standard["HIPAA 6"].found, 1);
    }

    #[test]
    fn missed_truth() {
        let (dir, m) = make_set("missed", vec![email_image("a.png", "dev")]);
        let rec = ScriptedRecognizer::new(vec![]);
        let ev = evaluate(
            &dir,
            &m,
            &opts(Split::Dev, false),
            &Source::Recognizer(&rec),
            "scripted",
            false,
        )
        .unwrap();
        let t = &ev.aggregate.overall;
        assert_eq!((t.truths, t.found, t.props), (1, 0, 0));
    }

    #[test]
    fn false_alarm_on_decoy_in_negative_image() {
        let decoy = truth_of(OTHER, 10.0, 10.0, "email", "x");
        let e = entry(
            "n.png",
            "dev",
            vec![drawn(OTHER, 10.0, 10.0)],
            vec![],
            vec![Decoy {
                what: "role_mailbox".into(),
                text: OTHER.into(),
                rect: decoy.rect,
            }],
        );
        let (dir, m) = make_set("decoy", vec![e]);
        let ev = evaluate(
            &dir,
            &m,
            &opts(Split::Dev, false),
            &Source::Truth,
            "truth",
            false,
        )
        .unwrap();
        let t = &ev.aggregate.overall;
        assert_eq!((t.neg_images, t.fa_decoy, t.fa_on_neg), (1, 1, 1));
        assert_eq!(t.fa_what["role_mailbox"], 1);
        let text = report::render(&ev.header, &ev.aggregate, &ev.timing);
        assert!(text.contains("| role_mailbox | 1 | 1 | 1.000 |"), "{text}");
    }

    #[test]
    fn leak_follows_the_second_reading() {
        let (dir, m) = make_set("leak", vec![email_image("a.png", "dev")]);
        // Read again after blurring: the text is still there -> leaked.
        let seq = Seq(Mutex::new(VecDeque::from([
            lines_of(EMAIL),
            lines_of(EMAIL),
        ])));
        let ev = evaluate(
            &dir,
            &m,
            &opts(Split::Dev, true),
            &Source::Recognizer(&seq),
            "seq",
            false,
        )
        .unwrap();
        let t = &ev.aggregate.overall;
        assert_eq!(
            (t.leak_total, t.leak_leaked, t.readable_before_leaked),
            (1, 1, 1)
        );
        // Nothing readable after blurring -> no leak.
        let seq = Seq(Mutex::new(VecDeque::from([lines_of(EMAIL), vec![]])));
        let ev = evaluate(
            &dir,
            &m,
            &opts(Split::Dev, true),
            &Source::Recognizer(&seq),
            "seq",
            false,
        )
        .unwrap();
        let t = &ev.aggregate.overall;
        assert_eq!((t.leak_total, t.leak_leaked, t.readable_before), (1, 0, 1));
    }

    #[test]
    fn split_filter_and_truth_source_needs_no_leak() {
        let (dir, m) = make_set(
            "split",
            vec![
                email_image("a.png", "dev"),
                email_image("b.png", "held_out"),
            ],
        );
        let ev = evaluate(
            &dir,
            &m,
            &opts(Split::HeldOut, false),
            &Source::Truth,
            "truth",
            false,
        )
        .unwrap();
        assert_eq!(ev.header.images_run, 1);
        let all = evaluate(
            &dir,
            &m,
            &opts(Split::All, false),
            &Source::Truth,
            "truth",
            false,
        )
        .unwrap();
        assert_eq!(all.header.images_run, 2);
        assert!(evaluate(
            &dir,
            &m,
            &opts(Split::Dev, true),
            &Source::Truth,
            "truth",
            false
        )
        .is_err());
    }

    #[test]
    fn report_text_is_stable_and_labelled() {
        let (dir, m) = make_set("report", vec![email_image("a.png", "dev")]);
        let ev = evaluate(
            &dir,
            &m,
            &opts(Split::Dev, false),
            &Source::Truth,
            "truth",
            false,
        )
        .unwrap();
        let mut h = ev.header.clone();
        h.date = "2026-10-30".into();
        h.commit = "abc1234".into();
        let a = report::render(&h, &ev.aggregate, &ev.timing);
        let b = report::render(&h, &ev.aggregate, &ev.timing);
        assert_eq!(a, b);
        for needle in [
            "seed 7",
            "Recognizer: `truth`",
            "All numbers here are synthetic",
            "Thresholds are tuned on dev only; held_out is read once per release.",
            "| email |",
            "| HIPAA 6 |",
            "## By layout",
            "| mail |",
            "100.0% (1/1)",
        ] {
            assert!(a.contains(needle), "missing {needle:?} in\n{a}");
        }
        assert!(!a.contains("WARNING"));
        h.split = "all".into();
        assert!(report::render(&h, &ev.aggregate, &ev.timing).contains("WARNING"));
    }
}
