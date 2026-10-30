//! Network identifiers: IPv4 (IPv6 joins it here) and MAC addresses.

use super::Span;
use crate::ocr::detect::RedactionKind;

pub fn find_ips(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if !c[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < c.len() && (c[j].is_ascii_digit() || c[j] == '.') {
            j += 1;
        }
        let run: String = c[i..j].iter().collect();
        let run = run.trim_end_matches('.');
        let parts: Vec<&str> = run.split('.').collect();
        let ok = parts.len() == 4
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 3 && p.parse::<u16>().is_ok_and(|v| v <= 255));
        let before_ok = i == 0 || !c[i - 1].is_ascii_alphanumeric();
        if ok && before_ok {
            out.push(Span {
                start: i,
                end: i + run.chars().count(),
                kind: RedactionKind::IpAddress,
                confidence: 0.5,
            });
        }
        i = j.max(i + 1);
    }
    out
}

/// MAC addresses (filled in by the 0.2.5 rules work).
pub fn find_macs(_c: &[char]) -> Vec<Span> {
    Vec::new()
}
