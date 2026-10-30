//! Secrets and credentials: API keys and tokens, private-key blocks, `NAME=value` pairs and
//! URLs that carry credentials.

use super::{Flat, Located, Span};
use crate::ocr::detect::RedactionKind;

fn is_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '+' | '/')
}

pub fn find_tokens(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    // JWTs first: three base64url segments with dots between them.
    let mut i = 0;
    while i < c.len() {
        let run_end = (i..c.len())
            .find(|&j| !(is_token_char(c[j]) || c[j] == '.'))
            .unwrap_or(c.len());
        let run: String = c[i..run_end].iter().collect();
        if run.starts_with("eyJ") {
            let trimmed = run.trim_end_matches('.');
            let parts: Vec<&str> = trimmed.split('.').collect();
            if parts.len() == 3 && parts.iter().all(|p| p.len() >= 8) {
                out.push(Span {
                    start: i,
                    end: i + trimmed.chars().count(),
                    kind: RedactionKind::Token,
                    confidence: 0.9,
                });
                i = run_end.max(i + 1);
                continue;
            }
        }
        i = run_end.max(i + 1);
    }
    // Everything else: runs of token characters (a '.' or '=' ends one; trailing '=' padding
    // belongs to it).
    let mut i = 0;
    while i < c.len() {
        if !is_token_char(c[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < c.len() && is_token_char(c[j]) {
            j += 1;
        }
        let mut end = j;
        while end < c.len() && end < j + 2 && c[end] == '=' {
            end += 1;
        }
        let word: String = c[i..j].iter().collect();
        if let Some(conf) = classify_token(&word) {
            if !out.iter().any(|s: &Span| i < s.end && s.start < end) {
                out.push(Span {
                    start: i,
                    end,
                    kind: RedactionKind::Token,
                    confidence: conf,
                });
            }
        }
        i = j.max(i + 1);
    }
    out
}

fn classify_token(w: &str) -> Option<f32> {
    let n = w.chars().count();
    let alnum_upper = |s: &str| {
        s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    };
    // Well-known key shapes.
    let prefixed: &[(&str, usize)] = &[
        ("sk-", 20),
        ("ghp_", 30),
        ("gho_", 30),
        ("ghu_", 30),
        ("ghs_", 30),
        ("ghr_", 30),
        ("github_pat_", 30),
        ("sk_live_", 20),
        ("pk_live_", 20),
        ("rk_live_", 20),
        ("AIza", 35),
    ];
    if prefixed
        .iter()
        .any(|(p, min)| w.starts_with(p) && n >= *min)
    {
        return Some(0.95);
    }
    if (w.starts_with("AKIA") || w.starts_with("ASIA")) && n == 20 && alnum_upper(w) {
        return Some(0.95);
    }
    if w.starts_with("xox") && w.as_bytes().get(3).is_some_and(|b| b"baprs".contains(b)) && n >= 15
    {
        return Some(0.95);
    }
    // Long, random-looking: letters and digits mixed (hex, base64).
    if n >= 24 {
        let digits = w.chars().filter(char::is_ascii_digit).count();
        let letters = w.chars().filter(char::is_ascii_alphabetic).count();
        if digits >= 3 && letters >= 3 && digits * 100 >= n * 7 && !w.starts_with('/') {
            return Some(0.6);
        }
    }
    None
}

/// `NAME=value` and `"name": "value"` pairs with a secret-looking name (filled in by the 0.2.5 rules work).
pub fn find_secret_assignments(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// URLs with credentials or token query strings (filled in by the 0.2.5 rules work).
pub fn find_url_secrets(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// PEM private-key blocks, from the BEGIN line to the END line (filled in by the 0.2.5 rules work).
pub fn find_private_keys(_lines: &[Flat], _found: &[Located]) -> Vec<Located> {
    Vec::new()
}
