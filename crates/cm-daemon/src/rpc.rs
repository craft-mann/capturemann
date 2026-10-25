//! JSON-RPC 2.0 over a byte stream, one JSON message per line (specs/02-agent-interface.md).
//!
//! # Methods
//!
//! * every action in [`crate::ACTIONS`], by its tool name (`capture_screenshot`, `annotate`, ...);
//! * `session.hello { client: "<name>", version?: "<v>" }` names the connection. The name is
//!   the `agent:<client>` actor in revisions, the key of the per-client policy and what the
//!   confirmation dialog shows. Until it is called the client is `unknown`. The name is
//!   **immutable per connection**: a second hello with a different name is refused
//!   (`invalid_input`), and a hello with a blocked name is refused with `user_declined` /
//!   `client_blocked` (the connection keeps that name, so it stays blocked). Policies are keyed
//!   by this self-declared name, so they are **not a security boundary** against a malicious
//!   local process (it can open a new connection under another name or none); the confirmation
//!   gate, the app-only settings list and the Never tier do not depend on names;
//! * `events.subscribe { topics?: ["capture.*", "job.done"] }` starts the event notifications
//!   (all events when `topics` is omitted); calling it again replaces the topics.
//!   `events.unsubscribe` stops them;
//! * `rpc.cancel { id }` cancels the in-flight request of this connection with that `id` (a
//!   waiting confirmation dialog closes); the cancelled request gets no response. Clients send
//!   it when their own caller gave up (MCP `notifications/cancelled`). It answers
//!   `{ cancelled: bool }` when it has an `id` of its own;
//! * `actions.list` returns `[{name, tier, mutating, available, internal}]` (`internal`:
//!   plumbing for first-party GUI clients, not a CLI command or MCP tool). A blocked client gets
//!   `client_blocked` for `events.subscribe` and `actions.list` too.
//!
//! Notifications to the client: `{"jsonrpc":"2.0","method":"event","params":{seq,event,at,data}}`.
//! If a client reads too slowly it gets an `events.dropped` event with the number lost.
//!
//! # Behavior
//!
//! * Requests on one connection run concurrently (a call waiting for a confirmation does
//!   not block the others); responses may arrive in any order and carry the request `id`.
//! * Socket clients are always **agents** (`Actor::Agent`), so policy and the confirmation
//!   gate apply to the CLI and MCP server too. The GUI calls [`crate::Daemon::execute`] in
//!   process as `Actor::User`.
//! * A request without `id` is a notification: it runs, but no response is sent. `"id": null`
//!   is accepted (JSON-RPC discourages it) and answered with `"id": null`. An `id` that is not
//!   a string, number or null (an object, array or boolean) is `-32600`.
//! * Batches (JSON arrays) are not supported: `-32600` ("send one request per line").
//! * At most [`MAX_INFLIGHT`] requests run at once per connection (more get `busy`), and at
//!   most [`MAX_OUT_BYTES`] of responses/events wait unread per connection (the sender waits;
//!   a slow event subscriber loses events and gets `events.dropped`).
//! * Lines longer than [`MAX_LINE_BYTES`] close the connection after a `-32600` error.
//! * When the client disconnects, its in-flight calls are cancelled (a waiting confirmation
//!   dialog closes) and its "allow for this session" approvals expire.
//! * Errors: see [`crate::error`].

use cm_core::{Actor, CmError};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tokio::task::{JoinHandle, JoinSet};

use crate::daemon::Daemon;
use crate::error::{DaemonError, INVALID_REQUEST, PARSE_ERROR};
use crate::events::{topic_matches, Event};

/// Longest accepted request line (a 5K PNG inside `annotate` is not sent; ops are small).
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
/// Most messages queued for the writer of one connection.
const OUT_QUEUE: usize = 1024;
/// Most bytes queued for the writer of one connection (R2-6). One message larger than this is
/// still sent, alone.
pub const MAX_OUT_BYTES: usize = 32 * 1024 * 1024;
/// Most requests running at the same time on one connection (R2-6).
pub const MAX_INFLIGHT: usize = 64;
const MAX_CLIENT_NAME: usize = 64;
/// Bytes per semaphore permit of the output budget.
const OUT_UNIT: usize = 1024;

/// The write side of a connection: bounded by messages and by bytes.
#[derive(Clone)]
struct Out {
    tx: mpsc::Sender<(String, OwnedSemaphorePermit)>,
    budget: Arc<Semaphore>,
    total_units: u32,
}

type OutRx = mpsc::Receiver<(String, OwnedSemaphorePermit)>;

impl Out {
    fn new(max_bytes: usize) -> (Out, OutRx) {
        let (tx, rx) = mpsc::channel(OUT_QUEUE);
        let total_units = u32::try_from((max_bytes / OUT_UNIT).max(1)).unwrap_or(u32::MAX);
        let budget = Arc::new(Semaphore::new(total_units as usize));
        (
            Out {
                tx,
                budget,
                total_units,
            },
            rx,
        )
    }

    /// Queues `line`, waiting while the connection's byte budget is used up. `Err` when the
    /// connection is gone.
    async fn send(&self, line: String) -> Result<(), ()> {
        let units = u32::try_from(line.len().div_ceil(OUT_UNIT).max(1))
            .unwrap_or(u32::MAX)
            .min(self.total_units);
        let permit = self
            .budget
            .clone()
            .acquire_many_owned(units)
            .await
            .map_err(|_| ())?;
        self.tx.send((line, permit)).await.map_err(|_| ())
    }
}

struct Conn {
    daemon: Daemon,
    session: u64,
    client: String,
    /// `session.hello` succeeded (or was refused for a blocked name): the name is fixed.
    named: bool,
    out: Out,
    subscription: Option<JoinHandle<()>>,
    /// Abort handles of the requests in flight, by the JSON text of their `id`.
    cancel: std::collections::HashMap<String, tokio::task::AbortHandle>,
}

fn response_ok(id: &Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn response_err(id: &Value, err: &DaemonError) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": err.to_rpc_error() }).to_string()
}

fn raw_error(id: &Value, code: i64, message: &str, kind: &str) -> String {
    json!({
        "jsonrpc": "2.0", "id": id,
        "error": { "code": code, "message": message,
                   "data": { "code": kind, "message": message, "data": null } },
    })
    .to_string()
}

fn event_line(e: &Event) -> String {
    json!({ "jsonrpc": "2.0", "method": "event", "params": e }).to_string()
}

/// Serves one connection until the peer closes it.
pub async fn serve_connection<R, W>(daemon: Daemon, reader: R, writer: W)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    serve_connection_with(daemon, reader, writer, MAX_OUT_BYTES).await;
}

/// [`serve_connection`] with an explicit output byte budget (tests).
async fn serve_connection_with<R, W>(daemon: Daemon, reader: R, writer: W, max_out_bytes: usize)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (out_tx, mut out_rx) = Out::new(max_out_bytes);
    let writer_task = tokio::spawn(async move {
        let mut writer = writer;
        // The permit is released once the line is written (or the writer fails).
        while let Some((line, _permit)) = out_rx.recv().await {
            let mut bytes = line.into_bytes();
            bytes.push(b'\n');
            if writer.write_all(&bytes).await.is_err() || writer.flush().await.is_err() {
                break;
            }
        }
    });

    let session = daemon.new_session();
    let mut conn = Conn {
        daemon: daemon.clone(),
        session,
        client: "unknown".to_owned(),
        named: false,
        out: out_tx,
        subscription: None,
        cancel: std::collections::HashMap::new(),
    };
    let mut inflight: JoinSet<()> = JoinSet::new();
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = match (&mut reader)
            .take(MAX_LINE_BYTES as u64 + 1)
            .read_until(b'\n', &mut buf)
            .await
        {
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 {
            break;
        }
        if buf.last() != Some(&b'\n') && buf.len() > MAX_LINE_BYTES {
            let _ = conn
                .out
                .send(raw_error(
                    &Value::Null,
                    INVALID_REQUEST,
                    "request line is too long",
                    "invalid_request",
                ))
                .await;
            break;
        }
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        conn.handle_line(line, &mut inflight).await;
        // Reap finished tasks so the set does not grow without bound.
        while inflight.try_join_next().is_some() {}
    }

    // Disconnect: cancel in-flight calls, stop events, forget session approvals.
    inflight.abort_all();
    if let Some(h) = conn.subscription.take() {
        h.abort();
    }
    daemon.end_session(session);
    drop(conn);
    // Remaining responses (already queued) are flushed when the senders are gone.
    let _ = writer_task.await;
}

impl Conn {
    async fn handle_line(&mut self, line: &str, inflight: &mut JoinSet<()>) {
        let msg: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let _ = self
                    .out
                    .send(raw_error(
                        &Value::Null,
                        PARSE_ERROR,
                        &format!("parse error: {e}"),
                        "parse_error",
                    ))
                    .await;
                return;
            }
        };
        let Some(obj) = msg.as_object() else {
            let text = if msg.is_array() {
                "JSON-RPC batches are not supported; send one request per line"
            } else {
                "expected a JSON-RPC request object"
            };
            let _ = self
                .out
                .send(raw_error(
                    &Value::Null,
                    INVALID_REQUEST,
                    text,
                    "invalid_request",
                ))
                .await;
            return;
        };
        // R2-13: an id is a string, a number or null; anything else is an invalid request.
        if matches!(
            obj.get("id"),
            Some(Value::Object(_) | Value::Array(_) | Value::Bool(_))
        ) {
            let _ = self
                .out
                .send(raw_error(
                    &Value::Null,
                    INVALID_REQUEST,
                    "\"id\" must be a string, a number or null",
                    "invalid_request",
                ))
                .await;
            return;
        }
        let id = obj.get("id").cloned();
        let reply_id = id.clone().unwrap_or(Value::Null);
        let method = obj.get("method").and_then(Value::as_str);
        if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_none() {
            let _ = self
                .out
                .send(raw_error(
                    &reply_id,
                    INVALID_REQUEST,
                    "not a JSON-RPC 2.0 request (needs \"jsonrpc\":\"2.0\" and a string \"method\")",
                    "invalid_request",
                ))
                .await;
            return;
        }
        let method = method.unwrap_or_default().to_owned();
        let params = obj.get("params").cloned().unwrap_or(Value::Null);

        // Session-level methods are quick and order-sensitive: handled inline.
        let inline = match method.as_str() {
            "session.hello" => Some(self.hello(&params)),
            "events.subscribe" => Some(self.blocked().map_or_else(|| self.subscribe(&params), Err)),
            "events.unsubscribe" => Some(self.unsubscribe()),
            "actions.list" => Some(self.blocked().map_or_else(|| Ok(self.actions_list()), Err)),
            "rpc.cancel" => Some(self.cancel_request(&params)),
            _ => None,
        };
        if let Some(result) = inline {
            if id.is_some() {
                let line = match result {
                    Ok(v) => response_ok(&reply_id, v),
                    Err(e) => response_err(&reply_id, &e),
                };
                let _ = self.out.send(line).await;
            }
            return;
        }

        // R2-6: bounded concurrency per connection.
        while inflight.try_join_next().is_some() {}
        if inflight.len() >= MAX_INFLIGHT {
            if id.is_some() {
                let e: DaemonError = CmError::busy(format!(
                    "too many requests in flight on this connection (max {MAX_INFLIGHT})"
                ))
                .into();
                let _ = self.out.send(response_err(&reply_id, &e)).await;
            }
            return;
        }

        let actor = match Actor::agent(self.client.clone()) {
            Ok(a) => a,
            Err(_) => Actor::Agent("unknown".to_owned()),
        };
        let daemon = self.daemon.clone();
        let session = self.session;
        let out = self.out.clone();
        let key = id.as_ref().map(Value::to_string);
        let handle = inflight.spawn(async move {
            let result = daemon
                .execute_method(actor, session, &method, &params)
                .await;
            if id.is_some() {
                let line = match result {
                    Ok(v) => response_ok(&reply_id, v),
                    Err(e) => response_err(&reply_id, &e),
                };
                let _ = out.send(line).await;
            }
        });
        if let Some(key) = key {
            self.cancel.retain(|_, h| !h.is_finished());
            self.cancel.insert(key, handle);
        }
    }

    fn cancel_request(&mut self, params: &Value) -> Result<Value, DaemonError> {
        let id = params
            .get("id")
            .filter(|v| v.is_string() || v.is_number())
            .ok_or_else(|| CmError::invalid_field("id", "is required (the id of the request)"))?;
        let cancelled = match self.cancel.remove(&id.to_string()) {
            Some(h) if !h.is_finished() => {
                h.abort();
                true
            }
            _ => false,
        };
        Ok(json!({ "cancelled": cancelled }))
    }

    /// `client_blocked` when this connection's name is blocked (R2-5).
    fn blocked(&self) -> Option<DaemonError> {
        self.daemon
            .policies()
            .blocked_reason(&self.client)
            .map(|reason| DaemonError::Declined {
                reason,
                message: format!("Client `{}` is blocked", self.client),
            })
    }

    fn hello(&mut self, params: &Value) -> Result<Value, DaemonError> {
        let name = params
            .get("client")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| CmError::invalid_field("client", "is required"))?;
        if name.len() > MAX_CLIENT_NAME || name.chars().any(char::is_control) {
            return Err(CmError::invalid_field(
                "client",
                format!("must be up to {MAX_CLIENT_NAME} printable characters"),
            )
            .into());
        }
        // R2-5: the name is fixed once set, so a connection cannot shed a blocked name.
        if self.named && name != self.client {
            return Err(CmError::invalid_field(
                "client",
                format!(
                    "this connection is already named `{}`; open a new connection to use another name",
                    self.client
                ),
            )
            .into());
        }
        if !self.named {
            self.client = name.to_owned();
            self.named = true;
        }
        if let Some(e) = self.blocked() {
            return Err(e);
        }
        Ok(json!({
            "protocol": "2.0",
            "daemon_version": env!("CARGO_PKG_VERSION"),
            "client": self.client,
        }))
    }

    fn subscribe(&mut self, params: &Value) -> Result<Value, DaemonError> {
        let topics: Vec<String> = match params.get("topics") {
            None | Some(Value::Null) => vec!["*".to_owned()],
            Some(Value::Array(a)) if !a.is_empty() => a
                .iter()
                .map(|t| {
                    t.as_str().map(str::to_owned).ok_or_else(|| {
                        CmError::invalid_field("topics", "must be an array of strings")
                    })
                })
                .collect::<Result<_, _>>()?,
            Some(_) => {
                return Err(CmError::invalid_field(
                    "topics",
                    "must be a non-empty array of strings",
                )
                .into())
            }
        };
        if let Some(h) = self.subscription.take() {
            h.abort();
        }
        let mut rx = self.daemon.subscribe();
        let out = self.out.clone();
        let bus = self.daemon.event_bus().clone();
        let filter = topics.clone();
        self.subscription = Some(tokio::spawn(async move {
            use tokio::sync::broadcast::error::RecvError;
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        if filter.iter().any(|t| topic_matches(t, &ev.name))
                            && out.send(event_line(&ev)).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        let ev = Event {
                            seq: bus.current_seq(),
                            name: crate::events::EVENTS_DROPPED.to_owned(),
                            at: bus.now(),
                            data: json!({ "dropped": n }),
                        };
                        if out.send(event_line(&ev)).await.is_err() {
                            break;
                        }
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }));
        Ok(json!({ "subscribed": true, "topics": topics }))
    }

    fn unsubscribe(&mut self) -> Result<Value, DaemonError> {
        let was = self.subscription.take().map(|h| h.abort()).is_some();
        Ok(json!({ "subscribed": false, "was_subscribed": was }))
    }

    fn actions_list(&self) -> Value {
        Value::Array(
            self.daemon
                .actions()
                .into_iter()
                .map(|(s, available)| {
                    json!({
                        "name": s.name, "tier": s.tier,
                        "mutating": s.mutating, "available": available,
                        "internal": crate::action::is_internal(s.name),
                    })
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// R2-6: queued output is bounded by bytes, not only by message count; the sender waits
    /// until the writer has taken earlier messages, and one oversized message still goes.
    #[tokio::test]
    async fn r2_6_output_is_bounded_by_bytes() {
        let (out, mut rx) = Out::new(16 * 1024);
        out.send("a".repeat(10 * 1024)).await.unwrap();
        // The second 10 KiB message does not fit next to the first: it waits.
        let o2 = out.clone();
        let second = tokio::spawn(async move { o2.send("b".repeat(10 * 1024)).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!second.is_finished(), "must wait for the writer");
        // The writer takes the first message and releases its bytes.
        let (first, permit) = rx.recv().await.unwrap();
        assert!(first.starts_with('a'));
        drop(permit);
        second.await.unwrap().unwrap();
        let (m, permit) = rx.recv().await.unwrap();
        assert!(m.starts_with('b'));
        drop(permit);
        // A message bigger than the whole budget is sent alone.
        out.send("c".repeat(100 * 1024)).await.unwrap();
        let (m, _) = rx.recv().await.unwrap();
        assert_eq!(m.len(), 100 * 1024);
    }

    #[tokio::test]
    async fn send_fails_when_the_connection_is_gone() {
        let (out, rx) = Out::new(1024);
        drop(rx);
        assert!(out.send("x".into()).await.is_err());
    }
}
