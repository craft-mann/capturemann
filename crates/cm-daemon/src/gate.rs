//! The confirmation gate (AGT-2), per-client policy and the agent activity log.
//!
//! * [`Confirmer`] is the UI hook: the desktop app implements it with the native modal
//!   (DESIGN section 5.6). The daemon only decides *when* to ask, queues requests so exactly one
//!   dialog is open at a time, and enforces the timeout. The 1 s "ignore Enter" rule
//!   (AGT-2.AC2) is the dialog's business.
//! * If a request is dropped while its dialog is open (the client disconnected), the
//!   `confirm` future is dropped; implementations must close the dialog on drop.
//! * The timeout starts when a request reaches the front of the queue and its dialog opens,
//!   not while it waits behind another dialog.
//! * The timeout is injectable ([`crate::DaemonBuilder::confirm_timeout`]); tests also run
//!   under tokio's paused clock, so five minutes cost nothing.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;
use time::OffsetDateTime;

use crate::action::{ActionSpec, Tier};
use crate::events::{self, EventBus};

/// Confirmation timeout from the specs: 5 minutes.
pub const DEFAULT_CONFIRM_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// The activity log keeps entries for 30 days (AGT-2).
pub const ACTIVITY_RETENTION: time::Duration = time::Duration::days(30);
/// ... and at most this many (oldest dropped first), so a chatty client cannot grow it without
/// bound inside the retention window (R2-6).
pub const MAX_ACTIVITY_ENTRIES: usize = 2000;
const MAX_ACTIVITY_CLIENT_CHARS: usize = 64;
const MAX_ACTIVITY_TOOL_CHARS: usize = 64;
const MAX_ACTIVITY_SUMMARY_CHARS: usize = 512;

fn truncate_chars(s: &mut String, max: usize) {
    if let Some((i, _)) = s.char_indices().nth(max) {
        s.truncate(i);
        s.push_str("...");
    }
}

/// What the dialog shows (DESIGN section 5.6: agent chip, one-sentence summary, details).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfirmationRequest {
    pub id: String,
    /// Client name, e.g. `claude-desktop` ("Requested by Claude Desktop").
    pub client: String,
    pub tool: String,
    /// The English sentence. A client that words the dialog itself builds it from `kind` and
    /// `params` and shows this one for a `kind` it does not know.
    pub summary: String,
    /// The action input (file, account, privacy, size...) as English label/value rows.
    pub details: Value,
    /// The stable id of what is asked: the action id (`start_recording`, `publish_video`,
    /// `delete_capture` ...). Same as `tool`.
    pub kind: String,
    /// The plan as typed parameters (title, platform, privacy, size, count ...) that
    /// `summary` and `details` are worded from. Additive: older clients ignore it. An object, or
    /// `{}` when the sentence has nothing to fill in.
    pub params: Value,
    /// Whether the dialog may offer "Allow for this session" (`start_recording` only).
    pub allow_session: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approve,
    /// Approve and skip the dialog for this client's later `start_recording` calls until it
    /// disconnects. Treated as `Approve` when the request does not allow it.
    ApproveForSession,
    Decline,
}

#[async_trait]
pub trait Confirmer: Send + Sync {
    /// Shows the dialog and resolves when the user answers. Cancel-safe: dropping the future
    /// must close the dialog.
    async fn confirm(&self, request: &ConfirmationRequest) -> Decision;

    /// Whether a person can actually answer (a dialog exists). `false` for the headless
    /// [`DenyAll`]: the gate then refuses with `data.reason = "no_confirmer"` and a message that
    /// says no confirmation UI is attached, instead of pretending the user declined (R2-8).
    /// Default `true`, so existing implementors are unaffected.
    fn is_attached(&self) -> bool {
        true
    }
}

/// Declines everything. The default when no UI is attached (a headless daemon never lets an
/// agent record, publish or delete).
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyAll;

#[async_trait]
impl Confirmer for DenyAll {
    async fn confirm(&self, _request: &ConfirmationRequest) -> Decision {
        Decision::Decline
    }

    fn is_attached(&self) -> bool {
        false
    }
}

/// How a gated request ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateOutcome {
    Approved,
    ApprovedForSession,
    /// Skipped the dialog because of an earlier "allow for this session".
    ApprovedBySession,
    Declined,
    TimedOut,
    /// No confirmation UI is attached to this daemon (headless): nobody was asked.
    NoConfirmer,
    /// Too many confirmation requests are already waiting (per client or in total).
    Busy,
}

/// Most Confirm-tier requests one client may have waiting at the gate (R2-6).
pub const MAX_PENDING_PER_CLIENT: usize = 3;
/// Most Confirm-tier requests waiting at the gate in total (R2-6).
pub const MAX_PENDING_TOTAL: usize = 16;

impl GateOutcome {
    /// The activity-log decision and the error (if any) the caller gets.
    pub(crate) fn resolve(
        self,
        timeout: Duration,
    ) -> (ActivityDecision, Option<crate::error::DaemonError>) {
        use crate::error::DaemonError;
        use cm_core::CmError;
        match self {
            GateOutcome::Approved | GateOutcome::ApprovedBySession => {
                (ActivityDecision::Approved, None)
            }
            GateOutcome::ApprovedForSession => (ActivityDecision::ApprovedForSession, None),
            GateOutcome::Declined => (
                ActivityDecision::Declined,
                Some(CmError::user_declined("The user declined the request").into()),
            ),
            GateOutcome::TimedOut => (
                ActivityDecision::TimedOut,
                Some(
                    CmError::confirmation_timeout(format!(
                        "No answer within {}",
                        crate::daemon::humanize(timeout)
                    ))
                    .into(),
                ),
            ),
            GateOutcome::NoConfirmer => (
                ActivityDecision::Declined,
                Some(DaemonError::Declined {
                    reason: "no_confirmer",
                    message: "No confirmation UI is attached to this daemon".to_owned(),
                }),
            ),
            GateOutcome::Busy => (
                ActivityDecision::Declined,
                Some(
                    CmError::busy(
                        "Too many confirmation requests are waiting; answer or dismiss them first",
                    )
                    .into(),
                ),
            ),
        }
    }
}

/// Counts waiting requests; released on drop (also when the caller is cancelled).
struct PendingSlot<'a> {
    gate: &'a ConfirmationGate,
    client: String,
}

impl Drop for PendingSlot<'_> {
    fn drop(&mut self) {
        let mut g = self
            .gate
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(n) = g.per_client.get_mut(&self.client) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                g.per_client.remove(&self.client);
            }
        }
        g.total = g.total.saturating_sub(1);
    }
}

#[derive(Default)]
struct Pending {
    per_client: HashMap<String, usize>,
    total: usize,
}

pub(crate) struct ConfirmationGate {
    confirmer: Arc<dyn Confirmer>,
    timeout: Duration,
    /// Tokio's mutex is fair (FIFO), which gives the queue order.
    queue: tokio::sync::Mutex<()>,
    next_id: std::sync::atomic::AtomicU64,
    session_approvals: Mutex<HashSet<(u64, String)>>,
    pending: Mutex<Pending>,
    events: EventBus,
}

impl ConfirmationGate {
    pub(crate) fn new(confirmer: Arc<dyn Confirmer>, timeout: Duration, events: EventBus) -> Self {
        ConfirmationGate {
            confirmer,
            timeout,
            queue: tokio::sync::Mutex::new(()),
            next_id: std::sync::atomic::AtomicU64::new(1),
            session_approvals: Mutex::new(HashSet::new()),
            pending: Mutex::new(Pending::default()),
            events,
        }
    }

    pub(crate) fn end_session(&self, session: u64) {
        self.session_approvals
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(s, _)| *s != session);
    }

    pub(crate) async fn confirm(
        &self,
        client: &str,
        session: u64,
        spec: &ActionSpec,
        summary: String,
        details: Value,
        params: Value,
    ) -> GateOutcome {
        let allow_session = spec.name == "start_recording";
        let key = (session, spec.name.to_owned());
        if !self.confirmer.is_attached() {
            return GateOutcome::NoConfirmer;
        }
        // Bounded queue (R2-6): a client cannot pile up dialogs.
        let _slot = {
            let mut g = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
            let mine = g.per_client.get(client).copied().unwrap_or(0);
            if mine >= MAX_PENDING_PER_CLIENT || g.total >= MAX_PENDING_TOTAL {
                return GateOutcome::Busy;
            }
            *g.per_client.entry(client.to_owned()).or_default() += 1;
            g.total += 1;
            PendingSlot {
                gate: self,
                client: client.to_owned(),
            }
        };
        let _turn = self.queue.lock().await;
        if allow_session
            && session != 0
            && self
                .session_approvals
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains(&key)
        {
            return GateOutcome::ApprovedBySession;
        }
        let id = format!(
            "conf_{}",
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        let request = ConfirmationRequest {
            id: id.clone(),
            client: client.to_owned(),
            tool: spec.name.to_owned(),
            summary: summary.clone(),
            details,
            kind: spec.name.to_owned(),
            params,
            allow_session,
        };
        self.events.publish(
            events::CONFIRMATION_REQUESTED,
            json!({
                "id": id, "client": client, "tool": spec.name,
                "summary": summary, "details": request.details,
                "kind": request.kind, "params": request.params,
            }),
        );
        let answer = tokio::time::timeout(self.timeout, self.confirmer.confirm(&request)).await;
        let (outcome, label) = match answer {
            Ok(Decision::Approve) => (GateOutcome::Approved, "approved"),
            Ok(Decision::ApproveForSession) if allow_session => {
                if session != 0 {
                    self.session_approvals
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(key);
                }
                (GateOutcome::ApprovedForSession, "approved_for_session")
            }
            Ok(Decision::ApproveForSession) => (GateOutcome::Approved, "approved"),
            Ok(Decision::Decline) => (GateOutcome::Declined, "declined"),
            Err(_) => (GateOutcome::TimedOut, "timeout"),
        };
        self.events.publish(
            events::CONFIRMATION_RESOLVED,
            json!({ "id": request.id, "decision": label }),
        );
        outcome
    }
}

/// Per-client policy (Settings -> Agents).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientPolicy {
    /// The default.
    #[default]
    Allowed,
    /// Confirm-tier and mutating Free actions are refused without a dialog.
    ReadOnly,
    /// Every action is refused.
    Blocked,
}

impl ClientPolicy {
    /// `Some(reason)` if this policy refuses `spec` (AGT-2.AC4).
    pub fn refusal(self, spec: &ActionSpec) -> Option<&'static str> {
        match self {
            ClientPolicy::Allowed => None,
            ClientPolicy::Blocked => Some("client_blocked"),
            ClientPolicy::ReadOnly => {
                (spec.mutating || spec.tier == Tier::Confirm).then_some("client_read_only")
            }
        }
    }
}

/// Names CaptureMann's own helper processes send in `session.hello`.
///
/// # Why there is no `user` role over the socket
///
/// A socket client could only prove "I am a first-party helper" with something an agent
/// cannot also get. Both run as the same OS user, so a 0600 token file in the runtime
/// directory (the obvious design) is readable by an agent that has a shell, and a connection
/// from the same uid proves nothing either. Letting such a claim skip policy or the
/// confirmation gate would let any local agent talk its way past both. So socket clients are
/// *always* `agent:<name>`, whatever they claim, and Confirm-tier actions always wait for the
/// dialog.
///
/// First-party helpers are attributed honestly instead (`agent:capturemann-overlay` in
/// revisions and `capture.created`), and these names are **allowed by default** like any
/// client: the overlay's user-initiated captures run without confirmation because they are
/// Free tier, not because of the name. Claiming the name grants nothing an unknown client
/// lacks; Settings -> Agents can still set a policy for it (blocking the overlay then makes
/// the picker fail with a clear `user_declined`).
pub const FIRST_PARTY_CLIENTS: &[&str] =
    &["capturemann-overlay", "capturemann-cli", "capturemann-mcp"];

/// # Policies are not a security boundary
///
/// The client name is **self-declared** (`session.hello`): any local process running as the
/// same user can claim any name, including a never-blocked one. A policy therefore limits
/// well-behaved clients and attributes their actions; it does not contain a malicious local
/// process. What does hold regardless of names: the confirmation gate (Confirm-tier actions
/// always wait for the dialog, and the pending queue is capped globally), the app-only
/// settings deny list, and the Never tier. To keep a name from being swapped mid-connection,
/// a connection's name is immutable once set (see [`crate::rpc`]).
///
/// Client name -> policy; unknown clients are `Allowed`. Owned by the daemon so the settings
/// UI and the server see the same table. Persisting it is the desktop app's job.
#[derive(Debug, Default)]
pub struct ClientPolicies {
    map: RwLock<HashMap<String, ClientPolicy>>,
    /// Policy for names without an entry (`Allowed` unless the app starts in deny-unknown mode).
    default: RwLock<ClientPolicy>,
}

impl ClientPolicies {
    pub fn set(&self, client: &str, policy: ClientPolicy) {
        self.map
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(client.to_owned(), policy);
    }

    pub fn get(&self, client: &str) -> ClientPolicy {
        self.map
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(client)
            .copied()
            .unwrap_or_else(|| self.default_policy())
    }

    /// Sets the policy of clients without an entry. `Blocked` starts a fail-closed
    /// "deny unknown clients" mode (used when the stored policy list could not be read).
    pub fn set_default(&self, policy: ClientPolicy) {
        *self.default.write().unwrap_or_else(PoisonError::into_inner) = policy;
    }

    /// The policy of clients without an entry (`Allowed` by default).
    pub fn default_policy(&self) -> ClientPolicy {
        *self.default.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// `Some(reason)` if `client` may not run `spec`: `client_blocked`, `client_read_only`, or
    /// `default_deny` when an unknown client is refused by a `Blocked` default.
    pub fn refusal(&self, client: &str, spec: &ActionSpec) -> Option<&'static str> {
        let reason = self.get(client).refusal(spec)?;
        Some(self.refine(client, reason))
    }

    /// `Some(reason)` if `client` is blocked outright (`client_blocked` or `default_deny`).
    pub fn blocked_reason(&self, client: &str) -> Option<&'static str> {
        (self.get(client) == ClientPolicy::Blocked).then(|| self.refine(client, "client_blocked"))
    }

    fn refine(&self, client: &str, reason: &'static str) -> &'static str {
        let known = self
            .map
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(client);
        if reason == "client_blocked" && !known {
            "default_deny"
        } else {
            reason
        }
    }

    pub fn all(&self) -> Vec<(String, ClientPolicy)> {
        let mut v: Vec<_> = self
            .map
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityDecision {
    Approved,
    ApprovedForSession,
    Declined,
    TimedOut,
    ClientReadOnly,
    ClientBlocked,
}

/// One row of the Agent activity log (AGT-2.AC6).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActivityEntry {
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
    pub client: String,
    pub tool: String,
    pub summary: String,
    pub decision: ActivityDecision,
}

/// In-memory log of confirmed, declined, timed-out and policy-refused requests, pruned to
/// [`ACTIVITY_RETENTION`]. Persisting it across restarts is the desktop app's job (a sink
/// can be added when the storage layer exists).
#[derive(Debug, Default)]
pub(crate) struct ActivityLog {
    entries: Mutex<VecDeque<ActivityEntry>>,
}

impl ActivityLog {
    pub(crate) fn record(&self, mut entry: ActivityEntry) {
        truncate_chars(&mut entry.client, MAX_ACTIVITY_CLIENT_CHARS);
        truncate_chars(&mut entry.tool, MAX_ACTIVITY_TOOL_CHARS);
        truncate_chars(&mut entry.summary, MAX_ACTIVITY_SUMMARY_CHARS);
        let mut g = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let cutoff = entry.at - ACTIVITY_RETENTION;
        while g.front().is_some_and(|e| e.at < cutoff) {
            g.pop_front();
        }
        g.push_back(entry);
        while g.len() > MAX_ACTIVITY_ENTRIES {
            g.pop_front();
        }
    }

    pub(crate) fn entries(&self, now: OffsetDateTime) -> Vec<ActivityEntry> {
        let cutoff = now - ACTIVITY_RETENTION;
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|e| e.at >= cutoff)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::spec_for;

    fn spec(n: &str) -> ActionSpec {
        *spec_for(n).unwrap()
    }

    /// P3: the gate's error messages have stable ids a client can translate; the English text
    /// (what the CLI and MCP print) is exactly as before.
    #[test]
    fn p3_gate_messages_have_stable_ids() {
        use cm_core::message::describe;
        let t = Duration::from_secs(300);
        let cases = [
            (
                GateOutcome::Declined,
                "declined",
                "The user declined the request",
            ),
            (
                GateOutcome::TimedOut,
                "confirmation_timeout",
                "No answer within 5 minutes",
            ),
            (
                GateOutcome::Busy,
                "confirmation_queue_full",
                "Too many confirmation requests are waiting; answer or dismiss them first",
            ),
        ];
        for (outcome, id, english) in cases {
            let (_, err) = outcome.resolve(t);
            let err = err.unwrap();
            assert_eq!(err.to_string(), english);
            let code = err.code().unwrap();
            assert_eq!(describe(code, english).unwrap().id, id);
        }
    }

    #[test]
    fn agt_2_ac4_read_only_refuses_mutating_and_confirm() {
        let p = ClientPolicy::ReadOnly;
        for n in [
            "capture_screenshot",
            "annotate",
            "undo",
            "set_settings",
            "publish_video",
        ] {
            assert_eq!(p.refusal(&spec(n)), Some("client_read_only"), "{n}");
        }
        for n in [
            "list_targets",
            "get_capture",
            "list_library",
            "get_settings",
        ] {
            assert_eq!(p.refusal(&spec(n)), None, "{n}");
        }
        assert_eq!(
            ClientPolicy::Blocked.refusal(&spec("list_targets")),
            Some("client_blocked")
        );
        assert_eq!(ClientPolicy::Allowed.refusal(&spec("delete_capture")), None);
    }

    #[test]
    fn r2_6_activity_log_is_capped_and_fields_truncated() {
        let log = ActivityLog::default();
        let t0 = time::macros::datetime!(2026-10-01 00:00 UTC);
        for i in 0..(MAX_ACTIVITY_ENTRIES + 50) {
            log.record(ActivityEntry {
                at: t0 + time::Duration::seconds(i as i64),
                client: "c".repeat(1000),
                tool: "t".repeat(1000),
                summary: "s".repeat(100_000),
                decision: ActivityDecision::Approved,
            });
        }
        let all = log.entries(t0 + time::Duration::days(1));
        assert_eq!(all.len(), MAX_ACTIVITY_ENTRIES);
        assert!(all[0].summary.chars().count() <= MAX_ACTIVITY_SUMMARY_CHARS + 3);
        assert!(all[0].client.chars().count() <= MAX_ACTIVITY_CLIENT_CHARS + 3);
    }

    struct Hang;
    #[async_trait]
    impl Confirmer for Hang {
        async fn confirm(&self, _r: &ConfirmationRequest) -> Decision {
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn r2_6_pending_confirmations_are_capped_per_client_and_total() {
        let bus = EventBus::new(Arc::new(crate::clock::SystemClock));
        let gate = Arc::new(ConfirmationGate::new(
            Arc::new(Hang),
            Duration::from_secs(300),
            bus,
        ));
        let sp = spec("delete_capture");
        let mut tasks = Vec::new();
        for _ in 0..MAX_PENDING_PER_CLIENT {
            let g = gate.clone();
            tasks.push(tokio::spawn(async move {
                g.confirm("a", 1, &sp, "s".into(), Value::Null, json!({}))
                    .await
            }));
        }
        tokio::task::yield_now().await;
        assert_eq!(
            gate.confirm("a", 1, &sp, "s".into(), Value::Null, json!({}))
                .await,
            GateOutcome::Busy
        );
        // Other clients fill the total.
        for i in 0..(MAX_PENDING_TOTAL - MAX_PENDING_PER_CLIENT) {
            let g = gate.clone();
            let name = format!("c{}", i / MAX_PENDING_PER_CLIENT);
            tasks.push(tokio::spawn(async move {
                g.confirm(&name, 1, &sp, "s".into(), Value::Null, json!({}))
                    .await
            }));
        }
        tokio::task::yield_now().await;
        assert_eq!(
            gate.confirm("zz", 1, &sp, "s".into(), Value::Null, json!({}))
                .await,
            GateOutcome::Busy
        );
        // Cancelling the waiters frees their slots.
        for t in &tasks {
            t.abort();
        }
        for t in tasks {
            let _ = t.await;
        }
        let g = gate.clone();
        let t = tokio::spawn(async move {
            g.confirm("zz", 1, &sp, "s".into(), Value::Null, json!({}))
                .await
        });
        tokio::task::yield_now().await;
        assert!(!t.is_finished(), "a slot was free again");
        t.abort();
    }

    #[tokio::test]
    async fn r2_8_deny_all_is_not_a_user_decline() {
        let bus = EventBus::new(Arc::new(crate::clock::SystemClock));
        let gate = ConfirmationGate::new(Arc::new(DenyAll), Duration::from_secs(1), bus);
        let out = gate
            .confirm(
                "a",
                1,
                &spec("delete_capture"),
                "s".into(),
                Value::Null,
                json!({}),
            )
            .await;
        assert_eq!(out, GateOutcome::NoConfirmer);
        let (_, err) = out.resolve(Duration::from_secs(1));
        let e = err.unwrap();
        assert_eq!(e.payload()["data"]["reason"], "no_confirmer");
        assert!(e.to_string().contains("No confirmation UI"));
    }

    #[test]
    fn default_deny_blocks_unknown_but_not_named_clients() {
        let p = ClientPolicies::default();
        assert_eq!(p.default_policy(), ClientPolicy::Allowed);
        p.set("known", ClientPolicy::Allowed);
        p.set_default(ClientPolicy::Blocked);
        assert_eq!(p.get("stranger"), ClientPolicy::Blocked);
        assert_eq!(p.get("known"), ClientPolicy::Allowed);
        let sp = spec("list_targets");
        assert_eq!(p.refusal("stranger", &sp), Some("default_deny"));
        assert_eq!(p.refusal("known", &sp), None);
        p.set("banned", ClientPolicy::Blocked);
        assert_eq!(p.refusal("banned", &sp), Some("client_blocked"));
        p.set_default(ClientPolicy::Allowed);
        assert_eq!(p.refusal("stranger", &sp), None);
    }

    #[test]
    fn default_policy_is_allowed() {
        let p = ClientPolicies::default();
        assert_eq!(p.get("anyone"), ClientPolicy::Allowed);
        p.set("a", ClientPolicy::Blocked);
        assert_eq!(p.get("a"), ClientPolicy::Blocked);
        assert_eq!(p.all().len(), 1);
    }

    #[test]
    fn activity_log_prunes_after_30_days() {
        let log = ActivityLog::default();
        let t0 = time::macros::datetime!(2026-10-01 00:00 UTC);
        let mk = |at| ActivityEntry {
            at,
            client: "c".into(),
            tool: "t".into(),
            summary: "s".into(),
            decision: ActivityDecision::Approved,
        };
        log.record(mk(t0));
        log.record(mk(t0 + time::Duration::days(31)));
        assert_eq!(log.entries(t0 + time::Duration::days(31)).len(), 1);
    }
}
