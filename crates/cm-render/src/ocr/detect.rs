//! Finding things worth redacting in recognized text (ANN-2 Auto-redact): email addresses,
//! API keys and other long tokens, plus phone numbers and IPv4 addresses as lower-confidence
//! extras. Pure Rust over [`TextLine`]s, so it is tested with fixtures and no OCR engine.
//!
//! A hit that is only part of a line (an email inside a sentence) gets a rectangle for just
//! that span: the word box is split in proportion to the characters, which is an estimate for
//! proportional fonts, and the padding absorbs the error. Nothing here applies anything; the
//! caller turns accepted suggestions into blur ops.

use super::rules::{self, flatten, Located, Span};
use super::TextLine;
use cm_core::Rc;

/// What a suggestion is about. Ordered by priority when two suggestions overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RedactionKind {
    Email,
    /// API keys, JWTs and other long random-looking strings.
    Token,
    /// PEM private-key blocks.
    PrivateKey,
    /// The value of a `NAME=value` or `"name": "value"` pair with a secret-looking name.
    SecretAssignment,
    /// A URL that carries credentials or a token in its query.
    UrlSecret,
    /// Payment card number (PCI DSS primary account number).
    Card,
    /// Expiry date or security code beside a detected card number (PCI DSS).
    CardExtra,
    Ssn,
    Itin,
    Ein,
    /// ABA routing number.
    Aba,
    Iban,
    /// National Provider Identifier.
    Npi,
    Dea,
    /// Medicare Beneficiary Identifier.
    Mbi,
    Passport,
    Vin,
    Imei,
    IpAddress,
    MacAddress,
    Phone,
}

impl RedactionKind {
    pub const ALL: [RedactionKind; 21] = [
        RedactionKind::Email,
        RedactionKind::Token,
        RedactionKind::PrivateKey,
        RedactionKind::SecretAssignment,
        RedactionKind::UrlSecret,
        RedactionKind::Card,
        RedactionKind::CardExtra,
        RedactionKind::Ssn,
        RedactionKind::Itin,
        RedactionKind::Ein,
        RedactionKind::Aba,
        RedactionKind::Iban,
        RedactionKind::Npi,
        RedactionKind::Dea,
        RedactionKind::Mbi,
        RedactionKind::Passport,
        RedactionKind::Vin,
        RedactionKind::Imei,
        RedactionKind::IpAddress,
        RedactionKind::MacAddress,
        RedactionKind::Phone,
    ];

    /// The kinds looked for when the caller names none. Chosen by the benchmark (RED-1): a kind
    /// whose false-alarm rate is above the gate leaves this list.
    pub const DEFAULT: [RedactionKind; 21] = RedactionKind::ALL;

    /// The name used in results, MCP and the CLI.
    pub fn as_str(self) -> &'static str {
        match self {
            RedactionKind::Email => "email",
            RedactionKind::Token => "token",
            RedactionKind::PrivateKey => "private_key",
            RedactionKind::SecretAssignment => "secret_assignment",
            RedactionKind::UrlSecret => "url_secret",
            RedactionKind::Card => "card",
            RedactionKind::CardExtra => "card_extra",
            RedactionKind::Ssn => "ssn",
            RedactionKind::Itin => "itin",
            RedactionKind::Ein => "ein",
            RedactionKind::Aba => "aba",
            RedactionKind::Iban => "iban",
            RedactionKind::Npi => "npi",
            RedactionKind::Dea => "dea",
            RedactionKind::Mbi => "mbi",
            RedactionKind::Passport => "passport",
            RedactionKind::Vin => "vin",
            RedactionKind::Imei => "imei",
            RedactionKind::IpAddress => "ip_address",
            RedactionKind::MacAddress => "mac_address",
            RedactionKind::Phone => "phone",
        }
    }

    /// The names of all kinds, for error messages and tool schemas.
    pub fn names() -> Vec<&'static str> {
        RedactionKind::ALL.iter().map(|k| k.as_str()).collect()
    }

    /// The public standards that name this kind of data (README table, RED-3). Empty for kinds
    /// that no standard names.
    pub fn standards(self) -> &'static [&'static str] {
        match self {
            RedactionKind::Email => &["HIPAA 6"],
            RedactionKind::Token | RedactionKind::PrivateKey | RedactionKind::SecretAssignment => {
                &["GLBA / state breach laws (credentials)"]
            }
            RedactionKind::UrlSecret => &["HIPAA 14", "GLBA / state breach laws (credentials)"],
            RedactionKind::Card => &["PCI DSS", "HIPAA 10", "GLBA"],
            RedactionKind::CardExtra => &["PCI DSS (sensitive authentication data)"],
            RedactionKind::Ssn | RedactionKind::Itin => &["NIST SP 800-122", "HIPAA 7", "GLBA"],
            RedactionKind::Ein => &["NIST SP 800-122"],
            RedactionKind::Aba | RedactionKind::Iban => &["HIPAA 10", "GLBA"],
            RedactionKind::Npi | RedactionKind::Dea => &["HIPAA 11"],
            RedactionKind::Mbi => &["HIPAA 9"],
            RedactionKind::Passport => &["HIPAA 11", "GLBA"],
            RedactionKind::Vin => &["HIPAA 12"],
            RedactionKind::Imei | RedactionKind::MacAddress => &["HIPAA 13"],
            RedactionKind::IpAddress => &["HIPAA 15"],
            RedactionKind::Phone => &["HIPAA 4, 5"],
        }
    }

    pub fn parse(s: &str) -> Option<RedactionKind> {
        RedactionKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// One proposed blur rectangle, in original-image pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct RedactionSuggestion {
    pub rect: Rc,
    pub kind: RedactionKind,
    /// 0.0..=1.0: how sure the detector is, scaled by the recognizer's confidence.
    pub confidence: f32,
    /// The text, masked: first and last two characters only (see [`mask_preview`]).
    pub text_preview: String,
}

#[derive(Debug, Clone, Default)]
pub struct SuggestOptions {
    /// Kinds to look for; empty means [`RedactionKind::DEFAULT`].
    pub kinds: Vec<RedactionKind>,
}

impl SuggestOptions {
    pub(crate) fn wants(&self, k: RedactionKind) -> bool {
        if self.kinds.is_empty() {
            RedactionKind::DEFAULT.contains(&k)
        } else {
            self.kinds.contains(&k)
        }
    }
}

/// Masks a secret for display: `"jo...om"`. Short strings are starred out entirely, so the
/// preview never reveals more than four characters of anything.
pub fn mask_preview(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() < 8 {
        return "*".repeat(c.len());
    }
    let head: String = c[..2].iter().collect();
    let tail: String = c[c.len() - 2..].iter().collect();
    format!("{head}...{tail}")
}

/// Detects redaction candidates in `lines` of an `image`-sized picture. Rectangles are padded,
/// clipped to the image and merged where two cover mostly the same area.
pub fn suggest_redactions(
    lines: &[TextLine],
    image: (u32, u32),
    opts: &SuggestOptions,
) -> Vec<RedactionSuggestion> {
    let found = scan_page(lines, opts);
    let bounds = Rc::new(0.0, 0.0, f64::from(image.0), f64::from(image.1));
    let mut padded: Vec<RedactionSuggestion> = found
        .into_iter()
        .filter_map(|mut s| {
            let h = s.rect.h;
            let pad = (h * 0.15).max(2.0);
            let r = Rc::new(
                s.rect.x - pad,
                s.rect.y - pad,
                s.rect.w + 2.0 * pad,
                s.rect.h + 2.0 * pad,
            );
            s.rect = intersect(r, bounds)?;
            (s.rect.w >= 1.0 && s.rect.h >= 1.0).then_some(s)
        })
        .collect();
    padded.sort_by(|a, b| {
        a.rect
            .y
            .total_cmp(&b.rect.y)
            .then(a.rect.x.total_cmp(&b.rect.x))
    });
    merge(padded)
}

fn area(r: Rc) -> f64 {
    r.w.max(0.0) * r.h.max(0.0)
}

fn intersect(a: Rc, b: Rc) -> Option<Rc> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    (x1 > x0 && y1 > y0).then(|| Rc::new(x0, y0, x1 - x0, y1 - y0))
}

fn union(a: Rc, b: Rc) -> Rc {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Rc::new(x0, y0, x1 - x0, y1 - y0)
}

/// Merges rectangles when the overlap is at least half of the smaller one. Two emails on
/// neighbouring lines keep separate rectangles; the same secret found twice does not.
fn merge(mut v: Vec<RedactionSuggestion>) -> Vec<RedactionSuggestion> {
    'again: loop {
        for i in 0..v.len() {
            for j in (i + 1)..v.len() {
                let Some(o) = intersect(v[i].rect, v[j].rect) else {
                    continue;
                };
                let smaller = area(v[i].rect).min(area(v[j].rect));
                if smaller > 0.0 && area(o) >= 0.5 * smaller {
                    let b = v.remove(j);
                    let a = &mut v[i];
                    a.rect = union(a.rect, b.rect);
                    if b.kind < a.kind {
                        a.kind = b.kind;
                        a.text_preview = b.text_preview;
                    }
                    a.confidence = a.confidence.max(b.confidence);
                    continue 'again;
                }
            }
        }
        return v;
    }
}

/// Runs the line rules over every line, then the page rules over the whole page, and turns the
/// surviving spans into suggestions (rectangles not yet padded).
fn scan_page(lines: &[TextLine], opts: &SuggestOptions) -> Vec<RedactionSuggestion> {
    let flats: Vec<rules::Flat> = lines.iter().map(flatten).collect();
    let mut located: Vec<Located> = Vec::new();
    // First come, first kept: a span that overlaps one already kept on its line is dropped.
    let keep = |l: Located, located: &mut Vec<Located>| {
        let s = l.span;
        if s.end > s.start
            && !located
                .iter()
                .any(|t| t.line == l.line && s.start < t.span.end && t.span.start < s.end)
        {
            located.push(l);
        }
    };
    for entry in rules::LINE_RULES {
        if !entry.kinds.iter().any(|&k| opts.wants(k)) {
            continue;
        }
        for (line, flat) in flats.iter().enumerate() {
            for span in (entry.run)(&flat.chars) {
                if opts.wants(span.kind) {
                    keep(Located { line, span }, &mut located);
                }
            }
        }
    }
    for entry in rules::PAGE_RULES {
        if !entry.kinds.iter().any(|&k| opts.wants(k)) {
            continue;
        }
        // A page rule sees more than a line rule does (a whole key block), so a page span that
        // fully covers line-rule spans replaces them; one that only partly overlaps a kept span
        // is dropped (an expiry-like number inside an IBAN must not cut the IBAN short).
        for l in (entry.run)(&flats, &located.clone()) {
            if !opts.wants(l.span.kind) || l.line >= flats.len() {
                continue;
            }
            let overlaps = |t: &Located| {
                t.line == l.line && l.span.start < t.span.end && t.span.start < l.span.end
            };
            let covers_all = located
                .iter()
                .filter(|t| overlaps(t))
                .all(|t| l.span.start <= t.span.start && t.span.end <= l.span.end);
            if covers_all {
                located.retain(|t| !overlaps(t));
                keep(l, &mut located);
            }
        }
    }
    let mut out = Vec::new();
    for l in located {
        let flat = &flats[l.line];
        let s: Span = l.span;
        if s.end > flat.chars.len() {
            continue;
        }
        let mut rect: Option<Rc> = None;
        for cell in flat.cells[s.start..s.end].iter().flatten() {
            rect = Some(rect.map_or(*cell, |r| union(r, *cell)));
        }
        let Some(mut rect) = rect else { continue };
        // A span that starts or ends inside a word (a password inside a URL) is placed by the
        // character-proportional estimate, which drifts in long words in a proportional font;
        // widen the cut sides by half a line height to absorb it.
        let slack = rect.h * 0.5;
        if s.start > 0
            && flat.cells[s.start - 1].is_some()
            && !flat.chars[s.start - 1].is_whitespace()
        {
            rect = Rc::new(rect.x - slack, rect.y, rect.w + slack, rect.h);
        }
        if flat.cells.get(s.end).is_some_and(Option::is_some) && !flat.chars[s.end].is_whitespace()
        {
            rect = Rc::new(rect.x, rect.y, rect.w + slack, rect.h);
        }
        let text: String = flat.chars[s.start..s.end].iter().collect();
        out.push(RedactionSuggestion {
            rect,
            kind: s.kind,
            confidence: s.confidence * lines[l.line].confidence.clamp(0.0, 1.0),
            text_preview: mask_preview(&text),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMG: (u32, u32) = (1200, 800);

    fn line(text: &str, y: f64) -> TextLine {
        TextLine::laid_out(text, 100.0, y, 10.0, 20.0)
    }

    fn run(texts: &[&str]) -> Vec<RedactionSuggestion> {
        let lines: Vec<TextLine> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| line(t, 100.0 + 60.0 * i as f64))
            .collect();
        suggest_redactions(&lines, IMG, &SuggestOptions::default())
    }

    /// ANN-2.AC4: two email addresses give two proposed rectangles.
    #[test]
    fn ann_2_ac4_two_emails_give_two_rects() {
        let r = run(&[
            "From: jane.doe@example.com",
            "Cc: ops-team+alerts@mail.example.co.uk",
        ]);
        assert_eq!(r.len(), 2, "{r:?}");
        assert!(r.iter().all(|s| s.kind == RedactionKind::Email));
    }

    #[test]
    fn email_in_a_sentence_gets_a_rect_for_just_that_span() {
        // "Contact " is 8 chars = 80 px, so the address starts at x = 180 and is 20 chars.
        let r = run(&["Contact jane.doe@example.com today"]);
        assert_eq!(r.len(), 1);
        let rc = r[0].rect;
        let pad = 3.0; // 15% of 20 px
        assert!((rc.x - (180.0 - pad)).abs() < 0.01, "{rc:?}");
        assert!((rc.w - (200.0 + 2.0 * pad)).abs() < 0.01, "{rc:?}");
        assert!((rc.y - (100.0 - pad)).abs() < 0.01);
    }

    #[test]
    fn emails_with_trailing_punctuation_and_in_brackets() {
        for t in [
            "<a.b@x.io>,",
            "(a.b@x.io).",
            "mailto:a.b@x.io;",
            "a.b@x.io.",
        ] {
            let r = run(&[t]);
            assert_eq!(r.len(), 1, "{t}: {r:?}");
        }
        // Not emails.
        for t in ["@mention", "user@localhost", "a@b.c", "x@.com", "1 @ 2"] {
            assert!(run(&[t]).is_empty(), "{t}");
        }
    }

    #[test]
    fn api_keys_are_found_by_shape() {
        let keys = [
            "sk-proj-abcdefghijklmnopqrstuvwx",
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "AKIAIOSFODNN7EXAMPLE",
            "xoxb-1234567890-abcdefghijkl",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dBjftJeZ4CVPmB92K27uhbUJU1p1r",
        ];
        for k in keys {
            let r = run(&[&format!("token = {k}")]);
            assert_eq!(r.len(), 1, "{k}: {r:?}");
            assert_eq!(r[0].kind, RedactionKind::Token, "{k}");
            assert!(r[0].confidence >= 0.9, "{k}");
        }
    }

    #[test]
    fn long_mixed_strings_are_tokens_with_lower_confidence() {
        let r = run(&["id 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, RedactionKind::Token);
        assert!(r[0].confidence < 0.9 && r[0].confidence >= 0.5);
        let r = run(&["Zm9vYmFyMTIzNDU2Nzg5MGFiY2RlZg=="]);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        let r = run(&[
            "The quick brown fox jumps over the lazy dog",
            "NSApplicationDidFinishLaunchingNotification",
            "/Users/someone/Documents/Projects/report.pdf",
            "https://example.com/docs/getting-started",
            "version 1.2.3 build 2026-10-23 14:05",
            "AKIA short",
            "1234567890123456",
        ]);
        assert!(r.is_empty(), "{r:?}");
    }

    #[test]
    fn phones_and_ips_are_extras_with_low_confidence() {
        let r = run(&["Call +1 (415) 555-0132 or 192.168.10.24"]);
        assert_eq!(r.len(), 2, "{r:?}");
        assert!(r.iter().all(|s| s.confidence <= 0.5));
        let mut kinds: Vec<_> = r.iter().map(|s| s.kind).collect();
        kinds.sort();
        assert_eq!(kinds, [RedactionKind::IpAddress, RedactionKind::Phone]);
        assert!(run(&["order 2026-10-23 1415 5550"]).is_empty());
        assert!(run(&["300.1.1.1 and 1.2.3"]).is_empty());
    }

    #[test]
    fn kinds_filter_limits_what_is_found() {
        let lines = [line("a@b.io 192.168.0.1 sk-abcdefghijklmnopqrstuv", 50.0)];
        let only = |k| {
            suggest_redactions(&lines, IMG, &SuggestOptions { kinds: vec![k] })
                .iter()
                .map(|s| s.kind)
                .collect::<Vec<_>>()
        };
        assert_eq!(only(RedactionKind::Email), [RedactionKind::Email]);
        assert_eq!(only(RedactionKind::Token), [RedactionKind::Token]);
        assert_eq!(only(RedactionKind::IpAddress), [RedactionKind::IpAddress]);
    }

    #[test]
    fn a_token_inside_an_email_is_one_rect_not_two() {
        // A 30-character mixed local part is both an email and a "long token".
        let r = run(&["a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3@example.com"]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, RedactionKind::Email);
    }

    #[test]
    fn rects_are_padded_clipped_to_the_image_and_merged_only_when_mostly_overlapping() {
        // Near the top-left corner the padding is clipped.
        let l = TextLine::laid_out("a@b.io", 0.0, 0.0, 10.0, 20.0);
        let r = suggest_redactions(&[l], IMG, &SuggestOptions::default());
        assert_eq!(r.len(), 1);
        assert!(r[0].rect.x >= 0.0 && r[0].rect.y >= 0.0);
        // The same address on two OCR lines (a duplicate) merges into one.
        let a = line("a@b.io", 100.0);
        let r = suggest_redactions(&[a.clone(), a], IMG, &SuggestOptions::default());
        assert_eq!(r.len(), 1);
        // Two emails on adjacent lines (padded boxes touch a little) stay separate.
        let l1 = TextLine::laid_out("a@b.io", 100.0, 100.0, 10.0, 20.0);
        let l2 = TextLine::laid_out("c@d.io", 100.0, 122.0, 10.0, 20.0);
        let r = suggest_redactions(&[l1, l2], IMG, &SuggestOptions::default());
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn a_line_without_word_boxes_uses_the_line_box() {
        let l = TextLine {
            text: "mail a@b.io now".into(),
            rect: Rc::new(100.0, 100.0, 150.0, 20.0),
            confidence: 0.8,
            words: Vec::new(),
        };
        let r = suggest_redactions(&[l], IMG, &SuggestOptions::default());
        assert_eq!(r.len(), 1);
        // chars 5..11 of 15 -> x from 150 to 190, minus/plus padding.
        assert!((r[0].rect.x - 147.0).abs() < 0.01, "{:?}", r[0].rect);
        assert!((r[0].confidence - 0.95 * 0.8).abs() < 1e-6);
    }

    #[test]
    fn previews_never_echo_the_secret() {
        assert_eq!(mask_preview("jane.doe@example.com"), "ja...om");
        assert_eq!(mask_preview("short"), "*****");
        assert_eq!(mask_preview(""), "");
        let r = run(&["sk-proj-abcdefghijklmnopqrstuvwx"]);
        assert_eq!(r[0].text_preview, "sk...wx");
        assert!(!r[0].text_preview.contains("proj"));
    }

    #[test]
    fn kind_names_round_trip() {
        for k in RedactionKind::ALL {
            assert_eq!(RedactionKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(RedactionKind::parse("face"), None);
    }

    /// Lessons of the first benchmark run (see verification/benchmarks).
    #[test]
    fn long_digit_groups_masks_placeholders_and_compound_names() {
        // A 22-digit tracking number is neither a phone nor a card.
        assert!(run(&["9400 0064 5377 9579 8607 53"]).is_empty());
        assert!(run(&["9400 4799 9287 2044 1624 10"]).is_empty());
        // Broadcast and null MAC addresses name no device.
        assert!(run(&["ff:ff:ff:ff:ff:ff", "00:00:00:00:00:00"]).is_empty());
        // Words are placeholders, a compound name still marks a secret value.
        assert!(run(&[
            "DB_PASSWORD=changeme",
            "token: REPLACE_ME",
            "key=your_api_key_here"
        ])
        .is_empty());
        let r = run(&["PGPASSWORD=NI0Mnndj9QqlRZBasR3XUn"]);
        assert_eq!(r.len(), 1, "{r:?}");
        assert_eq!(r[0].kind, RedactionKind::SecretAssignment);
        // A page span that only partly overlaps a kept span does not cut it short.
        let r = run(&[
            "4111 1111 1111 1111",
            "IBAN: FR80 0221 9537 5129 4596 1444 211",
        ]);
        assert!(
            r.iter()
                .any(|s| s.kind == RedactionKind::Iban && s.rect.w > 300.0),
            "{r:?}"
        );
    }
}
