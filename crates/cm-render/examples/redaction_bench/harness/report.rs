//! Roll-ups and the Markdown report. Deterministic: every map is ordered.

use super::score::*;
use cm_render::ocr::RedactionKind;
use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, Default)]
pub struct Header {
    pub date: String,
    pub commit: String,
    pub seed: u64,
    pub manifest_version: u32,
    pub images_in_manifest: usize,
    pub images_run: usize,
    pub negatives_run: usize,
    pub truths_run: usize,
    pub decoys_run: usize,
    pub recognizer: String,
    pub kinds: Vec<String>,
    pub split: String,
    pub detector_only: bool,
    pub leak: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Timing {
    /// Milliseconds per image, recognition (first pass only).
    pub recognize_ms: Vec<f64>,
    /// Milliseconds per image, detection.
    pub detect_ms: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct Aggregate {
    pub overall: Tally,
    /// In `RedactionKind` order.
    pub by_kind: Vec<(String, Tally)>,
    pub by_standard: BTreeMap<String, Tally>,
    /// (dimension, rows).
    pub buckets: Vec<(&'static str, BTreeMap<String, Tally>)>,
}

pub fn aggregate(results: &[ImageResult], kinds: &[RedactionKind]) -> Aggregate {
    let mut overall = Tally::default();
    for r in results {
        overall.add(r, &|_| true, &|_| true);
    }
    let by_kind = kinds
        .iter()
        .map(|k| {
            let mut t = Tally::default();
            for r in results {
                t.add(r, &|x| x.kind == k.as_str(), &|p| p.kind == k.as_str());
            }
            (k.as_str().to_owned(), t)
        })
        .collect();
    let mut by_standard: BTreeMap<String, Tally> = BTreeMap::new();
    for r in results {
        for s in r.truths.iter().flat_map(|t| t.standards.iter()) {
            by_standard.entry(s.clone()).or_default();
        }
    }
    for (s, t) in &mut by_standard {
        for r in results {
            t.add(r, &|x| x.standards.contains(s), &|_| false);
        }
    }
    type Key = fn(&Meta) -> String;
    let dims: [(&'static str, Key); 7] = [
        ("split", |m| m.split.clone()),
        ("layout", |m| m.layout.clone()),
        ("theme", |m| m.theme.clone()),
        ("scale", |m| format!("{}x", m.scale)),
        ("font size (smallest text, logical px)", |m| {
            font_bucket(m.font_px).to_owned()
        }),
        ("degraded", |m| {
            if m.degraded { "degraded" } else { "clean" }.to_owned()
        }),
        ("language", |m| m.language.clone()),
    ];
    let buckets = dims
        .iter()
        .map(|(name, key)| {
            let mut rows: BTreeMap<String, Tally> = BTreeMap::new();
            for r in results {
                rows.entry(key(&r.meta))
                    .or_default()
                    .add(r, &|_| true, &|_| true);
            }
            (*name, rows)
        })
        .collect();
    Aggregate {
        overall,
        by_kind,
        by_standard,
        buckets,
    }
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 {
        "n/a".to_owned()
    } else {
        format!("{:.1}% ({n}/{d})", 100.0 * n as f64 / d as f64)
    }
}

fn rate(n: usize, d: usize) -> String {
    if d == 0 {
        "n/a".to_owned()
    } else {
        format!("{:.3}", n as f64 / d as f64)
    }
}

const COLS: &str = "| images (neg) | truths | recall, any kind | recall, right kind | proposals | precision | false alarms per image | per negative image | leak rate |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|\n";

fn row(out: &mut String, label: &str, t: &Tally, recall_only: bool, leak: bool) {
    let (props, prec, fai, fan) = if recall_only {
        ("-".into(), "-".into(), "-".into(), "-".into())
    } else {
        (
            t.props.to_string(),
            pct(t.tp, t.props),
            rate(t.fa(), t.images),
            rate(t.fa_on_neg, t.neg_images),
        )
    };
    let leak_s = if leak {
        pct(t.leak_leaked, t.leak_total)
    } else {
        "-".into()
    };
    let _ = writeln!(
        out,
        "| {label} | {} ({}) | {} | {} | {} | {props} | {prec} | {fai} | {fan} | {leak_s} |",
        t.images,
        t.neg_images,
        t.truths,
        pct(t.found, t.truths),
        pct(t.found_kind, t.truths),
    );
}

fn table(out: &mut String, first: &str, rows: &[(String, &Tally)], recall_only: bool, leak: bool) {
    let _ = write!(out, "| {first} {COLS}");
    for (l, t) in rows {
        row(out, l, t, recall_only, leak);
    }
    out.push('\n');
}

fn stats(v: &[f64]) -> String {
    if v.is_empty() {
        return "n/a".into();
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let mean = s.iter().sum::<f64>() / s.len() as f64;
    let idx = ((0.95 * s.len() as f64).ceil() as usize).clamp(1, s.len()) - 1;
    format!(
        "mean {mean:.1} ms, p95 {:.1} ms ({} images)",
        s[idx],
        s.len()
    )
}

pub fn render(h: &Header, a: &Aggregate, timing: &Timing) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "# Auto-redact benchmark report\n");
    let _ = writeln!(o, "- Date: {}", h.date);
    let _ = writeln!(o, "- Commit: {}", h.commit);
    let _ = writeln!(
        o,
        "- Set: manifest version {}, seed {}, {} images in the manifest",
        h.manifest_version, h.seed, h.images_in_manifest
    );
    let _ = writeln!(
        o,
        "- Run: split `{}`: {} images ({} negative), {} truths, {} decoys",
        h.split, h.images_run, h.negatives_run, h.truths_run, h.decoys_run
    );
    let _ = writeln!(
        o,
        "- Recognizer: `{}`{}",
        h.recognizer,
        if h.detector_only {
            " (detector-only: perfect text from the manifest, no OCR errors, no end-to-end pass)"
        } else {
            ""
        }
    );
    let _ = writeln!(o, "- Kinds: {}\n", h.kinds.join(", "));
    let _ = writeln!(
        o,
        "> **All numbers here are synthetic.** The pictures come from a seeded generator with invented data \
         (see `tests/data/redaction/`); they are not real screenshots and can flatter the detector. \
         They are not a measure of any real-world compliance.\n>\n\
         > **Thresholds are tuned on dev only; held_out is read once per release.**\n"
    );
    if h.split != "dev" {
        let _ = writeln!(
            o,
            "> **WARNING: this run reads the `{}` split. Do not tune thresholds or rules on these numbers.**\n",
            h.split
        );
    }
    let _ = writeln!(o, "## Definitions\n");
    for d in [
        "**Found (recall):** a truth box is found when the union of the proposals' rectangles covers at least 90 % of its area. *Any kind* uses all proposals; *right kind* only proposals of the truth's own kind. Truths of kinds outside the selected kinds are left out.",
        "**Precision (per kind):** the share of proposals of that kind with at least 50 % of their area inside the truth boxes of the same kind, each grown by `max(15 % of its height, 2 px)` (the detector's own padding). A proposal that is inside a truth of another kind is counted as *wrong kind*: not a hit for its kind, and not a false alarm.",
        "**False alarm:** a proposal that is not a true positive and either covers at least 30 % of a decoy box (counted under that decoy's `what`; the decoy covered most wins) or touches nothing listed (`(no decoy)`). Reported per image overall and per negative image (an image whose manifest lists no truth).",
        "**Leak rate (end-to-end):** every proposal is blurred with strength 16 (the editor's minimum, `MIN_STRENGTH`) by the same blur op the editor writes, the recognizer reads the blurred picture, and a truth is *leaked* when its letters and digits (case-folded, other characters ignored, at least 3) appear in the text of the lines touching its box grown by its own height. Truths shorter than 3 letters/digits are skipped. Only the system recognizer; the rate is over all truths of the selected kinds, so a missed truth that was readable counts. A truth not readable before blurring cannot leak; *readable before* is shown separately.",
        "**Per-standard roll-up:** a truth counts under each standard it lists, so rows overlap; recall and leak only (proposals do not carry a standard).",
        "**Time:** per image, recognition and detection separately; p95 is nearest-rank.",
    ] {
        let _ = writeln!(o, "- {d}");
    }
    o.push('\n');

    let leak = h.leak;
    let _ = writeln!(o, "## Overall\n");
    table(&mut o, "scope", &[("all".into(), &a.overall)], false, leak);
    if leak {
        let t = &a.overall;
        let _ = writeln!(
            o,
            "Readable before blurring: {}; of those still readable after: {}.\n",
            pct(t.readable_before, t.leak_total),
            pct(t.readable_before_leaked, t.readable_before)
        );
    }

    let _ = writeln!(o, "## Per kind\n");
    let rows: Vec<(String, &Tally)> = a.by_kind.iter().map(|(k, t)| (k.clone(), t)).collect();
    table(&mut o, "kind", &rows, false, leak);
    let _ = writeln!(
        o,
        "Per-kind precision and false alarms count proposals of that kind; recall counts truths of that kind. Wrong-kind proposals: {}.\n",
        a.by_kind
            .iter()
            .filter(|(_, t)| t.wrong_kind > 0)
            .map(|(k, t)| format!("{k} {}", t.wrong_kind))
            .collect::<Vec<_>>()
            .join(", ")
            .pipe_empty("none")
    );

    let _ = writeln!(o, "## Per standard (recall and leak)\n");
    let rows: Vec<(String, &Tally)> = a.by_standard.iter().map(|(k, t)| (k.clone(), t)).collect();
    table(&mut o, "standard", &rows, true, leak);

    let _ = writeln!(o, "## False alarms by lookalike\n");
    let _ = writeln!(
        o,
        "| decoy `what` | decoys in set | false alarms | hit rate |\n|---|---:|---:|---:|"
    );
    let mut whats: Vec<&String> = a
        .overall
        .decoys_what
        .keys()
        .chain(a.overall.fa_what.keys())
        .collect();
    whats.sort();
    whats.dedup();
    for w in whats {
        let n = a.overall.decoys_what.get(w).copied().unwrap_or(0);
        let f = a.overall.fa_what.get(w).copied().unwrap_or(0);
        let _ = writeln!(o, "| {w} | {n} | {f} | {} |", rate(f, n));
    }
    o.push('\n');

    for (name, rows) in &a.buckets {
        let _ = writeln!(o, "## By {name}\n");
        let rows: Vec<(String, &Tally)> = rows.iter().map(|(k, t)| (k.clone(), t)).collect();
        table(&mut o, "bucket", &rows, false, leak);
    }

    let _ = writeln!(o, "## Time per image\n");
    let _ = writeln!(
        o,
        "- Recognition: {}",
        if h.detector_only {
            "n/a (detector-only)".to_owned()
        } else {
            stats(&timing.recognize_ms)
        }
    );
    let _ = writeln!(o, "- Detection: {}", stats(&timing.detect_ms));
    o
}

trait PipeEmpty {
    fn pipe_empty(self, alt: &str) -> String;
}

impl PipeEmpty for String {
    fn pipe_empty(self, alt: &str) -> String {
        if self.is_empty() {
            alt.to_owned()
        } else {
            self
        }
    }
}

/// `YYYY-MM-DD` (UTC) of a Unix time in seconds.
pub fn date_of(secs: u64) -> String {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{m:02}-{d:02}", if m <= 2 { y + 1 } else { y })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_793_318_400), "2026-10-30");
    }
}
