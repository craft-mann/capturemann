//! "Also save a PNG to a folder" (CAP-4.AC2 to AC4): the file name pattern, a name that never
//! overwrites, an atomic write and the reason a failed save is shown with. Display-free and
//! synchronous; the callers run it on a blocking thread.
//!
//! Settings (all app-only, see the registry in `specs/01-technical-design.md`):
//! `capture.save_folder_enabled` (default off), `capture.save_folder` and `capture.save_pattern`
//! (default [`DEFAULT_PATTERN`]).

use cm_i18n::tr;
use serde_json::{Map, Value};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use time::{OffsetDateTime, UtcOffset};

use crate::settings::{KEY_SAVE_FOLDER, KEY_SAVE_FOLDER_ENABLED, KEY_SAVE_PATTERN};

/// CAP-4: the default file name pattern.
pub const DEFAULT_PATTERN: &str = "CaptureMann {yyyy-MM-dd} at {HH.mm.ss}.png";

/// Longest file name stem kept (bytes), well under every file system's 255.
const MAX_STEM: usize = 150;

static LOCAL_OFFSET: OnceLock<UtcOffset> = OnceLock::new();

/// Remembers the local UTC offset (read once at startup, before other threads exist: the `time`
/// crate refuses to read it later) for file names.
pub fn remember_offset(offset: UtcOffset) {
    let _ = LOCAL_OFFSET.set(offset);
}

/// The current local time for a file name (UTC when the offset was never read).
pub fn local_now() -> OffsetDateTime {
    OffsetDateTime::now_utc().to_offset(LOCAL_OFFSET.get().copied().unwrap_or(UtcOffset::UTC))
}

/// What the settings ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFolderConfig {
    pub enabled: bool,
    pub folder: Option<PathBuf>,
    pub pattern: String,
}

impl SaveFolderConfig {
    /// Reads the three keys; absent or malformed values mean the defaults (off, no folder).
    pub fn from_settings(settings: &Map<String, Value>) -> SaveFolderConfig {
        let folder = settings
            .get(KEY_SAVE_FOLDER)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        SaveFolderConfig {
            enabled: settings
                .get(KEY_SAVE_FOLDER_ENABLED)
                .and_then(Value::as_bool)
                .unwrap_or(false),
            folder,
            pattern: settings
                .get(KEY_SAVE_PATTERN)
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(DEFAULT_PATTERN)
                .to_owned(),
        }
    }
}

/// How the folder save went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderResult {
    /// Off (or not asked for).
    Skipped,
    Saved(PathBuf),
    /// The capture is in the library; this copy was not written. `reason` finishes the sentence
    /// "Couldn't save to `<folder>`: ...".
    Failed {
        folder: String,
        reason: String,
    },
}

/// Why a write failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// The folder is not there: an unplugged or unmounted drive, or a deleted folder.
    Unavailable,
    PermissionDenied,
    DiskFull,
    NotAFolder,
    Other(String),
}

impl SaveError {
    /// CAP-4.AC4 wording.
    pub fn reason(&self) -> String {
        match self {
            SaveError::Unavailable => tr!("drive not available"),
            SaveError::PermissionDenied => tr!("permission denied"),
            SaveError::DiskFull => tr!("disk full"),
            SaveError::NotAFolder => tr!("not a folder"),
            SaveError::Other(m) => m.clone(),
        }
    }

    fn from_io(e: &io::Error) -> SaveError {
        // 28 = ENOSPC (POSIX), 112 = ERROR_DISK_FULL, 39 = ERROR_HANDLE_DISK_FULL (Windows).
        let full = matches!(e.raw_os_error(), Some(28 | 112 | 39));
        match e.kind() {
            _ if full => SaveError::DiskFull,
            io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => SaveError::DiskFull,
            io::ErrorKind::NotFound => SaveError::Unavailable,
            io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
                SaveError::PermissionDenied
            }
            io::ErrorKind::NotADirectory => SaveError::NotAFolder,
            _ => SaveError::Other(e.to_string()),
        }
    }
}

/// Expands `pattern` for the time `now`. `{...}` holds a date format built from `yyyy`, `MM`,
/// `dd`, `HH`, `mm` and `ss` (so `{yyyy-MM-dd}` and `{HH.mm.ss}`); anything else in the braces
/// stays as typed. The result is always one file name: path separators and characters Windows
/// refuses become `-`, `..` collapses, leading and trailing dots and spaces go, a reserved
/// Windows device name gets a `_`, and a name that does not end in `.png` gets it. An empty
/// result is the default pattern's name.
pub fn expand_pattern(pattern: &str, now: OffsetDateTime) -> String {
    let name = clean(&expand_tokens(pattern, now));
    if name.is_empty() || name.eq_ignore_ascii_case(".png") {
        return clean(&expand_tokens(DEFAULT_PATTERN, now));
    }
    name
}

fn expand_tokens(pattern: &str, now: OffsetDateTime) -> String {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                out.push_str(&expand_format(&after[..close], now));
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn expand_format(fmt: &str, now: OffsetDateTime) -> String {
    let mut out = String::new();
    let mut rest = fmt;
    'outer: while !rest.is_empty() {
        for (token, value) in [
            ("yyyy", format!("{:04}", now.year())),
            ("MM", format!("{:02}", u8::from(now.month()))),
            ("dd", format!("{:02}", now.day())),
            ("HH", format!("{:02}", now.hour())),
            ("mm", format!("{:02}", now.minute())),
            ("ss", format!("{:02}", now.second())),
        ] {
            if let Some(tail) = rest.strip_prefix(token) {
                out.push_str(&value);
                rest = tail;
                continue 'outer;
            }
        }
        let c = rest.chars().next().unwrap_or(' ');
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

fn clean(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect();
    while s.contains("..") {
        s = s.replace("..", ".");
    }
    let mut s = s
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_owned();
    // Keep the extension when cutting a long name.
    let lower = s.to_ascii_lowercase();
    let stem_len = if lower.ends_with(".png") {
        s.len() - 4
    } else {
        s.len()
    };
    let mut stem = s[..stem_len].trim_end().to_owned();
    if stem.len() > MAX_STEM {
        let mut cut = MAX_STEM;
        while !stem.is_char_boundary(cut) {
            cut -= 1;
        }
        stem.truncate(cut);
        stem = stem.trim_end().to_owned();
    }
    if stem.is_empty() {
        return String::new();
    }
    if is_reserved_device(&stem) {
        stem.insert(0, '_');
    }
    s = format!("{stem}.png");
    s
}

fn is_reserved_device(stem: &str) -> bool {
    let base = stem.split('.').next().unwrap_or(stem).to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((base.starts_with("COM") || base.starts_with("LPT"))
            && base.len() == 4
            && base.as_bytes()[3].is_ascii_digit())
}

/// `name`, or `name (2)`, `name (3)`... before the extension: candidate number `n` (1 is the
/// name itself).
pub fn numbered(name: &str, n: u32) -> String {
    if n <= 1 {
        return name.to_owned();
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) => format!("{stem} ({n}).{ext}"),
        None => format!("{name} ({n})"),
    }
}

/// Writes `png` into `dir` as `name`, or the first free ` (2)`, ` (3)`... variant, and returns
/// the path. Never overwrites: the bytes go to a temporary file in the same folder first and are
/// published with a hard link (which fails when the name exists), falling back to a checked
/// rename on file systems without links (FAT, exFAT). The folder is never created: a missing one
/// is an unplugged drive, and creating it would write into the mount point.
pub fn write_unique(dir: &Path, name: &str, png: &[u8]) -> Result<PathBuf, SaveError> {
    match fs::metadata(dir) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(SaveError::NotAFolder),
        Err(e) => return Err(SaveError::from_io(&e)),
    }
    let tmp = dir.join(format!(".capturemann-{}.tmp", uuid_like()));
    let written = fs::File::create_new(&tmp).and_then(|mut f| {
        f.write_all(png)?;
        f.sync_all()
    });
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(SaveError::from_io(&e));
    }
    let result = publish(&tmp, dir, name);
    let _ = fs::remove_file(&tmp);
    result
}

fn publish(tmp: &Path, dir: &Path, name: &str) -> Result<PathBuf, SaveError> {
    for n in 1..=9_999 {
        let target = dir.join(numbered(name, n));
        match fs::hard_link(tmp, &target) {
            Ok(()) => return Ok(target),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                // No hard links here: rename, but only onto a free name.
                if target.exists() {
                    continue;
                }
                match fs::rename(tmp, &target) {
                    Ok(()) => return Ok(target),
                    Err(e2) => {
                        let first = SaveError::from_io(&e);
                        return Err(match first {
                            SaveError::Other(_) => SaveError::from_io(&e2),
                            other => other,
                        });
                    }
                }
            }
        }
    }
    Err(SaveError::Other("too many files with this name".to_owned()))
}

fn uuid_like() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!(
        "{}-{}-{nanos}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// The CAP-4 "also save to a folder" step for one capture.
pub fn save_capture(cfg: &SaveFolderConfig, png: &[u8], now: OffsetDateTime) -> FolderResult {
    if !cfg.enabled {
        return FolderResult::Skipped;
    }
    let Some(folder) = &cfg.folder else {
        return FolderResult::Failed {
            folder: "the folder".to_owned(),
            reason: "no folder chosen".to_owned(),
        };
    };
    match write_unique(folder, &expand_pattern(&cfg.pattern, now), png) {
        Ok(path) => FolderResult::Saved(path),
        Err(e) => FolderResult::Failed {
            folder: folder.display().to_string(),
            reason: e.reason(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 2026-10-23 14:05:09 UTC.
    const NOW_TS: i64 = 1792764309;
    static NOW: std::sync::LazyLock<OffsetDateTime> =
        std::sync::LazyLock::new(|| OffsetDateTime::from_unix_timestamp(NOW_TS).unwrap());

    /// CAP-4.AC2: the default pattern is `CaptureMann 2026-10-23 at 14.05.09.png`.
    #[test]
    fn cap4_ac2_default_pattern() {
        assert_eq!(
            expand_pattern(DEFAULT_PATTERN, *NOW),
            "CaptureMann 2026-10-23 at 14.05.09.png"
        );
        assert_eq!(
            expand_pattern("{yyyy}{MM}{dd}-{HH}{mm}{ss}", *NOW),
            "20261023-140509.png"
        );
    }

    /// A pattern can never leave the folder or hide as a dot file.
    #[test]
    fn pattern_is_always_one_safe_file_name() {
        for bad in [
            "../../etc/passwd",
            "..\\..\\x",
            "/abs/path/file.png",
            "a/b/c",
            "C:\\Users\\x\\y.png",
            "....",
            "..",
            ".hidden",
            "  ",
            "",
            "a:b*c?d\"e<f>g|h",
        ] {
            let name = expand_pattern(bad, *NOW);
            assert!(!name.contains('/') && !name.contains('\\'), "{bad}: {name}");
            assert!(!name.contains(".."), "{bad}: {name}");
            assert!(!name.starts_with('.'), "{bad}: {name}");
            assert!(name.ends_with(".png"), "{bad}: {name}");
            assert!(!name.contains(':') && !name.contains('*'), "{bad}: {name}");
            assert!(Path::new(&name).components().count() == 1, "{bad}: {name}");
        }
        assert_eq!(expand_pattern("a/b", *NOW), "a-b.png");
        assert_eq!(
            expand_pattern("", *NOW),
            "CaptureMann 2026-10-23 at 14.05.09.png"
        );
        assert_eq!(expand_pattern("shot.PNG", *NOW), "shot.png");
        assert_eq!(expand_pattern("CON", *NOW), "_CON.png");
    }

    #[test]
    fn long_names_are_cut_and_keep_the_extension() {
        let name = expand_pattern(&"x".repeat(400), *NOW);
        assert!(name.len() <= MAX_STEM + 4);
        assert!(name.ends_with(".png"));
        let multibyte = expand_pattern(&"é".repeat(400), *NOW);
        assert!(multibyte.ends_with(".png"));
    }

    #[test]
    fn unclosed_brace_stays_literal() {
        assert_eq!(expand_pattern("a {yyyy", *NOW), "a {yyyy.png");
    }

    /// CAP-4.AC3: ` (2)`, ` (3)`... before the extension.
    #[test]
    fn numbering() {
        assert_eq!(numbered("a.png", 1), "a.png");
        assert_eq!(numbered("a.png", 2), "a (2).png");
        assert_eq!(numbered("a", 3), "a (3)");
    }

    /// CAP-4.AC2 and AC3: the file is written, and a second capture in the same second keeps
    /// both.
    #[test]
    fn cap4_ac2_ac3_writes_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = SaveFolderConfig {
            enabled: true,
            folder: Some(dir.path().to_owned()),
            pattern: DEFAULT_PATTERN.to_owned(),
        };
        let first = save_capture(&cfg, b"one", *NOW);
        let second = save_capture(&cfg, b"two", *NOW);
        let third = save_capture(&cfg, b"three", *NOW);
        let name = |r: &FolderResult| match r {
            FolderResult::Saved(p) => p.file_name().unwrap().to_string_lossy().into_owned(),
            other => panic!("{other:?}"),
        };
        assert_eq!(name(&first), "CaptureMann 2026-10-23 at 14.05.09.png");
        assert_eq!(name(&second), "CaptureMann 2026-10-23 at 14.05.09 (2).png");
        assert_eq!(name(&third), "CaptureMann 2026-10-23 at 14.05.09 (3).png");
        let FolderResult::Saved(p1) = &first else {
            unreachable!()
        };
        assert_eq!(fs::read(p1).unwrap(), b"one");
        // Only the three files: the temporary one is gone.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
    }

    /// A file that is already there is left alone even with the same name from an older run.
    #[test]
    fn existing_file_is_untouched() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.png"), b"old").unwrap();
        let p = write_unique(dir.path(), "a.png", b"new").unwrap();
        assert_eq!(p.file_name().unwrap(), "a (2).png");
        assert_eq!(fs::read(dir.path().join("a.png")).unwrap(), b"old");
    }

    /// CAP-4.AC4: a folder that is not there (an unplugged drive) is reported, and nothing is
    /// created in its place.
    #[test]
    fn cap4_ac4_offline_folder() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("Volumes").join("USB");
        let cfg = SaveFolderConfig {
            enabled: true,
            folder: Some(gone.clone()),
            pattern: DEFAULT_PATTERN.to_owned(),
        };
        let r = save_capture(&cfg, b"x", *NOW);
        assert_eq!(
            r,
            FolderResult::Failed {
                folder: gone.display().to_string(),
                reason: tr!("drive not available")
            }
        );
        assert!(!gone.exists());
        assert!(!dir.path().join("Volumes").exists());
    }

    #[test]
    fn a_file_in_place_of_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("file");
        fs::write(&f, b"x").unwrap();
        assert_eq!(write_unique(&f, "a.png", b"x"), Err(SaveError::NotAFolder));
    }

    #[cfg(unix)]
    #[test]
    fn read_only_folder_is_permission_denied() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let ro = dir.path().join("ro");
        fs::create_dir(&ro).unwrap();
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o555)).unwrap();
        let r = write_unique(&ro, "a.png", b"x");
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o755)).unwrap();
        // Running as root ignores the mode; anywhere else this is the permission error.
        if r != Ok(ro.join("a.png")) {
            assert_eq!(r, Err(SaveError::PermissionDenied));
        }
    }

    #[test]
    fn io_errors_map_to_reasons() {
        let e = |k: io::ErrorKind| SaveError::from_io(&io::Error::from(k));
        assert_eq!(e(io::ErrorKind::NotFound).reason(), "drive not available");
        assert_eq!(
            e(io::ErrorKind::PermissionDenied).reason(),
            "permission denied"
        );
        assert_eq!(e(io::ErrorKind::StorageFull).reason(), "disk full");
        assert_eq!(
            SaveError::from_io(&io::Error::from_raw_os_error(28)).reason(),
            "disk full"
        );
    }

    #[test]
    fn off_means_skipped_and_no_folder_is_reported() {
        let mut cfg = SaveFolderConfig::from_settings(&Map::new());
        assert_eq!(cfg.pattern, DEFAULT_PATTERN);
        assert_eq!(save_capture(&cfg, b"x", *NOW), FolderResult::Skipped);
        cfg.enabled = true;
        assert!(matches!(
            save_capture(&cfg, b"x", *NOW),
            FolderResult::Failed { .. }
        ));
    }

    #[test]
    fn reads_the_settings() {
        let m = json!({
            KEY_SAVE_FOLDER_ENABLED: true,
            KEY_SAVE_FOLDER: " /tmp/Shots ",
            KEY_SAVE_PATTERN: "S {HH}.png"
        });
        let cfg = SaveFolderConfig::from_settings(m.as_object().unwrap());
        assert!(cfg.enabled);
        assert_eq!(cfg.folder, Some(PathBuf::from("/tmp/Shots")));
        assert_eq!(cfg.pattern, "S {HH}.png");
        let bad =
            json!({KEY_SAVE_FOLDER_ENABLED: "yes", KEY_SAVE_FOLDER: 3, KEY_SAVE_PATTERN: " "});
        let cfg = SaveFolderConfig::from_settings(bad.as_object().unwrap());
        assert!(!cfg.enabled && cfg.folder.is_none());
        assert_eq!(cfg.pattern, DEFAULT_PATTERN);
    }
}
