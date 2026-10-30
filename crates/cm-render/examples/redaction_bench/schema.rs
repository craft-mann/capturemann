//! The contract between the generator (`gen/`) and the harness (`harness/`): the files of a
//! benchmark set. A set is a folder with `manifest.json` and the PNG files it names. Nothing in
//! it is a real person's data (see `tests/data/redaction/README.md`).

use serde::{Deserialize, Serialize};

/// `manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Bump when the format changes.
    pub version: u32,
    pub seed: u64,
    pub images: Vec<ImageEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEntry {
    /// PNG file name, relative to the set folder.
    pub file: String,
    /// Screen layout name, for example `bank_statement`, `env_file`, `terminal`.
    pub layout: String,
    /// `light` or `dark`.
    pub theme: String,
    /// 1 or 2 (device pixel ratio the picture was drawn at).
    pub scale: u32,
    /// Smallest text height used, in logical px.
    pub font_px: u32,
    /// `en` or `vi`.
    pub language: String,
    /// True when blur, JPEG compression or rescaling was applied after drawing.
    pub degraded: bool,
    /// `dev` or `held_out`. Thresholds are tuned on `dev` only; `held_out` is read once per release.
    pub split: String,
    /// What is in the picture that should be found. Empty for a negative image.
    pub truths: Vec<Truth>,
    /// Lookalikes that must NOT be found (negative images and the decoys inside positive ones).
    pub decoys: Vec<Decoy>,
}

/// One piece of sensitive text. `kind` is a `RedactionKind` name (`card`, `ssn`, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Truth {
    pub kind: String,
    /// The exact text as drawn.
    pub text: String,
    /// `[x, y, w, h]` in physical px of the PNG.
    pub rect: [f64; 4],
    /// The standard identifier it maps to, for the per-standard roll-up, for example `HIPAA 7`.
    pub standards: Vec<String>,
}

/// Text that looks like something sensitive but is not.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decoy {
    /// For example `order_number`, `uuid`, `git_hash`, `luhn_fail_16_digits`, `masked_ssn`.
    pub what: String,
    pub text: String,
    pub rect: [f64; 4],
}
