//! The library watcher (LIB-1 file watcher, LIB-2): bundles that appear, change, move or vanish
//! from outside the daemon are indexed incrementally and announced. The state machine is driven
//! with a manual millisecond clock, so nothing sleeps; one test runs the real task on paused
//! tokio time.

#![allow(clippy::unwrap_used)]

mod common;

use cm_daemon::watcher::{scan, Change, WatchOptions, WatchState};
use cm_library::Library;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use time::macros::datetime;

fn png() -> Vec<u8> {
    let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(4, 4));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

/// A bundle made in a scratch library (so it is "from another Mac"): returns its folder.
fn foreign_bundle(stage: &Path, title: &str) -> PathBuf {
    let mut lib = Library::open_or_create(stage).unwrap();
    let b = lib
        .create_screenshot(&png(), Some(title), datetime!(2026-10-23 10:00 UTC))
        .unwrap();
    b.dir().to_owned()
}

struct Rig {
    dir: tempfile::TempDir,
    stage: tempfile::TempDir,
    lib: Mutex<Library>,
    st: WatchState,
}

impl Rig {
    /// A library with a finished baseline scan.
    fn new() -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let rig = Rig {
            lib: Mutex::new(Library::open_or_create(dir.path()).unwrap()),
            dir,
            stage: tempfile::tempdir().unwrap(),
            st: WatchState::new(WatchOptions::default()),
        };
        rig
    }

    fn baseline(&mut self) {
        let t = self.st.tick(&self.lib, 0);
        assert!(t.available && t.changes.is_empty());
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn tick(&mut self, ms: u64) -> Vec<Change> {
        let t = self.st.tick(&self.lib, ms);
        assert!(t.error.is_none(), "{:?}", t.error);
        t.changes
    }

    fn count(&self) -> usize {
        self.lib.lock().unwrap().count().unwrap()
    }

    /// Copies a foreign bundle named `title` into the library, returns its folder name.
    fn copy_in(&self, title: &str) -> String {
        let src = foreign_bundle(self.stage.path(), title);
        let name = src.file_name().unwrap().to_string_lossy().into_owned();
        copy_dir(&src, &self.root().join(&name));
        name
    }
}

#[test]
fn a_copied_in_bundle_appears_once_it_settles() {
    let mut r = Rig::new();
    r.baseline();
    let name = r.copy_in("From elsewhere");
    assert!(r.tick(100).is_empty(), "not before it settled");
    assert!(r.tick(900).is_empty());
    let ch = r.tick(1200);
    assert_eq!(ch.len(), 1, "{ch:?}");
    assert!(
        matches!(&ch[0], Change::Created { title, broken: false, .. } if title == "From elsewhere")
    );
    let row = r.lib.lock().unwrap().get_by_path(&name).unwrap().unwrap();
    assert_eq!(row.title, "From elsewhere");
    // Nothing more happens, and a full rescan agrees (incremental == rescan).
    assert!(r.tick(5000).is_empty());
    let rep = r.lib.lock().unwrap().rescan().unwrap();
    assert_eq!((rep.indexed, rep.removed), (1, 0));
}

#[test]
fn a_bundle_is_indexed_only_when_its_manifest_parses() {
    let mut r = Rig::new();
    r.baseline();
    // The folder arrives first, the manifest last (as in a copy that goes file by file).
    let src = foreign_bundle(r.stage.path(), "Slow copy");
    let name = src.file_name().unwrap().to_string_lossy().into_owned();
    let dst = r.root().join(&name);
    copy_dir(&src, &dst);
    let manifest = std::fs::read(dst.join("manifest.json")).unwrap();
    std::fs::remove_file(dst.join("manifest.json")).unwrap();
    assert!(r.tick(100).is_empty());
    assert!(r.tick(2000).is_empty(), "no manifest: nothing indexed");
    assert_eq!(r.count(), 0);
    // A half-written manifest does not parse either.
    std::fs::write(dst.join("manifest.json"), &manifest[..manifest.len() / 2]).unwrap();
    assert!(r.tick(2100).is_empty());
    assert!(r.tick(4000).is_empty());
    assert_eq!(r.count(), 0);
    // The rest arrives: indexed one settle later, once.
    std::fs::write(dst.join("manifest.json"), &manifest).unwrap();
    assert!(r.tick(4100).is_empty());
    let ch = r.tick(5200);
    assert!(
        matches!(&ch[..], [Change::Created { broken: false, .. }]),
        "{ch:?}"
    );
    assert!(r.tick(9000).is_empty());
    assert_eq!(r.count(), 1);
}

#[test]
fn a_malformed_manifest_becomes_a_broken_row_like_rescan() {
    let mut r = Rig::new();
    r.baseline();
    let bad = r.root().join("Garbled.cmshot");
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(bad.join("manifest.json"), b"{ not json").unwrap();
    assert!(r.tick(100).is_empty());
    assert!(r.tick(5000).is_empty(), "still within the grace period");
    assert_eq!(r.count(), 0);
    let ch = r.tick(11_200);
    assert!(
        matches!(&ch[..], [Change::Created { id, broken: true, .. }] if id == "broken:Garbled.cmshot"),
        "{ch:?}"
    );
    let rep = r.lib.lock().unwrap().rescan().unwrap();
    assert_eq!(rep.broken, vec!["Garbled.cmshot".to_owned()]);
    assert_eq!(r.count(), 1);
    // Fixing the manifest replaces the broken row.
    let good = foreign_bundle(r.stage.path(), "Fixed");
    std::fs::copy(good.join("manifest.json"), bad.join("manifest.json")).unwrap();
    r.tick(20_000);
    let ch = r.tick(21_100);
    assert!(
        matches!(&ch[..], [Change::Created { broken: false, .. }]),
        "{ch:?}"
    );
    assert_eq!(r.count(), 1);
    assert!(
        !r.lib
            .lock()
            .unwrap()
            .get_by_path("Garbled.cmshot")
            .unwrap()
            .unwrap()
            .broken
    );
}

#[test]
fn a_deleted_bundle_disappears() {
    let mut r = Rig::new();
    let name = r.copy_in("Doomed");
    r.baseline();
    r.lib.lock().unwrap().rescan().unwrap();
    assert_eq!(r.count(), 1);
    std::fs::remove_dir_all(r.root().join(&name)).unwrap();
    assert!(r.tick(100).is_empty());
    let ch = r.tick(1300);
    assert!(
        matches!(&ch[..], [Change::Deleted { title, .. }] if title == "Doomed"),
        "{ch:?}"
    );
    assert_eq!(r.count(), 0);
    assert!(r.tick(3000).is_empty());
}

#[test]
fn a_renamed_folder_changes_the_row_not_delete_and_create() {
    let mut r = Rig::new();
    let name = r.copy_in("Original");
    r.baseline();
    r.lib.lock().unwrap().rescan().unwrap();
    std::fs::rename(
        r.root().join(&name),
        r.root().join("Renamed in Finder.cmshot"),
    )
    .unwrap();
    assert!(r.tick(100).is_empty());
    let ch = r.tick(1300);
    assert!(
        matches!(&ch[..], [Change::Revised { renamed: true, .. }]),
        "{ch:?}"
    );
    assert_eq!(r.count(), 1);
    let lib = r.lib.lock().unwrap();
    assert!(lib.get_by_path(&name).unwrap().is_none());
    assert!(lib
        .get_by_path("Renamed in Finder.cmshot")
        .unwrap()
        .is_some());
}

#[test]
fn a_second_copy_of_an_indexed_capture_is_ignored() {
    let mut r = Rig::new();
    let name = r.copy_in("Twin");
    r.baseline();
    r.lib.lock().unwrap().rescan().unwrap();
    copy_dir(&r.root().join(&name), &r.root().join("Twin (2).cmshot"));
    r.tick(100);
    assert!(r.tick(1300).is_empty());
    assert_eq!(r.count(), 1);
    assert!(r.lib.lock().unwrap().get_by_path(&name).unwrap().is_some());
}

#[test]
fn an_external_manifest_edit_is_a_revision() {
    let mut r = Rig::new();
    let name = r.copy_in("Edited elsewhere");
    r.baseline();
    r.lib.lock().unwrap().rescan().unwrap();
    let m = r.root().join(&name).join("manifest.json");
    let text = std::fs::read_to_string(&m).unwrap();
    std::fs::write(&m, text.replace("Edited elsewhere", "Edited elsewhere!")).unwrap();
    r.tick(100);
    let ch = r.tick(1300);
    assert!(
        matches!(&ch[..], [Change::Revised { title, renamed: false, .. }] if title == "Edited elsewhere!"),
        "{ch:?}"
    );
}

#[test]
fn a_bundle_with_a_recording_lock_is_ignored_until_the_lock_goes() {
    let mut r = Rig::new();
    r.baseline();
    let name = r.copy_in("Still recording");
    std::fs::write(r.root().join(&name).join("recording.lock"), b"{}").unwrap();
    r.tick(100);
    assert!(r.tick(1300).is_empty());
    assert!(r.tick(9000).is_empty());
    assert_eq!(r.count(), 0, "an unfinished recording is never indexed");
    std::fs::remove_file(r.root().join(&name).join("recording.lock")).unwrap();
    r.tick(9100);
    let ch = r.tick(10_300);
    assert!(matches!(&ch[..], [Change::Created { .. }]), "{ch:?}");
    assert_eq!(r.count(), 1);
}

#[test]
fn the_daemons_own_writes_are_indexed_but_not_announced_again() {
    let mut r = Rig::new();
    let name = r.copy_in("Mine");
    r.baseline();
    let id = {
        let mut lib = r.lib.lock().unwrap();
        lib.rescan().unwrap();
        lib.get_by_path(&name).unwrap().unwrap().id
    };
    // The daemon edits the manifest and announces `capture.revised`.
    let m = r.root().join(&name).join("manifest.json");
    let text = std::fs::read_to_string(&m).unwrap();
    std::fs::write(&m, format!("{text}\n")).unwrap();
    r.st.note_own(&id, 150);
    r.tick(200);
    assert!(r.tick(1400).is_empty(), "own write: no second event");
    // The same edit from outside is announced.
    std::fs::write(&m, format!("{text}\n\n")).unwrap();
    r.tick(20_000);
    assert_eq!(r.tick(21_200).len(), 1);
}

#[test]
fn export_growth_temp_files_and_hidden_folders_cause_no_work() {
    let mut r = Rig::new();
    let name = r.copy_in("Exporting");
    r.baseline();
    r.lib.lock().unwrap().rescan().unwrap();
    let bundle = r.root().join(&name);
    std::fs::create_dir(bundle.join("exports")).unwrap();
    r.tick(100);
    r.tick(1300); // the new `exports/` folder touches the bundle folder once: no change in row
    let mut t = 2000;
    for i in 0..20 {
        // Growth of the export file and the atomic temp files of the daemon's own writes.
        std::fs::write(
            bundle.join("exports").join("out.mp4"),
            vec![0u8; 1000 * (i + 1)],
        )
        .unwrap();
        std::fs::create_dir_all(r.root().join(format!("Next.cmshot.tmp-{i}"))).unwrap();
        std::fs::create_dir_all(r.root().join(".capturemann/scratch.cmshot")).unwrap();
        assert!(r.tick(t).is_empty(), "round {i}");
        t += 400;
    }
    assert_eq!(r.count(), 1);
}

#[test]
fn an_unavailable_folder_pauses_without_forgetting_and_resumes() {
    let mut r = Rig::new();
    let name = r.copy_in("On the drive");
    r.baseline();
    r.lib.lock().unwrap().rescan().unwrap();
    // Unplug: the folder is gone. Many ticks, no changes, no removals, no errors.
    let away = r.root().with_extension("unplugged");
    std::fs::rename(r.root(), &away).unwrap();
    for t in [100, 1500, 3000, 20_000, 40_000] {
        let tick = r.st.tick(&r.lib, t);
        assert!(!tick.available && tick.changes.is_empty() && tick.error.is_none());
    }
    // Re-plug with one more bundle and one gone.
    std::fs::rename(&away, r.root()).unwrap();
    let extra = r.copy_in("Added while away");
    std::fs::remove_dir_all(r.root().join(&name)).unwrap();
    let tick = r.st.tick(&r.lib, 41_000);
    assert!(tick.available && tick.changes.is_empty(), "settling first");
    let ch = r.tick(42_200);
    assert_eq!(ch.len(), 2, "{ch:?}");
    assert!(ch
        .iter()
        .any(|c| matches!(c, Change::Created { title, .. } if title == "Added while away")));
    assert!(ch
        .iter()
        .any(|c| matches!(c, Change::Deleted { title, .. } if title == "On the drive")));
    assert!(r.lib.lock().unwrap().get_by_path(&extra).unwrap().is_some());
}

#[test]
fn a_changed_root_starts_over_silently() {
    let mut r = Rig::new();
    r.copy_in("Old library");
    r.baseline();
    let other = tempfile::tempdir().unwrap();
    let mut st = WatchState::new(WatchOptions::default());
    let lib = Mutex::new(Library::open_or_create(other.path()).unwrap());
    st.tick(&r.lib, 0);
    // Pointed at another library: the first scan there is a baseline, not a flood of events.
    let t = st.tick(&lib, 10);
    assert!(t.changes.is_empty() && t.available);
}

#[test]
fn scan_cost_of_a_big_library() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..2000 {
        let b = dir.path().join(format!("Capture {i:05}.cmshot"));
        std::fs::create_dir(&b).unwrap();
        std::fs::write(b.join("manifest.json"), b"{}").unwrap();
        std::fs::write(b.join("original.png"), b"x").unwrap();
    }
    std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
    let mut best = Duration::MAX;
    for _ in 0..5 {
        let t = std::time::Instant::now();
        let s = scan(dir.path()).unwrap();
        best = best.min(t.elapsed());
        assert_eq!(s.len(), 2000);
    }
    println!("scan of 2,000 bundle folders: best of 5 = {best:?}");
    assert!(best < Duration::from_secs(1), "{best:?}");
}

// ---- the task -------------------------------------------------------------------------------

async fn next_watcher_event(
    rx: &mut tokio::sync::broadcast::Receiver<cm_daemon::events::Event>,
) -> Option<cm_daemon::events::Event> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let ev = rx.recv().await.unwrap();
            if ev.data.get("by").and_then(|b| b.as_str()) == Some("watcher") {
                return ev;
            }
        }
    })
    .await
    .ok()
}

#[tokio::test(start_paused = true)]
async fn the_task_publishes_events_and_stops_with_stop() {
    let (b, dir) = common::builder(common::desktop());
    let daemon = b.build();
    let mut rx = daemon.subscribe();
    daemon
        .start_library_watcher_with(WatchOptions {
            poll: Duration::from_millis(200),
            debounce: Duration::from_millis(400),
            broken_grace: Duration::from_secs(5),
        })
        .unwrap();
    // Idempotent.
    daemon.start_library_watcher().unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await; // baseline done
    let stage = tempfile::tempdir().unwrap();
    let src = foreign_bundle(stage.path(), "Dropped in");
    let name = src.file_name().unwrap().to_string_lossy().into_owned();
    copy_dir(&src, &dir.path().join(&name));
    let ev = next_watcher_event(&mut rx).await.expect("capture.created");
    assert_eq!(ev.name, "capture.created");
    assert_eq!(ev.data["title"], "Dropped in");
    assert_eq!(ev.data["external"], true);
    // The index has it: list_library sees it.
    let out = daemon
        .execute_method(
            cm_core::Actor::User,
            0,
            "list_library",
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    assert_eq!(
        out["items"]
            .as_array()
            .map(Vec::len)
            .or(out["captures"].as_array().map(Vec::len)),
        Some(1),
        "{out}"
    );
    // Deleting it announces capture.deleted.
    std::fs::remove_dir_all(dir.path().join(&name)).unwrap();
    let ev = next_watcher_event(&mut rx).await.expect("capture.deleted");
    assert_eq!(ev.name, "capture.deleted");
    // Stopped: a new bundle is not noticed.
    daemon.stop_library_watcher();
    let src = foreign_bundle(stage.path(), "Too late");
    let name = src.file_name().unwrap().to_string_lossy().into_owned();
    copy_dir(&src, &dir.path().join(name));
    assert!(next_watcher_event(&mut rx).await.is_none());
}

async fn next_library_event(
    rx: &mut tokio::sync::broadcast::Receiver<cm_daemon::events::Event>,
) -> Option<cm_daemon::events::Event> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let ev = rx.recv().await.unwrap();
            if ev.name.starts_with("library.") {
                return ev;
            }
        }
    })
    .await
    .ok()
}

/// The tray's Attention state needs to know when the library folder goes away and comes back.
#[tokio::test(start_paused = true)]
async fn the_task_announces_an_offline_library_once_and_its_return() {
    let (b, dir) = common::builder(common::desktop());
    let daemon = b.build();
    let mut rx = daemon.subscribe();
    daemon
        .start_library_watcher_with(WatchOptions {
            poll: Duration::from_millis(200),
            debounce: Duration::from_millis(400),
            broken_grace: Duration::from_secs(5),
        })
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let away = dir.path().with_extension("unplugged");
    std::fs::rename(dir.path(), &away).unwrap();
    let mut seen = Vec::new();
    let ev = next_library_event(&mut rx).await.expect("library.offline");
    seen.push(ev.name.clone());
    assert!(ev.data["path"].is_string());
    // Several more polls while it is away: no second event.
    tokio::time::sleep(Duration::from_secs(2)).await;
    std::fs::rename(&away, dir.path()).unwrap();
    let ev = next_library_event(&mut rx).await.expect("library.online");
    seen.push(ev.name);
    assert_eq!(seen, ["library.offline", "library.online"]);
}
