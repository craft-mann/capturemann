//! Scoring one image and adding the results into tallies. No I/O, no recognizer.

use super::geom::{area, coverage, expand, intersect, union_area, R};
use crate::schema::{Decoy, Truth};
use cm_render::ocr::TextLine;
use std::collections::BTreeMap;

/// A truth counts as found when the union of proposals covers at least this share of its box.
pub const RECALL_COVERAGE: f64 = 0.9;
/// A proposal is a true positive when at least this share of its area lies inside the padded
/// boxes of truths of the same kind.
pub const PRECISION_INSIDE: f64 = 0.5;
/// A proposal hits a decoy when it covers at least this share of the decoy box.
pub const DECOY_HIT: f64 = 0.3;
/// Blur strength of the end-to-end pass: the editor's `MIN_STRENGTH`
/// (`apps/desktop/src/editor/redact.rs`), the weakest blur an accepted proposal gets.
pub const LEAK_BLUR_STRENGTH: u32 = 16;

/// One proposed rectangle (physical px) with its kind name.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub rect: R,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropClass {
    /// Inside a truth of the same kind.
    Tp,
    /// Not a TP, but inside (at least half) a truth of another kind: still hides something.
    WrongKind,
    /// A false alarm on a decoy; the string is the decoy's `what`.
    FaDecoy(String),
    /// A false alarm on nothing the manifest lists.
    FaNothing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropResult {
    pub kind: String,
    pub class: PropClass,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TruthResult {
    pub kind: String,
    pub standards: Vec<String>,
    /// Covered at least 90 % by proposals of any kind.
    pub found: bool,
    /// Covered at least 90 % by proposals of the truth's own kind.
    pub found_kind: bool,
    /// End-to-end pass only: the text was readable in the recognizer output before blurring.
    pub readable_before: Option<bool>,
    /// End-to-end pass only: the text is still readable after blurring every proposal.
    pub leaked: Option<bool>,
}

/// Padding of a truth box when judging proposals: the detector pads its rectangles by
/// `max(15 % of the height, 2 px)`, so the same amount is allowed here.
fn pad_of(r: R) -> f64 {
    (r[3] * 0.15).max(2.0)
}

pub fn score_image(
    truths: &[Truth],
    decoys: &[Decoy],
    props: &[Proposal],
) -> (Vec<TruthResult>, Vec<PropResult>) {
    let all: Vec<R> = props.iter().map(|p| p.rect).collect();
    let tr = truths
        .iter()
        .map(|t| {
            let own: Vec<R> = props
                .iter()
                .filter(|p| p.kind == t.kind)
                .map(|p| p.rect)
                .collect();
            TruthResult {
                kind: t.kind.clone(),
                standards: t.standards.clone(),
                found: coverage(t.rect, &all) >= RECALL_COVERAGE - 1e-9,
                found_kind: coverage(t.rect, &own) >= RECALL_COVERAGE - 1e-9,
                readable_before: None,
                leaked: None,
            }
        })
        .collect();
    let pr = props
        .iter()
        .map(|p| {
            let inside = |same: bool| -> bool {
                let regions: Vec<R> = truths
                    .iter()
                    .filter(|t| (t.kind == p.kind) == same)
                    .map(|t| expand(t.rect, pad_of(t.rect)))
                    .collect();
                let a = area(p.rect);
                if a <= 0.0 || regions.is_empty() {
                    return false;
                }
                let clipped: Vec<R> = regions
                    .iter()
                    .filter_map(|r| intersect(p.rect, *r))
                    .collect();
                union_area(&clipped) / a >= PRECISION_INSIDE - 1e-9
            };
            let class = if inside(true) {
                PropClass::Tp
            } else if let Some(d) = best_decoy(p.rect, decoys) {
                PropClass::FaDecoy(d.what.clone())
            } else if inside(false) {
                PropClass::WrongKind
            } else {
                PropClass::FaNothing
            };
            PropResult {
                kind: p.kind.clone(),
                class,
            }
        })
        .collect();
    (tr, pr)
}

/// The decoy this proposal covers most (at least [`DECOY_HIT`] of the decoy's box), if any.
fn best_decoy(p: R, decoys: &[Decoy]) -> Option<&Decoy> {
    let mut best: Option<(&Decoy, f64)> = None;
    for d in decoys {
        let a = area(d.rect);
        if a <= 0.0 {
            continue;
        }
        let share = intersect(p, d.rect).map_or(0.0, area) / a;
        if share >= DECOY_HIT - 1e-9 && best.is_none_or(|(_, s)| share > s) {
            best = Some((d, share));
        }
    }
    best.map(|(d, _)| d)
}

/// Digits and letters only, case-folded: how text is compared in the end-to-end pass.
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether `truth`'s text can be read from `lines` near its box (lines that touch the box
/// grown by its own height on every side; their texts are joined in the order given). `None`
/// when the text has fewer than 3 letters or digits, which would match by chance.
pub fn readable_in(truth: &Truth, lines: &[TextLine]) -> Option<bool> {
    let want = normalize(&truth.text);
    if want.chars().count() < 3 {
        return None;
    }
    let zone = expand(truth.rect, truth.rect[3].max(1.0));
    let near: String = lines
        .iter()
        .filter(|l| intersect(zone, [l.rect.x, l.rect.y, l.rect.w, l.rect.h]).is_some())
        .map(|l| normalize(&l.text))
        .collect();
    Some(near.contains(&want))
}

/// Fills `readable_before` and `leaked` of `results` (same order as `truths`).
pub fn apply_leak(
    truths: &[Truth],
    results: &mut [TruthResult],
    before: &[TextLine],
    after: &[TextLine],
) {
    for (t, r) in truths.iter().zip(results.iter_mut()) {
        r.readable_before = readable_in(t, before);
        r.leaked = readable_in(t, after);
    }
}

/// Image-level facts the roll-ups group by.
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub split: String,
    pub layout: String,
    pub theme: String,
    pub scale: u32,
    pub font_px: u32,
    pub degraded: bool,
    pub language: String,
    /// The manifest lists no truth for the image.
    pub negative: bool,
}

#[derive(Debug, Clone)]
pub struct ImageResult {
    pub meta: Meta,
    pub truths: Vec<TruthResult>,
    pub props: Vec<PropResult>,
    /// `what` of every decoy in the image.
    pub decoys: Vec<String>,
}

/// Font-size bucket label (logical px of the smallest text).
pub fn font_bucket(px: u32) -> &'static str {
    match px {
        0..=10 => "a: <= 10 px",
        11..=12 => "b: 11-12 px",
        13..=14 => "c: 13-14 px",
        15..=18 => "d: 15-18 px",
        _ => "e: >= 19 px",
    }
}

/// Counters for one row of a table.
#[derive(Debug, Clone, Default)]
pub struct Tally {
    pub images: usize,
    pub neg_images: usize,
    pub truths: usize,
    pub found: usize,
    pub found_kind: usize,
    pub leak_total: usize,
    pub leak_leaked: usize,
    pub readable_before: usize,
    pub readable_before_leaked: usize,
    pub props: usize,
    pub tp: usize,
    pub wrong_kind: usize,
    pub fa_decoy: usize,
    pub fa_nothing: usize,
    /// False alarms (decoy or nothing) on negative images.
    pub fa_on_neg: usize,
    /// False alarms by the decoy's `what` (`(no decoy)` for [`PropClass::FaNothing`]).
    pub fa_what: BTreeMap<String, usize>,
    /// Decoys present, by `what`.
    pub decoys_what: BTreeMap<String, usize>,
}

pub const NO_DECOY: &str = "(no decoy)";

impl Tally {
    /// Adds one image, counting only the truths and proposals the filters accept.
    pub fn add(
        &mut self,
        r: &ImageResult,
        truth_ok: &dyn Fn(&TruthResult) -> bool,
        prop_ok: &dyn Fn(&PropResult) -> bool,
    ) {
        self.images += 1;
        if r.meta.negative {
            self.neg_images += 1;
        }
        for d in &r.decoys {
            *self.decoys_what.entry(d.clone()).or_default() += 1;
        }
        for t in r.truths.iter().filter(|t| truth_ok(t)) {
            self.truths += 1;
            self.found += usize::from(t.found);
            self.found_kind += usize::from(t.found_kind);
            if let Some(l) = t.leaked {
                self.leak_total += 1;
                self.leak_leaked += usize::from(l);
                if t.readable_before == Some(true) {
                    self.readable_before += 1;
                    self.readable_before_leaked += usize::from(l);
                }
            }
        }
        for p in r.props.iter().filter(|p| prop_ok(p)) {
            self.props += 1;
            let fa = match &p.class {
                PropClass::Tp => {
                    self.tp += 1;
                    None
                }
                PropClass::WrongKind => {
                    self.wrong_kind += 1;
                    None
                }
                PropClass::FaDecoy(w) => {
                    self.fa_decoy += 1;
                    Some(w.clone())
                }
                PropClass::FaNothing => {
                    self.fa_nothing += 1;
                    Some(NO_DECOY.to_owned())
                }
            };
            if let Some(w) = fa {
                *self.fa_what.entry(w).or_default() += 1;
                if r.meta.negative {
                    self.fa_on_neg += 1;
                }
            }
        }
    }

    pub fn fa(&self) -> usize {
        self.fa_decoy + self.fa_nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn truth(kind: &str, text: &str, rect: R) -> Truth {
        Truth {
            kind: kind.into(),
            text: text.into(),
            rect,
            standards: vec!["HIPAA 7".into()],
        }
    }

    fn prop(kind: &str, rect: R) -> Proposal {
        Proposal {
            rect,
            kind: kind.into(),
        }
    }

    #[test]
    fn ninety_percent_edge() {
        let t = [truth("ssn", "123-45-6789", [0.0, 0.0, 100.0, 10.0])];
        let (hit, _) = score_image(&t, &[], &[prop("ssn", [0.0, 0.0, 90.0, 10.0])]);
        assert!(hit[0].found && hit[0].found_kind);
        let (miss, _) = score_image(&t, &[], &[prop("ssn", [0.0, 0.0, 89.0, 10.0])]);
        assert!(!miss[0].found);
        // Two proposals that together cover 90 %, overlapping each other.
        let (two, _) = score_image(
            &t,
            &[],
            &[
                prop("ssn", [0.0, 0.0, 60.0, 10.0]),
                prop("ssn", [40.0, 0.0, 50.0, 10.0]),
            ],
        );
        assert!(two[0].found);
    }

    #[test]
    fn right_kind_is_separate() {
        let t = [truth("ssn", "123-45-6789", [0.0, 0.0, 100.0, 10.0])];
        let (r, p) = score_image(&t, &[], &[prop("phone", [0.0, 0.0, 100.0, 10.0])]);
        assert!(r[0].found && !r[0].found_kind);
        assert_eq!(p[0].class, PropClass::WrongKind);
    }

    #[test]
    fn decoy_and_nothing_are_false_alarms() {
        let d = [Decoy {
            what: "order_number".into(),
            text: "123456789".into(),
            rect: [0.0, 50.0, 100.0, 10.0],
        }];
        let (_, p) = score_image(
            &[],
            &d,
            &[
                prop("ssn", [0.0, 50.0, 40.0, 10.0]),
                prop("ssn", [0.0, 200.0, 40.0, 10.0]),
            ],
        );
        assert_eq!(p[0].class, PropClass::FaDecoy("order_number".into()));
        assert_eq!(p[1].class, PropClass::FaNothing);
        // 29 % of the decoy is not a hit.
        let (_, q) = score_image(&[], &d, &[prop("ssn", [0.0, 50.0, 29.0, 10.0])]);
        assert_eq!(q[0].class, PropClass::FaNothing);
    }

    #[test]
    fn leak_compares_alphanumerics() {
        let t = [truth("card", "4111 1111-1111", [10.0, 10.0, 100.0, 12.0])];
        let line = TextLine::laid_out("Card: 4111111111 11", 10.0, 10.0, 6.0, 12.0);
        assert_eq!(readable_in(&t[0], &[line]), Some(true));
        let far = TextLine::laid_out("4111111111 11", 10.0, 300.0, 6.0, 12.0);
        assert_eq!(readable_in(&t[0], &[far]), Some(false));
    }
}
