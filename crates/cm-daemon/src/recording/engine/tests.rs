use super::*;

#[test]
fn dialog_text_strips_controls_and_bidi_and_bounds_the_length() {
    assert_eq!(dialog_text("a\nb\tc\u{202E}d\u{200B}e"), "a b cde");
    let long = "x".repeat(500);
    let t = dialog_text(&long);
    assert_eq!(t.chars().count(), DIALOG_TEXT_MAX + 1);
    assert!(t.ends_with('\u{2026}'));
    assert_eq!(dialog_text("plain title"), "plain title");
}

/// R3-16: a killed self-test's folder is swept at startup when it is old enough; recent
/// ones (another running test) and unrelated folders are left alone.
#[cfg(unix)]
#[test]
fn r3_16_old_selftest_folders_are_swept() {
    let tmp = tempfile::tempdir().unwrap();
    let old = tmp.path().join(format!("{SELFTEST_DIR_PREFIX}st_old"));
    let recent = tmp.path().join(format!("{SELFTEST_DIR_PREFIX}st_recent"));
    let other = tmp.path().join("something-else");
    for d in [&old, &recent, &other] {
        std::fs::create_dir_all(d.join("inner")).unwrap();
        std::fs::write(d.join("inner/screen.mp4"), b"x").unwrap();
    }
    let long_ago = std::time::SystemTime::now() - Duration::from_secs(3600);
    for d in [&old, &other] {
        std::fs::File::open(d)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
    }
    assert_eq!(sweep_selftest_dirs(tmp.path(), Duration::from_secs(60)), 1);
    assert!(!old.exists());
    assert!(recent.exists() && other.exists());
    // The self-test's own folder is created owner-only.
    let f = TempFolder::create(tmp.path().join("capturemann-selftest-perm")).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&f.0).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
