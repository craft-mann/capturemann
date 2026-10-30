//! Contact details: email addresses and phone numbers.

use super::Span;
use crate::ocr::detect::RedactionKind;

fn is_local(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '%' | '+' | '-')
}

fn is_domain(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-')
}

pub fn find_emails(c: &[char]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for (i, &ch) in c.iter().enumerate() {
        if ch != '@' {
            continue;
        }
        let mut start = i;
        while start > 0 && is_local(c[start - 1]) {
            start -= 1;
        }
        while start < i && c[start] == '.' {
            start += 1;
        }
        let mut end = i + 1;
        while end < c.len() && is_domain(c[end]) {
            end += 1;
        }
        // Cut at an empty label and drop trailing dots and hyphens.
        let domain_start = i + 1;
        let mut e = domain_start;
        while e < end {
            if c[e] == '.' && (e == domain_start || e + 1 >= end || c[e + 1] == '.') {
                break;
            }
            e += 1;
        }
        let mut end = e;
        while end > domain_start && matches!(c[end - 1], '.' | '-') {
            end -= 1;
        }
        if start >= i || end <= domain_start {
            continue;
        }
        let domain: String = c[domain_start..end].iter().collect();
        let Some((_, tld)) = domain.rsplit_once('.') else {
            continue;
        };
        if tld.len() < 2 || !tld.chars().all(|x| x.is_ascii_alphabetic()) {
            continue;
        }
        if out.last().is_some_and(|s| s.end > start) {
            continue;
        }
        out.push(Span {
            start,
            end,
            kind: RedactionKind::Email,
            confidence: 0.95,
        });
    }
    out
}

pub fn find_phones(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let starts = c[i].is_ascii_digit() || c[i] == '+' || c[i] == '(';
        // Not the tail of a longer number or date ("2026-10-23 1415 5550").
        let before_ok =
            i == 0 || !(c[i - 1].is_ascii_alphanumeric() || matches!(c[i - 1], '-' | '.' | '/'));
        if !(starts && before_ok) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < c.len()
            && (c[j].is_ascii_digit() || matches!(c[j], ' ' | '(' | ')' | '+' | '.' | '-'))
        {
            j += 1;
        }
        // Trim to the last digit or closing parenthesis.
        let mut end = j;
        while end > i && !(c[end - 1].is_ascii_digit() || c[end - 1] == ')') {
            end -= 1;
        }
        let after_ok = end >= c.len() || !c[end].is_ascii_alphanumeric();
        let text: String = c[i..end].iter().collect();
        let digits = text.chars().filter(char::is_ascii_digit).count();
        let seps = text
            .chars()
            .filter(|ch| matches!(ch, ' ' | '(' | ')' | '.' | '-'))
            .count();
        let iso_date = text.len() >= 10
            && text.as_bytes()[4] == b'-'
            && text[..4].chars().all(|d| d.is_ascii_digit());
        if after_ok
            && (10..=15).contains(&digits)
            && (seps >= 2 || text.starts_with('+'))
            && !iso_date
            && !text.contains("  ")
        {
            out.push(Span {
                start: i,
                end,
                kind: RedactionKind::Phone,
                confidence: 0.5,
            });
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}
