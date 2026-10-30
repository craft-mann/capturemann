//! Person and provider identifiers: SSN and ITIN, EIN, NPI, DEA, Medicare Beneficiary ID,
//! passport, VIN and IMEI.

use super::Span;

/// SSN and ITIN. Filled in by the 0.2.5 rules work.
pub fn find_ssns(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// EIN. Filled in by the 0.2.5 rules work.
pub fn find_eins(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// NPI. Filled in by the 0.2.5 rules work.
pub fn find_npis(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// DEA numbers. Filled in by the 0.2.5 rules work.
pub fn find_deas(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// Medicare Beneficiary Identifiers. Filled in by the 0.2.5 rules work.
pub fn find_mbis(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// US passport numbers. Filled in by the 0.2.5 rules work.
pub fn find_passports(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// VINs. Filled in by the 0.2.5 rules work.
pub fn find_vins(_c: &[char]) -> Vec<Span> {
    Vec::new()
}

/// IMEIs. Filled in by the 0.2.5 rules work.
pub fn find_imeis(_c: &[char]) -> Vec<Span> {
    Vec::new()
}
