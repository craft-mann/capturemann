//! Quick Capture presets through the daemon (QC-4, QC-5): the file, the actions, `preset` on
//! `capture_screenshot` and `start_recording`, and the confirmation of a recording preset.
//! No hardware: fake displays and fake recording sources.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_capture::disk::FakeFreeSpace;
use cm_core::{Actor, ErrorCode, ManualHostClock};
use cm_daemon::preset_ops::PresetStore;
use cm_daemon::testing::{FakeSources, FixedConfirmer};
use cm_daemon::{ConfirmationRequest, Confirmer, Daemon, DaemonError, Decision, EngineOptions};
use common::{builder, desktop};
use serde_json::{json, Value};

struct ArcConfirm(Arc<FixedConfirmer>);

#[async_trait]
impl Confirmer for ArcConfirm {
    async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
        self.0.confirm(r).await
    }
}

struct Rig {
    daemon: Daemon,
    confirmer: Arc<FixedConfirmer>,
    file: std::path::PathBuf,
    sources: FakeSources,
    _dir: tempfile::TempDir,
    _cfg: tempfile::TempDir,
}

fn rig(decision: Decision) -> Rig {
    let cfg = tempfile::tempdir().unwrap();
    let file = cfg.path().join("presets.json");
    let (b, dir) = builder(desktop());
    let sources = FakeSources::new();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let daemon = b
        .presets(PresetStore::in_dir(cfg.path()))
        .recording_sources(sources.clone())
        .recording_options(EngineOptions {
            tick_every: Duration::from_millis(20),
            countdown_step: Duration::from_millis(30),
            host: Arc::new(ManualHostClock::new(1_000_000_000)),
            free_space: Arc::new(FakeFreeSpace::new(500_000_000_000)),
            disk_check_every: Duration::ZERO,
            own_pid: None,
            ..EngineOptions::default()
        })
        .confirmer(ArcConfirm(confirmer.clone()))
        .confirm_timeout(Duration::from_secs(5))
        .build();
    Rig {
        daemon,
        confirmer,
        file,
        sources,
        _dir: dir,
        _cfg: cfg,
    }
}

fn agent() -> Actor {
    Actor::Agent("claude".into())
}

async fn call(d: &Daemon, who: Actor, m: &str, p: Value) -> Result<Value, DaemonError> {
    d.execute_method(who, 0, m, &p).await
}

fn code(e: &DaemonError) -> ErrorCode {
    match e {
        DaemonError::Cm(c) => c.code(),
        DaemonError::WithData { error, .. } => error.code(),
        DaemonError::Declined { .. } => ErrorCode::UserDeclined,
        other => panic!("unexpected {other:?}"),
    }
}

fn names(v: &Value) -> Vec<String> {
    v["presets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_owned())
        .collect()
}

/// QC-5.AC2: `list_presets` returns every preset with name, kind and target, and writes nothing
/// once the file exists. The first call writes the four starters.
#[tokio::test]
async fn qc5_ac2_list_presets_reads_only() {
    let r = rig(Decision::Approve);
    assert!(!r.file.exists());
    let v = call(&r.daemon, agent(), "list_presets", json!({}))
        .await
        .unwrap();
    assert_eq!(
        names(&v),
        ["Bug report", "Docs shot", "Demo video", "Meeting"]
    );
    let bug = &v["presets"][0];
    assert_eq!(bug["kind"], "screenshot");
    assert_eq!(bug["target"], "region");
    assert!(bug["summary"].as_str().unwrap().contains("Region"));
    assert_eq!(v["presets"][3]["kind"], "audio");
    assert!(v["notice"].is_null());
    let before = std::fs::read_to_string(&r.file).unwrap();
    call(&r.daemon, agent(), "list_presets", json!({}))
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&r.file).unwrap(), before);
}

/// QC-4.AC6: the four starters deleted stay deleted on the next start.
#[tokio::test]
async fn qc4_ac6_deleted_starters_do_not_come_back() {
    let r = rig(Decision::Approve);
    for n in ["Bug report", "docs-shot", "Demo video", "meeting"] {
        call(
            &r.daemon,
            Actor::User,
            "delete_preset",
            json!({"preset": n}),
        )
        .await
        .unwrap();
    }
    // A second store on the same file ("the next start").
    let again = PresetStore::new(&r.file);
    assert!(again.load().presets.is_empty());
    let v = call(&r.daemon, Actor::User, "list_presets", json!({}))
        .await
        .unwrap();
    assert!(names(&v).is_empty());
}

/// QC-4.AC2 and AC3: a saved preset reads back as the same setup, and a field this version does
/// not know is still in the file afterwards.
#[tokio::test]
async fn qc4_ac2_ac3_save_round_trips_and_keeps_unknown_fields() {
    let r = rig(Decision::Approve);
    std::fs::write(
        &r.file,
        r#"{"team":"docs","starters_seeded":true,"presets":[
          {"id":"a","name":"A","kind":"screenshot","target":"window","badge":"blue",
           "options":{"glow":1}}]}"#,
    )
    .unwrap();
    let saved = call(
        &r.daemon,
        Actor::User,
        "save_preset",
        json!({"preset": {
            "name": "Bugs", "kind": "screenshot", "target": "region",
            "save_to": "~/Bugs/{date}", "options": {"delay_s": 2},
            "after": {"copy_to_clipboard": true, "open_in_editor": true, "tool": "step"},
            "shortcut": "Ctrl+Shift+B"
        }}),
    )
    .await
    .unwrap();
    assert_eq!(saved["preset"]["id"], "bugs");
    let listed = call(&r.daemon, Actor::User, "list_presets", json!({}))
        .await
        .unwrap();
    assert_eq!(names(&listed), ["A", "Bugs"]);
    assert_eq!(listed["presets"][1]["save_to"], "~/Bugs/{date}");
    assert_eq!(listed["presets"][1]["after"]["tool"], "step");
    let file: Value = serde_json::from_str(&std::fs::read_to_string(&r.file).unwrap()).unwrap();
    assert_eq!(file["team"], "docs");
    assert_eq!(file["presets"][0]["badge"], "blue");
    assert_eq!(file["presets"][0]["options"]["glow"], 1);
    // A second preset cannot take the shortcut; the error names the owner.
    let e = call(
        &r.daemon,
        Actor::User,
        "save_preset",
        json!({"preset": {"name": "Other", "kind": "audio", "shortcut": "ctrl+shift+b"}}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("Bugs"), "{e}");
    // Reorder.
    let o = call(
        &r.daemon,
        Actor::User,
        "reorder_presets",
        json!({"ids": ["bugs", "a"]}),
    )
    .await
    .unwrap();
    assert_eq!(names(&o), ["Bugs", "A"]);
}

/// QC-4.AC4: one invalid preset in a hand-edited file is skipped and one notice names it.
#[tokio::test]
async fn qc4_ac4_one_notice_names_the_skipped_preset() {
    let r = rig(Decision::Approve);
    std::fs::write(
        &r.file,
        r#"{"presets":[
          {"id":"ok","name":"Fine","kind":"video","target":"display"},
          {"id":"bad","name":"Broken","kind":"video","target":"long_shot"}]}"#,
    )
    .unwrap();
    let v = call(&r.daemon, Actor::User, "list_presets", json!({}))
        .await
        .unwrap();
    assert_eq!(names(&v), ["Fine"]);
    let n = v["notice"].as_str().unwrap();
    assert!(n.contains("\"Broken\"") && !n.contains("Fine"), "{n}");
}

/// Presets are the user's own setup: an agent can read and run them, not change them.
#[tokio::test]
async fn an_agent_cannot_change_presets() {
    let r = rig(Decision::Approve);
    for (m, p) in [
        (
            "save_preset",
            json!({"preset": {"name": "X", "kind": "audio"}}),
        ),
        ("delete_preset", json!({"preset": "Meeting"})),
        ("reorder_presets", json!({"ids": ["meeting"]})),
    ] {
        let e = call(&r.daemon, agent(), m, p).await.unwrap_err();
        assert!(matches!(e, DaemonError::Declined { .. }), "{m}: {e:?}");
    }
    let v = call(&r.daemon, agent(), "list_presets", json!({}))
        .await
        .unwrap();
    assert_eq!(names(&v).len(), 4);
}

/// QC-5.AC1 and AC3: the call with a preset equals the call with the options written out, and
/// an explicit argument in the same call wins.
#[tokio::test]
async fn qc5_ac1_ac3_the_preset_fills_what_the_call_leaves_out() {
    let r = rig(Decision::Approve);
    call(
        &r.daemon,
        Actor::User,
        "save_preset",
        json!({"preset": {"id": "bug-report", "name": "Bug report", "kind": "screenshot",
            "target": "region", "options": {"delay_s": 5},
            "after": {"copy_to_clipboard": true, "auto_redact": true}}}),
    )
    .await
    .unwrap();
    let t = json!({"display_id": 1});
    let resolved = r
        .daemon
        .resolve_preset_params(
            "capture_screenshot",
            &json!({"preset": "Bug report", "target": t}),
        )
        .unwrap();
    assert_eq!(
        resolved,
        json!({"target": t, "delay_s": 5.0, "copy_to_clipboard": true})
    );
    // By id, and with the delay overridden.
    let resolved = r
        .daemon
        .resolve_preset_params(
            "capture_screenshot",
            &json!({"preset": "bug-report", "target": t, "delay_s": 0}),
        )
        .unwrap();
    assert_eq!(resolved["delay_s"], 0);
    assert_eq!(resolved["copy_to_clipboard"], true);
    // Both calls capture a picture of the same size.
    let with = call(
        &r.daemon,
        agent(),
        "capture_screenshot",
        json!({"preset": "Bug report", "target": t, "delay_s": 0}),
    )
    .await
    .unwrap();
    let without = call(
        &r.daemon,
        agent(),
        "capture_screenshot",
        json!({"target": t, "delay_s": 0, "copy_to_clipboard": true}),
    )
    .await
    .unwrap();
    for k in ["width", "height", "kind"] {
        assert_eq!(with[k], without[k], "{k}");
    }
    // A video preset does not fit a screenshot, and the error says so.
    let e = call(
        &r.daemon,
        agent(),
        "capture_screenshot",
        json!({"preset": "Demo video", "target": t}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

/// QC-5.AC5: an unknown preset is `invalid_input` and names it; no capture is made.
#[tokio::test]
async fn qc5_ac5_unknown_preset_names_it() {
    let r = rig(Decision::Approve);
    for (m, p) in [
        (
            "capture_screenshot",
            json!({"preset": "nope", "target": {"display_id": 1}}),
        ),
        (
            "start_recording",
            json!({"preset": "nope", "target": {"display_id": 1}}),
        ),
    ] {
        let e = call(&r.daemon, agent(), m, p).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput);
        assert!(e.to_string().contains("\"nope\""), "{e}");
    }
    assert!(r.confirmer.shown().is_empty());
    assert_eq!(r.sources.builds(), 0);
}

/// QC-5.AC4: a recording preset started by an agent still opens the dialog, which shows the plan
/// the preset resolves to; declining starts nothing. An explicit argument wins in the plan.
#[tokio::test]
async fn qc5_ac4_a_recording_preset_still_asks_and_shows_the_resolved_plan() {
    let r = rig(Decision::Decline);
    let e = call(
        &r.daemon,
        agent(),
        "start_recording",
        json!({"preset": "Demo video", "target": {"display_id": 1}, "mic": false}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].tool, "start_recording");
    let text = format!(
        "{} {} {}",
        shown[0].summary, shown[0].details, shown[0].params
    );
    // The preset's webcam and 3 s countdown are in the plan; the call's `mic: false` won.
    assert_eq!(
        shown[0].details["camera"].as_str().unwrap()[..2].to_owned(),
        "On"
    );
    assert_eq!(shown[0].details["microphone"], "Off");
    assert_eq!(shown[0].details["countdown"], "3 s");
    assert!(text.contains("camera"), "{text}");
    assert_eq!(r.sources.builds(), 0);
    assert!(!r.daemon.recording_active());
}

/// A preset never loosens a tier: an audio preset from an agent asks too, and the older meaning
/// of `start_recording.preset` (the audio profile) is untouched.
#[tokio::test]
async fn an_audio_preset_asks_and_the_audio_profile_names_still_work() {
    let r = rig(Decision::Decline);
    let e = call(
        &r.daemon,
        agent(),
        "start_recording",
        json!({"preset": "Meeting"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert!(shown[0].summary.to_lowercase().contains("audio"));
    // `preset: "room"` with `mode: "audio"` is the audio profile, not a Quick Capture preset.
    let p = json!({"mode": "audio", "preset": "room"});
    assert_eq!(
        r.daemon
            .resolve_preset_params("start_recording", &p)
            .unwrap(),
        p
    );
}
