//! Person and provider identifiers: SSN and ITIN, EIN, NPI, DEA, Medicare Beneficiary ID,
//! passport, VIN and IMEI.
//!
//! Only structure and checksums are used: no label words. A bare run of digits that has no
//! checksum and no separators (nine digits for an SSN or a passport) is never proposed, because
//! nothing on the line could confirm it.

use super::Span;
use crate::ocr::detect::RedactionKind;

// ---------------------------------------------------------------------------------------------
// Shared helpers.

/// Whether `c[s..e]` stands alone: the characters on both sides are not letters or digits, and
/// not a joiner that continues a longer token (`-` `.` `/` ... followed by another letter or
/// digit, so `2026-10-30` or `v1.2.3` do not count as a value inside a bigger shape).
fn stands_alone(c: &[char], s: usize, e: usize, joiners: &[char]) -> bool {
    if s > 0 {
        let p = c[s - 1];
        if p.is_alphanumeric() {
            return false;
        }
        if joiners.contains(&p) && s >= 2 && c[s - 2].is_alphanumeric() {
            return false;
        }
    }
    if e < c.len() {
        let n = c[e];
        if n.is_alphanumeric() {
            return false;
        }
        if joiners.contains(&n) && e + 1 < c.len() && c[e + 1].is_alphanumeric() {
            return false;
        }
    }
    true
}

/// Joiners for numbers: a dash, dot, slash, comma or colon between digit groups.
const NUM_JOINERS: &[char] = &['-', '.', '/', ',', ':'];
/// Joiners for alphanumeric codes.
const CODE_JOINERS: &[char] = &['-', '.', '_', '/', '@'];

/// Reads digit groups of the given lengths from `at`, each pair separated by exactly one of
/// `seps`. Returns the end index and the digits.
fn digit_groups(
    c: &[char],
    at: usize,
    groups: &[usize],
    seps: &[char],
) -> Option<(usize, Vec<u32>)> {
    let mut i = at;
    let mut digits = Vec::new();
    for (g, &len) in groups.iter().enumerate() {
        if g > 0 {
            if i < c.len() && seps.contains(&c[i]) {
                i += 1;
            } else {
                return None;
            }
        }
        for _ in 0..len {
            let ch = *c.get(i)?;
            if !ch.is_ascii_digit() {
                return None;
            }
            digits.push(ch.to_digit(10)?);
            i += 1;
        }
    }
    Some((i, digits))
}

/// Scans `c` for digit-group shapes (non-overlapping), returning start, end and digits of each
/// that stands alone.
fn scan_groups(c: &[char], groups: &[usize], seps: &[char]) -> Vec<(usize, usize, Vec<u32>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if c[i].is_ascii_digit() {
            if let Some((e, d)) = digit_groups(c, i, groups, seps) {
                if stands_alone(c, i, e, NUM_JOINERS) {
                    out.push((i, e, d));
                    i = e;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// The Luhn (mod 10) check over digits (ISO/IEC 7812-1, Annex B).
fn luhn_ok(digits: &[u32]) -> bool {
    let mut sum = 0;
    for (i, &d) in digits.iter().rev().enumerate() {
        let mut v = d;
        if i % 2 == 1 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    sum % 10 == 0
}

fn in_ranges(v: u32, ranges: &[(u32, u32)]) -> bool {
    ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&v))
}

fn span(start: usize, end: usize, kind: RedactionKind, confidence: f32) -> Span {
    Span {
        start,
        end,
        kind,
        confidence,
    }
}

// ---------------------------------------------------------------------------------------------
// SSN and ITIN.

/// ITIN group numbers (IRS, "Individual Taxpayer Identification Number", Pub. 1915 and the
/// Internal Revenue Manual 3.21.263): 50-65, 70-88, 90-92 and 94-99.
const ITIN_GROUPS: &[(u32, u32)] = &[(50, 65), (70, 88), (90, 92), (94, 99)];

/// SSN and ITIN written `NNN-NN-NNNN` or `NNN NN NNNN` (single dash or space, mixed allowed).
/// Source for the issuable ranges: SSA, "Social Security Number Randomization" (area not 000,
/// 666 or 900-999; group not 00; serial not 0000). An area of 9xx with an ITIN group is an
/// ITIN (IRS Pub. 1915). A bare nine-digit run and masked forms give nothing.
pub fn find_ssns(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    for (s, e, d) in scan_groups(c, &[3, 2, 4], &['-', ' ']) {
        let area = d[0] * 100 + d[1] * 10 + d[2];
        let group = d[3] * 10 + d[4];
        let serial = d[5] * 1000 + d[6] * 100 + d[7] * 10 + d[8];
        if group == 0 || serial == 0 || area == 0 {
            continue;
        }
        if area >= 900 {
            if in_ranges(group, ITIN_GROUPS) {
                out.push(span(s, e, RedactionKind::Itin, 0.8));
            }
        } else if area != 666 {
            out.push(span(s, e, RedactionKind::Ssn, 0.85));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// EIN.

/// Valid IRS campus prefixes of an EIN (IRS, "How EIN numbers are assigned and valid EIN
/// prefixes"): 01-06, 10-16, 20-27, 30-39, 40-48, 50-59, 60-68, 71-77, 80-88, 90-95, 98, 99.
const EIN_PREFIXES: &[(u32, u32)] = &[
    (1, 6),
    (10, 16),
    (20, 27),
    (30, 39),
    (40, 48),
    (50, 59),
    (60, 68),
    (71, 77),
    (80, 88),
    (90, 95),
    (98, 99),
];

/// EIN: two digits, a dash, seven digits, with a valid campus prefix.
pub fn find_eins(c: &[char]) -> Vec<Span> {
    scan_groups(c, &[2, 7], &['-'])
        .into_iter()
        .filter(|(_, _, d)| in_ranges(d[0] * 10 + d[1], EIN_PREFIXES))
        .map(|(s, e, _)| span(s, e, RedactionKind::Ein, 0.5))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// NPI.

/// The constant prefix of the NPI check (CMS, "Final Rule: standard unique health identifier
/// for health care providers", Luhn formula with the prefix 80840 for health applications).
const NPI_PREFIX: [u32; 5] = [8, 0, 8, 4, 0];

/// NPI: ten digits starting with 1 or 2 whose Luhn check holds after the prefix 80840. About
/// one random ten-digit number in ten passes, hence the low confidence.
pub fn find_npis(c: &[char]) -> Vec<Span> {
    scan_groups(c, &[10], &[])
        .into_iter()
        .filter(|(_, _, d)| {
            let mut full = NPI_PREFIX.to_vec();
            full.extend_from_slice(d);
            (d[0] == 1 || d[0] == 2) && luhn_ok(&full)
        })
        .map(|(s, e, _)| span(s, e, RedactionKind::Npi, 0.4))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// DEA.

/// Registrant type letters of a DEA number (DEA Diversion Control Division, "Registrant
/// Validation": first letter A B F G M P R, X for DATA-waived practitioners).
const DEA_FIRST: &[char] = &['A', 'B', 'F', 'G', 'M', 'P', 'R', 'X'];

/// DEA registration number: two capital letters and seven digits; the sum of digits 1, 3, 5 plus
/// twice the sum of digits 2, 4, 6 ends in the seventh digit (DEA Pharmacist's Manual, "Valid
/// DEA Number").
pub fn find_deas(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 9 <= c.len() {
        let w = &c[i..i + 9];
        if DEA_FIRST.contains(&w[0])
            && w[1].is_ascii_uppercase()
            && w[2..].iter().all(|x| x.is_ascii_digit())
            && stands_alone(c, i, i + 9, CODE_JOINERS)
        {
            let d: Vec<u32> = w[2..].iter().filter_map(|x| x.to_digit(10)).collect();
            if (d[0] + d[2] + d[4] + 2 * (d[1] + d[3] + d[5])) % 10 == d[6] {
                out.push(span(i, i + 9, RedactionKind::Dea, 0.8));
                i += 9;
                continue;
            }
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// MBI.

/// Letters allowed in an MBI: A-Z without S, L, O, I, B and Z (CMS, "Understanding the
/// Medicare Beneficiary Identifier format").
fn mbi_letter(ch: char) -> bool {
    ch.is_ascii_uppercase() && !"SLOIBZ".contains(ch)
}

/// Whether the 11 characters follow the CMS position rules: 1 digit 1-9, 2 letter, 3 letter or
/// digit, 4 digit, 5 letter, 6 letter or digit, 7 digit, 8 letter, 9 letter, 10 digit, 11 digit.
fn mbi_ok(w: &[char]) -> bool {
    let digit = |ch: char| ch.is_ascii_digit();
    w.len() == 11
        && ('1'..='9').contains(&w[0])
        && mbi_letter(w[1])
        && (mbi_letter(w[2]) || digit(w[2]))
        && digit(w[3])
        && mbi_letter(w[4])
        && (mbi_letter(w[5]) || digit(w[5]))
        && digit(w[6])
        && mbi_letter(w[7])
        && mbi_letter(w[8])
        && digit(w[9])
        && digit(w[10])
}

/// Medicare Beneficiary Identifier: 11 characters by the CMS position rules, written plain or
/// with dashes in groups of 4-3-4.
pub fn find_mbis(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let mut end = None;
        if i + 11 <= c.len() && mbi_ok(&c[i..i + 11]) {
            end = Some(i + 11);
        } else if i + 13 <= c.len() && c[i + 4] == '-' && c[i + 8] == '-' {
            let w: Vec<char> = c[i..i + 13].iter().copied().filter(|&x| x != '-').collect();
            if w.len() == 11 && mbi_ok(&w) {
                end = Some(i + 13);
            }
        }
        if let Some(e) = end {
            if stands_alone(c, i, e, CODE_JOINERS) {
                out.push(span(i, e, RedactionKind::Mbi, 0.8));
                i = e;
                continue;
            }
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Passport.

/// US passport number, the letter-and-eight-digits form of the newer books (US Department of
/// State, passport book number format). The letter must not belong to a longer word. Nine bare
/// digits are never proposed.
pub fn find_passports(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 9 <= c.len() {
        if c[i].is_ascii_uppercase()
            && c[i + 1..i + 9].iter().all(|x| x.is_ascii_digit())
            && stands_alone(c, i, i + 9, CODE_JOINERS)
        {
            out.push(span(i, i + 9, RedactionKind::Passport, 0.5));
            i += 9;
        } else {
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// VIN.

/// Transliteration of letters to values (ISO 3779 / 49 CFR 565.15, Table V). I, O and Q are
/// not used in a VIN.
const VIN_VALUES: &[(char, u32)] = &[
    ('A', 1),
    ('B', 2),
    ('C', 3),
    ('D', 4),
    ('E', 5),
    ('F', 6),
    ('G', 7),
    ('H', 8),
    ('J', 1),
    ('K', 2),
    ('L', 3),
    ('M', 4),
    ('N', 5),
    ('P', 7),
    ('R', 9),
    ('S', 2),
    ('T', 3),
    ('U', 4),
    ('V', 5),
    ('W', 6),
    ('X', 7),
    ('Y', 8),
    ('Z', 9),
];

/// Position weights (49 CFR 565.15, Table IV); position 9 is the check digit itself.
const VIN_WEIGHTS: [u32; 17] = [8, 7, 6, 5, 4, 3, 2, 10, 0, 9, 8, 7, 6, 5, 4, 3, 2];

fn vin_value(ch: char) -> Option<u32> {
    if let Some(d) = ch.to_digit(10) {
        return Some(d);
    }
    VIN_VALUES.iter().find(|(l, _)| *l == ch).map(|&(_, v)| v)
}

fn vin_ok(w: &[char]) -> bool {
    if w.len() != 17
        || !w.iter().any(|x| x.is_ascii_alphabetic())
        || !w.iter().any(|x| x.is_ascii_digit())
    {
        return false;
    }
    let mut sum = 0;
    for (ch, wt) in w.iter().zip(VIN_WEIGHTS) {
        match vin_value(*ch) {
            Some(v) => sum += v * wt,
            None => return false,
        }
    }
    let check = match sum % 11 {
        10 => 'X',
        r => char::from_digit(r, 10).unwrap_or('?'),
    };
    w[8] == check
}

/// Vehicle identification number: 17 capital letters (no I, O, Q) and digits with a correct
/// check digit at position 9. At least one letter and one digit are required, since a run of
/// 17 digits is far more likely a different number.
pub fn find_vins(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 17 <= c.len() {
        if vin_ok(&c[i..i + 17]) && stands_alone(c, i, i + 17, CODE_JOINERS) {
            out.push(span(i, i + 17, RedactionKind::Vin, 0.85));
            i += 17;
        } else {
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// IMEI.

/// IMEI: 15 digits that pass Luhn, plain or as `NN-NNNNNN-NNNNNN-N` (dashes or spaces)
/// (GSMA TS.06 / 3GPP TS 23.003 section 6.2.1: TAC 8 digits, serial 6, check digit 1, Luhn).
/// A 15-digit number may also be a card; the card rule runs first and keeps the characters.
pub fn find_imeis(c: &[char]) -> Vec<Span> {
    let mut found = scan_groups(c, &[15], &[]);
    found.extend(scan_groups(c, &[2, 6, 6, 1], &['-', ' ']));
    found.sort_by_key(|f| f.0);
    let mut out: Vec<Span> = Vec::new();
    for (s, e, d) in found {
        if luhn_ok(&d) && out.last().is_none_or(|l| s >= l.end) {
            out.push(span(s, e, RedactionKind::Imei, 0.5));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::{suggest_redactions, SuggestOptions, TextLine};

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn texts(s: &str, spans: &[Span]) -> Vec<String> {
        let c = chars(s);
        spans
            .iter()
            .map(|p| c[p.start..p.end].iter().collect())
            .collect()
    }

    #[test]
    fn ssn_accepts_separated_forms() {
        for s in ["123-45-6789", "123 45 6789", "123-45 6789"] {
            let line = format!("id {s} ok");
            let r = find_ssns(&chars(&line));
            assert_eq!(texts(&line, &r), vec![s.to_string()], "{s}");
            assert_eq!(r[0].kind, RedactionKind::Ssn);
            assert_eq!(r[0].confidence, 0.85);
        }
    }

    #[test]
    fn ssn_rejects_unissuable_and_shapes() {
        for s in [
            "000-12-3456",
            "666-12-3456",
            "123-00-6789",
            "123-45-0000",
            "123456789",
            "***-**-6789",
            "XXX-XX-6789",
            "2026-10-30",
            "9123-45-6789",
            "123-45-67890",
            "123-45-6789-1",
            "555-12-3456.7",
            "900-12-3456",
            "987-93-1234",
        ] {
            assert!(find_ssns(&chars(s)).is_empty(), "{s}");
        }
    }

    #[test]
    fn itin_ranges() {
        let r = find_ssns(&chars("912-70-1234"));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, RedactionKind::Itin);
        assert_eq!(r[0].confidence, 0.8);
        assert_eq!(
            find_ssns(&chars("912 94 1234"))[0].kind,
            RedactionKind::Itin
        );
        assert!(find_ssns(&chars("912-69-1234")).is_empty());
        assert!(find_ssns(&chars("912-93-1234")).is_empty());
    }

    #[test]
    fn ein_accepts_valid_prefix_only() {
        let r = find_eins(&chars("EIN 12-3456789."));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].confidence, 0.5);
        for s in [
            "07-1234567",
            "69-1234567",
            "89-1234567",
            "96-1234567",
            "97-1234567",
        ] {
            assert!(find_eins(&chars(s)).is_empty(), "{s}");
        }
        for s in ["98-1234567", "99-1234567", "01-1234567"] {
            assert_eq!(find_eins(&chars(s)).len(), 1, "{s}");
        }
    }

    #[test]
    fn ein_rejects_longer_numbers_and_dates() {
        for s in [
            "2026-10-30",
            "12-34567890",
            "112-3456789",
            "12-3456789-1",
            "ab12-3456789",
        ] {
            assert!(find_eins(&chars(s)).is_empty(), "{s}");
        }
    }

    #[test]
    fn npi_checksum() {
        for s in ["1234567893", "2345678900", "1987654328"] {
            let r = find_npis(&chars(&format!("({s})")));
            assert_eq!(r.len(), 1, "{s}");
            assert_eq!(r[0].confidence, 0.4);
        }
        for s in [
            "1234567890",
            "3234567891",
            "123456789",
            "12345678931",
            "1234567893-1",
            "a1234567893",
        ] {
            assert!(find_npis(&chars(s)).is_empty(), "{s}");
        }
        // The first digit must be 1 or 2 even when the checksum holds.
        let mut seen = false;
        for k in 0..10 {
            let s = format!("323456789{k}");
            let mut full = NPI_PREFIX.to_vec();
            full.extend(s.chars().filter_map(|x| x.to_digit(10)));
            if luhn_ok(&full) {
                seen = true;
                assert!(find_npis(&chars(&s)).is_empty());
            }
        }
        assert!(seen);
    }

    #[test]
    fn dea_checksum() {
        for s in ["AB1234563", "MK2468103"] {
            let r = find_deas(&chars(&format!("DEA {s}")));
            assert_eq!(r.len(), 1, "{s}");
            assert_eq!(r[0].confidence, 0.8);
        }
        for s in [
            "AB1234564",
            "CB1234563",
            "AB123456",
            "AB12345633",
            "xAB1234563",
            "ab1234563",
        ] {
            assert!(find_deas(&chars(s)).is_empty(), "{s}");
        }
    }

    #[test]
    fn mbi_positions() {
        for s in ["2AC3DE4FG56", "2AC3-DE4-FG56", "9KT4V19WY00"] {
            let line = format!("[{s}]");
            let r = find_mbis(&chars(&line));
            assert_eq!(texts(&line, &r), vec![s.to_string()], "{s}");
            assert_eq!(r[0].confidence, 0.8);
        }
        for s in [
            "0AC3DE4FG56",
            "2SC3DE4FG56",
            "2AC3DE4FG5",
            "2AC3DE4FG567",
            "2AC3DE4FGA6",
            "2AC3DB4FG56",
            "2AC3-DE4-FG56-1",
            "x2AC3DE4FG56",
        ] {
            assert!(find_mbis(&chars(s)).is_empty(), "{s}");
        }
    }

    #[test]
    fn passport_letter_and_eight_digits() {
        let r = find_passports(&chars("No. C03005988 issued"));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].confidence, 0.5);
        for s in [
            "003005988",
            "C030059881",
            "AC03005988",
            "c03005988",
            "C0300598",
            "v1234567890",
        ] {
            assert!(find_passports(&chars(s)).is_empty(), "{s}");
        }
    }

    #[test]
    fn vin_check_digit() {
        let r = find_vins(&chars("VIN: JT2BG22K810123456"));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].confidence, 0.85);
        for s in [
            "JT2BG22K110123456",
            "JT2BG22K8101234567",
            "JT2BG22K81012345",
            "JT2BG22K8I0123456",
            "12345678901234567",
            "xJT2BG22K810123456",
            "JT2BG22K810123456-A",
        ] {
            assert!(find_vins(&chars(s)).is_empty(), "{s}");
        }
    }

    #[test]
    fn imei_luhn() {
        let r = find_imeis(&chars("IMEI 490154203237518"));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].confidence, 0.5);
        assert_eq!(find_imeis(&chars("49-015420-323751-8")).len(), 1);
        assert_eq!(find_imeis(&chars("49 015420 323751 8")).len(), 1);
        for s in [
            "490154203237519",
            "4901542032375180",
            "49015420323751",
            "49-015420-323751-9",
            "4901 5420 3237 518",
        ] {
            assert!(find_imeis(&chars(s)).is_empty(), "{s}");
        }
    }

    #[test]
    fn negatives_across_rules() {
        let samples = [
            "part 123456789",
            "order 4111111111111112",
            "550e8400-e29b-41d4-a716-446655440000",
            "3f786850e387550fdab836ed7e6dc881de23001b",
            "version 1.2.3-45",
            "1Z999AA10123456784",
            "2026-10-30 12:45:10",
            "***-**-6789",
        ];
        for s in samples {
            let c = chars(s);
            for f in [
                find_ssns,
                find_eins,
                find_npis,
                find_deas,
                find_mbis,
                find_passports,
                find_vins,
                find_imeis,
            ] {
                assert!(f(&c).is_empty(), "{s}");
            }
        }
    }

    #[test]
    fn red2_through_suggest_redactions() {
        let lines = vec![
            TextLine::laid_out("SSN 123-45-6789", 10.0, 10.0, 8.0, 14.0),
            TextLine::laid_out("part 123456789", 10.0, 40.0, 8.0, 14.0),
        ];
        let out = suggest_redactions(&lines, (400, 100), &SuggestOptions::default());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, RedactionKind::Ssn);

        let masked = vec![TextLine::laid_out("SSN ***-**-6789", 10.0, 10.0, 8.0, 14.0)];
        assert!(suggest_redactions(&masked, (400, 100), &SuggestOptions::default()).is_empty());

        let mixed = vec![
            TextLine::laid_out("SSN 123-45-6789", 10.0, 10.0, 8.0, 14.0),
            TextLine::laid_out("EIN 12-3456789", 10.0, 40.0, 8.0, 14.0),
        ];
        let only = SuggestOptions {
            kinds: vec![RedactionKind::Ssn],
        };
        let out = suggest_redactions(&mixed, (400, 100), &only);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, RedactionKind::Ssn);
    }
}
