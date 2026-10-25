//! AGT-2.AC9 to AC14: the per-tool permissions (`agents.tool_policy`, owner decision OD-24).
//! Fakes only: scripted capture backend, scripted recording sources, scripted confirmers.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_core::{Actor, CmError, ErrorCode};
use cm_daemon::testing::{CountingExternal, FakeSources, FixedConfirmer, NeverAnswers};
use cm_daemon::tool_policy::{self, KEY};
use cm_daemon::{
    ActivityDecision, Approval, ClientPolicy, ConfirmationRequest, Confirmer, Daemon, DaemonError,
    Decision, EngineOptions, MemorySettings, SettingsStore, Tier, ACTIONS,
};
use common::{builder, desktop, Client};
use serde_json::{json, Map, Value};
use tokio::sync::mpsc;

fn agent(name: &str) -> Actor {
    Actor::agent(name).unwrap()
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap()
}

#[derive(Clone, Default)]
struct Shared(Arc<MemorySettings>);

impl SettingsStore for Shared {
    fn all(&self) -> Result<Map<String, Value>, CmError> {
        self.0.all()
    }
    fn set(&self, v: &Map<String, Value>) -> Result<Map<String, Value>, CmError> {
        self.0.set(v)
    }
}

impl Shared {
    /// Writes the raw value, like a hand-edited settings file (no validation).
    fn put(&self, v: Value) {
        let m: Map<String, Value> = [(KEY.to_owned(), v)].into_iter().collect();
        self.0.set(&m).unwrap();
    }
}

struct ArcConfirm(Arc<FixedConfirmer>);

#[async_trait]
impl Confirmer for ArcConfirm {
    async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
        self.0.confirm(r).await
    }
}

struct Rig {
    daemon: Daemon,
    settings: Shared,
    confirmer: Arc<FixedConfirmer>,
    _dir: tempfile::TempDir,
}

fn rig(decision: Decision) -> Rig {
    let settings = Shared::default();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let (b, dir) = builder(desktop());
    let daemon = b
        .settings(settings.clone())
        .confirmer(ArcConfirm(confirmer.clone()))
        .build();
    Rig {
        daemon,
        settings,
        confirmer,
        _dir: dir,
    }
}

fn shot() -> Value {
    json!({"target": {"display_id": 1}})
}

async fn call(d: &Daemon, who: Actor, m: &str, p: Value) -> Result<Value, DaemonError> {
    d.execute_method(who, 0, m, &p).await
}

/// AGT-2.AC9: every action x every stored value x actor, through the daemon.
#[tokio::test]
async fn agt_2_ac9_effective_tier_matrix_through_the_daemon() {
    let r = rig(Decision::Approve);
    for s in ACTIONS {
        for stored in [None, Some("allow"), Some("ask")] {
            r.settings.put(match stored {
                Some(v) => json!({ s.name: v }),
                None => json!({}),
            });
            let control = tool_policy::control_of(s);
            let want = match (control, stored) {
                (tool_policy::ToolControl::Editable { .. }, Some("ask")) => Tier::Confirm,
                (tool_policy::ToolControl::Editable { .. }, Some("allow")) => Tier::Free,
                _ => s.tier,
            };
            assert_eq!(
                r.daemon.effective_tier(s.name, &agent("claude")),
                Some(want),
                "{} stored {stored:?}",
                s.name
            );
            // The user is never asked: always the table's tier.
            assert_eq!(
                r.daemon.effective_tier(s.name, &Actor::User),
                Some(s.tier),
                "{} for the user",
                s.name
            );
        }
    }
    assert_eq!(r.daemon.effective_tier("no_such_tool", &Actor::User), None);
    // Every editable Free tool is mutating; every read-only Free tool is locked.
    for s in ACTIONS {
        if s.tier == Tier::Free && !s.mutating && !cm_daemon::is_internal(s.name) {
            assert_eq!(
                tool_policy::control_of(s),
                tool_policy::ToolControl::LockedRead
            );
        }
    }
}

/// AGT-2.AC9, AC11: a tightened tool pauses on the generic dialog, then runs when approved.
#[tokio::test]
async fn agt_2_ac9_tightened_tool_asks_then_runs_and_the_log_says_who_approved() {
    let r = rig(Decision::Approve);
    // Not tightened: runs at once.
    call(
        &r.daemon,
        agent("claude-desktop"),
        "capture_screenshot",
        shot(),
    )
    .await
    .unwrap();
    assert!(r.confirmer.shown().is_empty());

    r.settings.put(json!({"capture_screenshot": "ask"}));
    let mut events = r.daemon.subscribe();
    let v = call(
        &r.daemon,
        agent("claude-desktop"),
        "capture_screenshot",
        shot(),
    )
    .await
    .unwrap();
    assert!(v["capture_id"].is_string(), "{v}");
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    let req = &shown[0];
    assert_eq!(req.kind, "tool_ask_first");
    assert_eq!(req.tool, "capture_screenshot");
    assert_eq!(req.client, "claude-desktop");
    assert!(!req.allow_session);
    assert_eq!(req.params["tool"], "capture_screenshot");
    assert_eq!(req.params["client"], "claude-desktop");
    assert_eq!(req.params["title"], "Capture screenshot");
    assert_eq!(req.params["lines"][0]["key"], "target");
    assert_eq!(req.params["lines"][0]["value"], "{...}");
    assert!(req.summary.contains("claude-desktop"));
    let ev = events.recv().await.unwrap();
    assert_eq!(ev.name, "confirmation.requested");
    assert_eq!(ev.data["kind"], "tool_ask_first");

    let log = r.daemon.activity();
    let last = log.last().unwrap();
    assert_eq!(last.tool, "capture_screenshot");
    assert_eq!(last.decision, ActivityDecision::Approved);
    assert_eq!(last.approval, Some(Approval::User));
    // The first (unasked) call left no entry.
    assert_eq!(log.len(), 1);
}

/// The user (the app's own GUI) is never asked, whatever the setting.
#[tokio::test]
async fn agt_2_ac9_the_gui_user_is_never_asked() {
    let r = rig(Decision::Decline);
    r.settings
        .put(json!({"capture_screenshot": "ask", "annotate": "ask"}));
    call(&r.daemon, Actor::User, "capture_screenshot", shot())
        .await
        .unwrap();
    assert!(r.confirmer.shown().is_empty());
}

/// AGT-2.AC12: Cancel is `user_declined` with `data.reason = "ask_first"` (exit code 3) and
/// nothing runs; over JSON-RPC the reason is in the payload.
#[tokio::test]
async fn agt_2_ac12_cancel_is_user_declined_ask_first() {
    let r = rig(Decision::Decline);
    r.settings.put(json!({"capture_screenshot": "ask"}));
    let e = call(&r.daemon, agent("claude"), "capture_screenshot", shot())
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert_eq!(e.exit_code(), 3);
    assert_eq!(e.payload()["data"]["reason"], "ask_first");
    assert_eq!(e.to_string(), "The user declined the request");
    // Nothing was captured.
    let l = call(&r.daemon, Actor::User, "list_library", json!({}))
        .await
        .unwrap();
    assert_eq!(l["items"].as_array().map_or(0, Vec::len), 0, "{l}");
    let last = r.daemon.activity().pop().unwrap();
    assert_eq!(last.decision, ActivityDecision::Declined);
    assert_eq!(last.approval, None);

    // Over JSON-RPC.
    let mut c = Client::connect(&r.daemon, "claude");
    c.hello("claude").await;
    let resp = c.call("capture_screenshot", shot()).await;
    assert_eq!(
        resp["error"]["data"]["data"]["reason"], "ask_first",
        "{resp}"
    );
}

#[tokio::test(start_paused = true)]
async fn agt_2_ac12_no_answer_for_five_minutes_is_confirmation_timeout() {
    let settings = Shared::default();
    let (b, _dir) = builder(desktop());
    let d = b.settings(settings.clone()).confirmer(NeverAnswers).build();
    settings.put(json!({"capture_screenshot": "ask"}));
    let t0 = tokio::time::Instant::now();
    let e = call(&d, agent("claude"), "capture_screenshot", shot())
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::ConfirmationTimeout);
    assert!(t0.elapsed() >= Duration::from_secs(300));
}

/// AGT-2.AC10: a hand-edited file that loosens publish or delete, or asks about a read, or names
/// a tool that does not exist, is ignored; the dialog still opens.
#[tokio::test]
async fn agt_2_ac10_hand_edited_loosening_of_publish_and_delete_is_ignored() {
    let settings = Shared::default();
    let engine = Arc::new(CountingExternal::default());
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Decline));
    let (b, _dir) = builder(desktop());
    let d = b
        .settings(settings.clone())
        .confirmer(ArcConfirm(confirmer.clone()))
        .external_actions(engine.clone())
        .build();
    settings.put(json!({
        "publish_video": "allow",
        "delete_capture": "allow",
        "run_fps_selftest": "allow",
        "list_library": "ask",
        "nope": "allow",
        "freeze_display": "ask",
    }));
    for a in ["publish_video", "delete_capture"] {
        assert_eq!(d.effective_tier(a, &agent("x")), Some(Tier::Confirm));
    }
    let p = json!({"capture_id":"rec_1","platform":"youtube","privacy":"unlisted"});
    let e = call(&d, agent("x"), "publish_video", p).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    let e = call(
        &d,
        agent("x"),
        "delete_capture",
        json!({"capture_id":"rec_1"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert_eq!(confirmer.shown().len(), 2, "both dialogs opened");
    assert!(engine.calls().is_empty());
    // A read is not asked about.
    call(&d, agent("x"), "list_library", json!({}))
        .await
        .unwrap();
    assert_eq!(confirmer.shown().len(), 2);
    // A non-object value is ignored as a whole.
    settings.put(json!("allow"));
    assert_eq!(
        d.effective_tier("publish_video", &agent("x")),
        Some(Tier::Confirm)
    );
}

/// AGT-2.AC10: writes through `set_settings`: the user's bad shapes are refused, an agent is
/// refused as for every app-only key.
#[tokio::test]
async fn agt_2_ac10_writes_are_validated_and_app_only() {
    let r = rig(Decision::Approve);
    for ok in [
        json!({}),
        json!({"capture_screenshot": "ask"}),
        json!({"start_recording": "allow", "annotate": "ask"}),
    ] {
        call(
            &r.daemon,
            Actor::User,
            "set_settings",
            json!({"values": {KEY: ok}}),
        )
        .await
        .unwrap();
    }
    for bad in [
        json!("ask"),
        json!({"publish_video": "allow"}),
        json!({"delete_capture": "allow"}),
        json!({"run_fps_selftest": "allow"}),
        json!({"list_library": "ask"}),
        json!({"unknown_tool": "ask"}),
        json!({"freeze_display": "ask"}),
        json!({"capture_screenshot": "perhaps"}),
    ] {
        let e = call(
            &r.daemon,
            Actor::User,
            "set_settings",
            json!({"values": {KEY: bad}}),
        )
        .await
        .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{bad}");
    }
    // The last good value is still in place.
    assert_eq!(
        r.daemon.effective_tier("start_recording", &agent("x")),
        Some(Tier::Free)
    );
    // An agent cannot write it (or loosen start_recording itself).
    let e = call(
        &r.daemon,
        agent("evil"),
        "set_settings",
        json!({"values": {KEY: {"start_recording": "allow"}}}),
    )
    .await
    .unwrap_err();
    assert_eq!(e.payload()["data"]["reason"], "app_only_setting");
}

/// AGT-2.AC14: the policy is read at call time. A change while a dialog is pending does not
/// touch that dialog; the next call follows the new value, with no restart.
#[tokio::test]
async fn agt_2_ac14_changes_apply_to_the_next_call_not_to_a_pending_dialog() {
    struct Manual {
        shown: mpsc::UnboundedSender<ConfirmationRequest>,
        answers: tokio::sync::Mutex<mpsc::UnboundedReceiver<Decision>>,
    }
    #[async_trait]
    impl Confirmer for Manual {
        async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
            self.shown.send(r.clone()).unwrap();
            self.answers
                .lock()
                .await
                .recv()
                .await
                .unwrap_or(Decision::Decline)
        }
    }
    let (shown_tx, mut shown) = mpsc::unbounded_channel();
    let (ans_tx, ans_rx) = mpsc::unbounded_channel();
    let settings = Shared::default();
    let (b, _dir) = builder(desktop());
    let d = b
        .settings(settings.clone())
        .confirmer(Manual {
            shown: shown_tx,
            answers: tokio::sync::Mutex::new(ans_rx),
        })
        .build();
    settings.put(json!({"capture_screenshot": "ask"}));
    let d2 = d.clone();
    let pending = tokio::spawn(async move {
        d2.execute_method(agent("claude"), 0, "capture_screenshot", &shot())
            .await
    });
    let req = shown.recv().await.unwrap();
    assert_eq!(req.kind, "tool_ask_first");
    // The user resets the policy while the dialog is open.
    settings.put(json!({}));
    assert_eq!(
        d.effective_tier("capture_screenshot", &agent("claude")),
        Some(Tier::Free)
    );
    // A new call runs at once, without a dialog.
    d.execute_method(agent("claude"), 0, "capture_screenshot", &shot())
        .await
        .unwrap();
    assert!(shown.try_recv().is_err(), "no second dialog");
    assert!(!pending.is_finished(), "the pending dialog is untouched");
    // And the pending one still ends with its own answer.
    ans_tx.send(Decision::Approve).unwrap();
    assert!(pending.await.unwrap().is_ok());
    // The opposite change: tightened while nothing is pending applies to the next call.
    settings.put(json!({"capture_screenshot": "ask"}));
    let d3 = d.clone();
    let next = tokio::spawn(async move {
        d3.execute_method(agent("claude"), 0, "capture_screenshot", &shot())
            .await
    });
    shown.recv().await.unwrap();
    ans_tx.send(Decision::Decline).unwrap();
    assert_eq!(
        code(&next.await.unwrap().unwrap_err()),
        ErrorCode::UserDeclined
    );
}

/// A repeated `request_id` of a tightened tool returns the first result without a second dialog.
#[tokio::test]
async fn agt_2_ac9_request_id_is_idempotent_with_one_dialog_per_call() {
    let r = rig(Decision::Approve);
    r.settings.put(json!({"capture_screenshot": "ask"}));
    let p = json!({"target": {"display_id": 1}, "request_id": "rid-1"});
    let a = call(&r.daemon, agent("claude"), "capture_screenshot", p.clone())
        .await
        .unwrap();
    let b = call(&r.daemon, agent("claude"), "capture_screenshot", p)
        .await
        .unwrap();
    assert_eq!(a["capture_id"], b["capture_id"]);
    assert_eq!(r.confirmer.shown().len(), 1);
}

/// AGT-2.AC4 first: a read-only or blocked client is refused before any dialog, whatever the
/// per-tool setting says (tightened or loosened).
#[tokio::test]
async fn agt_2_ac9_client_policy_still_refuses_first() {
    let r = rig(Decision::Approve);
    r.settings
        .put(json!({"capture_screenshot": "ask", "start_recording": "allow"}));
    r.daemon.policies().set("ro", ClientPolicy::ReadOnly);
    r.daemon.policies().set("blocked", ClientPolicy::Blocked);
    for (who, reason) in [("ro", "client_read_only"), ("blocked", "client_blocked")] {
        for (m, p) in [
            ("capture_screenshot", shot()),
            ("start_recording", json!({"target": {"display_id": 1}})),
        ] {
            let e = call(&r.daemon, agent(who), m, p).await.unwrap_err();
            assert_eq!(e.payload()["data"]["reason"], reason, "{who} {m}");
        }
    }
    assert!(r.confirmer.shown().is_empty());
}

/// An AI tool the build does not have fails with `not_implemented` before a dialog opens.
#[tokio::test]
async fn agt_2_ac9_unsupported_tightened_tool_fails_before_the_dialog() {
    let r = rig(Decision::Approve);
    r.settings.put(json!({"import_model": "ask"}));
    let e = call(
        &r.daemon,
        agent("claude"),
        "import_model",
        json!({"model": "small", "path": "/tmp/ggml-small.bin"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotImplemented);
    assert!(r.confirmer.shown().is_empty());
}

/// AGT-2.AC11 through the daemon: a long value is cut at 120 characters, nested values are
/// summarized, and `request_id` is not shown.
#[tokio::test]
async fn agt_2_ac11_dialog_parameters_are_summarized() {
    let r = rig(Decision::Decline);
    r.settings
        .put(json!({"rename_capture": "ask", "set_settings": "ask"}));
    let long = "t".repeat(400);
    let _ = call(
        &r.daemon,
        agent("claude"),
        "rename_capture",
        json!({"capture_id": "shot_1", "title": long, "request_id": "r"}),
    )
    .await;
    let _ = call(
        &r.daemon,
        agent("claude"),
        "set_settings",
        json!({"values": {"recording.fps": 30}}),
    )
    .await;
    let shown = r.confirmer.shown();
    let rename = &shown[0];
    let lines = rename.params["lines"].as_array().unwrap();
    let title = lines.iter().find(|l| l["key"] == "title").unwrap();
    assert_eq!(title["value"].as_str().unwrap().chars().count(), 123);
    assert!(title["value"].as_str().unwrap().ends_with("..."));
    assert!(lines.iter().all(|l| l["key"] != "request_id"));
    assert_eq!(rename.details["capture_id"], "shot_1");
    let set = &shown[1];
    let values = set.params["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["key"] == "values")
        .unwrap();
    assert_eq!(values["value"], "{...}");
    assert!(!serde_json::to_string(&set.params)
        .unwrap()
        .contains("recording.fps"));
}

/// AGT-2.AC13: `start_recording` set to Allow starts with no dialog, through the same engine
/// path (events, indicator facts), and the log says the setting approved it.
#[tokio::test]
async fn agt_2_ac13_start_recording_allow_runs_without_a_dialog() {
    let settings = Shared::default();
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Decline));
    let (b, _dir) = builder(desktop());
    let d = b
        .settings(settings.clone())
        .confirmer(ArcConfirm(confirmer.clone()))
        .recording_sources(FakeSources::new())
        .recording_options(EngineOptions {
            tick_every: Duration::from_millis(15),
            countdown_step: Duration::from_millis(40),
            ..EngineOptions::default()
        })
        .build();
    let p = json!({"target": {"display_id": 1}, "countdown_s": 0});
    let mut events = d.subscribe();

    // Default: Ask first (the dialog opens, here it is declined).
    let e = call(&d, agent("claude"), "start_recording", p.clone())
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert_eq!(confirmer.shown().len(), 1);
    assert_eq!(confirmer.shown()[0].kind, "start_recording");

    settings.put(json!({"start_recording": "allow"}));
    let started = call(&d, agent("claude"), "start_recording", p.clone())
        .await
        .unwrap();
    assert!(started["recording_id"].is_string(), "{started}");
    assert_eq!(confirmer.shown().len(), 1, "no second dialog");

    // The recording events (what the HUD and the indicator listen to) are still published.
    let mut names = Vec::new();
    while let Ok(ev) = tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
        let n = ev.unwrap().name;
        let done = n == "recording.started";
        names.push(n);
        if done {
            break;
        }
    }
    assert!(names.iter().any(|n| n == "recording.started"), "{names:?}");

    let last = d.activity().pop().unwrap();
    assert_eq!(last.tool, "start_recording");
    assert_eq!(last.client, "claude");
    assert_eq!(last.decision, ActivityDecision::Approved);
    assert_eq!(last.approval, Some(Approval::Policy));
    let json = serde_json::to_value(&last).unwrap();
    assert_eq!(json["approval"], "policy");

    // A second start is busy (the same engine rules), still without a dialog.
    let e = call(&d, agent("claude"), "start_recording", p)
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::Busy);
    assert_eq!(confirmer.shown().len(), 1);
    call(&d, Actor::User, "stop_recording", json!({}))
        .await
        .unwrap();
}

/// AGT-2.AC13: the same plan checks run before anything starts (no engine, bad request).
#[tokio::test]
async fn agt_2_ac13_loosened_start_still_fails_when_it_cannot_run() {
    let r = rig(Decision::Decline);
    r.settings.put(json!({"start_recording": "allow"}));
    let e = call(
        &r.daemon,
        agent("claude"),
        "start_recording",
        json!({"target": {"display_id": 1}}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotImplemented);
    assert!(r.confirmer.shown().is_empty());
}

/// AGT-2.AC8: `actions.list` reports the effective tier next to the table's.
#[tokio::test]
async fn agt_2_ac8_actions_list_reports_the_effective_tier() {
    let r = rig(Decision::Approve);
    r.settings.put(json!({
        "capture_screenshot": "ask",
        "start_recording": "allow",
        "publish_video": "allow",
    }));
    let mut c = Client::connect(&r.daemon, "claude");
    c.hello("claude").await;
    let list = c.call("actions.list", json!({})).await;
    let rows = list["result"].as_array().unwrap();
    let row = |n: &str| rows.iter().find(|r| r["name"] == n).unwrap().clone();
    assert_eq!(row("capture_screenshot")["tier"], "free");
    assert_eq!(row("capture_screenshot")["effective_tier"], "confirm");
    assert_eq!(row("start_recording")["tier"], "confirm");
    assert_eq!(row("start_recording")["effective_tier"], "free");
    assert_eq!(row("publish_video")["effective_tier"], "confirm");
    assert_eq!(row("list_library")["effective_tier"], "free");
    for r in rows {
        assert!(r["effective_tier"].is_string(), "{r}");
        // The existing fields are all still there.
        for k in ["name", "tier", "mutating", "available", "internal"] {
            assert!(r.get(k).is_some(), "{k}");
        }
    }
}
