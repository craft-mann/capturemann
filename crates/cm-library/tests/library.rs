#![allow(clippy::unwrap_used)]
mod common;
use cm_core::{ErrorCode, MediaTime};
use cm_library::index::{ListQuery, SortKey, TranscriptSegment};
use cm_library::manifest::CaptureKind;
use cm_library::Library;
use common::*;
use std::fs;

fn lib(tmp: &tempfile::TempDir) -> Library {
    Library::open_or_create(&tmp.path().join("lib")).unwrap()
}

fn write_rec(
    root: &std::path::Path,
    folder: &str,
    id: &str,
    title: &str,
    created: &str,
    secs: i64,
) {
    let dir = root.join(folder);
    fs::create_dir_all(&dir).unwrap();
    let m = serde_json::json!({
        "schema": 1, "id": id, "title": title, "created": created, "tick_rate": 120000,
        "duration": secs * 120_000,
        "tracks": { "screen": { "file": "screen.mp4", "size": [3840, 2160], "fps": 60 } },
        "edits": [], "revisions": [], "future_field": { "kept": true }
    });
    fs::write(dir.join("manifest.json"), serde_json::to_vec(&m).unwrap()).unwrap();
    fs::write(dir.join("screen.mp4"), vec![0u8; 1000]).unwrap();
}

#[test]
fn create_index_list_and_default_title_lib_1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    let b = l.create_screenshot(&png(64, 48, 1), None, T0).unwrap();
    assert_eq!(b.manifest().title, "Screenshot 2026-10-19 09.12");
    assert!(b.dir().join("thumbs/poster.jpg").is_file());
    let rec = l.get(&b.manifest().id).unwrap();
    assert_eq!(rec.kind, CaptureKind::Screenshot);
    assert_eq!((rec.width, rec.height), (Some(64), Some(48)));
    assert!(rec.size_bytes > 0);
    assert!(l.poster_path(&rec).is_some());
    assert_eq!(l.list(&ListQuery::default()).unwrap(), vec![rec]);
    assert_eq!(l.get("nope").unwrap_err().code(), ErrorCode::NotFound);
}

#[test]
fn save_shot_updates_index_and_poster_cap_4() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    let mut b = l.create_screenshot(&png(64, 48, 1), Some("A"), T0).unwrap();
    let before = l.get(&b.manifest().id).unwrap().size_bytes;
    b.add_ops(vec![arrow()], user(), t(1)).unwrap();
    b.write_render(&png(64, 48, 7)).unwrap();
    let rec = l.save_shot(&mut b).unwrap();
    assert!(rec.size_bytes > before);
    let reopened = l.open_screenshot(&rec.id).unwrap();
    assert_eq!(reopened.ops().len(), 1);
}

#[test]
fn filter_sort_and_page_lib_1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    l.create_screenshot(&png(8, 8, 1), Some("banana"), t(0))
        .unwrap();
    l.create_screenshot(&png(8, 8, 2), Some("Apple"), t(10))
        .unwrap();
    write_rec(
        l.root(),
        "Cherry.cmrec",
        "rec_1",
        "Cherry",
        "2026-10-19T09:30:00Z",
        60,
    );
    l.rescan().unwrap();
    let titles = |q: &ListQuery| -> Vec<String> {
        l.list(q).unwrap().into_iter().map(|r| r.title).collect()
    };
    assert_eq!(titles(&ListQuery::default()), ["Cherry", "Apple", "banana"]);
    assert_eq!(
        titles(&ListQuery {
            sort: SortKey::Title,
            descending: false,
            ..Default::default()
        }),
        ["Apple", "banana", "Cherry"]
    );
    assert_eq!(
        titles(&ListQuery {
            kind: Some(CaptureKind::Recording),
            ..Default::default()
        }),
        ["Cherry"]
    );
    assert_eq!(
        titles(&ListQuery {
            created_from: Some(t(5)),
            created_to: Some(t(15)),
            ..Default::default()
        }),
        ["Apple"]
    );
    assert_eq!(
        titles(&ListQuery {
            limit: Some(1),
            offset: 1,
            ..Default::default()
        }),
        ["Apple"]
    );
    let rec = &l
        .list(&ListQuery {
            kind: Some(CaptureKind::Recording),
            ..Default::default()
        })
        .unwrap()[0];
    assert_eq!(rec.duration, Some(MediaTime::from_secs(60)));
    assert_eq!((rec.width, rec.height), (Some(3840), Some(2160)));
    assert!(rec.size_bytes >= 1000);
}

#[test]
fn search_title_and_transcript_with_snippet_lib_1_ac2() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    l.create_screenshot(&png(8, 8, 1), Some("K8s probes"), t(0))
        .unwrap();
    write_rec(
        l.root(),
        "Lesson.cmrec",
        "rec_1",
        "Lesson",
        "2026-10-19T09:30:00Z",
        600,
    );
    l.rescan().unwrap();
    l.index_transcript(
        "rec_1",
        &[
            TranscriptSegment {
                start: MediaTime::from_secs(5),
                text: "first we add a liveness check".into(),
            },
            TranscriptSegment {
                start: MediaTime::from_millis(12_340),
                text: "then the readiness probe gates traffic".into(),
            },
        ],
    )
    .unwrap();

    let hits = l.search("readiness", &ListQuery::default()).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.id, "rec_1");
    assert!(!hits[0].title_match);
    let m = hits[0].transcript.as_ref().unwrap();
    assert_eq!(m.start, MediaTime::from_millis(12_340));
    assert!(m.snippet.contains("«readiness»"), "{}", m.snippet);
    // Prefix and multi-word.
    assert_eq!(
        l.search("readi probe", &ListQuery::default())
            .unwrap()
            .len(),
        1
    );
    // Title match, case-insensitive; the transcript-only capture is excluded by kind filter.
    let hits = l.search("PROBES", &ListQuery::default()).unwrap();
    assert!(hits[0].title_match && hits[0].transcript.is_none());
    assert!(l
        .search(
            "readiness",
            &ListQuery {
                kind: Some(CaptureKind::Screenshot),
                ..Default::default()
            }
        )
        .unwrap()
        .is_empty());
    // Hostile input is just text.
    assert!(l
        .search("\"; DROP TABLE captures; --", &ListQuery::default())
        .unwrap()
        .is_empty());
    assert!(l.search("100%_", &ListQuery::default()).unwrap().is_empty());
    assert!(l.search("   ", &ListQuery::default()).unwrap().is_empty());
    // Re-indexing replaces; removing clears.
    l.index_transcript("rec_1", &[]).unwrap();
    assert!(l
        .search("readiness", &ListQuery::default())
        .unwrap()
        .is_empty());
}

#[test]
fn rebuild_after_index_deleted_lib_1_ac3() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let mut l = Library::open_or_create(&root).unwrap();
    l.create_screenshot(&png(8, 8, 1), Some("one"), t(0))
        .unwrap();
    l.create_screenshot(&png(8, 8, 2), Some("two"), t(1))
        .unwrap();
    write_rec(&root, "r.cmrec", "rec_1", "rec", "2026-10-19T09:30:00Z", 1);
    l.rescan().unwrap();
    let before = l.list(&ListQuery::default()).unwrap();
    assert_eq!(before.len(), 3);
    drop(l);
    fs::remove_file(root.join(cm_library::index::INDEX_RELATIVE_PATH)).unwrap();
    let l = Library::open(&root).unwrap();
    assert_eq!(l.startup_rescan().unwrap().indexed, 3);
    assert_eq!(l.list(&ListQuery::default()).unwrap(), before);
}

#[test]
fn corrupted_index_is_rebuilt_lib_1_ac3() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let mut l = Library::open_or_create(&root).unwrap();
    l.create_screenshot(&png(8, 8, 1), Some("one"), t(0))
        .unwrap();
    drop(l);
    fs::write(
        root.join(cm_library::index::INDEX_RELATIVE_PATH),
        b"this is not sqlite at all",
    )
    .unwrap();
    let l = Library::open(&root).unwrap();
    assert_eq!(l.count().unwrap(), 1);
    assert!(l.startup_rescan().is_some());
}

#[test]
fn rescan_adds_removes_and_flags_broken_lib_1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    let b = l
        .create_screenshot(&png(8, 8, 1), Some("gone"), t(0))
        .unwrap();
    let dir = b.dir().to_owned();
    drop(b);
    // Bundle copied in by hand appears after a rescan.
    write_rec(
        l.root(),
        "copied.cmrec",
        "rec_9",
        "copied",
        "2026-10-19T09:30:00Z",
        3,
    );
    // A duplicate id (bundle copied twice) is skipped, a damaged manifest is flagged.
    write_rec(
        l.root(),
        "copied (2).cmrec",
        "rec_9",
        "copied",
        "2026-10-19T09:30:00Z",
        3,
    );
    fs::create_dir_all(l.root().join("hurt.cmrec")).unwrap();
    fs::write(l.root().join("hurt.cmrec/manifest.json"), b"{").unwrap();
    fs::create_dir_all(l.root().join("unrelated")).unwrap();
    let r = l.rescan().unwrap();
    assert_eq!(r.broken, ["hurt.cmrec"]);
    // R1-5: the original (oldest, shortest) stays; the copy is the ignored one.
    assert_eq!(r.duplicate_ids, ["copied (2).cmrec"]);
    assert_eq!(r.duplicates.len(), 1);
    assert_eq!(r.duplicates[0].kept, "copied.cmrec");
    assert_eq!(l.get("rec_9").unwrap().path, "copied.cmrec");
    assert_eq!(l.count().unwrap(), 3);
    let broken = l.get("broken:hurt.cmrec").unwrap();
    assert!(broken.broken);
    // Delete a bundle on disk, rescan removes it.
    fs::remove_dir_all(dir).unwrap();
    let r = l.rescan().unwrap();
    assert_eq!(r.removed, 1);
    assert!(l
        .list(&ListQuery::default())
        .unwrap()
        .iter()
        .all(|r| r.title != "gone"));
}

#[test]
fn rename_moves_folder_and_updates_index_lib_1_ac5() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    write_rec(
        l.root(),
        "Recording.cmrec",
        "rec_1",
        "Recording",
        "2026-10-19T09:30:00Z",
        3,
    );
    write_rec(
        l.root(),
        "K8s probes.cmrec",
        "rec_2",
        "other",
        "2026-10-19T09:31:00Z",
        3,
    );
    l.rescan().unwrap();
    let r = l.rename("rec_1", "K8s probes").unwrap();
    assert_eq!(r.path, "K8s probes (2).cmrec");
    assert_eq!(r.title, "K8s probes");
    assert!(l.root().join("K8s probes (2).cmrec").is_dir());
    assert!(!l.root().join("Recording.cmrec").exists());
    // Unknown manifest fields survive the rewrite.
    let m: serde_json::Value = serde_json::from_slice(
        &fs::read(l.root().join("K8s probes (2).cmrec/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(m["future_field"]["kept"], true);
    // Unsafe characters are sanitized in the folder name only.
    let r = l.rename("rec_1", "a/b: c?").unwrap();
    assert_eq!(r.path, "a_b_ c_.cmrec");
    assert_eq!(r.title, "a/b: c?");
    assert!(l.rename("rec_1", "  ").is_err());
}

#[test]
fn remove_only_touches_the_index() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    let b = l.create_screenshot(&png(8, 8, 1), Some("x"), t(0)).unwrap();
    assert!(l.remove(&b.manifest().id).unwrap());
    assert!(!l.remove(&b.manifest().id).unwrap());
    assert!(b.dir().is_dir());
    assert_eq!(l.count().unwrap(), 0);
}

#[test]
fn configurable_root_and_unavailable_library_lib_2() {
    let tmp = tempfile::tempdir().unwrap();
    let gone = tmp.path().join("unplugged");
    let e = Library::open(&gone).unwrap_err();
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert!(e.to_string().contains("Library unavailable"));
    assert!(!gone.exists());
    // Two independent libraries; relocating the folder keeps the index valid (relative paths).
    let a = tmp.path().join("a");
    let mut l = Library::open_or_create(&a).unwrap();
    let id = l
        .create_screenshot(&png(8, 8, 1), Some("moved"), T0)
        .unwrap()
        .manifest()
        .id
        .clone();
    drop(l);
    let b = tmp.path().join("b");
    fs::rename(&a, &b).unwrap();
    let l = Library::open(&b).unwrap();
    assert!(l.startup_rescan().is_none());
    let rec = l.get(&id).unwrap();
    assert!(l.open_screenshot(&id).unwrap().dir().starts_with(&b));
    assert!(l.bundle_path(&rec).is_dir());
}

#[test]
fn thousand_captures_list_is_fast_lib_1_ac1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    let png = png(8, 8, 1);
    for i in 0..1000 {
        let dir = l.root().join(format!("c{i}.cmrec"));
        write_rec(
            l.root(),
            &format!("c{i}.cmrec"),
            &format!("rec_{i}"),
            &format!("capture {i}"),
            "2026-10-19T09:30:00Z",
            i,
        );
        let _ = (&dir, &png);
    }
    let r = l.rescan().unwrap();
    assert_eq!(r.indexed, 1000);
    let start = std::time::Instant::now();
    let rows = l.list(&ListQuery::default()).unwrap();
    let hits = l.search("capture 99", &ListQuery::default()).unwrap();
    assert_eq!(rows.len(), 1000);
    assert!(!hits.is_empty());
    assert!(
        start.elapsed() < std::time::Duration::from_millis(500),
        "{:?}",
        start.elapsed()
    );
}

/// R1-5: the folder already indexed for an id wins, even when its name sorts after the copy's
/// and it is newer; and `duplicate` mints a new id.
#[test]
fn r1_5_rescan_keeps_the_indexed_bundle_and_duplicate_mints_a_new_id() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    write_rec(l.root(), "Z.cmrec", "rec_5", "Z", "2026-10-19T09:30:00Z", 3);
    l.rescan().unwrap();
    assert_eq!(l.get("rec_5").unwrap().path, "Z.cmrec");
    write_rec(
        l.root(),
        "A (2).cmrec",
        "rec_5",
        "Z",
        "2026-10-19T09:30:00Z",
        3,
    );
    let r = l.rescan().unwrap();
    assert_eq!(r.duplicate_ids, ["A (2).cmrec"]);
    assert_eq!(l.get("rec_5").unwrap().path, "Z.cmrec");
    assert_eq!(r.removed, 0);
    // The supported way to copy a capture.
    let b = l
        .create_screenshot(&png(8, 8, 1), Some("one"), t(0))
        .unwrap();
    let id = b.manifest().id.clone();
    drop(b);
    let copy = l.duplicate(&id).unwrap();
    assert_ne!(copy.id, id);
    assert_eq!(copy.title, "one copy");
    let r = l.rescan().unwrap();
    assert!(
        r.duplicates.iter().all(|d| d.id == "rec_5"),
        "{:?}",
        r.duplicates
    );
    assert_eq!(l.get(&id).unwrap().title, "one");
}

/// R1-14: a case-only rename keeps the plain name (no " (2)" on a case-insensitive file
/// system), and a 300-char title renames instead of failing with `internal`.
#[test]
fn r1_14_case_only_and_very_long_renames() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    let b = l
        .create_screenshot(&png(8, 8, 1), Some("k8s probes"), t(0))
        .unwrap();
    let id = b.manifest().id.clone();
    drop(b);
    let r = l.rename(&id, "K8S Probes").unwrap();
    assert_eq!(r.path, "K8S Probes.cmshot");
    let long = "t".repeat(300);
    let r = l.rename(&id, &long).unwrap();
    assert!(r.path.len() < 255, "{}", r.path.len());
    assert!(l.root().join(&r.path).is_dir());
    let r = l.rename(&id, &"字".repeat(300)).unwrap();
    assert!(l.root().join(&r.path).is_dir());
}

/// R1-15: `usize::MAX` limit/offset neither panics nor wraps into "no limit"/"offset 0".
#[test]
fn r1_15_huge_limit_and_offset_do_not_overflow() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = lib(&tmp);
    l.create_screenshot(&png(8, 8, 1), Some("probes"), t(0))
        .unwrap();
    let q = ListQuery {
        limit: Some(usize::MAX),
        offset: usize::MAX,
        ..ListQuery::default()
    };
    assert!(l.list(&q).unwrap().is_empty());
    assert!(l.search("probes", &q).unwrap().is_empty());
    let q = ListQuery {
        limit: Some(usize::MAX),
        offset: 1,
        ..ListQuery::default()
    };
    assert!(l.search("probes", &q).unwrap().is_empty());
    let q = ListQuery {
        limit: Some(usize::MAX),
        ..ListQuery::default()
    };
    assert_eq!(l.search("probes", &q).unwrap().len(), 1);
}

/// R1-20: an index that is merely locked by another process is not deleted; a corrupt one is.
#[test]
fn r1_20_a_busy_index_is_kept_not_rebuilt() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let mut l = Library::open_or_create(&root).unwrap();
    l.create_screenshot(&png(8, 8, 1), Some("one"), t(0))
        .unwrap();
    drop(l);
    let index = root.join(cm_library::index::INDEX_RELATIVE_PATH);
    let before = fs::read(&index).unwrap();
    let holder = rusqlite::Connection::open(&index).unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let err = Library::open(&root).map(|_| ()).unwrap_err();
    assert_ne!(err.code(), cm_core::ErrorCode::InvalidInput, "{err}");
    holder.execute_batch("ROLLBACK").unwrap();
    drop(holder);
    assert_eq!(fs::read(&index).unwrap(), before, "index untouched");
    // Released: opens normally and nothing was rebuilt.
    let l = Library::open(&root).unwrap();
    assert!(l.startup_rescan().is_none());
    assert_eq!(l.count().unwrap(), 1);
}

// ---- LIB-1: the Uploaded / Published filters -------------------------------------------------

fn remote_lib(tmp: &tempfile::TempDir) -> Library {
    let mut l = lib(tmp);
    write_rec(
        l.root(),
        "A.cmrec",
        "rec_a",
        "Alpha",
        "2026-10-19T09:00:00Z",
        10,
    );
    write_rec(
        l.root(),
        "B.cmrec",
        "rec_b",
        "Beta",
        "2026-10-19T09:10:00Z",
        10,
    );
    write_rec(
        l.root(),
        "C.cmrec",
        "rec_c",
        "Gamma",
        "2026-10-19T09:20:00Z",
        10,
    );
    l.rescan().unwrap();
    let a = l.root().join("A.cmrec");
    let b = l.root().join("B.cmrec");
    let up =
        |id: &str| serde_json::json!({"destination_id": "d", "what": "export", "remote_id": id});
    cm_library::record_upload(&a, up("1")).unwrap();
    cm_library::record_publish(
        &a,
        serde_json::json!({"platform": "youtube", "account_id": "y", "remote_id": "v1"}),
    )
    .unwrap();
    cm_library::record_upload(&b, up("2")).unwrap();
    l.index_dir(&a).unwrap();
    l.index_dir(&b).unwrap();
    l
}

fn titles_of(l: &Library, q: &ListQuery) -> Vec<String> {
    l.list(q).unwrap().into_iter().map(|r| r.title).collect()
}

#[test]
fn status_filters_use_the_derived_counts_lib_1() {
    use cm_library::index::StatusFilter::*;
    let tmp = tempfile::tempdir().unwrap();
    let l = remote_lib(&tmp);
    let q = |s| ListQuery {
        status: Some(s),
        ..Default::default()
    };
    assert_eq!(titles_of(&l, &q(Uploaded)), ["Beta", "Alpha"]);
    assert_eq!(titles_of(&l, &q(NotUploaded)), ["Gamma"]);
    assert_eq!(titles_of(&l, &q(Published)), ["Alpha"]);
    assert_eq!(titles_of(&l, &q(NotPublished)), ["Gamma", "Beta"]);
    let a = l.get("rec_a").unwrap();
    assert_eq!((a.upload_count, a.publish_count), (1, 1));
    // Paging keeps the filter: the second page of "uploaded" is the second match.
    let page = ListQuery {
        status: Some(Uploaded),
        limit: Some(1),
        offset: 1,
        ..Default::default()
    };
    assert_eq!(titles_of(&l, &page), ["Alpha"]);
    // Search combines with the filter (title hits).
    let hits = l
        .search(
            "a",
            &ListQuery {
                status: Some(Published),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.title, "Alpha");
}

#[test]
fn status_filter_applies_to_transcript_hits_too_lib_1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = remote_lib(&tmp);
    for id in ["rec_a", "rec_c"] {
        l.index_transcript(
            id,
            &[TranscriptSegment {
                start: MediaTime::from_secs(1),
                text: "readiness probe".into(),
            }],
        )
        .unwrap();
    }
    let hits = l
        .search(
            "readiness",
            &ListQuery {
                status: Some(cm_library::index::StatusFilter::NotPublished),
                ..Default::default()
            },
        )
        .unwrap();
    let ids: Vec<_> = hits.iter().map(|h| h.record.id.as_str()).collect();
    assert_eq!(ids, ["rec_c"]);
}

#[test]
fn counts_follow_the_manifest_and_a_rescan_lib_1() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = remote_lib(&tmp);
    // The index is derived: a rescan gives the same counts.
    l.rescan().unwrap();
    assert_eq!(l.get("rec_b").unwrap().upload_count, 1);
    // Dropping the record from the manifest and re-indexing drops the count.
    let b = l.root().join("B.cmrec");
    let mut m: serde_json::Value =
        serde_json::from_slice(&fs::read(b.join("manifest.json")).unwrap()).unwrap();
    m["uploads"] = serde_json::json!([]);
    fs::write(b.join("manifest.json"), serde_json::to_vec(&m).unwrap()).unwrap();
    l.index_dir(&b).unwrap();
    assert_eq!(l.get("rec_b").unwrap().upload_count, 0);
}

#[test]
fn an_index_of_the_old_schema_is_rebuilt_with_counts_lib_2_ac3() {
    let tmp = tempfile::tempdir().unwrap();
    let root = {
        let l = remote_lib(&tmp);
        l.root().to_owned()
    };
    let index = root.join(cm_library::index::INDEX_RELATIVE_PATH);
    {
        let c = rusqlite::Connection::open(&index).unwrap();
        c.pragma_update(None, "user_version", 1).unwrap();
    }
    let l = Library::open(&root).unwrap();
    assert_eq!(l.get("rec_a").unwrap().upload_count, 1);
    assert_eq!(l.get("rec_a").unwrap().publish_count, 1);
}
