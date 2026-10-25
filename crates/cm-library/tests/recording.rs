//! `.cmrec` life cycle through the library: create, finish, crash, recover on next launch (REC-8).
#![allow(clippy::unwrap_used)]
mod common;
use cm_core::{ErrorCode, MediaTime, TrackInfo, TrackKind};
use cm_encode::fmp4::synth::{fragment, init_segment, Spec};
use cm_library::index::ListQuery;
use cm_library::manifest::CaptureKind;
use cm_library::rec::{SessionInfo, LOCK_FILE};
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

fn write_screen(dir: &Path, fragments: u32, cut_tail: usize) {
    let spec = Spec {
        width: 1920,
        height: 1080,
        ..Default::default()
    };
    let mut v = init_segment(&spec);
    for i in 0..fragments {
        v.extend(fragment(&spec, i + 1, u64::from(i) * 240_000, 60, 4000, 64));
    }
    if cut_tail > 0 {
        let f = fragment(
            &spec,
            fragments + 1,
            u64::from(fragments) * 240_000,
            60,
            4000,
            64,
        );
        v.extend(&f[..cut_tail]);
    }
    fs::write(dir.join("screen.mp4"), v).unwrap();
}

/// Rewrites the lock's pid, standing in for "the recording process died".
fn kill_owner(dir: &Path) {
    // A dead process leaves no flock; in one test process the guard lives, so also drop the owner
    // file: the pid check below it then says "dead".
    let _ = fs::remove_file(dir.join(cm_library::rec::OWNER_FILE));
    let p = dir.join(LOCK_FILE);
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    v["pid"] = json!(0);
    fs::write(p, serde_json::to_vec(&v).unwrap()).unwrap();
}

#[test]
fn finished_recording_is_indexed_with_duration_and_size() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = Library::open_or_create(&tmp.path().join("lib")).unwrap();
    let b = l
        .create_recording(None, T0, json!({"fps": 30}), &session())
        .unwrap();
    assert!(b.dir().ends_with("Recording 2026-10-19 09.12.cmrec"));
    // Not visible in the library while it is running; a rescan leaves it alone.
    let r = l.rescan().unwrap();
    assert_eq!(r.incomplete, ["Recording 2026-10-19 09.12.cmrec"]);
    assert!(l.list(&ListQuery::default()).unwrap().is_empty());
    write_screen(b.dir(), 3, 0);
    let info = TrackInfo {
        kind: TrackKind::Screen,
        file: "screen.mp4".into(),
        start: MediaTime::ZERO,
        end: MediaTime::from_secs(6),
        details: json!({"size": [1920, 1080], "fps": 30, "codec": "h264"}),
        stats: Default::default(),
    };
    let rec = l.finish_recording(b, &[info], t(1)).unwrap();
    assert_eq!(rec.kind, CaptureKind::Recording);
    assert_eq!(rec.duration, Some(MediaTime::from_secs(6)));
    assert_eq!((rec.width, rec.height), (Some(1920), Some(1080)));
    assert_eq!(l.list(&ListQuery::default()).unwrap(), vec![rec]);
    assert!(l.rescan().unwrap().incomplete.is_empty());
}

/// REC-8.AC1 at the library level: kill the recorder mid-fragment, relaunch, find the recording
/// with at least elapsed - 2 s of media.
#[test]
fn rec8_next_launch_recovers_a_crashed_recording() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let dir = {
        let l = Library::open_or_create(&root).unwrap();
        let b = l
            .create_recording(Some("Lesson"), T0, json!({}), &session())
            .unwrap();
        write_screen(b.dir(), 4, 900); // 8 s complete + a cut ninth-second fragment
        b.dir().to_owned()
    }; // crash: no finalize
    kill_owner(&dir);

    let mut l = Library::open(&root).unwrap();
    assert!(l.list(&ListQuery::default()).unwrap().is_empty());
    let s = l.recover_incomplete_at(t(5)).unwrap();
    assert!(s.failed.is_empty() && s.in_progress.is_empty());
    assert_eq!(s.recovered.len(), 1);
    let r = &s.recovered[0];
    assert_eq!(r.title, "Lesson");
    // Elapsed was about 9 s; at least elapsed - 2 s survives.
    assert!(r.duration >= MediaTime::from_secs(7));
    let rec = l.get(&r.id).unwrap();
    assert_eq!(rec.duration, Some(MediaTime::from_secs(8)));
    assert_eq!((rec.width, rec.height), (Some(1920), Some(1080)));
    assert!(!dir.join(LOCK_FILE).exists());
    // Running it again finds nothing to do.
    assert!(l.recover_incomplete_at(t(6)).unwrap().recovered.is_empty());
}

#[test]
fn a_live_owner_is_left_alone_and_a_hopeless_bundle_stays_for_later() {
    let tmp = tempfile::tempdir().unwrap();
    let mut l = Library::open_or_create(&tmp.path().join("lib")).unwrap();
    // This process owns the lock (pid = ours): it is "in progress", not recovered.
    let live = l
        .create_recording(Some("Live"), T0, json!({}), &session())
        .unwrap();
    write_screen(live.dir(), 2, 0);
    // Dead owner, no media at all.
    let hopeless = l
        .create_recording(Some("Hopeless"), T0, json!({}), &session())
        .unwrap();
    kill_owner(hopeless.dir());
    let s = l.recover_incomplete_at(t(1)).unwrap();
    assert_eq!(s.in_progress, ["Live.cmrec"]);
    assert_eq!(s.failed.len(), 1);
    assert_eq!(s.failed[0].0, "Hopeless.cmrec");
    assert_eq!(s.failed[0].1.code(), ErrorCode::InvalidInput);
    assert!(hopeless.dir().join(LOCK_FILE).exists());
    assert!(s.recovered.is_empty());
}

/// R3-17: a stale lock whose pid was reused by a live process is still recovered: the owner
/// file's advisory lock (held by the kernel for the life of the recording process) decides, not
/// the pid.
#[cfg(unix)]
#[test]
fn r3_17_a_reused_pid_does_not_hide_a_crashed_recording() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let dir = {
        let l = Library::open_or_create(&root).unwrap();
        let b = l
            .create_recording(Some("Reused"), T0, json!({}), &session())
            .unwrap();
        write_screen(b.dir(), 3, 0);
        b.dir().to_owned()
    }; // the recording process "died": the guard is gone, the owner file stays
    assert!(dir.join(cm_library::rec::OWNER_FILE).exists());
    // The lock names a pid that is alive (ours).
    let p = dir.join(LOCK_FILE);
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    v["pid"] = json!(std::process::id());
    fs::write(&p, serde_json::to_vec(&v).unwrap()).unwrap();
    let mut l = Library::open(&root).unwrap();
    let s = l.recover_incomplete_at(t(5)).unwrap();
    assert!(s.in_progress.is_empty(), "{:?}", s.in_progress);
    assert_eq!(s.recovered.len(), 1);
    assert!(!dir.join(LOCK_FILE).exists());
    assert!(!dir.join(cm_library::rec::OWNER_FILE).exists());
}

/// R3-5 / R3-18: a bundle with nothing usable is reported once and then skipped (not retried on
/// every launch), and one folder that fails never stops the others.
#[test]
fn r3_5_hopeless_bundles_are_quarantined_and_do_not_stop_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let (hopeless, good) = {
        let l = Library::open_or_create(&root).unwrap();
        let h = l
            .create_recording(Some("A hopeless"), T0, json!({}), &session())
            .unwrap();
        let g = l
            .create_recording(Some("B good"), T0, json!({}), &session())
            .unwrap();
        write_screen(g.dir(), 2, 0);
        (h.dir().to_owned(), g.dir().to_owned())
    };
    let mut l = Library::open(&root).unwrap();
    let s = l.recover_incomplete_at(t(1)).unwrap();
    assert_eq!(s.failed.len(), 1);
    assert_eq!(s.recovered.len(), 1, "the good one was still recovered");
    assert!(!good.join(LOCK_FILE).exists());
    assert!(hopeless.join(LOCK_FILE).exists());
    let s2 = l.recover_incomplete_at(t(2)).unwrap();
    assert!(s2.failed.is_empty(), "reported once");
    assert_eq!(s2.quarantined, ["A hopeless.cmrec"]);
}

/// R3-2 / REC-8.AC2: a recovered recording keeps its audio start offsets, channels, denoised file
/// and pauses, from the facts the session kept in the lock; a stale or too-long WAV header is
/// repaired from the file length.
#[test]
fn r3_2_recovered_audio_keeps_start_offsets_and_pauses() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let dir = {
        let l = Library::open_or_create(&root).unwrap();
        let mut b = l
            .create_recording(Some("Sync"), T0, json!({}), &session())
            .unwrap();
        write_screen(b.dir(), 4, 0); // 8 s of video
                                     // mic: first sample at 80 ms (9_600 ticks), 2 s of mono audio in mic.wav + denoised copy
        for name in ["mic.wav", "mic.denoised.wav"] {
            let mut w = cm_core::wav::WavWriter::create(&b.dir().join(name), 1, 48_000).unwrap();
            w.write(&vec![0.1f32; 96_000]).unwrap();
            w.finalize().unwrap();
        }
        // system: first sample at 280 ms, stereo, 1 s
        let mut w =
            cm_core::wav::WavWriter::create(&b.dir().join("system.wav"), 2, 48_000).unwrap();
        w.write(&vec![0.1f32; 96_000]).unwrap();
        w.finalize().unwrap();
        b.set_track_hint(
            "mic",
            json!({"start": 9_600, "file": "mic.denoised.wav", "raw_file": "mic.wav",
                   "denoised_file": "mic.denoised.wav", "channels": 1, "rate": 48_000}),
        )
        .unwrap();
        b.set_track_hint(
            "system",
            json!({"start": 33_600, "file": "system.wav", "raw_file": "system.wav",
                   "denoised_file": null, "channels": 2, "rate": 48_000}),
        )
        .unwrap();
        // a pause of 0.5 s at 1.0 s session time (inside both audio tracks)
        b.add_gap(cm_core::PauseGap {
            from: MediaTime::from_secs(1),
            to: MediaTime::from_millis(1500),
        })
        .unwrap();
        b.dir().to_owned()
    };
    // A power loss left the mic header claiming more than the disk holds (cut mid-frame) ...
    let f = fs::OpenOptions::new()
        .write(true)
        .open(dir.join("mic.wav"))
        .unwrap();
    f.set_len(cm_core::wav::WAV_HEADER_LEN + 4 * 90_000 + 2)
        .unwrap();
    drop(f);
    let mut l = Library::open(&root).unwrap();
    let s = l.recover_incomplete_at(t(5)).unwrap();
    assert_eq!(s.recovered.len(), 1, "{:?}", s.failed);
    let m = &s.recovered[0].manifest;
    let mic = &m.tracks["mic"];
    assert_eq!(mic["start"], 9_600, "80 ms, not 0");
    assert_eq!(mic["file"], "mic.denoised.wav");
    assert_eq!(mic["raw_file"], "mic.wav");
    assert_eq!(mic["channels"], 1);
    // 2 s of denoised audio from 80 ms plus the 0.5 s pause inside it
    assert_eq!(mic["end"], 9_600 + 240_000 + 60_000);
    assert_eq!(mic["pauses"][0]["from"], 120_000);
    assert_eq!(mic["pauses"][0]["to"], 180_000);
    assert_eq!(
        cm_core::wav::read_info(&dir.join("mic.wav"))
            .unwrap()
            .frames(),
        90_000,
        "the stale raw header was repaired from the file length"
    );
    let sys = &m.tracks["system"];
    assert_eq!(sys["start"], 33_600, "280 ms");
    assert_eq!(sys["channels"], 2);
    assert_eq!(sys["end"], 33_600 + 120_000 + 60_000);
    assert!(!sys.as_object().unwrap().contains_key("start_unknown"));
}
