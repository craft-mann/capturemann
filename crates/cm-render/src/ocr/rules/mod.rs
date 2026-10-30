//! The detection rules of Auto-redact, one file per group. Every rule is pure Rust over the
//! characters of recognized text, so it is tested with fixtures and no OCR engine.
//!
//! * [`LineRule`]s look at one line and return spans inside it.
//! * [`PageRule`]s see every line plus the spans the line rules found, for things that depend on
//!   a neighbour (an expiry date beside a card number, a private key block over many lines).
//!
//! The order of [`LINE_RULES`] is the priority when two spans overlap: the first one found keeps
//! the characters. A rule runs only when one of its `kinds` is wanted, and spans of kinds that
//! were not asked for are dropped. To add a rule, write a function in the file of its group and
//! put it in the list here; each rule cites its public source in a comment and has a test.

use super::detect::RedactionKind;
use super::TextLine;
use super::TextWord;
use cm_core::Rc;

pub mod contact;
pub mod financial;
pub mod identity;
pub mod network;
pub mod secrets;

/// A found piece of text inside one line: `start..end` are indexes into [`Flat::chars`].
#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: RedactionKind,
    pub confidence: f32,
}

/// A span and the line it is on (page rules may place spans on several lines).
#[derive(Debug, Clone, Copy)]
pub struct Located {
    pub line: usize,
    pub span: Span,
}

/// A line rule: the characters of one line in, spans out.
pub type LineRule = fn(&[char]) -> Vec<Span>;

/// A page rule: every line, and everything the line rules found, in. Spans out. The lines are in
/// reading order and `Flat::cells` carries each character's box (a rule can check that two lines
/// are neighbours by comparing boxes).
pub type PageRule = fn(&[Flat], &[Located]) -> Vec<Located>;

/// One entry of a rule list: the kinds it can produce and the function.
pub struct Entry<F> {
    pub kinds: &'static [RedactionKind],
    pub run: F,
}

/// Line rules in priority order.
pub const LINE_RULES: &[Entry<LineRule>] = &[
    Entry {
        kinds: &[RedactionKind::UrlSecret],
        run: secrets::find_url_secrets,
    },
    Entry {
        kinds: &[RedactionKind::SecretAssignment],
        run: secrets::find_secret_assignments,
    },
    Entry {
        kinds: &[RedactionKind::Email],
        run: contact::find_emails,
    },
    Entry {
        kinds: &[RedactionKind::Token],
        run: secrets::find_tokens,
    },
    Entry {
        kinds: &[RedactionKind::Card],
        run: financial::find_cards,
    },
    Entry {
        kinds: &[RedactionKind::Iban],
        run: financial::find_ibans,
    },
    Entry {
        kinds: &[RedactionKind::Aba],
        run: financial::find_abas,
    },
    Entry {
        kinds: &[RedactionKind::Ssn, RedactionKind::Itin],
        run: identity::find_ssns,
    },
    Entry {
        kinds: &[RedactionKind::Ein],
        run: identity::find_eins,
    },
    Entry {
        kinds: &[RedactionKind::Npi],
        run: identity::find_npis,
    },
    Entry {
        kinds: &[RedactionKind::Dea],
        run: identity::find_deas,
    },
    Entry {
        kinds: &[RedactionKind::Mbi],
        run: identity::find_mbis,
    },
    Entry {
        kinds: &[RedactionKind::Passport],
        run: identity::find_passports,
    },
    Entry {
        kinds: &[RedactionKind::Vin],
        run: identity::find_vins,
    },
    Entry {
        kinds: &[RedactionKind::Imei],
        run: identity::find_imeis,
    },
    Entry {
        kinds: &[RedactionKind::IpAddress],
        run: network::find_ips,
    },
    Entry {
        kinds: &[RedactionKind::MacAddress],
        run: network::find_macs,
    },
    Entry {
        kinds: &[RedactionKind::Phone],
        run: contact::find_phones,
    },
];

/// Page rules, run after all line rules, in priority order.
pub const PAGE_RULES: &[Entry<PageRule>] = &[
    Entry {
        kinds: &[RedactionKind::PrivateKey],
        run: secrets::find_private_keys,
    },
    Entry {
        kinds: &[RedactionKind::CardExtra],
        run: financial::find_card_extras,
    },
];

/// A line flattened to characters, each with the box it occupies (None for the separator
/// between words).
pub struct Flat {
    pub chars: Vec<char>,
    pub cells: Vec<Option<Rc>>,
}

pub fn flatten(line: &TextLine) -> Flat {
    let mut chars = Vec::new();
    let mut cells = Vec::new();
    let spread = |text: &str, rect: Rc, chars: &mut Vec<char>, cells: &mut Vec<Option<Rc>>| {
        let n = text.chars().count().max(1);
        let w = rect.w / n as f64;
        for (i, c) in text.chars().enumerate() {
            chars.push(c);
            cells.push(Some(Rc::new(rect.x + w * i as f64, rect.y, w, rect.h)));
        }
    };
    let words: Vec<&TextWord> = line
        .words
        .iter()
        .filter(|w| !w.text.trim().is_empty())
        .collect();
    if words.is_empty() {
        spread(&line.text, line.rect, &mut chars, &mut cells);
    } else {
        for (i, w) in words.iter().enumerate() {
            if i > 0 {
                chars.push(' ');
                cells.push(None);
            }
            spread(w.text.trim(), w.rect, &mut chars, &mut cells);
        }
    }
    Flat { chars, cells }
}
