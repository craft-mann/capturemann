//! A library folder whose path is too long fails fast with `invalid_input` (it used to stall the
//! app on "Loading library...").
#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use cm_core::ErrorCode;
use cm_library::Library;

fn deep(base: &std::path::Path, chars: usize) -> std::path::PathBuf {
    let mut p = base.to_owned();
    // Folder names of 50 characters until the whole path is `chars` long.
    while p.to_string_lossy().chars().count() < chars {
        p.push("d".repeat(50));
    }
    p
}

/// The longest root the check accepts.
fn limit() -> usize {
    if std::path::MAIN_SEPARATOR == '\\' {
        260 - 80
    } else {
        1024 - 80
    }
}

#[test]
fn a_path_past_the_limit_is_refused_at_once_by_both_opens() {
    let tmp = tempfile::tempdir().unwrap();
    for chars in [limit() + 60, 3000] {
        let p = deep(tmp.path(), chars);
        let t = Instant::now();
        for r in [Library::open(&p), Library::open_or_create(&p)] {
            let Err(e) = r else { panic!("not refused") };
            assert_eq!(e.code(), ErrorCode::InvalidInput, "{e}");
            assert!(e.to_string().contains("too long"), "{e}");
        }
        assert!(t.elapsed() < Duration::from_secs(2));
        assert!(!p.exists(), "nothing was created");
    }
}

#[test]
fn a_single_name_over_255_characters_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("n".repeat(300));
    let Err(e) = Library::open_or_create(&p) else {
        panic!("not refused")
    };
    assert_eq!(e.code(), ErrorCode::InvalidInput);
}

#[test]
fn a_normal_path_still_opens() {
    let tmp = tempfile::tempdir().unwrap();
    Library::open_or_create(&tmp.path().join("CaptureMann")).unwrap();
}
