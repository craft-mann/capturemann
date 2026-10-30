//! Financial identifiers: payment cards (and the expiry and code beside them), ABA routing
//! numbers and IBANs. Every rule is a structure-and-checksum test, so none needs a label or a
//! word list.

use super::{Flat, Located, Span};
use crate::ocr::RedactionKind;
use cm_core::Rc;

// ---------------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------------

/// A maximal run of ASCII digits: `start..end` are char indexes.
type Group = (usize, usize);

fn digit_groups(c: &[char]) -> Vec<Group> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if c[i].is_ascii_digit() {
            let s = i;
            while i < c.len() && c[i].is_ascii_digit() {
                i += 1;
            }
            out.push((s, i));
        } else {
            i += 1;
        }
    }
    out
}

fn word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// True when the character at `idx` (outside a candidate) would glue the candidate to a longer
/// token: a letter, digit or underscore, or a `. , / : -` that has a digit on its far side
/// (a decimal, a date, a time, a dashed number).
fn glued(c: &[char], idx: Option<usize>, beyond: Option<usize>) -> bool {
    let Some(i) = idx else { return false };
    let ch = c[i];
    if word_char(ch) {
        return true;
    }
    matches!(ch, '.' | ',' | '/' | ':' | '-') && beyond.is_some_and(|b| c[b].is_ascii_digit())
}

/// A candidate `s..e` is isolated when neither side is glued to a longer token.
fn isolated(c: &[char], s: usize, e: usize) -> bool {
    let before = s.checked_sub(1);
    let before2 = s.checked_sub(2);
    let after = (e < c.len()).then_some(e);
    let after2 = (e + 1 < c.len()).then_some(e + 1);
    !glued(c, before, before2) && !glued(c, after, after2)
}

fn digits_of(c: &[char], groups: &[Group]) -> Vec<u8> {
    groups
        .iter()
        .flat_map(|&(s, e)| c[s..e].iter().map(|ch| *ch as u8 - b'0'))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Payment cards
// ---------------------------------------------------------------------------------------------

/// The Luhn (mod 10) check, ISO/IEC 7812-1 Annex B: from the right, double every second digit,
/// subtract 9 when the double is above 9, and the sum must be a multiple of 10.
fn luhn_ok(d: &[u8]) -> bool {
    let sum: u32 = d
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &x)| {
            let x = u32::from(x);
            if i % 2 == 1 {
                let y = x * 2;
                if y > 9 {
                    y - 9
                } else {
                    y
                }
            } else {
                x
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

/// Issuer identification ranges: (lowest prefix, highest prefix, prefix digits, lengths).
/// Source: the public IIN ranges documented by the card networks and summarised on Wikipedia's
/// "Payment card number" page (Visa 4, Mastercard 51-55 and 2221-2720, American Express 34/37,
/// Discover 6011/65/644-649, Diners Club 300-305/36/38, JCB 3528-3589, UnionPay 62).
const ISSUERS: &[(u32, u32, usize, &[usize])] = &[
    (4, 4, 1, &[13, 16, 19]),
    (51, 55, 2, &[16]),
    (2221, 2720, 4, &[16]),
    (34, 34, 2, &[15]),
    (37, 37, 2, &[15]),
    (6011, 6011, 4, &[16, 17, 18, 19]),
    (65, 65, 2, &[16, 17, 18, 19]),
    (644, 649, 3, &[16, 17, 18, 19]),
    (300, 305, 3, &[14]),
    (36, 36, 2, &[14]),
    (38, 38, 2, &[14]),
    (3528, 3589, 4, &[16, 17, 18, 19]),
    (62, 62, 2, &[16, 17, 18, 19]),
];

fn known_issuer(d: &[u8]) -> bool {
    ISSUERS.iter().any(|&(lo, hi, n, lens)| {
        if d.len() < n || !lens.contains(&d.len()) {
            return false;
        }
        let p = d[..n].iter().fold(0u32, |a, &x| a * 10 + u32::from(x));
        (lo..=hi).contains(&p)
    })
}

/// Payment card numbers (Luhn, issuer prefix, length). Groups of digits separated by single
/// spaces or dashes are read as one number; a number that is part of a longer digit run or a
/// letter-digit token is skipped, and so is a masked one (fewer than 13 digits are visible).
pub fn find_cards(c: &[char]) -> Vec<Span> {
    let groups = digit_groups(c);
    let mut out = Vec::new();
    let mut i = 0;
    while i < groups.len() {
        // The chain of groups joined by exactly one space or dash.
        let mut j = i;
        while j + 1 < groups.len()
            && groups[j + 1].0 == groups[j].1 + 1
            && matches!(c[groups[j].1], ' ' | '-')
        {
            j += 1;
        }
        // Longest valid contiguous range of groups inside the chain, from the left.
        let mut a = i;
        while a <= j {
            let mut found = None;
            for b in (a..=j).rev() {
                let (s, e) = (groups[a].0, groups[b].1);
                let total: usize = groups[a..=b].iter().map(|g| g.1 - g.0).sum();
                if !(13..=19).contains(&total) || !isolated(c, s, e) {
                    continue;
                }
                let d = digits_of(c, &groups[a..=b]);
                if known_issuer(&d) && luhn_ok(&d) {
                    found = Some((b, s, e));
                    break;
                }
            }
            match found {
                Some((b, s, e)) => {
                    out.push(Span {
                        start: s,
                        end: e,
                        kind: RedactionKind::Card,
                        confidence: 0.9,
                    });
                    a = b + 1;
                }
                None => a += 1,
            }
        }
        i = j + 1;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// IBAN
// ---------------------------------------------------------------------------------------------

/// Country code and total IBAN length. Source: the SWIFT IBAN registry (ISO 13616-2).
const IBAN_LENGTHS: &[(&str, usize)] = &[
    ("AD", 24),
    ("AE", 23),
    ("AL", 28),
    ("AT", 20),
    ("AZ", 28),
    ("BA", 20),
    ("BE", 16),
    ("BG", 22),
    ("BH", 22),
    ("BR", 29),
    ("BY", 28),
    ("CH", 21),
    ("CR", 22),
    ("CY", 28),
    ("CZ", 24),
    ("DE", 22),
    ("DK", 18),
    ("DO", 28),
    ("EE", 20),
    ("EG", 29),
    ("ES", 24),
    ("FI", 18),
    ("FO", 18),
    ("FR", 27),
    ("GB", 22),
    ("GE", 22),
    ("GI", 23),
    ("GL", 18),
    ("GR", 27),
    ("GT", 28),
    ("HR", 21),
    ("HU", 28),
    ("IE", 22),
    ("IL", 23),
    ("IQ", 23),
    ("IS", 26),
    ("IT", 27),
    ("JO", 30),
    ("KW", 30),
    ("KZ", 20),
    ("LB", 28),
    ("LC", 32),
    ("LI", 21),
    ("LT", 20),
    ("LU", 20),
    ("LV", 21),
    ("MC", 27),
    ("MD", 24),
    ("ME", 22),
    ("MK", 19),
    ("MR", 27),
    ("MT", 31),
    ("MU", 30),
    ("NL", 18),
    ("NO", 15),
    ("PK", 24),
    ("PL", 28),
    ("PS", 29),
    ("PT", 25),
    ("QA", 29),
    ("RO", 24),
    ("RS", 22),
    ("SA", 24),
    ("SC", 31),
    ("SE", 24),
    ("SI", 19),
    ("SK", 24),
    ("SM", 27),
    ("ST", 25),
    ("SV", 28),
    ("TL", 23),
    ("TN", 24),
    ("TR", 26),
    ("UA", 29),
    ("VA", 22),
    ("VG", 24),
    ("XK", 20),
];

/// The mod-97 test of ISO 13616: move the first four characters to the end, turn letters into
/// numbers (A=10 .. Z=35), and the whole number must leave remainder 1 when divided by 97.
fn iban_mod97(s: &[char]) -> bool {
    if s.len() < 5 {
        return false;
    }
    let mut rem: u32 = 0;
    for &ch in s[4..].iter().chain(s[..4].iter()) {
        let v = match ch {
            '0'..='9' => ch as u32 - '0' as u32,
            'A'..='Z' => ch as u32 - 'A' as u32 + 10,
            _ => return false,
        };
        rem = if v >= 10 {
            (rem * 100 + v) % 97
        } else {
            (rem * 10 + v) % 97
        };
    }
    rem == 1
}

/// IBANs (mod 97), written solid or in groups of four separated by single spaces.
pub fn find_ibans(c: &[char]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= c.len() {
        let starts = c[i].is_ascii_uppercase()
            && c[i + 1].is_ascii_uppercase()
            && c[i + 2].is_ascii_digit()
            && c[i + 3].is_ascii_digit()
            && (i == 0 || !word_char(c[i - 1]));
        let len = starts.then(|| {
            let code: String = c[i..i + 2].iter().collect();
            IBAN_LENGTHS.iter().find(|(k, _)| *k == code).map(|x| x.1)
        });
        if let Some(Some(len)) = len {
            // Read `len` alphanumerics, allowing a single space after every fourth one.
            let mut s: Vec<char> = Vec::with_capacity(len);
            let mut p = i;
            while p < c.len() && s.len() < len {
                if c[p].is_ascii_uppercase() || c[p].is_ascii_digit() {
                    s.push(c[p]);
                    p += 1;
                } else if c[p] == ' '
                    && s.len().is_multiple_of(4)
                    && p + 1 < c.len()
                    && c[p + 1] != ' '
                {
                    p += 1;
                } else {
                    break;
                }
            }
            if s.len() == len && (p >= c.len() || !word_char(c[p])) && iban_mod97(&s) {
                out.push(Span {
                    start: i,
                    end: p,
                    kind: RedactionKind::Iban,
                    confidence: 0.85,
                });
                i = p;
                continue;
            }
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// ABA routing numbers
// ---------------------------------------------------------------------------------------------

/// Valid first-two-digit ranges (Federal Reserve routing symbols): 00-12 (Fed districts and
/// thrifts), 21-32 (thrift), 61-72 (electronic), 80 (traveler's cheques).
/// Source: the public description of the ABA routing transit number.
const ABA_PREFIXES: &[(u32, u32)] = &[(0, 12), (21, 32), (61, 72), (80, 80)];

/// The ABA checksum: 3(d1+d4+d7) + 7(d2+d5+d8) + (d3+d6+d9) must be a multiple of 10.
fn aba_ok(d: &[u8]) -> bool {
    if d.len() != 9 {
        return false;
    }
    let prefix = u32::from(d[0]) * 10 + u32::from(d[1]);
    if !ABA_PREFIXES
        .iter()
        .any(|&(lo, hi)| (lo..=hi).contains(&prefix))
    {
        return false;
    }
    let s = |i: usize| u32::from(d[i]);
    (3 * (s(0) + s(3) + s(6)) + 7 * (s(1) + s(4) + s(7)) + (s(2) + s(5) + s(8))).is_multiple_of(10)
}

/// ABA routing numbers: exactly nine digits that pass the checksum and the prefix ranges, not
/// part of a longer run or a dashed, slashed or decimal shape. A bare nine-digit number passes
/// the checksum one time in ten, hence the low confidence.
pub fn find_abas(c: &[char]) -> Vec<Span> {
    digit_groups(c)
        .into_iter()
        .filter(|&(s, e)| e - s == 9 && isolated(c, s, e))
        .filter(|&(s, e)| {
            let d: Vec<u8> = c[s..e].iter().map(|ch| *ch as u8 - b'0').collect();
            aba_ok(&d)
        })
        .map(|(s, e)| Span {
            start: s,
            end: e,
            kind: RedactionKind::Aba,
            confidence: 0.4,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Card extras (expiry and security code)
// ---------------------------------------------------------------------------------------------

/// Furthest a neighbouring line may be from the card line, in line heights.
const MAX_LINE_GAP: f64 = 2.5;
/// Two-digit expiry years accepted (2015 to 2049).
const EXPIRY_YEARS: (u32, u32) = (15, 49);

fn num(c: &[char]) -> u32 {
    c.iter().fold(0, |a, ch| a * 10 + (*ch as u32 - '0' as u32))
}

/// `MM/YY`, `MM/YYYY` or `MM-YY` with month 01-12 (the layout printed on payment cards).
fn find_expiries(c: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let groups = digit_groups(c);
    for w in groups.windows(2) {
        let (m, y) = (w[0], w[1]);
        if m.1 + 1 != y.0 || !matches!(c[m.1], '/' | '-') || m.1 - m.0 != 2 {
            continue;
        }
        let month = num(&c[m.0..m.1]);
        if !(1..=12).contains(&month) {
            continue;
        }
        let year_ok = match y.1 - y.0 {
            2 => (EXPIRY_YEARS.0..=EXPIRY_YEARS.1).contains(&num(&c[y.0..y.1])),
            4 => (2015..=2049).contains(&num(&c[y.0..y.1])),
            _ => false,
        };
        if year_ok && isolated(c, m.0, y.1) {
            out.push((m.0, y.1));
        }
    }
    out
}

/// A stand-alone number of exactly `n` digits.
fn find_codes(c: &[char], n: usize) -> Vec<(usize, usize)> {
    digit_groups(c)
        .into_iter()
        .filter(|&(s, e)| {
            e - s == n
                && isolated(c, s, e)
                && !(s > 0 && matches!(c[s - 1], '$' | '#'))
                && !(e < c.len() && c[e] == '%')
        })
        .collect()
}

/// The box of a whole line: (x0, y0, x1, y1).
fn line_box(f: &Flat) -> Option<(f64, f64, f64, f64)> {
    let mut it = f.cells.iter().flatten();
    let first: &Rc = it.next()?;
    let mut b = (first.x, first.y, first.x + first.w, first.y + first.h);
    for r in it {
        b = (
            b.0.min(r.x),
            b.1.min(r.y),
            b.2.max(r.x + r.w),
            b.3.max(r.y + r.h),
        );
    }
    Some(b)
}

/// Is line `other` vertically adjacent to `card` (within [`MAX_LINE_GAP`] line heights) and
/// overlapping it horizontally?
fn adjacent(card: (f64, f64, f64, f64), other: (f64, f64, f64, f64)) -> bool {
    let h = (card.3 - card.1).max(1.0);
    let gap = if other.1 >= card.3 {
        other.1 - card.3
    } else if card.1 >= other.3 {
        card.1 - other.3
    } else {
        0.0
    };
    gap <= MAX_LINE_GAP * h && other.0 < card.2 && card.0 < other.2
}

/// Expiry dates and security codes beside a detected card number: on the card's line, or on
/// the line directly before or after it when that line sits right under or over it and shares
/// its horizontal range. No label words are used; a lone `12/27` or `123` is never proposed.
pub fn find_card_extras(lines: &[Flat], found: &[Located]) -> Vec<Located> {
    let mut out: Vec<Located> = Vec::new();
    let mut seen: Vec<(usize, usize, usize)> = Vec::new();
    for card in found.iter().filter(|l| l.span.kind == RedactionKind::Card) {
        let Some(flat) = lines.get(card.line) else {
            continue;
        };
        let card_box = line_box(flat);
        let first = flat.chars[card.span.start..card.span.end]
            .iter()
            .filter(|ch| ch.is_ascii_digit())
            .take(2)
            .collect::<String>();
        let code_len = if first == "34" || first == "37" { 4 } else { 3 };
        let mut targets = vec![card.line];
        for other in [card.line.checked_sub(1), Some(card.line + 1)]
            .into_iter()
            .flatten()
        {
            if let (Some(cb), Some(ob)) = (card_box, lines.get(other).and_then(line_box)) {
                if adjacent(cb, ob) {
                    targets.push(other);
                }
            }
        }
        for line in targets {
            let chars = &lines[line].chars;
            let mut hits: Vec<(usize, usize)> = find_expiries(chars);
            let expiries = hits.clone();
            hits.extend(
                find_codes(chars, code_len)
                    .into_iter()
                    .filter(|&(s, e)| !expiries.iter().any(|&(a, b)| s < b && a < e)),
            );
            for (s, e) in hits {
                // Another card on the line owns its own characters.
                let inside_card = found.iter().any(|l| {
                    l.line == line
                        && l.span.kind == RedactionKind::Card
                        && s < l.span.end
                        && l.span.start < e
                });
                if inside_card || seen.contains(&(line, s, e)) {
                    continue;
                }
                seen.push((line, s, e));
                out.push(Located {
                    line,
                    span: Span {
                        start: s,
                        end: e,
                        kind: RedactionKind::CardExtra,
                        confidence: 0.8,
                    },
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::flatten;
    use super::*;
    use crate::ocr::{suggest_redactions, SuggestOptions, TextLine};

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    fn texts(spans: &[Span], s: &str) -> Vec<String> {
        let c = chars(s);
        spans
            .iter()
            .map(|x| c[x.start..x.end].iter().collect())
            .collect()
    }

    #[test]
    fn cards_accepted() {
        // Public test numbers of the card networks.
        for n in [
            "4111 1111 1111 1111",
            "4111111111111111",
            "4111-1111-1111-1111",
            "378282246310005",
            "3782 822463 10005",
            "5555555555554444",
            "2223003122003222",
            "6011111111111117",
            "30569309025904",
            "3530111333300000",
            "6200000000000005",
            "4222222222222",
        ] {
            let line = format!("card: {n} ok");
            let r = find_cards(&chars(&line));
            assert_eq!(texts(&r, &line), vec![n.to_string()], "{n}");
            assert!((r[0].confidence - 0.9).abs() < 1e-6);
        }
    }

    #[test]
    fn cards_rejected() {
        for line in [
            "4111 1111 1111 1112",      // fails Luhn
            "order 1234567812345678",   // 16 digits, no issuer
            "9111111111111111",         // no issuer
            "41111111111111111",        // 17 digits, Visa has no 17
            "A4111111111111111",        // letter-digit token
            "41111111111111111234",     // longer digit run
            "**** **** **** 1111",      // masked
            "XXXX-XXXX-XXXX-1111",      // masked
            "x 4111111111111111.5",     // decimal
            "1700000000000 1700000000", // timestamps
            "2024-01-02 12:30:45",      // date and time
        ] {
            assert!(find_cards(&chars(line)).is_empty(), "{line}");
        }
    }

    #[test]
    fn ibans() {
        for n in [
            "GB82 WEST 1234 5698 7654 32",
            "GB82WEST12345698765432",
            "DE89 3704 0044 0532 0130 00",
            "FR14 2004 1010 0505 0001 3M02 606",
            "NL91ABNA0417164300",
        ] {
            let line = format!("iban {n}.");
            assert_eq!(
                texts(&find_ibans(&chars(&line)), &line),
                vec![n.to_string()],
                "{n}"
            );
        }
        for line in [
            "GB82 WEST 1234 5698 7654 33", // bad check digits
            "GB82WEST1234569876543",       // too short
            "XX82WEST12345698765432",      // unknown country
            "xGB82WEST12345698765432",     // glued to a letter
            "gb82west12345698765432",      // lower case
        ] {
            assert!(find_ibans(&chars(line)).is_empty(), "{line}");
        }
    }

    #[test]
    fn abas() {
        for n in ["021000021", "011000015", "322271627"] {
            let line = format!("Routing {n}");
            let r = find_abas(&chars(&line));
            assert_eq!(texts(&r, &line), vec![n.to_string()], "{n}");
            assert!((r[0].confidence - 0.4).abs() < 1e-6);
        }
        for line in [
            "123456789",    // fails the checksum
            "021000022",    // one digit off
            "921000020",    // prefix out of range
            "1021000021",   // longer run
            "021000021-45", // dashed shape
            "02-1000021",   // not nine contiguous
            "id021000021",  // letter-digit token
            "1698765432",   // timestamp
            "2024-01-02",   // date
            "021000021.50", // decimal
        ] {
            assert!(find_abas(&chars(line)).is_empty(), "{line}");
        }
    }

    fn page(lines: &[(&str, f64)]) -> Vec<Flat> {
        lines
            .iter()
            .map(|(t, y)| flatten(&TextLine::laid_out(t, 100.0, *y, 10.0, 20.0)))
            .collect()
    }

    fn extras(lines: &[(&str, f64)]) -> Vec<(usize, String)> {
        let flats = page(lines);
        let mut found = Vec::new();
        for (line, f) in flats.iter().enumerate() {
            for span in find_cards(&f.chars) {
                found.push(Located { line, span });
            }
        }
        find_card_extras(&flats, &found)
            .into_iter()
            .map(|l| {
                (
                    l.line,
                    flats[l.line].chars[l.span.start..l.span.end]
                        .iter()
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn extras_same_and_neighbour_lines() {
        let r = extras(&[("4111 1111 1111 1111 12/27 123", 100.0)]);
        assert_eq!(r, vec![(0, "12/27".into()), (0, "123".into())]);
        let r = extras(&[("4111 1111 1111 1111", 100.0), ("01/2029 987", 130.0)]);
        assert_eq!(r, vec![(1, "01/2029".into()), (1, "987".into())]);
        // American Express wants four digits.
        let r = extras(&[("378282246310005 11-28 1234 567", 100.0)]);
        assert_eq!(r, vec![(0, "11-28".into()), (0, "1234".into())]);
    }

    #[test]
    fn extras_need_a_card_and_adjacency() {
        assert!(extras(&[("12/27 123", 100.0)]).is_empty());
        // Month 13, a full date and four digits for a Visa are not extras.
        assert!(extras(&[("4111111111111111 13/27 2025-12-27 1234", 100.0)]).is_empty());
        // A line far below the card is not a neighbour.
        assert!(extras(&[("4111111111111111", 100.0), ("12/27", 400.0)]).is_empty());
    }

    #[test]
    fn through_suggest_redactions() {
        let run = |t: &str| {
            suggest_redactions(
                &[TextLine::laid_out(t, 100.0, 100.0, 10.0, 20.0)],
                (1200, 800),
                &SuggestOptions::default(),
            )
        };
        // RED-2.AC4: card with expiry and code.
        let r = run("Card 4111 1111 1111 1111 12/27 123");
        assert_eq!(r.len(), 3, "{r:?}");
        assert_eq!(
            r.iter().filter(|s| s.kind == RedactionKind::Card).count(),
            1
        );
        assert_eq!(
            r.iter()
                .filter(|s| s.kind == RedactionKind::CardExtra)
                .count(),
            2
        );
        // Same expiry and code without a card: nothing.
        assert!(run("Valid thru 12/27 code 123").is_empty());
        // RED-2.AC2: a masked card: nothing.
        assert!(run("Card **** **** **** 1111 12/27 123").is_empty());
    }
}
