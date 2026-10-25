//! Small shared helpers: error mapping, ids, atomic writes, hashing, file names.

use cm_core::CmError;
use sha2::{Digest, Sha256};
use std::collections::hash_map::RandomState;
use std::fmt::Write as _;
use std::hash::{BuildHasher, Hasher};
use std::io::{ErrorKind, Write};
use std::path::Path;
use time::OffsetDateTime;

/// Maps an I/O error to a shared error code (`not_found`, `disk_full`, else `internal`).
pub(crate) fn io_err(what: &str, path: &Path, e: &std::io::Error) -> CmError {
    let msg = format!("{what} `{}`: {e}", path.display());
    match e.kind() {
        ErrorKind::NotFound => CmError::not_found(msg),
        ErrorKind::StorageFull => CmError::disk_full(msg),
        _ => CmError::internal(msg),
    }
}

pub(crate) fn sql_err(e: &rusqlite::Error) -> CmError {
    CmError::internal(format!("library index error: {e}"))
}

pub(crate) fn json_err(what: &str, e: &serde_json::Error) -> CmError {
    CmError::internal(format!("{what}: {e}"))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, CmError> {
    let mut f = std::fs::File::open(path).map_err(|e| io_err("cannot open", path, &e))?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).map_err(|e| io_err("cannot read", path, &e))?;
    Ok(hex(&h.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Writes `bytes` to `path` via a temp file and rename, so readers never see a partial file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CmError> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    res.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io_err("cannot write", path, &e)
    })
}

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// `<prefix>_<16 Crockford base32 chars>`: 48-bit millisecond timestamp (sortable) plus 32
/// random bits.
pub(crate) fn new_id(prefix: &str, now: OffsetDateTime) -> String {
    let millis =
        u64::try_from(now.unix_timestamp_nanos() / 1_000_000).unwrap_or(0) & ((1 << 48) - 1);
    let rand = RandomState::new().build_hasher().finish() & 0xFFFF_FFFF;
    let v = (u128::from(millis) << 32) | u128::from(rand);
    let mut out = String::from(prefix);
    out.push('_');
    for i in (0..16).rev() {
        out.push(CROCKFORD[((v >> (i * 5)) & 31) as usize] as char);
    }
    out
}

/// Longest folder-name base kept, in chars (R1-14).
const MAX_TITLE_CHARS: usize = 120;
/// ... and in bytes: with ` (9999).cmshot` appended the name stays under the 255-byte limit of
/// APFS, NTFS and ext4.
const MAX_TITLE_BYTES: usize = 200;

/// Characters that reorder or hide text (bidi embeddings and overrides, isolates, marks,
/// zero-width spaces, BOM): a `.cmshot` name must show what it is.
fn is_invisible_format_char(c: char) -> bool {
    matches!(
        c,
        '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{061C}'
            | '\u{FEFF}'
    )
}

/// Makes a title safe as a folder name on macOS and Windows: no path separators, control or
/// bidi-override characters, no Windows reserved device names, at most 120 chars and 200 bytes.
pub(crate) fn sanitize_title(title: &str) -> Result<String, CmError> {
    let cleaned: String = title
        .chars()
        .filter(|c| !is_invisible_format_char(*c))
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let mut cleaned = cleaned.trim().trim_end_matches('.').trim().to_owned();
    if cleaned.is_empty() {
        return Err(CmError::invalid_field("title", "must not be empty"));
    }
    if cleaned.chars().count() > MAX_TITLE_CHARS || cleaned.len() > MAX_TITLE_BYTES {
        let mut out = String::new();
        for c in cleaned.chars().take(MAX_TITLE_CHARS) {
            if out.len() + c.len_utf8() > MAX_TITLE_BYTES {
                break;
            }
            out.push(c);
        }
        cleaned = out.trim_end_matches(['.', ' ']).to_owned();
        if cleaned.is_empty() {
            return Err(CmError::invalid_field("title", "must not be empty"));
        }
    }
    if is_reserved_device_name(&cleaned) {
        return Ok(format!("{cleaned}_"));
    }
    Ok(cleaned)
}

/// Windows device names (with or without an extension): CON, PRN, AUX, NUL, COM1-9, LPT1-9.
fn is_reserved_device_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or("").trim_end();
    if matches!(stem, "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$") {
        return true;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(n) = stem.strip_prefix(prefix) {
            if n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9') {
                return true;
            }
        }
    }
    false
}

/// Total size in bytes of all files below `dir`.
pub(crate) fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in rd.flatten() {
        let Ok(md) = entry.metadata() else { continue };
        if md.is_dir() {
            total += dir_size(&entry.path());
        } else {
            total += md.len();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// R1-14: reserved names, bidi overrides and over-long titles.
    #[test]
    fn r1_14_sanitize_title_edge_cases() {
        for n in ["COM3", "com9", "LPT4.txt", "lpt9", "NUL", "Con"] {
            assert_eq!(sanitize_title(n).unwrap(), format!("{n}_"), "{n}");
        }
        assert_eq!(sanitize_title("COM10").unwrap(), "COM10");
        assert_eq!(sanitize_title("COM0").unwrap(), "COM0");
        // U+202E would show the extension reversed.
        assert_eq!(
            sanitize_title("evil\u{202E}gnp.exe").unwrap(),
            "evilgnp.exe"
        );
        assert_eq!(sanitize_title("a\u{200B}b\u{FEFF}").unwrap(), "ab");
        assert!(sanitize_title("\u{202E}\u{200B}").is_err());
        // 300 ASCII chars: 120 chars. 300 CJK chars: 120 chars is 360 bytes, cut to 200 bytes.
        assert_eq!(
            sanitize_title(&"a".repeat(300)).unwrap().chars().count(),
            120
        );
        let cjk = sanitize_title(&"字".repeat(300)).unwrap();
        assert!(
            cjk.len() <= 200 && cjk.chars().count() == 66,
            "{}",
            cjk.len()
        );
        // A cut that ends in a dot or space is trimmed.
        let t = format!("{} .{}", "a".repeat(118), "b".repeat(20));
        assert_eq!(sanitize_title(&t).unwrap(), "a".repeat(118));
    }
}
