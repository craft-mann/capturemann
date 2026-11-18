//! Quick Capture runs against a real daemon with fakes: no screen, no clipboard, no window. They
//! check what the panel and the presets ask of the existing actions (QC-1, QC-3, QC-4, QC-5).

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_core::preset::{PresetImageFormat, PresetKind};
use cm_daemon::preset_ops::PresetStore;
use cm_daemon::testing::{ScriptedRecognizer, TextLine};
use cm_daemon::Daemon;
use serde_json::{json, Value};

use super::quick_flow::{redact, remove_capture, save_to};
use crate::backend::Backend;
use crate::quick::run::{Run, ShotRun};
use crate::quick::state::{PanelState, Tile};

struct Fx {
    rt: tokio::runtime::Runtime,
    d: Daemon,
    dir: tempfile::TempDir,
}

fn fx() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let lib = cm_library::Library::open_or_create(&dir.path().join("lib")).unwrap();
    let rec = ScriptedRecognizer::new(vec![
        TextLine::laid_out("Owner: jane.doe@example.com", 5.0, 5.0, 4.0, 10.0),
        TextLine::laid_out("Billing: ap@vendor.example.org", 5.0, 40.0, 4.0, 10.0),
    ]);
    let d = Daemon::builder(
        Capturer::new(FakeBackend::new().with_display(1, 0, 0, 160, 80, 1.0)).with_own_pid(None),
        lib,
    )
    .text_recognizer(rec)
    .presets(PresetStore::in_dir(&dir.path().join("cfg")))
    .build();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    Fx { rt, d, dir }
}

async fn capture(d: &Daemon) -> String {
    let v = Backend::call(
        d,
        "capture_screenshot",
        json!({"target": {"display_id": 1}}),
    )
    .await
    .unwrap();
    v["capture_id"].as_str().unwrap().to_owned()
}

fn run_of(s: &PanelState) -> ShotRun {
    match s.run() {
        Run::Shot(r) => r,
        other => panic!("{other:?}"),
    }
}

/// QC-4.AC7: a preset with `Save to ~/Bugs/{date}` writes the picture into that folder with the
/// local date, and the capture is also in the library.
#[test]
fn qc4_ac7_the_capture_goes_to_the_dated_folder_and_the_library() {
    let f = fx();
    let mut s = PanelState::default();
    let mut p = cm_core::preset::starter_presets()[0].clone();
    p.save_to = Some("~/Bugs/{date}".into());
    s.apply_preset(&p);
    let run = run_of(&s);
    f.rt.block_on(async {
        let id = capture(&f.d).await;
        let date = crate::save_folder::local_now().date();
        let dir = crate::quick::files::resolve_dir(
            run.save_to.as_deref().unwrap(),
            date,
            Some(f.dir.path()),
        );
        assert!(!dir.exists(), "the folder is made by the run");
        let written = save_to(&f.d, &dir, &run, &id).await.unwrap();
        let stamp = format!(
            "{:04}-{:02}-{:02}",
            date.year(),
            u8::from(date.month()),
            date.day()
        );
        assert!(written.contains(&format!("Bugs/{stamp}")), "{written}");
        assert!(std::path::Path::new(&written).is_file());
        let listed = Backend::call(&f.d, "list_library", json!({}))
            .await
            .unwrap();
        assert!(listed.to_string().contains(&id), "also in the library");
    });
}

/// QC-4: a preset's format and scale shape the folder copy; the library original is untouched.
#[test]
fn a_preset_folder_copy_follows_its_format_and_scale() {
    let f = fx();
    let mut run = run_of(&PanelState::default());
    run.format = Some(PresetImageFormat::Jpeg);
    run.scale = Some(1);
    f.rt.block_on(async {
        let id = capture(&f.d).await;
        let dir = f.dir.path().join("out");
        let written = save_to(&f.d, &dir, &run, &id).await.unwrap();
        assert!(written.ends_with(".jpg"), "{written}");
        let img = image::open(&written).unwrap();
        assert_eq!((img.width(), img.height()), (80, 40));
        let (_, png) = Backend::capture_png(&f.d, &id).await.unwrap();
        let orig = image::load_from_memory(&png).unwrap();
        assert_eq!((orig.width(), orig.height()), (160, 80));
    });
}

/// QC-1 (the Auto-redact tile): the proposals are blurred in one step, which one undo removes.
#[test]
fn the_auto_redact_tile_blurs_what_the_daemon_proposes_in_one_step() {
    let f = fx();
    f.rt.block_on(async {
        let id = capture(&f.d).await;
        assert_eq!(redact(&f.d, &id).await.unwrap(), 2);
        let snap = crate::editor::session::fetch_snapshot(&f.d, &id)
            .await
            .unwrap();
        assert_eq!(snap.ops.len(), 2);
        Backend::call(&f.d, "undo", json!({ "capture_id": id }))
            .await
            .unwrap();
        let snap = crate::editor::session::fetch_snapshot(&f.d, &id)
            .await
            .unwrap();
        assert!(snap.ops.is_empty(), "one undo removes both blurs");
    });
}

/// QC-3.AC5: Undo removes the capture from the library.
#[test]
fn qc3_ac5_undo_removes_the_capture() {
    let f = fx();
    f.rt.block_on(async {
        let id = capture(&f.d).await;
        remove_capture(&f.d, &id).await.unwrap();
        let listed = Backend::call(&f.d, "list_library", json!({}))
            .await
            .unwrap();
        assert!(!listed.to_string().contains(&id));
        assert!(remove_capture(&f.d, &id).await.is_err(), "already gone");
    });
}

/// QC-4.AC2 and AC3 through the daemon: "Save current as preset" writes the panel's setup to
/// `presets.json`, reading it back gives the same setup, and a field this version does not know
/// stays in the file.
#[test]
fn qc4_ac2_ac3_save_current_as_preset_round_trips_through_the_file() {
    let f = fx();
    let mut s = PanelState::default();
    s.toggle(Tile::Redact);
    s.set_value(Tile::Delay, 5);
    f.rt.block_on(async {
        let saved = Backend::call(
            &f.d,
            "save_preset",
            json!({ "preset": s.to_preset_json("My shot") }),
        )
        .await
        .unwrap();
        let store = f.d.preset_store().unwrap();
        let id = saved
            .pointer("/preset/id")
            .and_then(Value::as_str)
            .or_else(|| saved.get("id").and_then(Value::as_str))
            .unwrap_or("my-shot")
            .to_owned();
        let loaded = store.find(&id).unwrap();
        let mut back = PanelState::default();
        back.apply_preset(&loaded);
        assert_eq!(back.mem.shot, s.mem.shot);
        assert_eq!(back.target(), s.target());
        // A hand-added field survives the next save.
        let mut v: Value =
            serde_json::from_str(&std::fs::read_to_string(store.path()).unwrap()).unwrap();
        v["team"] = json!("docs");
        std::fs::write(store.path(), serde_json::to_string(&v).unwrap()).unwrap();
        Backend::call(
            &f.d,
            "save_preset",
            json!({ "preset": s.to_preset_json("Another") }),
        )
        .await
        .unwrap();
        let after: Value =
            serde_json::from_str(&std::fs::read_to_string(store.path()).unwrap()).unwrap();
        assert_eq!(after["team"], "docs");
    });
}

/// QC-4.AC5 and QC-5: a preset's shortcut run reads the call the way an agent's `preset` argument
/// is read, and agrees with the panel's run of the same preset.
#[test]
fn a_shortcut_run_reads_the_daemons_resolved_call() {
    let f = fx();
    let loaded = f.d.preset_store().unwrap().load();
    assert_eq!(loaded.presets.len(), 4, "the starters");
    for p in &loaded.presets {
        let method = if p.kind == PresetKind::Screenshot {
            "capture_screenshot"
        } else {
            "start_recording"
        };
        let resolved =
            f.d.resolve_preset_params(method, &json!({ "preset": p.id }))
                .unwrap();
        let mut panel = PanelState::default();
        panel.apply_preset(p);
        assert_eq!(
            crate::quick::run::run_from_resolved(p, &resolved),
            panel.run(),
            "{}",
            p.name
        );
    }
    let e =
        f.d.resolve_preset_params("capture_screenshot", &json!({ "preset": "nope" }))
            .unwrap_err();
    assert!(e.to_string().contains("nope"));
}

/// QC-4.AC4: a hand-edited file with one invalid preset skips only that one, with one notice.
#[test]
fn qc4_ac4_one_invalid_preset_is_skipped_with_one_notice() {
    let f = fx();
    let store = f.d.preset_store().unwrap().clone();
    std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    std::fs::write(
        store.path(),
        r#"{"presets":[{"id":"a","name":"Fine","kind":"screenshot","target":"region"},
                       {"id":"b","name":"Broken","kind":"video","target":"long_shot"}]}"#,
    )
    .unwrap();
    let loaded = store.load();
    assert_eq!(loaded.presets.len(), 1);
    let n = loaded.notice.unwrap();
    assert!(n.contains("Broken") && !n.contains("Fine"), "{n}");
}

/// QC-4.AC6: deleting the starters does not bring them back on the next read.
#[test]
fn qc4_ac6_deleted_starters_stay_deleted() {
    let f = fx();
    let store = f.d.preset_store().unwrap().clone();
    for p in store.load().presets {
        store.delete(&p.id).unwrap();
    }
    assert!(store.load().presets.is_empty());
    assert!(
        store.load().presets.is_empty(),
        "not on the next start either"
    );
}
