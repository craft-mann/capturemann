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

/// North American Numbering Plan shape (public plan): ten digits, optionally behind a country
/// code 1; the area code and the exchange start with 2-9 and are not N11 service codes.
fn is_nanp(digits: &[u8]) -> bool {
    let d = match digits.len() {
        10 => digits,
        11 if digits[0] == b'1' => &digits[1..],
        _ => return false,
    };
    let (area, exch) = (&d[0..3], &d[3..6]);
    let n11 = |g: &[u8]| g[1] == b'1' && g[2] == b'1';
    area[0] >= b'2' && exch[0] >= b'2' && !n11(area) && !n11(exch)
}

/// An extension right after a number: `ext. 123`, `x123`, `#123` (1 to 5 digits). Returns the
/// index just past it.
fn phone_extension(c: &[char], at: usize) -> Option<usize> {
    let mut k = at;
    while k < c.len() && c[k] == ' ' {
        k += 1;
    }
    let lower = |i: usize| c.get(i).map(|ch| ch.to_ascii_lowercase());
    if lower(k) == Some('e') && lower(k + 1) == Some('x') && lower(k + 2) == Some('t') {
        k += 3;
        if c.get(k) == Some(&'.') {
            k += 1;
        }
    } else if matches!(lower(k), Some('x' | '#')) {
        k += 1;
    } else {
        return None;
    }
    while k < c.len() && c[k] == ' ' {
        k += 1;
    }
    let ds = k;
    while k < c.len() && c[k].is_ascii_digit() {
        k += 1;
    }
    let n = k - ds;
    if (1..=5).contains(&n) && c.get(k).is_none_or(|ch| !ch.is_ascii_alphanumeric()) {
        Some(k)
    } else {
        None
    }
}

/// Phone and fax numbers: 10 to 15 digits with separators or a leading `+`. US NANP numbers
/// that satisfy the plan's rules and have two or more separators get 0.7, other shapes 0.5.
/// An extension is part of the span.
pub fn find_phones(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let starts = c[i].is_ascii_digit() || c[i] == '+' || c[i] == '(';
        // Not the tail of a longer number or date ("2026-10-23 1415 5550").
        // Nor the tail of a longer run of digit groups ("9400 0064 5377 9579 8607 53").
        let continues_groups = i >= 2 && c[i - 1] == ' ' && c[i - 2].is_ascii_digit();
        let before_ok = !continues_groups
            && (i == 0
                || !(c[i - 1].is_ascii_alphanumeric() || matches!(c[i - 1], '-' | '.' | '/')));
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
        let ext_end = phone_extension(c, end);
        let span_end = ext_end.unwrap_or(end);
        let after_ok = span_end >= c.len() || !c[span_end].is_ascii_alphanumeric();
        let text: String = c[i..end].iter().collect();
        let digit_bytes: Vec<u8> = text.bytes().filter(u8::is_ascii_digit).collect();
        let digits = digit_bytes.len();
        let seps = text
            .chars()
            .filter(|ch| matches!(ch, ' ' | '(' | ')' | '.' | '-'))
            .count();
        let iso_date = text.len() >= 10
            && text.as_bytes()[4] == b'-'
            && text[..4].chars().all(|d| d.is_ascii_digit());
        // Four or more dot-separated groups are an IPv4 address or a version, not a phone.
        let dotted_only = !text.starts_with('+')
            && text.contains('.')
            && !text.contains([' ', '(', ')', '-'])
            && text.split('.').count() >= 4;
        if after_ok
            && (10..=15).contains(&digits)
            && (seps >= 2 || text.starts_with('+'))
            && !iso_date
            && !dotted_only
            && !text.contains("  ")
        {
            // A leading "+" keeps the old rule (international form, even for +1).
            let nanp = !text.starts_with('+') && is_nanp(&digit_bytes);
            out.push(Span {
                start: i,
                end: span_end,
                kind: RedactionKind::Phone,
                confidence: if nanp && seps >= 2 { 0.7 } else { 0.5 },
            });
            i = span_end.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod phone_tests {
    use super::*;

    fn phones(s: &str) -> Vec<(String, f32)> {
        let c: Vec<char> = s.chars().collect();
        find_phones(&c)
            .into_iter()
            .map(|p| (c[p.start..p.end].iter().collect(), p.confidence))
            .collect()
    }

    #[test]
    fn nanp_forms_get_higher_confidence() {
        for t in [
            "(415) 555-0132",
            "415.555.0132",
            "415-555-0132",
            "1-800-555-0132",
            "1 (415) 555-0132",
        ] {
            assert_eq!(
                phones(&format!("call {t} now")),
                [(t.to_string(), 0.7)],
                "{t}"
            );
        }
    }

    #[test]
    fn non_nanp_and_ordinary_numbers_keep_the_old_confidence() {
        // Exchange starting with 0, N11 exchange, area code starting with 1, international.
        for t in [
            "415-055-0132",
            "415-911-0132",
            "115-555-0132",
            "+44 20 7946 0958",
        ] {
            assert_eq!(phones(t), [(t.to_string(), 0.5)], "{t}");
        }
        // A leading "+" keeps the old rule, even for +1.
        assert_eq!(
            phones("+1 415 555 0132"),
            [("+1 415 555 0132".to_string(), 0.5)]
        );
    }

    #[test]
    fn extensions_are_part_of_the_span() {
        for t in [
            "(415) 555-0132 ext. 123",
            "415-555-0132 x123",
            "415-555-0132x45",
            "415-555-0132 #7",
            "415-555-0132 Ext 99",
        ] {
            assert_eq!(phones(&format!("tel {t}.")), [(t.to_string(), 0.7)], "{t}");
        }
        // Not extensions: no digits, or too many.
        for t in ["415-555-0132 xylophone", "415-555-0132 x1234567"] {
            assert_eq!(phones(t), [("415-555-0132".to_string(), 0.7)], "{t}");
        }
    }

    #[test]
    fn dates_times_addresses_versions_and_long_runs_are_not_phones() {
        for t in [
            "2026-10-23 14:15:30",
            "2026-10-23T14:15:30Z",
            "192.0.2.123",
            "198.51.100.200 and 203.0.113.9",
            "10.15.7.1234",
            "1.2.3.4567890",
            "12345678901234567890",
            "version 1.2.3",
        ] {
            assert!(phones(t).is_empty(), "{t}: {:?}", phones(t));
        }
    }
}
