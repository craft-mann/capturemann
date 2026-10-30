//! Financial identifiers: payment cards (and the expiry and code beside them), ABA routing
//! numbers and IBANs.

use super::{Flat, Located, Span};

/// Payment card numbers (Luhn, issuer prefix, length). Filled in by the 0.2.5 rules work.
pub fn find_cards(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// IBANs (mod 97). Filled in by the 0.2.5 rules work.
pub fn find_ibans(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// ABA routing numbers (checksum). Filled in by the 0.2.5 rules work.
pub fn find_abas(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// Expiry dates and security codes beside a detected card number. Filled in by the 0.2.5 rules work.
pub fn find_card_extras(_lines: &[Flat], _found: &[Located]) -> Vec<Located> {
    Vec::new()
}
