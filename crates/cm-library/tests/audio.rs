//! Audio recordings in the library (VOC-1): `kind: "audio"`, no screen track, markers.
#![allow(clippy::unwrap_used)]
mod common;
use cm_core::{Actor, ErrorCode, MediaTime, TrackInfo, TrackKind};
use cm_library::index::ListQuery;
use cm_library::manifest::CaptureKind;
use cm_library::rec::{SessionInfo, LOCK_FILE};
use cm_library::rec_edit::{apply_marker, step_history};
use cm_library::Library;
use common::*;
use serde_json::{json, Map};
use std::fs;
use std::path::Path;

fn session() -> SessionInfo {
    SessionInfo {
        clock_origin_ns: 42,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({"mic": null}),
        app_version: "0.0.1".into(),
        extra: Map::new(),
    }
}

fn write_wav(path: &Path, secs: u32) {
    let mut w = cm_core::wav::WavWriter::create(path, 1, 48_000).unwrap();
    w.write(&vec![0.1f32; 48_000 * secs as usize]).unwrap();
    w.finalize().unwrap();
}

fn mic_info(secs: i64) -> TrackInfo {
    TrackInfo {
        kind: TrackKind::Mic,
        file: "mic.wav".into(),
        start: MediaTime::ZERO,
        end: MediaTime::from_secs(secs),
        details: json!({"rate": 48_000, "channels": 1}),
        stats: Default::default(),
    }
}

/// A finished 60 s audio recording with a mic track and no screen file.
fn finished_audio(l: &mut Library) -> String {
    let mut b = l
        .create_recording(Some("Interview"), T0, json!({"mode": "audio"}), &session())
        .unwrap();
    b.set_extra("kind", json!("audio")).unwrap();
    write_wav(&b.dir().join("mic.wav"), 1);
    l.finish_recording(b, &[mic_info(60)], t(1)).unwrap().id
}

/// VOC-1.AC1: the bundle is `kind: "audio"` with a mic track and no screen, camera or cursor
/// file, and the index says `audio` (with no size).
#[test]
fn voc1_ac1_audio_recording_is_indexed_as_audio_without_a_screen_track() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = Library::open_or_create(&tmp.path().join("lib")).unwrap();
    let id = finished_audio(&mut l);
    let rec = l.get(&id).unwrap();
    assert_eq!(rec.kind, CaptureKind::Audio);
    assert_eq!(rec.duration, Some(MediaTime::from_secs(60)));
    assert_eq!((rec.width, rec.height), (None, None));
    let dir = l.bundle_path(&rec);
    assert!(dir.extension().unwrap() == "cmrec");
    for gone in ["screen.mp4", "camera.mp4", "cursor.jsonl"] {
        assert!(!dir.join(gone).exists(), "{gone}");
    }
    let m = cm_library::read_manifest(&dir).unwrap();
    assert_eq!(m.kind.as_deref(), Some("audio"));
    assert_eq!(m.tracks.keys().collect::<Vec<_>>(), ["mic"]);
    // `list_library { kind: "audio" }` finds it; the screen recording filter does not.
    let q = |k| ListQuery {
        kind: Some(k),
        ..ListQuery::default()
    };
    assert_eq!(l.list(&q(CaptureKind::Audio)).unwrap().len(), 1);
    assert!(l.list(&q(CaptureKind::Recording)).unwrap().is_empty());
    // A rescan keeps the kind (it is read from the manifest, not the folder name).
    l.rescan().unwrap();
    assert_eq!(l.get(&id).unwrap().kind, CaptureKind::Audio);
    // Rename and duplicate keep an audio recording an audio recording.
    let r = l.rename(&id, "Board meeting").unwrap();
    assert_eq!(r.kind, CaptureKind::Audio);
}

/// VOC-1.AC5 at the library level: a crash leaves the lock; the next launch recovers the audio
/// with its kind and the markers set before the crash.
#[test]
fn voc1_ac5_a_crashed_audio_recording_is_recovered_with_kind_and_markers() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let dir = {
        let l = Library::open_or_create(&root).unwrap();
        let mut b = l
            .create_recording(Some("Crash"), T0, json!({}), &session())
            .unwrap();
        b.set_extra("kind", json!("audio")).unwrap();
        b.set_extra(
            "markers",
            json!([{ "at": 120_000, "label": "before the crash", "by": "user" }]),
        )
        .unwrap();
        write_wav(&b.dir().join("mic.wav"), 3);
        b.set_track_hint(
            "mic",
            json!({"start": 0, "file": "mic.wav", "raw_file": "mic.wav",
                   "denoised_file": null, "channels": 1, "rate": 48_000}),
        )
        .unwrap();
        b.dir().to_owned()
    }; // crash: no finalize
    let _ = fs::remove_file(dir.join(cm_library::rec::OWNER_FILE));
    let p = dir.join(LOCK_FILE);
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    v["pid"] = json!(0);
    fs::write(&p, serde_json::to_vec(&v).unwrap()).unwrap();

    let mut l = Library::open(&root).unwrap();
    let s = l.recover_incomplete_at(t(5)).unwrap();
    assert_eq!(s.recovered.len(), 1, "{:?}", s.failed);
    let r = &s.recovered[0];
    assert!(r.duration >= MediaTime::from_millis(2_900));
    let rec = l.get(&r.id).unwrap();
    assert_eq!(rec.kind, CaptureKind::Audio);
    let m = cm_library::read_manifest(&dir).unwrap();
    assert_eq!(m.markers.len(), 1);
    assert_eq!(m.markers[0].at, MediaTime::from_ticks(120_000));
    assert_eq!(m.markers[0].label.as_deref(), Some("before the crash"));
    assert_eq!(m.tracks.keys().collect::<Vec<_>>(), ["mic"]);
}

/// VOC-1.AC4 after stop: a marker on a finished recording is one revision tagged with who set
/// it and one undo step; markers stay sorted; a time outside the recording is refused.
#[test]
fn voc1_ac4_a_marker_on_a_finished_recording_is_a_revision_and_an_undo_step() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = Library::open_or_create(&tmp.path().join("lib")).unwrap();
    let id = finished_audio(&mut l);
    let dir = l.bundle_path(&l.get(&id).unwrap());
    let at = |s: i64| MediaTime::from_secs(s);

    let (m1, out) = apply_marker(&dir, at(30), Some("  Decision  "), Actor::User, t(2)).unwrap();
    assert_eq!(m1.label.as_deref(), Some("Decision"), "trimmed");
    assert_eq!(m1.by.as_deref(), Some("user"));
    assert_eq!(out.revision.action, "add_marker");
    assert_eq!(out.revision.by, Actor::User);
    assert!(out.can_undo);
    apply_marker(&dir, at(10), None, Actor::Agent("claude".into()), t(3)).unwrap();
    let m = cm_library::read_manifest(&dir).unwrap();
    assert_eq!(
        m.markers.iter().map(|x| x.at).collect::<Vec<_>>(),
        [at(10), at(30)],
        "sorted by time"
    );
    assert_eq!(m.markers[0].by.as_deref(), Some("agent:claude"));
    assert_eq!(m.markers[0].label, None);
    assert_eq!(
        m.revisions.last().unwrap().by,
        Actor::Agent("claude".into())
    );

    // Undo takes the agent's marker away, redo puts it back.
    step_history(&dir, true, Actor::User, t(4))
        .unwrap()
        .unwrap();
    let m = cm_library::read_manifest(&dir).unwrap();
    assert_eq!(m.markers.len(), 1);
    assert_eq!(m.markers[0].at, at(30));
    step_history(&dir, false, Actor::User, t(5))
        .unwrap()
        .unwrap();
    assert_eq!(cm_library::read_manifest(&dir).unwrap().markers.len(), 2);

    // Outside the recording, negative and over-long labels are `invalid_input`.
    for bad in [
        apply_marker(&dir, at(61), None, Actor::User, t(6)),
        apply_marker(&dir, MediaTime::from_ticks(-1), None, Actor::User, t(6)),
        apply_marker(&dir, at(1), Some(&"x".repeat(201)), Actor::User, t(6)),
    ] {
        assert_eq!(bad.err().unwrap().code(), ErrorCode::InvalidInput);
    }
    assert_eq!(cm_library::read_manifest(&dir).unwrap().markers.len(), 2);
    // The media was not touched.
    assert!(dir.join("mic.wav").is_file());
}

/// A library whose index was written before audio existed (version 2, a `kind` check without
/// `audio`) is migrated in place: the rows and the transcript search survive, and an audio
/// recording can then be indexed.
#[test]
fn index_version_2_is_migrated_in_place_and_keeps_transcripts() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let shot_id = {
        let mut l = Library::open_or_create(&root).unwrap();
        let shot = l
            .create_screenshot_without_poster(&png(64, 48, 1), Some("Old shot"), T0)
            .unwrap();
        let id = shot.manifest().id.clone();
        drop(shot);
        l.index_transcript(
            &id,
            &[cm_library::index::TranscriptSegment {
                start: MediaTime::from_secs(1),
                text: "needle in a haystack".into(),
            }],
        )
        .unwrap();
        id
    };
    // Step the index back to the version-2 table.
    let db = root.join(cm_library::index::INDEX_RELATIVE_PATH);
    {
        let c = rusqlite::Connection::open(&db).unwrap();
        c.execute_batch(
            "BEGIN;
             ALTER TABLE captures RENAME TO captures_now;
             CREATE TABLE captures (
               id TEXT PRIMARY KEY, path TEXT NOT NULL UNIQUE, title TEXT NOT NULL,
               kind TEXT NOT NULL CHECK (kind IN ('screenshot', 'recording')),
               duration INTEGER, created TEXT NOT NULL, created_unix INTEGER NOT NULL,
               size_bytes INTEGER NOT NULL DEFAULT 0, width INTEGER, height INTEGER,
               broken INTEGER NOT NULL DEFAULT 0,
               upload_count INTEGER NOT NULL DEFAULT 0, publish_count INTEGER NOT NULL DEFAULT 0);
             INSERT INTO captures SELECT * FROM captures_now;
             DROP TABLE captures_now;
             PRAGMA user_version = 2;
             COMMIT;",
        )
        .unwrap();
    }
    let mut l = Library::open(&root).unwrap();
    assert!(l.startup_rescan().is_none(), "migrated, not rebuilt");
    assert_eq!(l.get(&shot_id).unwrap().title, "Old shot");
    let hits = l.search("needle", &ListQuery::default()).unwrap();
    assert_eq!(hits.len(), 1, "the transcript search rows survived");
    let id = finished_audio(&mut l);
    assert_eq!(l.get(&id).unwrap().kind, CaptureKind::Audio);
}
