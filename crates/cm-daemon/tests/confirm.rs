//! AGT-2 confirmation gate, per-client policy, activity log; AGT-3.AC4 for Confirm-tier calls.

#![allow(clippy::unwrap_used)]

mod common;

use async_trait::async_trait;
use cm_core::{Actor, ErrorCode};
use cm_daemon::testing::{CountingExternal, FixedConfirmer, NeverAnswers};
use cm_daemon::{
    ActivityDecision, ClientPolicy, ConfirmationRequest, Confirmer, Daemon, DaemonError, Decision,
    ManualClock,
};
use common::{builder, desktop, Client};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

fn agent(name: &str) -> Actor {
    Actor::agent(name).unwrap()
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap()
}

/// Daemon with a counting engine behind the Confirm tier.
fn with_engine(
    confirmer: impl Confirmer + 'static,
) -> (Daemon, Arc<CountingExternal>, tempfile::TempDir) {
    let engine = Arc::new(CountingExternal::default());
    let (b, dir) = builder(desktop());
    let d = b
        .confirmer(confirmer)
        .external_actions(engine.clone())
        .build();
    (d, engine, dir)
}

async fn publish(d: &Daemon, who: &str, rid: Option<&str>) -> Result<Value, DaemonError> {
    let mut p = json!({"capture_id":"rec_1","platform":"youtube","privacy":"unlisted"});
    if let Some(r) = rid {
        p["request_id"] = json!(r);
    }
    d.execute_method(agent(who), 0, "publish_video", &p).await
}

/// A confirmer the test answers by hand: shows requests on a channel, answers from another.
struct Manual {
    shown: mpsc::UnboundedSender<ConfirmationRequest>,
    answers: tokio::sync::Mutex<mpsc::UnboundedReceiver<Decision>>,
}

fn manual() -> (
    Manual,
    mpsc::UnboundedReceiver<ConfirmationRequest>,
    mpsc::UnboundedSender<Decision>,
) {
    let (shown, shown_rx) = mpsc::unbounded_channel();
    let (ans_tx, ans_rx) = mpsc::unbounded_channel();
    (
        Manual {
            shown,
            answers: tokio::sync::Mutex::new(ans_rx),
        },
        shown_rx,
        ans_tx,
    )
}

#[async_trait]
impl Confirmer for Manual {
    async fn confirm(&self, request: &ConfirmationRequest) -> Decision {
        self.shown.send(request.clone()).unwrap();
        self.answers
            .lock()
            .await
            .recv()
            .await
            .unwrap_or(Decision::Decline)
    }
}

#[tokio::test]
async fn agt_2_ac1_nothing_happens_until_the_user_approves() {
    let (m, mut shown, answer) = manual();
    let (d, engine, _dir) = with_engine(m);
    let d2 = d.clone();
    let call = tokio::spawn(async move { publish(&d2, "claude", Some("pub-1")).await });

    let req = shown.recv().await.unwrap();
    assert_eq!(req.client, "claude");
    assert_eq!(req.tool, "publish_video");
    assert_eq!(req.summary, "Publish capture rec_1 to youtube");
    assert_eq!(req.details["rest"]["privacy"], "unlisted");
    assert!(!req.allow_session);
    tokio::task::yield_now().await;
    assert!(engine.calls().is_empty(), "nothing runs before approval");
    assert!(!call.is_finished());

    answer.send(Decision::Approve).unwrap();
    let r = call.await.unwrap().unwrap();
    assert_eq!(r["ok"], true);
    assert_eq!(engine.calls().len(), 1);
    assert_eq!(engine.calls()[0].1, agent("claude"));
}

#[tokio::test]
async fn agt_2_decline_returns_user_declined_and_runs_nothing() {
    let (d, engine, _dir) = with_engine(FixedConfirmer::new(Decision::Decline));
    let e = publish(&d, "claude", None).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert_eq!(e.exit_code(), 3);
    assert!(engine.calls().is_empty());
}

#[tokio::test(start_paused = true)]
async fn agt_2_ac3_no_answer_for_five_minutes_is_confirmation_timeout() {
    let (d, engine, _dir) = with_engine(NeverAnswers);
    let mut events = d.subscribe();
    let t0 = tokio::time::Instant::now();
    let e = publish(&d, "claude", None).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::ConfirmationTimeout);
    assert!(e.to_string().contains("5 minutes"));
    assert!(t0.elapsed() >= Duration::from_secs(300));
    assert!(t0.elapsed() < Duration::from_secs(301));
    assert!(engine.calls().is_empty());
    // confirmation.requested then confirmation.resolved { decision: timeout }.
    let a = events.recv().await.unwrap();
    let b = events.recv().await.unwrap();
    assert_eq!(a.name, "confirmation.requested");
    assert_eq!(a.data["tool"], "publish_video");
    // P3: the event carries the kind and typed parameters next to the English summary.
    assert_eq!(a.data["kind"], "publish_video");
    assert_eq!(a.data["params"]["platform"], "youtube");
    assert!(a.data["summary"].is_string() && a.data["details"].is_object());
    // The timeout message has a stable id and its parameter.
    let parts = cm_core::message::describe(ErrorCode::ConfirmationTimeout, &e.to_string()).unwrap();
    assert_eq!(
        (parts.id, parts.get("wait")),
        ("confirmation_timeout", "5 minutes")
    );
    assert_eq!(b.name, "confirmation.resolved");
    assert_eq!(b.data["decision"], "timeout");
    assert_eq!(a.data["id"], b.data["id"]);
}

#[tokio::test(start_paused = true)]
async fn agt_2_timeout_is_injectable() {
    let engine = Arc::new(CountingExternal::default());
    let (b, _dir) = builder(desktop());
    let d = b
        .confirmer(NeverAnswers)
        .confirm_timeout(Duration::from_secs(5))
        .external_actions(engine)
        .build();
    let t0 = tokio::time::Instant::now();
    let e = publish(&d, "c", None).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::ConfirmationTimeout);
    assert_eq!(t0.elapsed(), Duration::from_secs(5));
}

#[tokio::test]
async fn agt_2_requests_queue_one_dialog_at_a_time() {
    let (m, mut shown, answer) = manual();
    let (d, engine, _dir) = with_engine(m);
    let (d1, d2) = (d.clone(), d.clone());
    let first = tokio::spawn(async move { publish(&d1, "a", None).await });
    let r1 = shown.recv().await.unwrap();
    let second = tokio::spawn(async move { publish(&d2, "b", None).await });
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    assert!(shown.try_recv().is_err(), "second dialog must wait");
    answer.send(Decision::Decline).unwrap();
    assert_eq!(
        code(&first.await.unwrap().unwrap_err()),
        ErrorCode::UserDeclined
    );
    let r2 = shown.recv().await.unwrap();
    assert_ne!(r1.id, r2.id);
    assert_eq!(r2.client, "b");
    answer.send(Decision::Approve).unwrap();
    second.await.unwrap().unwrap();
    assert_eq!(engine.calls().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn agt_2_timeout_counts_from_when_the_dialog_opens() {
    // First dialog is answered after 4 minutes; the queued one still gets its full 5.
    struct Slow;
    #[async_trait]
    impl Confirmer for Slow {
        async fn confirm(&self, _r: &ConfirmationRequest) -> Decision {
            tokio::time::sleep(Duration::from_secs(240)).await;
            Decision::Approve
        }
    }
    let (d, engine, _dir) = with_engine(Slow);
    let (d1, d2) = (d.clone(), d.clone());
    let a = tokio::spawn(async move { publish(&d1, "a", None).await });
    let b = tokio::spawn(async move { publish(&d2, "b", None).await });
    a.await.unwrap().unwrap();
    b.await.unwrap().unwrap();
    assert_eq!(engine.calls().len(), 2);
}

#[tokio::test]
async fn agt_2_ac4_read_only_client_is_refused_without_a_dialog() {
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    struct Shared(Arc<FixedConfirmer>);
    #[async_trait]
    impl Confirmer for Shared {
        async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
            self.0.confirm(r).await
        }
    }
    let (d, engine, _dir) = with_engine(Shared(confirmer.clone()));
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    d.policies().set("banned", ClientPolicy::Blocked);

    // Read-only: reads are fine; mutating Free and Confirm actions are user_declined.
    d.execute_method(agent("viewer"), 0, "list_targets", &json!({}))
        .await
        .unwrap();
    for (m, p) in [
        ("capture_screenshot", json!({"target":{"display_id":1}})),
        (
            "annotate",
            json!({"capture_id":"x","ops":[{"op":"rect","rect":[1,1,2,2]}]}),
        ),
        ("undo", json!({"capture_id":"x"})),
        (
            "publish_video",
            json!({"capture_id":"x","platform":"youtube"}),
        ),
    ] {
        let e = d
            .execute_method(agent("viewer"), 0, m, &p)
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::UserDeclined, "{m}");
        assert_eq!(e.payload()["data"]["reason"], "client_read_only", "{m}");
    }
    // Blocked: everything.
    let e = d
        .execute_method(agent("banned"), 0, "list_targets", &json!({}))
        .await
        .unwrap_err();
    assert_eq!(e.payload()["data"]["reason"], "client_blocked");
    assert_eq!(e.to_rpc_error()["code"], -32002);
    assert!(
        confirmer.shown().is_empty(),
        "no dialog for policy refusals"
    );
    assert!(engine.calls().is_empty());
    // Other clients and the user are unaffected.
    d.execute_method(agent("other"), 0, "list_targets", &json!({}))
        .await
        .unwrap();
    d.execute_method(Actor::User, 0, "list_targets", &json!({}))
        .await
        .unwrap();
}

#[tokio::test]
async fn agt_2_ac4_policy_applies_over_jsonrpc() {
    let (d, _engine, _dir) = with_engine(FixedConfirmer::new(Decision::Approve));
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    let mut c = Client::connect(&d, "x");
    c.hello("viewer").await;
    let r = c
        .call(
            "annotate",
            json!({"capture_id":"x","ops":[{"op":"rect","rect":[1,1,2,2]}]}),
        )
        .await;
    assert_eq!(r["error"]["data"]["code"], "user_declined");
    assert_eq!(r["error"]["data"]["data"]["reason"], "client_read_only");
}

#[tokio::test(start_paused = true)]
async fn agt_2_timeout_and_decline_over_jsonrpc() {
    let (d, _engine, _dir) = with_engine(NeverAnswers);
    let mut c = Client::connect(&d, "x");
    c.hello("claude").await;
    let r = c
        .call(
            "publish_video",
            json!({"capture_id":"rec_1","platform":"youtube"}),
        )
        .await;
    assert_eq!(r["error"]["data"]["code"], "confirmation_timeout");
    assert_eq!(r["error"]["code"], -32003);

    let (d, _engine, _dir) = with_engine(FixedConfirmer::new(Decision::Decline));
    let mut c = Client::connect(&d, "x");
    c.hello("claude").await;
    let r = c
        .call("delete_capture", json!({"capture_id":"rec_1"}))
        .await;
    assert_eq!(r["error"]["data"]["code"], "user_declined");
    assert_eq!(r["error"]["code"], -32002);
}

#[tokio::test]
async fn agt_2_unsupported_confirm_actions_fail_before_any_dialog() {
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    struct Shared(Arc<FixedConfirmer>);
    #[async_trait]
    impl Confirmer for Shared {
        async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
            self.0.confirm(r).await
        }
    }
    let (b, _dir) = builder(desktop());
    let d = b.confirmer(Shared(confirmer.clone())).build();
    let e = d
        .execute_method(
            agent("c"),
            0,
            "start_recording",
            &json!({"target":{"display_id":1}}),
        )
        .await
        .unwrap_err();
    assert!(matches!(e, DaemonError::NotImplemented(_)));
    assert_eq!(e.to_rpc_error()["code"], -32099);
    assert!(confirmer.shown().is_empty());
}

#[tokio::test]
async fn agt_2_user_actor_skips_the_gate() {
    let (d, engine, _dir) = with_engine(FixedConfirmer::new(Decision::Decline));
    let r = d
        .execute_method(
            Actor::User,
            0,
            "delete_capture",
            &json!({"capture_id":"rec_1"}),
        )
        .await
        .unwrap();
    assert_eq!(r["ok"], true);
    assert_eq!(engine.calls()[0].1, Actor::User);
}

#[tokio::test]
async fn agt_2_allow_for_session_is_start_recording_only_and_expires_with_the_session() {
    let c = Arc::new(FixedConfirmer::new(Decision::ApproveForSession));
    struct Shared(Arc<FixedConfirmer>);
    #[async_trait]
    impl Confirmer for Shared {
        async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
            self.0.confirm(r).await
        }
    }
    let (d, engine, _dir) = with_engine(Shared(c.clone()));
    let rec = json!({"target":{"display_id":1}});
    let s1 = d.new_session();
    let go = |s, m: &'static str, p: Value| {
        let d = d.clone();
        async move { d.execute_method(agent("claude"), s, m, &p).await }
    };
    go(s1, "start_recording", rec.clone()).await.unwrap();
    go(s1, "start_recording", rec.clone()).await.unwrap();
    assert_eq!(c.shown().len(), 1, "second recording skips the dialog");
    assert!(c.shown()[0].allow_session);
    // Publish and delete always ask, and never offer the checkbox.
    go(
        s1,
        "publish_video",
        json!({"capture_id":"r","platform":"youtube"}),
    )
    .await
    .unwrap();
    go(
        s1,
        "publish_video",
        json!({"capture_id":"r","platform":"youtube"}),
    )
    .await
    .unwrap();
    assert_eq!(c.shown().len(), 3);
    assert!(!c.shown()[1].allow_session);
    // Another connection is not covered; neither is the same one after it disconnects.
    let s2 = d.new_session();
    go(s2, "start_recording", rec.clone()).await.unwrap();
    assert_eq!(c.shown().len(), 4);
    d.end_session(s1);
    go(s1, "start_recording", rec).await.unwrap();
    assert_eq!(c.shown().len(), 5);
    assert_eq!(engine.calls().len(), 6);
}

#[tokio::test]
async fn agt_2_ac6_activity_log_lists_client_tool_summary_decision_time() {
    let (d, _engine, _dir) = with_engine(FixedConfirmer::new(Decision::Decline));
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    publish(&d, "claude", None).await.unwrap_err();
    d.execute_method(agent("viewer"), 0, "undo", &json!({"capture_id":"x"}))
        .await
        .unwrap_err();
    let log = d.activity();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].client, "claude");
    assert_eq!(log[0].tool, "publish_video");
    assert_eq!(log[0].summary, "Publish capture rec_1 to youtube");
    assert_eq!(log[0].decision, ActivityDecision::Declined);
    assert_eq!(log[1].client, "viewer");
    assert_eq!(log[1].decision, ActivityDecision::ClientReadOnly);
    let v = serde_json::to_value(&log[0]).unwrap();
    for k in ["client", "tool", "summary", "decision", "at"] {
        assert!(v.get(k).is_some(), "{k}");
    }
}

#[tokio::test]
async fn agt_3_ac4_repeat_request_id_returns_first_result_without_asking_again() {
    let c = Arc::new(FixedConfirmer::new(Decision::Approve));
    struct Shared(Arc<FixedConfirmer>);
    #[async_trait]
    impl Confirmer for Shared {
        async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
            self.0.confirm(r).await
        }
    }
    let (d, engine, _dir) = with_engine(Shared(c.clone()));
    let first = publish(&d, "claude", Some("pub-2026-10-19-001"))
        .await
        .unwrap();
    let second = publish(&d, "claude", Some("pub-2026-10-19-001"))
        .await
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(engine.calls().len(), 1, "published once");
    assert_eq!(c.shown().len(), 1, "asked once");
    // Another client with the same id is a different request.
    publish(&d, "other", Some("pub-2026-10-19-001"))
        .await
        .unwrap();
    assert_eq!(engine.calls().len(), 2);
    // Same id for a different action is refused, not silently replayed.
    let e = d
        .execute_method(
            agent("claude"),
            0,
            "delete_capture",
            &json!({"capture_id":"rec_1","request_id":"pub-2026-10-19-001"}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    // Declined calls are not remembered: a retry asks again.
    let (d, engine, _dir) = with_engine(FixedConfirmer::new(Decision::Decline));
    publish(&d, "claude", Some("r")).await.unwrap_err();
    publish(&d, "claude", Some("r")).await.unwrap_err();
    assert!(engine.calls().is_empty());
}

#[tokio::test]
async fn agt_3_ac4_concurrent_repeats_run_once() {
    let (d, engine, _dir) = with_engine(FixedConfirmer::new(Decision::Approve));
    let mut tasks = Vec::new();
    for _ in 0..5 {
        let d = d.clone();
        tasks.push(tokio::spawn(async move {
            publish(&d, "claude", Some("same")).await
        }));
    }
    let mut results = Vec::new();
    for t in tasks {
        results.push(t.await.unwrap().unwrap());
    }
    assert!(results.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(engine.calls().len(), 1);
}

#[tokio::test]
async fn agt_3_idempotency_expires_after_24_hours() {
    let clock = Arc::new(ManualClock::new(
        time::macros::datetime!(2026-10-19 09:00 UTC),
    ));
    struct Shared(Arc<ManualClock>);
    impl cm_daemon::Clock for Shared {
        fn now(&self) -> time::OffsetDateTime {
            self.0.now()
        }
    }
    let engine = Arc::new(CountingExternal::default());
    let (b, _dir) = builder(desktop());
    let d = b
        .clock(Shared(clock.clone()))
        .confirmer(FixedConfirmer::new(Decision::Approve))
        .external_actions(engine.clone())
        .build();
    publish(&d, "c", Some("r")).await.unwrap();
    clock.advance(time::Duration::hours(23));
    publish(&d, "c", Some("r")).await.unwrap();
    assert_eq!(engine.calls().len(), 1);
    clock.advance(time::Duration::hours(2));
    publish(&d, "c", Some("r")).await.unwrap();
    assert_eq!(engine.calls().len(), 2);
}

#[tokio::test]
async fn agt_2_dropping_a_pending_call_frees_the_queue() {
    // A client that disconnects while its dialog is open must not block the next request.
    struct FirstHangs(std::sync::atomic::AtomicUsize);
    #[async_trait]
    impl Confirmer for FirstHangs {
        async fn confirm(&self, _r: &ConfirmationRequest) -> Decision {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                std::future::pending().await
            }
            Decision::Approve
        }
    }
    let (d, engine, _dir) = with_engine(FirstHangs(Default::default()));
    let d1 = d.clone();
    let stuck = tokio::spawn(async move { publish(&d1, "gone", None).await });
    while !stuck.is_finished() {
        tokio::task::yield_now().await;
        if engine.calls().is_empty() {
            // Let the first call reach its dialog.
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
            break;
        }
    }
    stuck.abort();
    let _ = stuck.await;
    publish(&d, "next", None).await.unwrap();
    assert_eq!(engine.calls().len(), 1);
}

/// AGT-2: a first-party helper (the overlay) is allowed by default and attributed as an agent;
/// claiming its name gives no way around the confirmation gate, and a policy still applies.
#[tokio::test]
async fn agt_2_first_party_helpers_are_agents_allowed_by_default_but_still_gated() {
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Decline));
    struct Shared(Arc<FixedConfirmer>);
    #[async_trait]
    impl Confirmer for Shared {
        async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
            self.0.confirm(r).await
        }
    }
    let (d, engine, _dir) = with_engine(Shared(confirmer.clone()));
    let mut c = Client::connect(&d, "overlay");
    c.hello("capturemann-overlay").await;
    assert!(cm_daemon::FIRST_PARTY_CLIENTS.contains(&"capturemann-overlay"));

    // Free tier: runs without a dialog, attributed to the helper.
    let shot = c
        .call("capture_screenshot", json!({"target": {"display_id": 1}}))
        .await;
    let id = shot["result"]["capture_id"].as_str().unwrap().to_owned();
    let ann = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": [{"op":"rect","rect":[1,1,5,5]}]}),
        )
        .await;
    assert_eq!(ann["result"]["by"], "agent:capturemann-overlay", "{ann}");
    assert!(confirmer.shown().is_empty());

    // Confirm tier: the name changes nothing; the dialog is shown and declining refuses.
    let r = c.call("delete_capture", json!({"capture_id": id})).await;
    assert_eq!(r["error"]["data"]["code"], "user_declined");
    assert_eq!(confirmer.shown().len(), 1);
    assert_eq!(confirmer.shown()[0].client, "capturemann-overlay");
    assert!(engine.calls().is_empty());

    // Policy still applies to the name.
    d.policies()
        .set("capturemann-overlay", ClientPolicy::Blocked);
    let r = c.call("freeze_display", json!({"display_id": 1})).await;
    assert_eq!(r["error"]["data"]["code"], "user_declined");
}
